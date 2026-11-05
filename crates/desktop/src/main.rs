// 릴리즈 빌드에서는 콘솔 창을 띄우지 않음
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod platform;
mod ui;

use eframe::egui::{self, RichText};
use obsidian_sync_core::{auth, service, AppConfig, BackendConfig, ServiceStatus, SyncService};
use platform::SharedTrigger;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use ui::{ACCENT, DANGER, MUTED, OK, WARN};

/// 저장소 종류: (kind, 배지, 배지 색, 이름, 설명)
const BACKENDS: &[(&str, &str, egui::Color32, &str, &str)] = &[
    ("web_dav", "NAS", egui::Color32::from_rgb(40, 90, 170), "Synology NAS (WebDAV)", "집/회사 NAS에 저장. DSM의 WebDAV Server 사용"),
    ("local_folder", "폴더", egui::Color32::from_rgb(110, 114, 128), "폴더 / 공유 드라이브", "NAS 공유폴더(SMB), 외장 디스크 등"),
    ("one_drive", "OD", egui::Color32::from_rgb(0, 120, 212), "OneDrive", "Microsoft 계정으로 로그인"),
    ("google_drive", "GD", egui::Color32::from_rgb(30, 142, 62), "Google Drive", "Google 계정으로 로그인"),
    ("s3", "S3", egui::Color32::from_rgb(3, 199, 90), "네이버 클라우드 / GCS", "S3 호환 스토리지 (액세스 키 필요)"),
];

const SETUP_DOC: &str = "https://github.com/bluewind201/obsidian-sync/blob/main/docs/SETUP.md";

