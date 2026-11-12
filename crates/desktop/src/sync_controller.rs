//! 동기화 서비스 생명주기 및 비동기 원격 저장소 연결/인증 컨트롤러.
//! 모든 네트워크·디스크 전환 작업은 백그라운드 스레드에서 수행되어 UI/API 호출을 절대 블로킹하지 않습니다.

use crate::platform::{self, SharedTrigger};
use obsidian_sync_core::backend::synology;
use obsidian_sync_core::{
    auth, service, AppConfig, BackendConfig, Profile, ProfileKind, ServiceStatus, SyncService,
};
use serde::Serialize;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub const SETUP_DOC: &str = "https://github.com/bluewind201/obsidian-sync/blob/main/docs/SETUP.md";

pub fn app_dirs() -> (PathBuf, PathBuf) {
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

pub fn backend_help(b: &BackendConfig) -> String {
    let anchor = match b {
        BackendConfig::Synology { .. } => "synology-nas",
        BackendConfig::LocalFolder { .. } => "폴더--공유-드라이브",
        BackendConfig::S3 { .. } => "네이버-클라우드--google-cloud-storage",
        BackendConfig::OneDrive { .. } => "onedrive",
        BackendConfig::GoogleDrive { .. } => "google-drive",
    };
    format!("{SETUP_DOC}#{anchor}")
}

/// 기술적인 오류 메시지를 이해하기 쉬운 한국어 안내로 변환
pub fn friendly_error(raw: &str) -> String {
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

pub enum Job {
    Running,
    Ok(String),
    Failed(String),
    NeedOtp(String),
}

pub enum BrowseState {
    Loading,
    Ready(Vec<synology::NasFolder>),
    NeedOtp(String),
    Failed(String),
}

pub struct NasBrowser {
    pub path: Arc<Mutex<String>>,
    pub state: Arc<Mutex<BrowseState>>,
}

pub enum LoginState {
    Requesting,
    Code { user_code: String, uri: String },
    Done,
    Failed(String),
}

pub struct Login {
    pub state: Arc<Mutex<LoginState>>,
    pub cancel: Arc<AtomicBool>,
    pub notified: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct ProfileExtraDto {
    pub id: String,
    pub oauth_logged_in: bool,
    pub synology_device_remembered: bool,
    pub help_url: String,
    pub summary: String,
    pub valid: bool,
    pub validation_error: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct TestJobDto {
    pub status: String,
    pub message: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct OauthJobDto {
    pub status: String,
    pub user_code: Option<String>,
    pub uri: Option<String>,
    pub message: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct NasBrowserDto {
    pub path: String,
    pub status: String,
    pub folders: Vec<synology::NasFolder>,
    pub message: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct MessageDto {
    pub text: String,
    pub error: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct SyncSnapshotDto {
    pub version: String,
    pub cfg: AppConfig,
    pub saved: AppConfig,
    pub dirty: bool,
    pub autostart: bool,
    pub service_running: bool,
    pub transitioning: bool,
    pub status: Option<ServiceStatus>,
    pub profile_extras: Vec<ProfileExtraDto>,
    pub test_job: Option<TestJobDto>,
    pub oauth_job: Option<OauthJobDto>,
    pub nas_browser: Option<NasBrowserDto>,
    pub message: Option<MessageDto>,
    pub default_device_name: String,
}

pub struct App {
    pub config_path: PathBuf,
    pub data_dir: PathBuf,
    pub cfg: AppConfig,
    pub saved: AppConfig,
    pub service: Option<SyncService>,
    /// 백그라운드에서 멈추는 중인 이전 서비스 (화면/요청이 멈추지 않도록)
    pub stopping: Option<std::thread::JoinHandle<()>>,
    pub pending_start: bool,
    pub pending_reset: Option<(AppConfig, String)>,
    pub resetting: Option<std::thread::JoinHandle<anyhow::Result<()>>>,
    pub trigger: SharedTrigger,
    pub login: Option<Login>,
    pub test: Option<Arc<Mutex<Job>>>,
    pub browser: Option<NasBrowser>,
    pub message: Option<(String, bool)>,
    pub autostart: bool,
}

impl App {
    pub fn new(config_path: PathBuf, data_dir: PathBuf, trigger: SharedTrigger) -> Self {
        let cfg = AppConfig::load(&config_path).unwrap_or_default();
        let configured = cfg.validate().is_ok();
        let mut app = Self {
            config_path,
            data_dir,
            saved: cfg.clone(),
            cfg,
            service: None,
            stopping: None,
            pending_start: false,
            pending_reset: None,
            resetting: None,
            trigger,
            login: None,
            test: None,
            browser: None,
            message: None,
            autostart: platform::autostart_enabled(),
        };
        if configured && app.cfg.start_on_launch {
            app.start_service();
        }
        app
    }

    pub fn set_msg(&mut self, text: impl Into<String>, err: bool) {
        self.message = Some((text.into(), err));
    }

    pub fn clear_message(&mut self) {
        self.message = None;
    }

    pub fn dirty(&self) -> bool {
        self.cfg != self.saved
    }

    pub fn save_config(&mut self, mut cfg: AppConfig, start_after_save: bool) -> bool {
        // 프로필 ID 무결성 보장
        let mut seen = HashSet::new();
        for p in &mut cfg.profiles {
            if p.id.trim().is_empty() || !seen.insert(p.id.clone()) {
                p.id = obsidian_sync_core::config::new_profile_id();
                seen.insert(p.id.clone());
            }
        }
        self.cfg = cfg;
        match self.cfg.save(&self.config_path) {
            Ok(()) => {
                self.saved = self.cfg.clone();
                if start_after_save {
                    self.start_service();
                } else {
                    self.set_msg("설정을 저장했습니다", false);
                }
                true
            }
            Err(e) => {
                self.set_msg(format!("저장하지 못했습니다: {e:#}"), true);
                false
            }
        }
    }

    pub fn start_service(&mut self) {
        if self.service.is_some() {
            self.stop_service();
        }
        if self.transitioning() {
            self.pending_start = true;
            return;
        }
        if let Err(e) = self.cfg.validate() {
            self.set_msg(format!("{e:#}"), true);
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
            self.set_msg(msg, true);
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

    pub fn transitioning(&self) -> bool {
        self.stopping.is_some() || self.pending_reset.is_some() || self.resetting.is_some()
    }

    pub fn reset_profile(&mut self, id: String) {
        if self.pending_reset.is_some() || self.resetting.is_some() {
            return;
        }
        let restart = self.service.is_some() || self.pending_start;
        self.stop_service();
        self.pending_start = restart;
        self.pending_reset = Some((self.cfg.clone(), id));
    }

    pub fn stop_service(&mut self) {
        self.pending_start = false;
        *self.trigger.lock().unwrap() = None;
        if let Some(s) = self.service.take() {
            debug_assert!(self.stopping.is_none());
            self.stopping = Some(std::thread::spawn(move || s.stop()));
        }
    }

    /// 완료된 백그라운드 작업만 수거하며 네트워크 I/O를 절대 기다리지 않음.
    pub fn poll_lifecycle(&mut self) {
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

    pub fn sync_now(&self) {
        if let Some(s) = &self.service {
            s.sync_now();
        }
    }

    pub fn allow_deletes(&self, profile_id: &str) {
        if let Some(s) = &self.service {
            s.allow_deletes(profile_id);
        }
    }

    pub fn new_profile(
        &self,
        kind: ProfileKind,
        copy_synology_from: Option<BackendConfig>,
    ) -> Profile {
        let source =
            copy_synology_from.or_else(|| self.cfg.profiles.last().map(|p| p.backend.clone()));
        let backend = match source {
            Some(BackendConfig::Synology {
                url,
                username,
                password,
                ..
            }) => BackendConfig::Synology {
                url,
                username,
                password,
                folder: "/home".into(),
            },
            _ => BackendConfig::default(),
        };
        Profile {
            name: match kind {
                ProfileKind::Obsidian => "Obsidian".into(),
                ProfileKind::Folder => "새 폴더".into(),
            },
            kind,
            backend,
            ..Default::default()
        }
    }

    pub fn test_connection(&mut self, backend: BackendConfig) {
        let job = Arc::new(Mutex::new(Job::Running));
        let j = job.clone();
        let dir = self.data_dir.clone();
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
        });
        self.test = Some(job);
    }

    pub fn submit_otp(&mut self, backend: BackendConfig, otp: String) {
        let job = Arc::new(Mutex::new(Job::Running));
        let j = job.clone();
        let dir = self.data_dir.clone();
        let code = otp.trim().to_string();
        std::thread::spawn(move || {
            let r = synology::login(&backend, &dir, Some(&code))
                .and_then(|_| service::test_connection(&backend, &dir));
            *j.lock().unwrap() = match r {
                Ok(n) => Job::Ok(format!(
                    "인증 완료! 이 PC를 기억하므로 다음부터는 코드를 묻지 않습니다. (원격 파일 {n}개)"
                )),
                Err(e) if synology::needs_otp(&e) => Job::NeedOtp(format!("{e:#}")),
                Err(e) => Job::Failed(friendly_error(&format!("{e:#}"))),
            };
        });
        self.test = Some(job);
    }

    pub fn forget_device(&mut self, backend: &BackendConfig) {
        synology::forget_device(&self.data_dir, backend);
        self.set_msg("이 PC의 2단계 인증 기억을 해제했습니다", false);
    }

    pub fn nas_browse_open(&mut self, backend: BackendConfig, initial_folder: String) {
        let raw_folder = if !initial_folder.trim().is_empty() {
            initial_folder
        } else if let BackendConfig::Synology { folder, .. } = &backend {
            folder.clone()
        } else {
            String::new()
        };
        let start = if !raw_folder.trim().is_empty() {
            let f = format!("/{}", raw_folder.trim().trim_matches('/'));
            match f.rfind('/') {
                Some(0) | None => "/".to_string(),
                Some(i) => f[..i].to_string(),
            }
        } else {
            "/".to_string()
        };
        self.browser = Some(NasBrowser {
            path: Arc::new(Mutex::new(start.clone())),
            state: Arc::new(Mutex::new(BrowseState::Loading)),
        });
        self.nas_browse_step(backend, start, None, None);
    }

    pub fn nas_browse_step(
        &mut self,
        backend: BackendConfig,
        path: String,
        create: Option<String>,
        otp: Option<String>,
    ) {
        let norm_path = if path.trim().is_empty() {
            "/".to_string()
        } else if path.trim() == "/" {
            "/".to_string()
        } else {
            format!("/{}", path.trim().trim_matches('/'))
        };
        let create_clean = create
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let otp_clean = otp.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());

        let path_arc = Arc::new(Mutex::new(norm_path.clone()));
        let state_arc = Arc::new(Mutex::new(BrowseState::Loading));
        self.browser = Some(NasBrowser {
            path: path_arc.clone(),
            state: state_arc.clone(),
        });

        let dir = self.data_dir.clone();
        std::thread::spawn(move || {
            let r = (|| -> anyhow::Result<(String, Vec<synology::NasFolder>)> {
                if let Some(code) = &otp_clean {
                    synology::login(&backend, &dir, Some(code))?;
                }
                let mut at = norm_path;
                if let Some(name) = &create_clean {
                    at = synology::create_folder(&backend, &dir, &at, name)?;
                }
                let folders = synology::list_folders(&backend, &dir, &at)?;
                Ok((at, folders))
            })();
            match r {
                Ok((actual_path, folders)) => {
                    *path_arc.lock().unwrap() = actual_path;
                    *state_arc.lock().unwrap() = BrowseState::Ready(folders);
                }
                Err(e) if synology::needs_otp(&e) => {
                    *state_arc.lock().unwrap() = BrowseState::NeedOtp(format!("{e:#}"));
                }
                Err(e) => {
                    *state_arc.lock().unwrap() =
                        BrowseState::Failed(friendly_error(&format!("{e:#}")));
                }
            }
        });
    }

    pub fn nas_browse_close(&mut self) {
        self.browser = None;
    }

    pub fn oauth_start(&mut self, backend: BackendConfig) {
        if let Some(prev) = self.login.take() {
            prev.cancel.store(true, Ordering::Relaxed);
        }
        let state = Arc::new(Mutex::new(LoginState::Requesting));
        let cancel = Arc::new(AtomicBool::new(false));
        let st = state.clone();
        let cn = cancel.clone();
        let dir = self.data_dir.clone();
        std::thread::spawn(move || {
            let st2 = st.clone();
            let res = auth::login_device(
                &dir,
                &backend,
                |dc| {
                    *st2.lock().unwrap() = LoginState::Code {
                        user_code: dc.user_code.clone(),
                        uri: dc.verification_uri.clone(),
                    };
                    let _ = open::that(&dc.verification_uri);
                },
                &cn,
            );
            *st.lock().unwrap() = match res {
                Ok(()) => LoginState::Done,
                Err(e) => LoginState::Failed(format!("{e:#}")),
            };
        });
        self.login = Some(Login {
            state,
            cancel,
            notified: false,
        });
    }

    pub fn oauth_cancel(&mut self) {
        if let Some(login) = self.login.take() {
            login.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn oauth_logout(&mut self, backend: &BackendConfig) {
        self.oauth_cancel();
        auth::logout(&self.data_dir, backend);
        self.set_msg("로그아웃했습니다", false);
    }

    pub fn set_autostart(&mut self, enable: bool) {
        match platform::set_autostart(enable) {
            Ok(()) => {
                self.autostart = enable;
            }
            Err(e) => {
                self.set_msg(format!("{e:#}"), true);
            }
        }
    }

    pub fn snapshot(&mut self) -> SyncSnapshotDto {
        self.poll_lifecycle();

        let status = self.service.as_ref().map(|s| s.status());
        let profile_extras: Vec<ProfileExtraDto> = self
            .cfg
            .profiles
            .iter()
            .map(|p| {
                let val_res = p.validate(true);
                ProfileExtraDto {
                    id: p.id.clone(),
                    oauth_logged_in: auth::is_logged_in(&self.data_dir, &p.backend),
                    synology_device_remembered: synology::has_device_token(
                        &self.data_dir,
                        &p.backend,
                    ),
                    help_url: backend_help(&p.backend),
                    summary: p.backend.summary(),
                    valid: val_res.is_ok(),
                    validation_error: val_res.err().map(|e| format!("{e:#}")),
                }
            })
            .collect();

        let test_job = self.test.as_ref().map(|j| match &*j.lock().unwrap() {
            Job::Running => TestJobDto {
                status: "running".into(),
                message: "연결을 확인하는 중…".into(),
            },
            Job::Ok(m) => TestJobDto {
                status: "ok".into(),
                message: m.clone(),
            },
            Job::NeedOtp(m) => TestJobDto {
                status: "need_otp".into(),
                message: m.clone(),
            },
            Job::Failed(m) => TestJobDto {
                status: "failed".into(),
                message: m.clone(),
            },
        });

        let mut oauth_toast: Option<(String, bool)> = None;
        let oauth_job = self.login.as_mut().map(|l| {
            let guard = l.state.lock().unwrap();
            match &*guard {
                LoginState::Requesting => OauthJobDto {
                    status: "requesting".into(),
                    user_code: None,
                    uri: None,
                    message: Some("로그인 코드를 발급받는 중…".into()),
                },
                LoginState::Code { user_code, uri } => OauthJobDto {
                    status: "code".into(),
                    user_code: Some(user_code.clone()),
                    uri: Some(uri.clone()),
                    message: Some("브라우저에서 코드를 입력하고 승인해 주세요.".into()),
                },
                LoginState::Done => {
                    let msg = "로그인했습니다. '연결 테스트'로 확인해 보세요.".to_string();
                    if !l.notified {
                        l.notified = true;
                        oauth_toast = Some((msg.clone(), false));
                    }
                    OauthJobDto {
                        status: "done".into(),
                        user_code: None,
                        uri: None,
                        message: Some(msg),
                    }
                }
                LoginState::Failed(e) => {
                    let msg = friendly_error(e);
                    if !l.notified {
                        l.notified = true;
                        oauth_toast = Some((msg.clone(), true));
                    }
                    OauthJobDto {
                        status: "failed".into(),
                        user_code: None,
                        uri: None,
                        message: Some(msg),
                    }
                }
            }
        });
        if let Some((txt, err)) = oauth_toast {
            self.set_msg(txt, err);
        }

        let nas_browser = self.browser.as_ref().map(|b| {
            let path = b.path.lock().unwrap().clone();
            match &*b.state.lock().unwrap() {
                BrowseState::Loading => NasBrowserDto {
                    path,
                    status: "loading".into(),
                    folders: Vec::new(),
                    message: Some("NAS 폴더를 불러오는 중…".into()),
                },
                BrowseState::Ready(folders) => NasBrowserDto {
                    path,
                    status: "ready".into(),
                    folders: folders.clone(),
                    message: None,
                },
                BrowseState::NeedOtp(m) => NasBrowserDto {
                    path,
                    status: "need_otp".into(),
                    folders: Vec::new(),
                    message: Some(m.clone()),
                },
                BrowseState::Failed(m) => NasBrowserDto {
                    path,
                    status: "failed".into(),
                    folders: Vec::new(),
                    message: Some(m.clone()),
                },
            }
        });

        let message = self.message.as_ref().map(|(text, error)| MessageDto {
            text: text.clone(),
            error: *error,
        });

        SyncSnapshotDto {
            version: obsidian_sync_core::VERSION.to_string(),
            dirty: self.dirty(),
            cfg: self.cfg.clone(),
            saved: self.saved.clone(),
            autostart: self.autostart,
            service_running: self.service.is_some(),
            transitioning: self.transitioning(),
            status,
            profile_extras,
            test_job,
            oauth_job,
            nas_browser,
            message,
            default_device_name: self.saved.effective_device_name(),
        }
    }
}
