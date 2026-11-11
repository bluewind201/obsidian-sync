// 릴리즈 빌드에서는 콘솔 창을 띄우지 않음
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod platform;
mod ui;

use eframe::egui::{self, RichText};
use obsidian_sync_core::backend::synology;
use obsidian_sync_core::{
    auth, service, AppConfig, BackendConfig, ConflictPolicy, Profile, ProfileKind, ServiceStatus,
    SyncMode, SyncService,
};
use platform::SharedTrigger;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use ui::{ACCENT, DANGER, MUTED, OK, WARN};

/// 저장소 종류: (kind, 배지, 배지 색, 이름, 설명)
const BACKENDS: &[(&str, &str, egui::Color32, &str, &str)] = &[
    (
        "synology",
        "NAS",
        egui::Color32::from_rgb(40, 90, 170),
        "Synology NAS",
        "DSM 계정으로 로그인 (File Station)",
    ),
    (
        "local_folder",
        "폴더",
        egui::Color32::from_rgb(110, 114, 128),
        "폴더 / 공유 드라이브",
        "NAS 공유폴더(SMB), 외장 디스크 등",
    ),
    (
        "one_drive",
        "OD",
        egui::Color32::from_rgb(0, 120, 212),
        "OneDrive",
        "Microsoft 계정으로 로그인",
    ),
    (
        "google_drive",
        "GD",
        egui::Color32::from_rgb(30, 142, 62),
        "Google Drive",
        "Google 계정으로 로그인",
    ),
    (
        "s3",
        "S3",
        egui::Color32::from_rgb(3, 199, 90),
        "네이버 클라우드 / GCS",
        "S3 호환 스토리지 (액세스 키 필요)",
    ),
];

const SETUP_DOC: &str = "https://github.com/bluewind201/obsidian-sync/blob/main/docs/SETUP.md";

fn default_backend(kind: &str) -> BackendConfig {
    match kind {
        "s3" => BackendConfig::S3 {
            endpoint: "https://kr.object.ncloudstorage.com".into(),
            region: "kr-standard".into(),
            bucket: String::new(),
            prefix: "obsidian".into(),
            access_key: String::new(),
            secret_key: String::new(),
        },
        "one_drive" => BackendConfig::OneDrive {
            client_id: String::new(),
            tenant: "common".into(),
            folder: obsidian_sync_core::config::default_folder(),
        },
        "google_drive" => BackendConfig::GoogleDrive {
            client_id: String::new(),
            client_secret: String::new(),
            folder: obsidian_sync_core::config::default_folder(),
        },
        "local_folder" => BackendConfig::LocalFolder {
            path: String::new(),
        },
        _ => BackendConfig::default(),
    }
}

#[derive(PartialEq, Clone, Copy)]
enum Page {
    Home,
    Settings,
    Log,
}

enum Job {
    Running,
    Ok(String),
    Failed(String),
    /// Synology 2단계 인증 코드가 필요함
    NeedOtp(String),
}

/// NAS 폴더 선택 창 상태
enum BrowseState {
    Loading,
    Ready(Vec<synology::NasFolder>),
    NeedOtp(String),
    Failed(String),
}

struct NasBrowser {
    /// 지금 보고 있는 NAS 폴더 ("/" = 공유 폴더 목록)
    path: String,
    state: Arc<Mutex<BrowseState>>,
    new_name: String,
    otp: String,
}

enum LoginState {
    Requesting,
    Code { user_code: String, uri: String },
    Done,
    Failed(String),
}

struct Login {
    state: Arc<Mutex<LoginState>>,
    cancel: Arc<AtomicBool>,
}

struct App {
    config_path: PathBuf,
    data_dir: PathBuf,
    cfg: AppConfig,
    saved: AppConfig,
    /// 설정 화면에서 선택한 프로필 번호
    sel: usize,
    /// 프로필별 제외 패턴 입력칸 내용
    ignore_texts: HashMap<String, String>,
    /// 저장소 종류를 바꿨다가 돌아올 때 입력값 복원용 (프로필 ID, 종류)
    backend_cache: HashMap<(String, &'static str), BackendConfig>,
    service: Option<SyncService>,
    /// 백그라운드에서 멈추는 중인 이전 서비스 (화면이 멈추지 않도록)
    stopping: Option<std::thread::JoinHandle<()>>,
    pending_start: bool,
    pending_reset: Option<(AppConfig, String)>,
    resetting: Option<std::thread::JoinHandle<anyhow::Result<()>>>,
    trigger: SharedTrigger,
    login: Option<Login>,
    test: Option<Arc<Mutex<Job>>>,
    otp: String,
    browser: Option<NasBrowser>,
    confirm_delete: bool,
    message: Option<(String, bool)>,
    autostart: bool,
    show_secret: bool,
    page: Page,
    hwnd: isize,
    hide_on_start: bool,
    _tray: Option<platform::Tray>,
    has_tray: bool,
}

fn app_dirs() -> (PathBuf, PathBuf) {
    // 실행 파일 옆에 portable.txt가 있으면 포터블 모드 (설정을 exe 옆 data 폴더에 저장)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if dir.join("portable.txt").exists() {
                let d = dir.join("data");
                return (d.join("config.json"), d);
            }
        }
    }
    let base = directories::BaseDirs::new()
        .map(|b| b.config_dir().join("ObsidianSync"))
        .unwrap_or_else(|| PathBuf::from("ObsidianSync"));
    (base.join("config.json"), base)
}