fn default_backend(kind: &str) -> BackendConfig {
    match kind {
        "web_dav" => BackendConfig::WebDav {
            url: String::new(),
            username: String::new(),
            password: String::new(),
        },
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
        _ => BackendConfig::LocalFolder {
            path: String::new(),
        },
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
    backend_cache: HashMap<&'static str, BackendConfig>,
    ignore_text: String,
    service: Option<SyncService>,
    trigger: SharedTrigger,
    login: Option<Login>,
    test: Option<Arc<Mutex<Job>>>,
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
            ignore_text: cfg.ignore_patterns.join("\n"),
            saved: cfg.clone(),
            backend_cache: HashMap::new(),
            cfg,
            service: None,
            trigger,
            login: None,
            test: None,
            message: None,
            autostart: platform::autostart_enabled(),
            show_secret: false,
            page: if configured { Page::Home } else { Page::Settings },
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
        self.cfg != self.saved || self.ignore_text != self.saved.ignore_patterns.join("\n")
    }

    fn save(&mut self) -> bool {
        self.cfg.ignore_patterns = self
            .ignore_text
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
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
        self.stop_service();
        if let Err(e) = self.cfg.validate() {
            self.set_msg(format!("{e:#}"), true);
            self.page = Page::Settings;
            return;
        }
        if self.cfg.backend.needs_oauth() && !auth::is_logged_in(&self.data_dir, &self.cfg.backend) {
            self.set_msg("먼저 '로그인' 버튼으로 계정을 연결해 주세요", true);
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

    fn stop_service(&mut self) {
        *self.trigger.lock().unwrap() = None;
        if let Some(s) = self.service.take() {
            s.stop();
        }
    }

    fn start_test(&mut self, ctx: &egui::Context) {
        let job = Arc::new(Mutex::new(Job::Running));
        let (j, backend, dir, ctx) = (job.clone(), self.cfg.backend.clone(), self.data_dir.clone(), ctx.clone());
        std::thread::spawn(move || {
            let r = service::test_connection(&backend, &dir);
            *j.lock().unwrap() = match r {
                Ok(0) => Job::Ok("연결 성공! 원격 폴더가 비어 있어 첫 동기화 때 노트를 올립니다.".into()),
                Ok(n) => Job::Ok(format!("연결 성공! 원격에 파일 {n}개가 있습니다.")),
                Err(e) => Job::Failed(friendly_error(&format!("{e:#}"))),
            };
            ctx.request_repaint();
        });
        self.test = Some(job);
    }

    fn start_login(&mut self, ctx: &egui::Context) {
        let state = Arc::new(Mutex::new(LoginState::Requesting));
        let cancel = Arc::new(AtomicBool::new(false));
        let (st, cn, backend, dir, ctx) = (
            state.clone(),
            cancel.clone(),
            self.cfg.backend.clone(),
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
            Some(s) if s.syncing => ("동기화 중…", "변경된 파일을 주고받고 있습니다".to_string(), ACCENT),
            Some(s) if s.last_error.is_some() => (
                "확인이 필요합니다",
                friendly_error(s.last_error.as_deref().unwrap_or("")),
                DANGER,
            ),
            Some(s) => (
                "실시간 동기화 켜짐",
                match &s.last_sync {
                    Some(t) => format!("마지막 동기화 {t} · {}", s.last_summary),
                    None => "첫 동기화를 준비하고 있습니다".into(),
                },
                OK,
            ),
            None if self.cfg.validate().is_err() => (
                "설정이 필요합니다",
                "볼트 폴더와 저장소를 먼저 연결해 주세요".into(),
                WARN,
            ),
            None => ("동기화 꺼짐", "시작 버튼을 누르면 실시간 동기화가 켜집니다".into(), MUTED),
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
                    if ui::primary_button(ui, "▶  동기화 시작").clicked() && self.save() {
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
                if !self.cfg.vault_path.is_empty() && ui.add(ui::big_button("📂  볼트 열기")).clicked() {
                    let _ = open::that(&self.cfg.vault_path);
                }
            });
        });

        ui.add_space(8.0);
        ui::card(ui, |ui| {
            egui::Grid::new("summary").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                ui.label(RichText::new("볼트").color(MUTED));
                ui.add(egui::Label::new(if self.cfg.vault_path.is_empty() { "-" } else { &self.cfg.vault_path }).truncate());
                ui.end_row();
                ui.label(RichText::new("저장소").color(MUTED));
                ui.add(egui::Label::new(backend_summary(&self.cfg.backend)).truncate());
                ui.end_row();
                ui.label(RichText::new("방식").color(MUTED));
                ui.label(if self.cfg.watch_local {
                    format!("저장 즉시 업로드 · {}초마다 원격 확인", self.cfg.poll_interval_secs)
                } else {
                    format!("{}초마다 확인", self.cfg.poll_interval_secs)
                });
                ui.end_row();
            });
            ui.add_space(4.0);
            if ui.link("설정 변경 →").clicked() {
                self.page = Page::Settings;
            }
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
            RichText::new("충돌이 나면 두 버전을 모두 남깁니다 (예: 노트 (충돌 PC 2025-01-01 120000).md). 다른 곳에서 지워진 파일은 볼트의 .trash 폴더로 옮겨집니다.")
                .small()
                .color(MUTED),
        );
    }

    // ───────────────────────────── 설정 ─────────────────────────────
    fn settings_page(&mut self, ui: &mut egui::Ui) {
        let vault_ok = !self.cfg.vault_path.trim().is_empty() && std::path::Path::new(self.cfg.vault_path.trim()).is_dir();
        ui::step_card(ui, 1, "Obsidian 볼트 선택", vault_ok, |ui| {
            ui.label(RichText::new("Obsidian에서 사용하는 볼트(보관함) 폴더를 고르세요. .obsidian 폴더가 들어 있는 폴더입니다.").color(MUTED));
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.cfg.vault_path)
                        .hint_text(r"C:\Users\나\Documents\MyVault")
                        .desired_width(ui.available_width() - 110.0),
                );
                if ui.button("폴더 선택…").clicked() {
                    if let Some(p) = platform::pick_folder() {
                        self.cfg.vault_path = p;
                    }
                }
            });
            if vault_ok && !std::path::Path::new(self.cfg.vault_path.trim()).join(".obsidian").is_dir() {
                ui.label(RichText::new("⚠ 이 폴더에 .obsidian 폴더가 없습니다. 새 기기라면 빈 폴더여도 괜찮습니다.").color(WARN).small());
            }
        });

        ui.add_space(8.0);
        let backend_ok = self.cfg.backend.validate().is_ok();
        ui::step_card(ui, 2, "저장소 선택", backend_ok, |ui| {
            self.backend_picker(ui);
            ui.add_space(6.0);
            self.backend_fields(ui);
        });

        ui.add_space(8.0);
        let logged_in = !self.cfg.backend.needs_oauth() || auth::is_logged_in(&self.data_dir, &self.cfg.backend);
        let tested = matches!(self.test.as_ref().map(|j| matches!(&*j.lock().unwrap(), Job::Ok(_))), Some(true));
        ui::step_card(ui, 3, "연결 확인", logged_in && tested, |ui| {
            if self.cfg.backend.needs_oauth() {
                self.login_ui(ui);
                ui.add_space(6.0);
            }
            ui.horizontal(|ui| {
                let running = matches!(self.test.as_ref().map(|j| matches!(&*j.lock().unwrap(), Job::Running)), Some(true));
                if ui
                    .add_enabled(backend_ok && logged_in && !running, egui::Button::new("🔌  연결 테스트"))
                    .clicked()
                {
                    self.start_test(ui.ctx());
                }
                if running {
                    ui.spinner();
                    ui.label("확인하는 중…");
                }
            });
            if let Some(job) = &self.test {
                match &*job.lock().unwrap() {
                    Job::Ok(m) => {
                        ui.label(RichText::new(format!("✔ {m}")).color(OK));
                    }
                    Job::Failed(m) => {
                        ui.label(RichText::new(format!("✖ {m}")).color(DANGER));
                    }
                    Job::Running => {}
                }
            }
        });

        ui.add_space(8.0);
        ui::step_card(ui, 4, "저장하고 시작", self.service.is_some() && !self.dirty(), |ui| {
            ui.horizontal(|ui| {
                let label = if self.service.is_some() { "저장 후 다시 시작" } else { "저장하고 동기화 시작" };
                if ui::primary_button(ui, label).clicked() && self.save() {
                    self.start_service();
                    if self.service.is_some() {
                        self.page = Page::Home;
                    }
                }
                if ui.add_enabled(self.dirty(), egui::Button::new("저장만 하기")).clicked() && self.save() {
                    self.set_msg("저장했습니다", false);
                }
            });
        });

        ui.add_space(8.0);
        ui::card(ui, |ui| {
            egui::CollapsingHeader::new(RichText::new("⚙  고급 설정").strong())
                .default_open(false)
                .show(ui, |ui| self.advanced(ui));
        });
    }

    fn backend_picker(&mut self, ui: &mut egui::Ui) {
        let current = self.cfg.backend.kind();
        let mut selected = current;
        let w = ((ui.available_width() - 8.0) / 2.0).max(200.0);
        egui::Grid::new("backend_cards").num_columns(2).spacing([8.0, 8.0]).show(ui, |ui| {
            for (i, (kind, badge, color, name, desc)) in BACKENDS.iter().enumerate() {
                if ui::choice_card(ui, w, badge, *color, name, desc, selected == *kind).clicked() {
                    selected = kind;
                }
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
        if selected != current {
            let old = std::mem::replace(
                &mut self.cfg.backend,
                self.backend_cache.remove(selected).unwrap_or_else(|| default_backend(selected)),
            );
            self.backend_cache.insert(current, old);
            self.test = None;
        }
    }

    fn backend_fields(&mut self, ui: &mut egui::Ui) {
        let secret = !self.show_secret;
        let mut pick_path = false;
        egui::Grid::new("backend_fields").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            match &mut self.cfg.backend {
                BackendConfig::LocalFolder { path } => {
                    ui::field(ui, "저장 위치", path, false, r"\\NAS\home\Obsidian 또는 Z:\Obsidian");
                    ui.label("");
                    if ui.button("폴더 선택…").clicked() {
                        pick_path = true;
                    }
                    ui.end_row();
                }
                BackendConfig::WebDav { url, username, password } => {
                    ui::field(ui, "WebDAV 주소", url, false, "https://내NAS주소:5006/home/Obsidian");
                    ui::field(ui, "NAS 아이디", username, false, "");
                    ui::field(ui, "NAS 비밀번호", password, secret, "");
                }
                BackendConfig::S3 { endpoint, region, bucket, prefix, access_key, secret_key } => {
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
                    ui::field(ui, "엔드포인트", endpoint, false, "https://kr.object.ncloudstorage.com");
                    ui::field(ui, "리전", region, false, "kr-standard");
                    ui::field(ui, "버킷 이름", bucket, false, "my-obsidian");
                    ui::field(ui, "저장 폴더", prefix, false, "obsidian");
                    ui::field(ui, "Access Key", access_key, false, "");
                    ui::field(ui, "Secret Key", secret_key, secret, "");
                }
                BackendConfig::OneDrive { client_id, folder, .. } => {
                    ui::field(ui, "앱 Client ID", client_id, false, "Azure 앱 등록에서 받은 애플리케이션 ID");
                    ui::field(ui, "OneDrive 폴더", folder, false, "ObsidianSync");
                }
                BackendConfig::GoogleDrive { client_id, client_secret, folder } => {
                    ui::field(ui, "Client ID", client_id, false, "OAuth 클라이언트 ID (TV 및 입력 제한 기기)");
                    ui::field(ui, "Client Secret", client_secret, secret, "");
                    ui::field(ui, "Drive 폴더", folder, false, "ObsidianSync");
                }
            }
        });
        if pick_path {
            if let (Some(p), BackendConfig::LocalFolder { path }) = (platform::pick_folder(), &mut self.cfg.backend) {
                *path = p;
            }
        }
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.show_secret, "비밀번호 표시");
            ui.hyperlink_to(RichText::new("❓ 이 저장소 설정 방법").small(), backend_help(&self.cfg.backend));
        });
    }

    fn login_ui(&mut self, ui: &mut egui::Ui) {
        let logged_in = auth::is_logged_in(&self.data_dir, &self.cfg.backend);
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
                            ui.label(RichText::new(user_code).size(28.0).strong().monospace().color(ACCENT));
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
                _ => ("로그인했습니다. '연결 테스트'로 확인해 보세요.".into(), false),
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
                        auth::logout(&self.data_dir, &self.cfg.backend);
                    }
                } else {
                    let ok = self.cfg.backend.validate().is_ok();
                    if ui.add_enabled(ok, egui::Button::new("🔑  로그인")).clicked() {
                        self.start_login(ui.ctx());
                    }
                    ui.label(RichText::new(if ok { "브라우저에서 계정을 승인합니다" } else { "위 항목을 먼저 입력하세요" }).color(MUTED));
                }
            });
        }
    }

    fn advanced(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.cfg.watch_local, "저장하자마자 바로 업로드 (실시간 감시)");
        ui.checkbox(&mut self.cfg.sync_obsidian_folder, ".obsidian 설정 폴더도 동기화 (플러그인, 테마, 단축키)");
        ui.checkbox(&mut self.cfg.propagate_deletes, "삭제도 동기화 (지워진 파일은 .trash 폴더로 이동)");
        ui.checkbox(&mut self.cfg.start_on_launch, "앱을 켜면 자동으로 동기화 시작");
        if cfg!(windows) {
            let mut a = self.autostart;
            if ui.checkbox(&mut a, "Windows 시작 시 자동 실행 (트레이에서 조용히)").changed() {
                match platform::set_autostart(a) {
                    Ok(()) => self.autostart = a,
                    Err(e) => self.set_msg(format!("{e:#}"), true),
                }
            }
        }
        ui.add_space(4.0);
        egui::Grid::new("adv").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("원격 확인 주기");
            ui.add(egui::DragValue::new(&mut self.cfg.poll_interval_secs).range(10..=3600).suffix(" 초"));
            ui.end_row();
            ui.label("저장 후 대기");
            ui.add(egui::DragValue::new(&mut self.cfg.debounce_ms).range(200..=60000).suffix(" ms"));
            ui.end_row();
            ui.label("기기 이름");
            ui.add(
                egui::TextEdit::singleline(&mut self.cfg.device_name)
                    .hint_text(self.saved.effective_device_name())
                    .desired_width(200.0),
            );
            ui.end_row();
        });
        ui.add_space(4.0);
        ui.label("동기화하지 않을 파일/폴더 (한 줄에 하나, 예: Private, *.mp4)");
        ui.add(egui::TextEdit::multiline(&mut self.ignore_text).desired_rows(3).desired_width(f32::INFINITY));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .button("동기화 기록 초기화")
                .on_hover_text("다음 동기화 때 양쪽 파일을 처음부터 다시 비교합니다. 파일은 지워지지 않습니다.")
                .clicked()
            {
                let was_running = self.service.is_some();
                self.stop_service();
                match service::reset_state(self.cfg.clone(), &self.data_dir) {
                    Ok(()) => self.set_msg("동기화 기록을 초기화했습니다", false),
                    Err(e) => self.set_msg(format!("{e:#}"), true),
                }
                if was_running {
                    self.start_service();
                }
            }
            if ui.button("설정 폴더 열기").clicked() {
                let _ = open::that(&self.data_dir);
            }
        });
    }

    // ───────────────────────────── 기록 ─────────────────────────────
    fn log_page(&mut self, ui: &mut egui::Ui, status: &Option<ServiceStatus>) {
        let logs = status.as_ref().map(|s| s.logs.clone()).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("최근 기록 {}건", logs.len())).color(MUTED));
            if ui.small_button("📋 전체 복사").clicked() {
                let text: String = logs.iter().map(|l| format!("{} {}\n", l.time, l.text)).collect();
                ui.output_mut(|o| o.copied_text = text);
            }
        });
        ui::card(ui, |ui| {
            egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink(false).show(ui, |ui| {
                if logs.is_empty() {
                    ui.label(RichText::new("동기화를 시작하면 여기에 기록이 표시됩니다").color(MUTED));
                }
                for l in &logs {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(&l.time).color(MUTED).monospace().size(12.0));
                        ui.label(RichText::new(&l.text).size(13.0).color(if l.error { DANGER } else { ui::TEXT }));
                    });
                }
            });
        });
    }
}

fn backend_summary(b: &BackendConfig) -> String {
    match b {
        BackendConfig::LocalFolder { path } => format!("폴더 · {path}"),
        BackendConfig::WebDav { url, .. } => format!("WebDAV · {url}"),
        BackendConfig::S3 { endpoint, bucket, prefix, .. } => format!("S3 · {endpoint} / {bucket}/{prefix}"),
        BackendConfig::OneDrive { folder, .. } => format!("OneDrive · /{folder}"),
        BackendConfig::GoogleDrive { folder, .. } => format!("Google Drive · /{folder}"),
    }
}

fn backend_help(b: &BackendConfig) -> String {
    let anchor = match b {
        BackendConfig::LocalFolder { .. } => "폴더--공유-드라이브",
        BackendConfig::WebDav { .. } => "synology-nas-webdav",
        BackendConfig::S3 { .. } => "네이버-클라우드--google-cloud-storage",
        BackendConfig::OneDrive { .. } => "onedrive",
        BackendConfig::GoogleDrive { .. } => "google-drive",
    };
    format!("{SETUP_DOC}#{anchor}")
}

/// 기술적인 오류 메시지를 이해하기 쉬운 안내로 변환
fn friendly_error(raw: &str) -> String {
    let hint = if raw.contains("HTTP 401") || raw.contains("HTTP 403") || raw.contains("SignatureDoesNotMatch") || raw.contains("InvalidAccessKeyId") {
        "아이디/비밀번호 또는 키가 맞지 않거나 권한이 없습니다."
    } else if raw.contains("HTTP 404") || raw.contains("NoSuchBucket") {
        "주소나 버킷/폴더 이름을 찾을 수 없습니다."
    } else if raw.contains("네트워크 오류") || raw.contains("dns") || raw.contains("Connection") {
        "서버에 연결할 수 없습니다. 주소, 포트, 인터넷 연결을 확인하세요."
    } else if raw.contains("invalid_grant") || raw.contains("다시 로그인") {
        "로그인이 만료되었습니다. 설정에서 다시 로그인해 주세요."
    } else if raw.contains("certificate") || raw.contains("Certificate") {
        "서버 인증서를 신뢰할 수 없습니다. NAS에 정식 인증서(예: Let's Encrypt)를 설정하세요."
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
        let status = self.service.as_ref().map(|s| s.status());

        egui::SidePanel::left("nav")
            .resizable(false)
            .exact_width(170.0)
            .frame(egui::Frame::none().fill(ui::SIDEBAR).inner_margin(egui::Margin::symmetric(12.0, 16.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui::status_dot(ui, ACCENT, 10.0);
                    ui.label(RichText::new("Obsidian Sync").size(17.0).strong());
                });
                ui.label(RichText::new(format!("v{}", obsidian_sync_core::VERSION)).small().color(MUTED));
                ui.add_space(18.0);
                for (page, label) in [(Page::Home, "🏠  홈"), (Page::Settings, "⚙  설정"), (Page::Log, "📜  기록")] {
                    if ui::nav_button(ui, label, self.page == page).clicked() {
                        self.page = page;
                    }
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    if self.has_tray {
                        ui.label(RichText::new("창을 닫아도 트레이에서 계속 동기화합니다").small().color(MUTED));
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
                .frame(egui::Frame::none().fill(if err { ui::DANGER_BG } else { ui::OK_BG }).inner_margin(egui::Margin::symmetric(16.0, 10.0)))
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
            .frame(egui::Frame::none().fill(ui::BG).inner_margin(egui::Margin::symmetric(20.0, 16.0)))
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
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    ui.set_max_width((ui.available_width() - 16.0).min(720.0));
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
            .with_inner_size([900.0, 700.0])
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