fn parse_lines(t: &str) -> Vec<String> {
    t.lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, hide_on_start: bool) -> Self {
        ui::setup_style(&cc.egui_ctx);
        let (config_path, data_dir) = app_dirs();
        let cfg = AppConfig::load(&config_path).unwrap_or_default();
        let mut hwnd = 0isize;
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(h) = cc.window_handle() {
                if let RawWindowHandle::Win32(w) = h.as_raw() {
                    hwnd = w.hwnd.get();
                }
            }
        }
        let trigger: SharedTrigger = Arc::new(Mutex::new(None));
        let tray = platform::create_tray(hwnd, trigger.clone(), cc.egui_ctx.clone());
        let configured = cfg.validate().is_ok();
        let mut app = Self {
            config_path,
            data_dir,
            ignore_texts: cfg
                .profiles
                .iter()
                .map(|p| (p.id.clone(), p.ignore_patterns.join("\n")))
                .collect(),
            saved: cfg.clone(),
            sel: 0,
            backend_cache: HashMap::new(),
            cfg,
            service: None,
            stopping: None,
            pending_start: false,
            pending_reset: None,
            resetting: None,
            trigger,
            login: None,
            test: None,
            otp: String::new(),
            browser: None,
            confirm_delete: false,
            message: None,
            autostart: platform::autostart_enabled(),
            show_secret: false,
            page: if configured {
                Page::Home
            } else {
                Page::Settings
            },
            hwnd,
            hide_on_start: hide_on_start && tray.is_some(),
            has_tray: tray.is_some(),
            _tray: tray,
        };
        if configured && app.cfg.start_on_launch {
            app.start_service();
        }
        app
    }

    fn set_msg(&mut self, text: impl Into<String>, err: bool) {
        self.message = Some((text.into(), err));
    }

    fn dirty(&self) -> bool {
        self.cfg != self.saved
    }

    fn save(&mut self) -> bool {
        match self.cfg.save(&self.config_path) {
            Ok(()) => {
                self.saved = self.cfg.clone();
                true
            }
            Err(e) => {
                self.set_msg(format!("저장하지 못했습니다: {e:#}"), true);
                false
            }
        }
    }

    fn start_service(&mut self) {
        if self.service.is_some() {
            self.stop_service();
        }
        if self.transitioning() {
            self.pending_start = true;
            return;
        }
        if let Err(e) = self.cfg.validate() {
            self.set_msg(format!("{e:#}"), true);
            self.page = Page::Settings;
            return;
        }
        let need_login = self.cfg.profiles.iter().position(|p| {
            p.enabled && p.backend.needs_oauth() && !auth::is_logged_in(&self.data_dir, &p.backend)
        });
        if let Some(i) = need_login {
            let msg = format!(
                "'{}' 프로필: 먼저 '로그인' 버튼으로 계정을 연결해 주세요",
                self.cfg.profiles[i].display_name()
            );
            self.select(i);
            self.set_msg(msg, true);
            self.page = Page::Settings;
            return;
        }
        match SyncService::start(self.cfg.clone(), &self.data_dir) {
            Ok(s) => {
                *self.trigger.lock().unwrap() = Some(s.trigger());
                self.service = Some(s);
                self.message = None;
            }
            Err(e) => self.set_msg(format!("시작하지 못했습니다: {e:#}"), true),
        }
    }

    fn transitioning(&self) -> bool {
        self.stopping.is_some() || self.pending_reset.is_some() || self.resetting.is_some()
    }

    fn reset_profile(&mut self, id: String) {
        if self.pending_reset.is_some() || self.resetting.is_some() {
            return;
        }
        let restart = self.service.is_some() || self.pending_start;
        self.stop_service();
        self.pending_start = restart;
        self.pending_reset = Some((self.cfg.clone(), id));
    }

    fn stop_service(&mut self) {
        self.pending_start = false;
        *self.trigger.lock().unwrap() = None;
        if let Some(s) = self.service.take() {
            debug_assert!(self.stopping.is_none());
            self.stopping = Some(std::thread::spawn(move || s.stop()));
        }
    }

    // Join only completed tasks; never wait for network I/O in a UI callback.
    fn poll_lifecycle(&mut self) {
        if let Some(h) = &self.stopping {
            if !h.is_finished() {
                return;
            }
            if self.stopping.take().unwrap().join().is_err() {
                self.pending_start = false;
                self.pending_reset = None;
                self.set_msg(
                    "동기화 중지를 완료하지 못했습니다. 앱을 다시 시작해 주세요",
                    true,
                );
                return;
            }
        }
        if let Some(h) = &self.resetting {
            if !h.is_finished() {
                return;
            }
            match self.resetting.take().unwrap().join() {
                Ok(Ok(())) => self.set_msg("동기화 기록을 초기화했습니다", false),
                result => {
                    self.pending_start = false;
                    self.set_msg(
                        match result {
                            Ok(Err(e)) => format!("초기화하지 못했습니다: {e:#}"),
                            _ => "초기화 작업을 완료하지 못했습니다".into(),
                        },
                        true,
                    );
                    return;
                }
            }
        }
        if let Some((cfg, id)) = self.pending_reset.take() {
            let dir = self.data_dir.clone();
            self.resetting = Some(std::thread::spawn(move || {
                service::reset_state(&cfg, &id, &dir)
            }));
            return;
        }
        if std::mem::take(&mut self.pending_start) {
            self.start_service();
        }
    }

    // ───────────────────────────── 프로필 ─────────────────────────────
    fn cur(&mut self) -> &mut Profile {
        &mut self.cfg.profiles[self.sel]
    }

    fn select(&mut self, i: usize) {
        if i != self.sel {
            self.sel = i;
            self.test = None;
            self.otp.clear();
            self.confirm_delete = false;
        }
    }

    fn add_profile(&mut self, kind: ProfileKind) {
        // 같은 NAS를 쓰는 경우가 많으므로 현재 프로필의 Synology 계정을 복사 (폴더만 비움)
        let backend = match self.cfg.profiles.get(self.sel).map(|p| &p.backend) {
            Some(BackendConfig::Synology {
                url,
                username,
                password,
                ..
            }) => BackendConfig::Synology {
                url: url.clone(),
                username: username.clone(),
                password: password.clone(),
                folder: "/home".into(),
            },
            _ => BackendConfig::default(),
        };
        let p = Profile {
            name: match kind {
                ProfileKind::Obsidian => "Obsidian".into(),
                ProfileKind::Folder => "새 폴더".into(),
            },
            kind,
            backend,
            ..Default::default()
        };
        self.ignore_texts.insert(p.id.clone(), String::new());
        self.cfg.profiles.push(p);
        let n = self.cfg.profiles.len() - 1;
        self.select(n);
        self.sel = n;
        self.page = Page::Settings;
    }

    fn start_test(&mut self, ctx: &egui::Context) {
        let job = Arc::new(Mutex::new(Job::Running));
        let (j, backend, dir, ctx) = (
            job.clone(),
            self.cur().backend.clone(),
            self.data_dir.clone(),
            ctx.clone(),
        );
        std::thread::spawn(move || {
            let r = service::test_connection(&backend, &dir);
            *j.lock().unwrap() = match r {
                Ok(0) => Job::Ok(
                    "연결 성공! 원격 폴더가 비어 있어(또는 아직 없어) 첫 동기화 때 파일을 올립니다."
                        .into(),
                ),
                Ok(n) => Job::Ok(format!("연결 성공! 원격에 파일 {n}개가 있습니다.")),
                Err(e) if synology::needs_otp(&e) => Job::NeedOtp(format!("{e:#}")),
                Err(e) => Job::Failed(friendly_error(&format!("{e:#}"))),
            };
            ctx.request_repaint();
        });
        self.test = Some(job);
    }

    /// Synology 2단계 인증 코드로 로그인한 뒤 연결 테스트
    fn start_otp(&mut self, ctx: &egui::Context) {
        let job = Arc::new(Mutex::new(Job::Running));
        let (j, backend, dir, ctx, otp) = (
            job.clone(),
            self.cur().backend.clone(),
            self.data_dir.clone(),
            ctx.clone(),
            self.otp.trim().to_string(),
        );
        std::thread::spawn(move || {
            let r = synology::login(&backend, &dir, Some(&otp))
                .and_then(|_| service::test_connection(&backend, &dir));
            *j.lock().unwrap() = match r {
                Ok(n) => Job::Ok(format!(
                    "인증 완료! 이 PC를 기억하므로 다음부터는 코드를 묻지 않습니다. (원격 파일 {n}개)"
                )),
                Err(e) if synology::needs_otp(&e) => Job::NeedOtp(format!("{e:#}")),
                Err(e) => Job::Failed(friendly_error(&format!("{e:#}"))),
            };
            ctx.request_repaint();
        });
        self.otp.clear();
        self.test = Some(job);
    }

    /// NAS 폴더 선택 창 열기 (지금 입력된 폴더의 상위에서 시작)
    fn open_browser(&mut self, ctx: &egui::Context) {
        let start = match &self.cur().backend {
            BackendConfig::Synology { folder, .. } if !folder.trim().is_empty() => {
                let f = format!("/{}", folder.trim().trim_matches('/'));
                match f.rfind('/') {
                    Some(0) | None => "/".to_string(),
                    Some(i) => f[..i].to_string(),
                }
            }
            _ => "/".to_string(),
        };
        self.browser = Some(NasBrowser {
            path: start.clone(),
            state: Arc::new(Mutex::new(BrowseState::Loading)),
            new_name: String::new(),
            otp: String::new(),
        });
        self.browse(ctx, start, None, None);
    }

    /// NAS 폴더 목록 불러오기. `create`가 있으면 그 이름으로 새 폴더를 만든 뒤 그 안으로 들어갑니다.
    fn browse(
        &mut self,
        ctx: &egui::Context,
        path: String,
        create: Option<String>,
        otp: Option<String>,
    ) {
        let Some(b) = &mut self.browser else { return };
        let state = Arc::new(Mutex::new(BrowseState::Loading));
        b.state = state.clone();
        b.path = path.clone();
        let (backend, dir, ctx) = (
            self.cfg.profiles[self.sel].backend.clone(),
            self.data_dir.clone(),
            ctx.clone(),
        );
        let created = create.clone();
        std::thread::spawn(move || {
            let r = (|| {
                if let Some(code) = &otp {
                    synology::login(&backend, &dir, Some(code))?;
                }
                let mut at = path;
                if let Some(name) = &create {
                    at = synology::create_folder(&backend, &dir, &at, name)?;
                }
                synology::list_folders(&backend, &dir, &at)
            })();
            *state.lock().unwrap() = match r {
                Ok(folders) => BrowseState::Ready(folders),
                Err(e) if synology::needs_otp(&e) => BrowseState::NeedOtp(format!("{e:#}")),
                Err(e) => BrowseState::Failed(friendly_error(&format!("{e:#}"))),
            };
            ctx.request_repaint();
        });
        if let Some(b) = &mut self.browser {
            b.new_name.clear();
            if let Some(name) = created {
                // 새 폴더를 만들면 그 안을 보여 줍니다
                let p = b.path.trim_end_matches('/').to_string();
                b.path = format!("{p}/{}", name.trim());
            }
        }
    }

    fn browser_ui(&mut self, ctx: &egui::Context) {
        let Some(b) = &mut self.browser else { return };
        let mut open = true;
        let mut go: Option<String> = None;
        let mut create: Option<String> = None;
        let mut otp: Option<String> = None;
        let mut choose = false;
        let mut cancel = false;
        let path = b.path.clone();
        let at_root = path == "/";
        egui::Window::new("NAS 폴더 선택")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([440.0, 420.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!at_root, egui::Button::new("⬆ 위로"))
                        .clicked()
                    {
                        let up = match path.rfind('/') {
                            Some(0) | None => "/".to_string(),
                            Some(i) => path[..i].to_string(),
                        };
                        go = Some(up);
                    }
                    ui.label(
                        RichText::new(if at_root {
                            "공유 폴더".to_string()
                        } else {
                            path.clone()
                        })
                        .strong(),
                    );
                });
                ui.separator();
                let state = b.state.lock().unwrap();
                match &*state {
                    BrowseState::Loading => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("NAS 폴더를 불러오는 중…");
                        });
                    }
                    BrowseState::Failed(m) => {
                        ui.label(RichText::new(format!("✖ {m}")).color(DANGER));
                        if ui.button("다시 시도").clicked() {
                            go = Some(path.clone());
                        }
                    }
                    BrowseState::NeedOtp(m) => {
                        ui.label(RichText::new("🔐 2단계 인증").strong());
                        ui.label(
                            RichText::new(format!(
                                "{}\nOTP 앱의 6자리 코드를 입력하세요.",
                                m.lines().next().unwrap_or("")
                            ))
                            .small()
                            .color(MUTED),
                        );
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut b.otp)
                                    .hint_text("123456")
                                    .desired_width(120.0),
                            );
                            if ui::primary_button(ui, "인증").clicked() && !b.otp.trim().is_empty()
                            {
                                otp = Some(b.otp.trim().to_string());
                            }
                        });
                    }
                    BrowseState::Ready(folders) => {
                        egui::ScrollArea::vertical()
                            .max_height(260.0)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                if folders.is_empty() {
                                    ui.label(RichText::new("하위 폴더가 없습니다").color(MUTED));
                                }
                                for f in folders {
                                    let r = ui.add(
                                        egui::Button::new(format!("{}   ›", f.name))
                                            .frame(false)
                                            .min_size(egui::vec2(ui.available_width(), 24.0)),
                                    );
                                    if r.clicked() {
                                        go = Some(f.path.clone());
                                    }
                                }
                            });
                    }
                }
                let ready = matches!(&*state, BrowseState::Ready(_));
                drop(state);
                ui.separator();
                if !at_root {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut b.new_name)
                                .hint_text("새 폴더 이름")
                                .desired_width(180.0),
                        );
                        if ui
                            .add_enabled(
                                ready && !b.new_name.trim().is_empty(),
                                egui::Button::new("＋ 새 폴더"),
                            )
                            .clicked()
                        {
                            create = Some(b.new_name.trim().to_string());
                        }
                    });
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !at_root && ready,
                            ui::primary_button_widget(&format!("이 폴더 사용: {path}")),
                        )
                        .clicked()
                    {
                        choose = true;
                    }
                    if ui.button("취소").clicked() {
                        cancel = true;
                    }
                });
                if at_root {
                    ui.label(
                        RichText::new("공유 폴더를 눌러 들어간 뒤 동기화할 폴더를 고르세요.")
                            .small()
                            .color(MUTED),
                    );
                }
            });
        if choose {
            if let BackendConfig::Synology { folder, .. } = &mut self.cfg.profiles[self.sel].backend
            {
                *folder = path.clone();
            }
            self.test = None;
            self.browser = None;
            return;
        }
        if cancel || !open {
            self.browser = None;
            return;
        }
        if let Some(code) = otp {
            self.browse(ctx, path, None, Some(code));
        } else if let Some(name) = create {
            self.browse(ctx, path, Some(name), None);
        } else if let Some(p) = go {
            self.browse(ctx, p, None, None);
        }
    }

    fn start_login(&mut self, ctx: &egui::Context) {
        let state = Arc::new(Mutex::new(LoginState::Requesting));
        let cancel = Arc::new(AtomicBool::new(false));
        let (st, cn, backend, dir, ctx) = (
            state.clone(),
            cancel.clone(),
            self.cur().backend.clone(),
            self.data_dir.clone(),
            ctx.clone(),
        );
        std::thread::spawn(move || {
            let st2 = st.clone();
            let ctx2 = ctx.clone();
            let res = auth::login_device(
                &dir,
                &backend,
                |dc| {
                    *st2.lock().unwrap() = LoginState::Code {
                        user_code: dc.user_code.clone(),
                        uri: dc.verification_uri.clone(),
                    };
                    let _ = open::that(&dc.verification_uri);
                    ctx2.request_repaint();
                },
                &cn,
            );
            *st.lock().unwrap() = match res {
                Ok(()) => LoginState::Done,
                Err(e) => LoginState::Failed(format!("{e:#}")),
            };
            ctx.request_repaint();
        });
        self.login = Some(Login { state, cancel });
    }

    // ───────────────────────────── 홈 ─────────────────────────────
    fn home_page(&mut self, ui: &mut egui::Ui, status: &Option<ServiceStatus>) {
        let (title, sub, color) = match status {
            Some(s) if s.syncing => (
                "동기화 중…",
                "변경된 파일을 주고받고 있습니다".to_string(),
                ACCENT,
            ),
            Some(s) if s.last_error.is_some() => (
                "확인이 필요합니다",
                friendly_error(s.last_error.as_deref().unwrap_or("")),
                DANGER,
            ),
            Some(s) => (
                "실시간 동기화 켜짐",
                match &s.last_sync {
                    Some(t) => format!("마지막 동기화 {t}"),
                    None => "첫 동기화를 준비하고 있습니다".into(),
                },
                OK,
            ),
            None if self.transitioning() => (
                "동기화 전환 중…",
                "진행 중인 작업을 마친 뒤 요청을 처리합니다".into(),
                WARN,
            ),
            None if self.cfg.validate().is_err() => (
                "설정이 필요합니다",
                "동기화할 폴더(프로필)를 먼저 추가해 주세요".into(),
                WARN,
            ),
            None => (
                "동기화 꺼짐",
                "시작 버튼을 누르면 실시간 동기화가 켜집니다".into(),
                MUTED,
            ),
        };

        ui::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui::status_dot(ui, color, 14.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).size(22.0).strong());
                    ui.label(RichText::new(sub).color(MUTED));
                });
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if self.service.is_none() {
                    if ui
                        .add_enabled(!self.transitioning(), ui::big_button("▶  동기화 시작"))
                        .clicked()
                        && self.save()
                    {
                        self.start_service();
                    }
                } else {
                    let syncing = status.as_ref().is_some_and(|s| s.syncing);
                    if ui
                        .add_enabled(!syncing, ui::big_button("⟳  지금 동기화"))
                        .clicked()
                    {
                        if let Some(s) = &self.service {
                            s.sync_now();
                        }
                    }
                    if ui.add(ui::big_button("⏸  일시 중지")).clicked() {
                        self.stop_service();
                    }
                }
            });
            let pending: usize = status
                .as_ref()
                .map(|s| s.profiles.iter().filter_map(|p| p.pending_deletes).sum())
                .unwrap_or(0);
            if pending > 0 {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(
                        "대량 삭제를 보류했습니다. 아래 개수는 모든 프로필의 보류된 삭제 합계입니다. 의도한 삭제인지 확인한 후 '삭제 진행'을 누르세요.",
                    )
                    .color(WARN),
                );
                if ui
                    .add(ui::big_button(&format!("삭제 진행 ({pending}개)")))
                    .clicked()
                {
                    if let Some(s) = &self.service {
                        s.allow_deletes("");
                    }
                }
            }
        });

        ui.add_space(8.0);
        ui::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("동기화 폴더").strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("＋ 추가").clicked() {
                        self.add_profile(ProfileKind::Folder);
                    }
                });
            });
            ui.add_space(4.0);
            if self.cfg.profiles.is_empty() {
                ui.label(
                    RichText::new("아직 프로필이 없습니다. 설정에서 추가해 주세요").color(MUTED),
                );
            }
            let mut goto = None;
            for (i, p) in self.cfg.profiles.iter().enumerate() {
                let st = status
                    .as_ref()
                    .and_then(|s| s.profiles.iter().find(|x| x.id == p.id));
                let (dot, line) = match st {
                    _ if !p.enabled => (MUTED, "꺼짐".to_string()),
                    Some(s) if s.syncing => (ACCENT, "동기화 중…".into()),
                    Some(s) if s.last_error.is_some() => (
                        DANGER,
                        friendly_error(s.last_error.as_deref().unwrap_or(""))
                            .lines()
                            .next()
                            .unwrap_or("")
                            .to_string(),
                    ),
                    Some(s) => (
                        OK,
                        match &s.last_sync {
                            Some(t) => format!("{} · {}", &t[5..], s.last_summary),
                            None => "준비 중".into(),
                        },
                    ),
                    None => (MUTED, "대기".into()),
                };
                ui::inset(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui::status_dot(ui, dot, 10.0);
                        ui.label(RichText::new(p.display_name()).strong());
                        ui.label(
                            RichText::new(format!(
                                "{} · {}",
                                match p.kind {
                                    ProfileKind::Obsidian => "Obsidian",
                                    ProfileKind::Folder => "폴더",
                                },
                                p.mode.display_name()
                            ))
                            .small()
                            .color(MUTED),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("설정").clicked() {
                                goto = Some(i);
                            }
                            if !p.local_path.is_empty() && ui.small_button("폴더 열기").clicked()
                            {
                                let _ = open::that(&p.local_path);
                            }
                        });
                    });
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!("{}  ⇄  {}", p.local_path, p.backend.summary()))
                                .small(),
                        )
                        .truncate(),
                    );
                    ui.label(RichText::new(line).small().color(if dot == DANGER {
                        DANGER
                    } else {
                        MUTED
                    }));
                });
                ui.add_space(4.0);
            }
            if let Some(i) = goto {
                self.select(i);
                self.page = Page::Settings;
            }
            ui.label(
                RichText::new(if self.cfg.watch_local {
                    if self.cfg.quick_check_secs > 0 {
                        format!(
                            "저장 즉시 업로드 · 다른 기기 변경 {}초마다 확인",
                            self.cfg.quick_check_secs.max(2)
                        )
                    } else {
                        format!(
                            "저장 즉시 업로드 · {}초마다 원격 확인",
                            self.cfg.poll_interval_secs
                        )
                    }
                } else {
                    format!("{}초마다 확인", self.cfg.poll_interval_secs)
                })
                .small()
                .color(MUTED),
            );
        });

        ui.add_space(8.0);
        ui::card(ui, |ui| {
            ui.label(RichText::new("최근 활동").strong());
            ui.add_space(4.0);
            let logs: Vec<_> = status
                .as_ref()
                .map(|s| s.logs.iter().rev().take(8).cloned().collect())
                .unwrap_or_default();
            if logs.is_empty() {
                ui.label(RichText::new("아직 기록이 없습니다").color(MUTED));
            }
            for l in logs {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&l.time).color(MUTED).monospace().size(12.0));
                    ui.label(RichText::new(&l.text).color(if l.error { DANGER } else { ui::TEXT }));
                });
            }
        });
        ui.add_space(6.0);
        ui.label(
            RichText::new("충돌이 나면 두 버전을 모두 남깁니다 (예: 노트 (충돌 PC 2025-01-01 120000).md). 다른 곳에서 지워진 파일은 폴더 안 .trash 로 옮겨집니다.")
                .small()
                .color(MUTED),
        );
    }

    // ───────────────────────────── 설정 ─────────────────────────────
    fn settings_page(&mut self, ui: &mut egui::Ui) {
        // 프로필 선택 줄
        ui::card(ui, |ui| {
            ui.label(RichText::new("동기화 프로필").strong());
            ui.label(
                RichText::new("프로필 하나가 '내 PC 폴더 ↔ 저장소 폴더' 1:1 연결입니다. Obsidian 볼트, 사진 폴더 등을 따로 만들 수 있습니다.")
                    .small()
                    .color(MUTED),
            );
            ui.add_space(6.0);
            let mut pick = None;
            ui.horizontal_wrapped(|ui| {
                for (i, p) in self.cfg.profiles.iter().enumerate() {
                    let mut t = RichText::new(p.display_name());
                    if !p.enabled {
                        t = t.color(MUTED).italics();
                    }
                    if ui.selectable_label(i == self.sel, t).clicked() {
                        pick = Some(i);
                    }
                }
                if ui.button("＋ Obsidian 볼트").clicked() {
                    pick = Some(usize::MAX);
                }
                if ui.button("＋ 일반 폴더 (사진 등)").clicked() {
                    pick = Some(usize::MAX - 1);
                }
            });
            match pick {
                Some(usize::MAX) => self.add_profile(ProfileKind::Obsidian),
                Some(x) if x == usize::MAX - 1 => self.add_profile(ProfileKind::Folder),
                Some(i) => self.select(i),
                None => {}
            }
        });
        ui.add_space(8.0);

        if self.cfg.profiles.is_empty() {
            ui::card(ui, |ui| {
                ui.label("위의 '＋' 버튼으로 동기화할 폴더를 추가하세요.");
            });
        } else {
            self.sel = self.sel.min(self.cfg.profiles.len() - 1);
            self.profile_editor(ui);
        }

        ui.add_space(8.0);
        ui::card(ui, |ui| {
            ui.horizontal(|ui| {
                let label = if self.service.is_some() {
                    "저장 후 다시 시작"
                } else {
                    "저장하고 동기화 시작"
                };
                if ui
                    .add_enabled(!self.transitioning(), ui::big_button(label))
                    .clicked()
                    && self.save()
                {
                    self.start_service();
                    if self.service.is_some() {
                        self.page = Page::Home;
                    }
                }
                if ui
                    .add_enabled(self.dirty(), egui::Button::new("저장만 하기"))
                    .clicked()
                    && self.save()
                {
                    self.set_msg("저장했습니다", false);
                }
                if self.dirty() {
                    ui.label(
                        RichText::new("저장하지 않은 변경 사항이 있습니다")
                            .small()
                            .color(WARN),
                    );
                }
            });
        });

        ui.add_space(8.0);
        ui::card(ui, |ui| {
            egui::CollapsingHeader::new(RichText::new("⚙  고급 설정 (모든 프로필 공통)").strong())
                .default_open(false)
                .show(ui, |ui| self.advanced(ui));
        });
    }

    fn profile_editor(&mut self, ui: &mut egui::Ui) {
        let data_dir = self.data_dir.clone();
        let p = self.cur();
        let kind = p.kind;
        let id = p.id.clone();

        // ① 이름과 종류
        ui::step_card(ui, 1, "이름과 종류", !p.name.trim().is_empty(), |ui| {
            egui::Grid::new("pname")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui::field(ui, "프로필 이름", &mut p.name, false, "예: Obsidian, 사진");
                    ui.label(RichText::new("종류").color(MUTED));
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut p.kind, ProfileKind::Obsidian, "Obsidian 볼트");
                        ui.radio_value(
                            &mut p.kind,
                            ProfileKind::Folder,
                            "일반 폴더 (사진·문서 등)",
                        );
                    });
                    ui.end_row();
                });
            ui.checkbox(&mut p.enabled, "이 프로필 동기화하기");
        });

        // ② 내 PC 폴더
        ui.add_space(8.0);
        let dir_ok =
            !p.local_path.trim().is_empty() && std::path::Path::new(p.local_path.trim()).is_dir();
        let mut pick_local = false;
        ui::step_card(ui, 2, "내 PC 폴더", dir_ok, |ui| {
            ui.label(
                RichText::new(match kind {
                    ProfileKind::Obsidian => {
                        "Obsidian 볼트(보관함) 폴더를 고르세요. .obsidian 폴더가 들어 있는 폴더입니다."
                    }
                    ProfileKind::Folder => "동기화할 폴더를 고르세요 (예: 사진, 문서 폴더).",
                })
                .color(MUTED),
            );
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut p.local_path)
                        .hint_text(match kind {
                            ProfileKind::Obsidian => r"C:\Users\나\Documents\MyVault",
                            ProfileKind::Folder => r"C:\Users\나\Pictures",
                        })
                        .desired_width(ui.available_width() - 110.0),
                );
                if ui.button("폴더 선택…").clicked() {
                    pick_local = true;
                }
            });
            if kind == ProfileKind::Obsidian
                && dir_ok
                && !std::path::Path::new(p.local_path.trim())
                    .join(".obsidian")
                    .is_dir()
            {
                ui.label(RichText::new("⚠ 이 폴더에 .obsidian 폴더가 없습니다. 새 기기라면 빈 폴더여도 괜찮습니다.").color(WARN).small());
            }
        });
        if pick_local {
            if let Some(path) = platform::pick_folder() {
                let p = self.cur();
                if p.name.trim().is_empty() || p.name == "새 폴더" {
                    p.name = std::path::Path::new(&path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                }
                p.local_path = path;
            }
        }

        // ③ 저장소
        ui.add_space(8.0);
        let backend_ok = self.cur().backend.validate().is_ok();
        ui::step_card(ui, 3, "저장소", backend_ok, |ui| {
            self.backend_picker(ui);
            ui.add_space(6.0);
            self.backend_fields(ui);
        });

        // ④ 연결 확인
        ui.add_space(8.0);
        let backend = self.cur().backend.clone();
        let logged_in = !backend.needs_oauth() || auth::is_logged_in(&data_dir, &backend);
        let tested = matches!(
            self.test
                .as_ref()
                .map(|j| matches!(&*j.lock().unwrap(), Job::Ok(_))),
            Some(true)
        );
        ui::step_card(ui, 4, "연결 확인", logged_in && tested, |ui| {
            if backend.needs_oauth() {
                self.login_ui(ui);
                ui.add_space(6.0);
            }
            if matches!(backend, BackendConfig::Synology { .. })
                && synology::has_device_token(&data_dir, &backend)
            {
                ui.horizontal(|ui| {
                    ui::status_dot(ui, OK, 8.0);
                    ui.label(RichText::new("2단계 인증 완료 (이 PC 기억됨)").small());
                    if ui.small_button("기억 해제").clicked() {
                        synology::forget_device(&data_dir, &backend);
                    }
                });
            }
            ui.horizontal(|ui| {
                let running = matches!(
                    self.test
                        .as_ref()
                        .map(|j| matches!(&*j.lock().unwrap(), Job::Running)),
                    Some(true)
                );
                if ui
                    .add_enabled(
                        backend_ok && logged_in && !running,
                        egui::Button::new("🔌  연결 테스트"),
                    )
                    .clicked()
                {
                    self.start_test(ui.ctx());
                }
                if running {
                    ui.spinner();
                    ui.label("확인하는 중…");
                }
            });
            let mut send_otp = false;
            if let Some(job) = &self.test {
                match &*job.lock().unwrap() {
                    Job::Ok(m) => {
                        ui.label(RichText::new(format!("✔ {m}")).color(OK));
                    }
                    Job::Failed(m) => {
                        ui.label(RichText::new(format!("✖ {m}")).color(DANGER));
                    }
                    Job::NeedOtp(m) => {
                        ui::inset(ui, |ui| {
                            ui.label(RichText::new("🔐 2단계 인증").strong());
                            ui.label(
                                RichText::new(format!(
                                    "{}\nOTP 앱(Synology Secure SignIn, Google Authenticator 등)의 6자리 코드를 입력하세요. 한 번 인증하면 이 PC를 기억합니다.",
                                    m.lines().next().unwrap_or("")
                                ))
                                .small()
                                .color(MUTED),
                            );
                            ui.horizontal(|ui| {
                                let r = ui.add(
                                    egui::TextEdit::singleline(&mut self.otp)
                                        .hint_text("123456")
                                        .desired_width(120.0),
                                );
                                let enter =
                                    r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                if (ui::primary_button(ui, "인증").clicked() || enter)
                                    && !self.otp.trim().is_empty()
                                {
                                    send_otp = true;
                                }
                            });
                        });
                    }
                    Job::Running => {}
                }
            }
            if send_otp {
                self.start_otp(ui.ctx());
            }
        });

        // ⑤ 동기화 방식
        ui.add_space(8.0);
        let transitioning = self.transitioning();
        let ignore = self.ignore_texts.entry(id.clone()).or_default();
        let p = &mut self.cfg.profiles[self.sel];
        let mut reset = false;
        let mut delete = false;
        ui::step_card(ui, 5, "동기화 방식", true, |ui| {
            ui.label(RichText::new("동기화 방향").strong());
            ui.radio_value(
                &mut p.mode,
                SyncMode::TwoWay,
                "양방향 — 어느 쪽에서 바꿔도 서로 맞춥니다",
            );
            ui.radio_value(
                &mut p.mode,
                SyncMode::UploadOnly,
                "올리기만 (백업) — 이 PC의 변경만 저장소로 올립니다",
            );
            ui.radio_value(
                &mut p.mode,
                SyncMode::DownloadOnly,
                "내려받기만 — 저장소의 변경만 이 PC로 받습니다. 이 PC에서 고친 내용은 올리지 않습니다",
            );
            ui.add_space(4.0);
            match p.mode {
                SyncMode::TwoWay => {
                    ui.checkbox(
                        &mut p.propagate_deletes,
                        "삭제도 동기화 (지워진 파일은 폴더 안 .trash 로 이동)",
                    );
                    ui.add_space(4.0);
                    ui.label(RichText::new("데이터 기준 (양쪽이 모두 바뀌었을 때)").strong());
                    ui.radio_value(
                        &mut p.conflict_policy,
                        ConflictPolicy::KeepBoth,
                        "둘 다 보존 — 이 PC 쪽은 '(충돌 …)' 사본으로 남깁니다 (Synology Drive와 같은 방식)",
                    );
                    ui.radio_value(
                        &mut p.conflict_policy,
                        ConflictPolicy::PreferLocal,
                        "이 PC 기준 — 이 PC 내용으로 저장소를 덮어씁니다",
                    );
                    ui.radio_value(
                        &mut p.conflict_policy,
                        ConflictPolicy::PreferRemote,
                        "저장소 기준 — 저장소 내용으로 덮어씁니다 (이전 내용은 .trash 에 보관)",
                    );
                }
                SyncMode::UploadOnly => {
                    ui.checkbox(
                        &mut p.delete_in_target,
                        "이 PC에서 지운 파일은 저장소에서도 지우기 (끄면 저장소에 남음)",
                    );
                }
                SyncMode::DownloadOnly => {
                    ui.checkbox(
                        &mut p.delete_in_target,
                        "저장소에서 지운 파일은 이 PC에서도 지우기 (.trash 로 이동, 끄면 이 PC에 남음)",
                    );
                    ui.label(
                        RichText::new("처음 연결할 때 내용이 다른 파일은 저장소 기준으로 맞추고, 이 PC 파일은 .trash 에 보관합니다.")
                            .small()
                            .color(MUTED),
                    );
                }
            }
            ui.add_space(4.0);
            if p.kind == ProfileKind::Obsidian {
                ui.checkbox(
                    &mut p.sync_obsidian_folder,
                    ".obsidian 설정 폴더도 동기화 (플러그인, 테마, 단축키)",
                );
            }
            ui.add_space(4.0);
            ui.label("동기화하지 않을 파일/폴더 (한 줄에 하나, 예: Private, *.mp4)");
            if ui
                .add(
                    egui::TextEdit::multiline(ignore)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                )
                .changed()
            {
                p.ignore_patterns = parse_lines(ignore);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!transitioning, egui::Button::new("이 프로필 동기화 기록 초기화"))
                    .on_hover_text("다음 동기화 때 양쪽 파일을 처음부터 다시 비교합니다. 파일은 지워지지 않습니다.")
                    .clicked()
                {
                    reset = true;
                }
                if !self.confirm_delete {
                    if ui
                        .button(RichText::new("프로필 삭제").color(DANGER))
                        .clicked()
                    {
                        self.confirm_delete = true;
                    }
                } else {
                    ui.label(RichText::new("프로필을 지울까요? (파일은 지워지지 않습니다)").color(DANGER));
                    if ui.button(RichText::new("삭제").color(DANGER)).clicked() {
                        delete = true;
                    }
                    if ui.button("취소").clicked() {
                        self.confirm_delete = false;
                    }
                }
            });
        });

        if reset {
            self.reset_profile(id.clone());
        }
        if delete {
            self.cfg.profiles.remove(self.sel);
            self.ignore_texts.remove(&id);
            self.confirm_delete = false;
            self.test = None;
            self.sel = self.sel.saturating_sub(1);
            self.set_msg("프로필을 지웠습니다. '저장'을 눌러야 반영됩니다", false);
        }
    }

    fn backend_picker(&mut self, ui: &mut egui::Ui) {
        let current = self.cur().backend.kind();
        let id = self.cur().id.clone();
        let mut selected = current;
        let w = ((ui.available_width() - 8.0) / 2.0).max(200.0);
        egui::Grid::new("backend_cards")
            .num_columns(2)
            .spacing([8.0, 8.0])
            .show(ui, |ui| {
                for (i, (kind, badge, color, name, desc)) in BACKENDS.iter().enumerate() {
                    if ui::choice_card(ui, w, badge, *color, name, desc, selected == *kind)
                        .clicked()
                    {
                        selected = kind;
                    }
                    if i % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
        if selected != current {
            let new = self
                .backend_cache
                .remove(&(id.clone(), selected))
                .unwrap_or_else(|| default_backend(selected));
            let old = std::mem::replace(&mut self.cur().backend, new);
            self.backend_cache.insert((id, current), old);
            self.test = None;
        }
    }

    fn backend_fields(&mut self, ui: &mut egui::Ui) {
        let secret = !self.show_secret;
        let mut pick_path = false;
        let mut pick_nas = false;
        let kind = self.cur().kind;
        egui::Grid::new("backend_fields")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| match &mut self.cfg.profiles[self.sel].backend {
                BackendConfig::Synology {
                    url,
                    username,
                    password,
                    folder,
                } => {
                    ui::field(ui, "NAS 주소", url, false, "https://내NAS주소:5001");
                    ui::field(ui, "아이디", username, false, "DSM 로그인 아이디");
                    ui::field(ui, "비밀번호", password, secret, "");
                    ui.label(RichText::new("NAS 폴더").color(MUTED));
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(folder)
                                .hint_text(match kind {
                                    ProfileKind::Obsidian => "/home/Obsidian",
                                    ProfileKind::Folder => "/photo/PC 사진",
                                })
                                .desired_width(ui.available_width().min(420.0) - 130.0),
                        );
                        if ui
                            .add_enabled(
                                !url.trim().is_empty() && !username.trim().is_empty(),
                                egui::Button::new("NAS에서 고르기…"),
                            )
                            .on_disabled_hover_text("NAS 주소와 아이디를 먼저 입력하세요")
                            .clicked()
                        {
                            pick_nas = true;
                        }
                    });
                    ui.end_row();
                    ui.label("");
                    ui.label(
                        RichText::new(
                            "[NAS에서 고르기]로 목록에서 고르면 오타가 없습니다. 직접 적을 때는 공유 폴더 이름부터 적습니다.",
                        )
                        .small()
                        .color(MUTED),
                    );
                    ui.end_row();
                }
                BackendConfig::LocalFolder { path } => {
                    ui::field(
                        ui,
                        "저장 위치",
                        path,
                        false,
                        r"\\NAS\home\Obsidian 또는 Z:\Obsidian",
                    );
                    ui.label("");
                    if ui.button("폴더 선택…").clicked() {
                        pick_path = true;
                    }
                    ui.end_row();
                }
                BackendConfig::S3 {
                    endpoint,
                    region,
                    bucket,
                    prefix,
                    access_key,
                    secret_key,
                } => {
                    ui.label("");
                    ui.horizontal(|ui| {
                        if ui.small_button("네이버 클라우드").clicked() {
                            *endpoint = "https://kr.object.ncloudstorage.com".into();
                            *region = "kr-standard".into();
                        }
                        if ui.small_button("Google Cloud Storage").clicked() {
                            *endpoint = "https://storage.googleapis.com".into();
                            *region = "auto".into();
                        }
                    });
                    ui.end_row();
                    ui::field(
                        ui,
                        "엔드포인트",
                        endpoint,
                        false,
                        "https://kr.object.ncloudstorage.com",
                    );
                    ui::field(ui, "리전", region, false, "kr-standard");
                    ui::field(ui, "버킷 이름", bucket, false, "my-obsidian");
                    ui::field(ui, "저장 폴더", prefix, false, "obsidian");
                    ui::field(ui, "Access Key", access_key, false, "");
                    ui::field(ui, "Secret Key", secret_key, secret, "");
                }
                BackendConfig::OneDrive {
                    client_id, folder, ..
                } => {
                    ui::field(
                        ui,
                        "앱 Client ID",
                        client_id,
                        false,
                        "Azure 앱 등록에서 받은 애플리케이션 ID",
                    );
                    ui::field(ui, "OneDrive 폴더", folder, false, "ObsidianSync");
                }
                BackendConfig::GoogleDrive {
                    client_id,
                    client_secret,
                    folder,
                } => {
                    ui::field(
                        ui,
                        "Client ID",
                        client_id,
                        false,
                        "OAuth 클라이언트 ID (TV 및 입력 제한 기기)",
                    );
                    ui::field(ui, "Client Secret", client_secret, secret, "");
                    ui::field(ui, "Drive 폴더", folder, false, "ObsidianSync");
                }
            });
        if pick_nas {
            self.open_browser(ui.ctx());
        }
        if pick_path {
            if let (Some(p), BackendConfig::LocalFolder { path }) =
                (platform::pick_folder(), &mut self.cur().backend)
            {
                *path = p;
            }
        }
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.show_secret, "비밀번호 표시");
            ui.hyperlink_to(
                RichText::new("❓ 이 저장소 설정 방법").small(),
                backend_help(&self.cfg.profiles[self.sel].backend),
            );
        });
    }

    fn login_ui(&mut self, ui: &mut egui::Ui) {
        let backend = self.cur().backend.clone();
        let logged_in = auth::is_logged_in(&self.data_dir, &backend);
        let mut finished = false;
        if let Some(login) = &self.login {
            match &*login.state.lock().unwrap() {
                LoginState::Requesting => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("로그인 준비 중…");
                    });
                }
                LoginState::Code { user_code, uri } => {
                    ui::inset(ui, |ui| {
                        ui.label("브라우저가 열리면 아래 코드를 입력하고 계정을 승인해 주세요.");
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(user_code)
                                    .size(28.0)
                                    .strong()
                                    .monospace()
                                    .color(ACCENT),
                            );
                            if ui.button("📋 복사").clicked() {
                                ui.output_mut(|o| o.copied_text = user_code.clone());
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("브라우저가 안 열리면:").color(MUTED));
                            ui.hyperlink(uri);
                        });
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("승인을 기다리는 중…");
                            if ui.small_button("취소").clicked() {
                                login.cancel.store(true, Ordering::Relaxed);
                            }
                        });
                    });
                }
                LoginState::Done | LoginState::Failed(_) => finished = true,
            }
        }
        if finished {
            let login = self.login.take().unwrap();
            let st = login.state.lock().unwrap();
            self.message = Some(match &*st {
                LoginState::Failed(e) => (friendly_error(e), true),
                _ => (
                    "로그인했습니다. '연결 테스트'로 확인해 보세요.".into(),
                    false,
                ),
            });
        }
        if self.login.is_none() {
            ui.horizontal(|ui| {
                if logged_in {
                    ui::status_dot(ui, OK, 8.0);
                    ui.label("계정 연결됨");
                    if ui.small_button("다시 로그인").clicked() {
                        self.start_login(ui.ctx());
                    }
                    if ui.small_button("로그아웃").clicked() {
                        auth::logout(&self.data_dir, &backend);
                    }
                } else {
                    let ok = backend.validate().is_ok();
                    if ui
                        .add_enabled(ok, egui::Button::new("🔑  로그인"))
                        .clicked()
                    {
                        self.start_login(ui.ctx());
                    }
                    ui.label(
                        RichText::new(if ok {
                            "브라우저에서 계정을 승인합니다"
                        } else {
                            "위 항목을 먼저 입력하세요"
                        })
                        .color(MUTED),
                    );
                }
            });
        }
    }

    fn advanced(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(
            &mut self.cfg.watch_local,
            "저장하자마자 바로 업로드 (실시간 감시)",
        );
        ui.checkbox(
            &mut self.cfg.start_on_launch,
            "앱을 켜면 자동으로 동기화 시작",
        );
        if cfg!(windows) {
            let mut a = self.autostart;
            if ui
                .checkbox(&mut a, "Windows 시작 시 자동 실행 (트레이에서 조용히)")
                .changed()
            {
                match platform::set_autostart(a) {
                    Ok(()) => self.autostart = a,
                    Err(e) => self.set_msg(format!("{e:#}"), true),
                }
            }
        }
        ui.add_space(4.0);
        egui::Grid::new("adv")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("다른 기기 변경 확인");
                ui.add(
                    egui::DragValue::new(&mut self.cfg.quick_check_secs)
                        .range(0..=600)
                        .suffix(" 초"),
                )
                .on_hover_text("작은 신호 파일 하나만 확인합니다. 0이면 끔");
                ui.end_row();
                ui.label("전체 목록 확인 주기");
                ui.add(
                    egui::DragValue::new(&mut self.cfg.poll_interval_secs)
                        .range(10..=3600)
                        .suffix(" 초"),
                )
                .on_hover_text(
                    "다른 프로그램(파일 탐색기, Synology Drive 등)이 바꾼 파일을 찾습니다",
                );
                ui.end_row();
                ui.label("저장 후 대기");
                ui.add(
                    egui::DragValue::new(&mut self.cfg.debounce_ms)
                        .range(200..=60000)
                        .suffix(" ms"),
                );
                ui.end_row();
                ui.label("기기 이름");
                ui.add(
                    egui::TextEdit::singleline(&mut self.cfg.device_name)
                        .hint_text(self.saved.effective_device_name())
                        .desired_width(200.0),
                );
                ui.end_row();
            });
        ui.add_space(6.0);
        if ui.button("설정 폴더 열기").clicked() {
            let _ = open::that(&self.data_dir);
        }
    }

    // ───────────────────────────── 기록 ─────────────────────────────
    fn log_page(&mut self, ui: &mut egui::Ui, status: &Option<ServiceStatus>) {
        let logs = status.as_ref().map(|s| s.logs.clone()).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("최근 기록 {}건", logs.len())).color(MUTED));
            if ui.small_button("📋 전체 복사").clicked() {
                let text: String = logs
                    .iter()
                    .map(|l| format!("{} {}\n", l.time, l.text))
                    .collect();
                ui.output_mut(|o| o.copied_text = text);
            }
        });
        ui::card(ui, |ui| {
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .auto_shrink(false)
                .show(ui, |ui| {
                    if logs.is_empty() {
                        ui.label(
                            RichText::new("동기화를 시작하면 여기에 기록이 표시됩니다")
                                .color(MUTED),
                        );
                    }
                    for l in &logs {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(&l.time).color(MUTED).monospace().size(12.0));
                            ui.label(RichText::new(&l.text).size(13.0).color(if l.error {
                                DANGER
                            } else {
                                ui::TEXT
                            }));
                        });
                    }
                });
        });
    }
}

fn backend_help(b: &BackendConfig) -> String {
    let anchor = match b {
        BackendConfig::Synology { .. } => "synology-nas",
        BackendConfig::LocalFolder { .. } => "폴더--공유-드라이브",
        BackendConfig::S3 { .. } => "네이버-클라우드--google-cloud-storage",
        BackendConfig::OneDrive { .. } => "onedrive",
        BackendConfig::GoogleDrive { .. } => "google-drive",
    };
    format!("{SETUP_DOC}#{anchor}")
}

/// 기술적인 오류 메시지를 이해하기 쉬운 안내로 변환
fn friendly_error(raw: &str) -> String {
    let hint = if raw.contains("Synology:") {
        ""
    } else if raw.contains("HTTP 401")
        || raw.contains("HTTP 403")
        || raw.contains("SignatureDoesNotMatch")
        || raw.contains("InvalidAccessKeyId")
    {
        "아이디/비밀번호 또는 키가 맞지 않거나 권한이 없습니다."
    } else if raw.contains("HTTP 404") || raw.contains("NoSuchBucket") {
        "주소나 버킷/폴더 이름을 찾을 수 없습니다."
    } else if raw.contains("네트워크 오류") || raw.contains("dns") || raw.contains("Connection")
    {
        "서버에 연결할 수 없습니다. 주소, 포트, 인터넷 연결을 확인하세요."
    } else if raw.contains("invalid_grant") || raw.contains("다시 로그인") {
        "로그인이 만료되었습니다. 설정에서 다시 로그인해 주세요."
    } else if raw.contains("certificate") || raw.contains("Certificate") {
        "서버 인증서를 신뢰할 수 없습니다. NAS에 정식 인증서(예: Let's Encrypt)를 설정하거나, 집 안에서는 http://NAS주소:5000 을 사용하세요."
    } else {
        ""
    };
    let first: String = raw.lines().next().unwrap_or("").chars().take(220).collect();
    if hint.is_empty() {
        first
    } else {
        format!("{hint}\n({first})")
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_lifecycle();
        if self.hide_on_start {
            self.hide_on_start = false;
            platform::hide_window(self.hwnd);
        }
        // 창 닫기 → 트레이로 숨김 (동기화는 백그라운드에서 계속)
        if ctx.input(|i| i.viewport().close_requested()) && self.has_tray {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            platform::hide_window(self.hwnd);
        }
        ctx.request_repaint_after(Duration::from_millis(700));
        self.browser_ui(ctx);
        let status = self.service.as_ref().map(|s| s.status());

        egui::SidePanel::left("nav")
            .resizable(false)
            .exact_width(170.0)
            .frame(
                egui::Frame::none()
                    .fill(ui::SIDEBAR)
                    .inner_margin(egui::Margin::symmetric(12.0, 16.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui::status_dot(ui, ACCENT, 10.0);
                    ui.label(RichText::new("Obsidian Sync").size(17.0).strong());
                });
                ui.label(
                    RichText::new(format!("v{}", obsidian_sync_core::VERSION))
                        .small()
                        .color(MUTED),
                );
                ui.add_space(18.0);
                for (page, label) in [
                    (Page::Home, "🏠  홈"),
                    (Page::Settings, "⚙  설정"),
                    (Page::Log, "📜  기록"),
                ] {
                    if ui::nav_button(ui, label, self.page == page).clicked() {
                        self.page = page;
                    }
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    if self.has_tray {
                        ui.label(
                            RichText::new("창을 닫아도 트레이에서 계속 동기화합니다")
                                .small()
                                .color(MUTED),
                        );
                    }
                    ui.hyperlink_to(RichText::new("도움말").small(), SETUP_DOC);
                    let (txt, color) = match &status {
                        Some(s) if s.syncing => ("동기화 중", ACCENT),
                        Some(s) if s.last_error.is_some() => ("확인 필요", DANGER),
                        Some(_) => ("켜짐", OK),
                        None => ("꺼짐", MUTED),
                    };
                    ui.horizontal(|ui| {
                        ui::status_dot(ui, color, 8.0);
                        ui.label(RichText::new(txt).color(color));
                    });
                });
            });

        if let Some((m, err)) = self.message.clone() {
            egui::TopBottomPanel::bottom("msg")
                .frame(
                    egui::Frame::none()
                        .fill(if err { ui::DANGER_BG } else { ui::OK_BG })
                        .inner_margin(egui::Margin::symmetric(16.0, 10.0)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&m).color(if err { DANGER } else { OK }));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✕").clicked() {
                                self.message = None;
                            }
                        });
                    });
                });
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(ui::BG)
                    .inner_margin(egui::Margin::symmetric(20.0, 16.0)),
            )
            .show(ctx, |ui| {
                let title = match self.page {
                    Page::Home => "홈",
                    Page::Settings => "설정",
                    Page::Log => "기록",
                };
                ui.label(RichText::new(title).size(24.0).strong());
                ui.add_space(8.0);
                if self.page == Page::Log {
                    self.log_page(ui, &status);
                    return;
                }
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        ui.set_max_width((ui.available_width() - 16.0).min(760.0));
                        match self.page {
                            Page::Home => self.home_page(ui, &status),
                            _ => self.settings_page(ui),
                        }
                        ui.add_space(12.0);
                    });
            });
    }
}

fn main() -> eframe::Result<()> {
    if !platform::single_instance() {
        // 이미 실행 중이면 새로 띄우지 않음 (트레이 아이콘을 눌러 여세요)
        return Ok(());
    }
    let hide = std::env::args().any(|a| a == "--minimized");
    let icon = egui::IconData {
        rgba: platform::icon_rgba(64),
        width: 64,
        height: 64,
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Obsidian Sync")
            .with_inner_size([920.0, 760.0])
            .with_min_inner_size([680.0, 520.0])
            .with_icon(Arc::new(icon)),
        ..Default::default()
    };
    eframe::run_native(
        "Obsidian Sync",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, hide)))),
    )
}

#[cfg(test)]
mod lifecycle_tests;
