#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use eframe::egui;
use notify::Watcher;
use std::{
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};
use vault_sync_core::Config;

fn state_root() -> PathBuf {
    directories::ProjectDirs::from("io", "bluewind", "ObsidianSync")
        .expect("User data folder")
        .data_local_dir()
        .to_path_buf()
}
#[cfg(windows)]
fn load_config() -> Option<Config> {
    let text = keyring::Entry::new("bluewind-obsidian-sync", "default")
        .ok()?
        .get_password()
        .ok()?;
    serde_json::from_str(&text).ok()
}
#[cfg(not(windows))]
fn load_config() -> Option<Config> {
    None
}
#[cfg(windows)]
fn save_config(config: &Config) -> anyhow::Result<()> {
    keyring::Entry::new("bluewind-obsidian-sync", "default")?
        .set_password(&serde_json::to_string(config)?)?;
    Ok(())
}
#[cfg(not(windows))]
fn save_config(_: &Config) -> anyhow::Result<()> {
    anyhow::bail!("Credential persistence is supported on Windows")
}

struct App {
    config: Config,
    status: String,
    history: Vec<String>,
    result_tx: mpsc::Sender<String>,
    result_rx: mpsc::Receiver<String>,
    event_rx: mpsc::Receiver<notify::Result<notify::Event>>,
    event_tx: mpsc::Sender<notify::Result<notify::Event>>,
    watcher: Option<notify::RecommendedWatcher>,
    running: bool,
    busy: bool,
    next: Instant,
    options_json: String,
    show_secret: bool,
    #[cfg(windows)]
    tray: Option<tray_icon::TrayIcon>,
    #[cfg(windows)]
    quit: tray_icon::menu::MenuId,
}
impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut fonts = egui::FontDefinitions::default();
        if let Ok(bytes) = std::fs::read("C:/Windows/Fonts/malgun.ttf") {
            fonts
                .font_data
                .insert("korean".into(), egui::FontData::from_owned(bytes).into());
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "korean".into());
        }
        cc.egui_ctx.set_fonts(fonts);
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(12.0, 12.0);
        style.visuals.selection.bg_fill = egui::Color32::from_rgb(68, 91, 164);
        cc.egui_ctx.set_style(style);
        let config = load_config().unwrap_or_default();
        let options_json = serde_json::to_string_pretty(&config.options).unwrap();
        let (result_tx, result_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        #[cfg(windows)]
        let (tray, quit) = {
            use tray_icon::{
                menu::{Menu, MenuItem},
                Icon, TrayIconBuilder,
            };
            let menu = Menu::new();
            let open = MenuItem::new("Open Obsidian Sync", true, None);
            let quit = MenuItem::new("Quit", true, None);
            let _ = menu.append_items(&[&open, &quit]);
            let mut rgba = vec![0u8; 32 * 32 * 4];
            for y in 0..32 {
                for x in 0..32 {
                    let p = (y * 32 + x) * 4;
                    let bright = (8..24).contains(&x) && (6..26).contains(&y);
                    rgba[p..p + 4].copy_from_slice(if bright {
                        &[230, 236, 255, 255]
                    } else {
                        &[68, 91, 164, 255]
                    });
                }
            }
            let tray = Icon::from_rgba(rgba, 32, 32).ok().and_then(|icon| {
                TrayIconBuilder::new()
                    .with_tooltip("Obsidian Sync")
                    .with_menu(Box::new(menu))
                    .with_icon(icon)
                    .build()
                    .ok()
            });
            (tray, quit.id().clone())
        };
        Self {
            config,
            options_json,
            status: "보관함과 저장소를 연결하세요 / Connect your vault".into(),
            history: vec![],
            result_tx,
            result_rx,
            event_tx,
            event_rx,
            watcher: None,
            running: false,
            busy: false,
            next: Instant::now(),
            show_secret: false,
            #[cfg(windows)]
            tray,
            #[cfg(windows)]
            quit,
        }
    }
    fn start_job(&mut self) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.status = "동기화 중 / Synchronizing…".into();
        let json = serde_json::to_string(&self.config).unwrap();
        let tx = self.result_tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(vault_sync_core::run_json(&json, &state_root()));
        });
    }
    fn preset(&mut self, backend: &str) {
        self.config.backend = backend.into();
        let fields: &[(&str, &str)] = match backend {
            "webdav" => &[
                ("root", "/ObsidianSync/"),
                ("endpoint", "https://nas.example.com:5006"),
                ("username", ""),
                ("password", ""),
            ],
            "s3" => &[
                ("root", "/ObsidianSync/"),
                ("endpoint", "https://kr.object.ncloudstorage.com"),
                ("region", "kr-standard"),
                ("bucket", ""),
                ("access_key_id", ""),
                ("secret_access_key", ""),
            ],
            "onedrive" => &[("root", "/ObsidianSync/"), ("access_token", "")],
            "gdrive" => &[("root", "/ObsidianSync/"), ("access_token", "")],
            _ => &[("root", "")],
        };
        self.config.options = fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        self.options_json = serde_json::to_string_pretty(&self.config.options).unwrap();
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(300));
        #[cfg(windows)]
        {
            while let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
                if event.id == self.quit {
                    self.running = false;
                    self.tray.take();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                } else {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
            }
            if ctx.input(|i| i.viewport().close_requested()) && self.tray.is_some() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
        }
        while let Ok(result) = self.result_rx.try_recv() {
            self.busy = false;
            let parsed = serde_json::from_str::<serde_json::Value>(&result).unwrap_or_default();
            self.status = if parsed["ok"] == true {
                let r = &parsed["report"];
                format!(
                    "동기화 완료 · 업로드 {} · 다운로드 {} · 삭제 {} · 충돌 {} · 다음 확인 {}",
                    r["uploaded"], r["downloaded"], r["deleted"], r["conflicts"], r["deferred"]
                )
            } else {
                "동기화 중단: 폴더 권한, 인증 정보, 네트워크, 파일명, 원격 이력을 확인하세요."
                    .into()
            };
            self.history.insert(0, self.status.clone());
            self.history.truncate(40);
            self.next = Instant::now() + Duration::from_secs(self.config.interval_seconds.max(5));
        }
        while let Ok(event) = self.event_rx.try_recv() {
            if let Ok(event) = event {
                if !matches!(event.kind, notify::EventKind::Access(_)) {
                    self.next = self.next.min(Instant::now() + Duration::from_secs(2));
                }
            }
        }
        if self.running && !self.busy && Instant::now() >= self.next {
            self.start_job();
        }
        egui::TopBottomPanel::top("heading").show(ctx, |ui| {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.heading("Obsidian Sync");
                ui.label("Windows · Preview 0.1");
            });
            ui.label("내 노트를, 내가 선택한 저장소로 / Your vault, your storage");
            ui.add_space(8.0);
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading(if self.running {
                    "자동 동기화 켜짐"
                } else {
                    "동기화 대기"
                });
                ui.label(&self.status);
                ui.separator();
                ui.add_enabled_ui(!self.running && !self.busy, |ui| {
                    ui.label("Obsidian 보관함 / Vault folder");
                    ui.horizontal(|ui| {
                        let mut path = self.config.vault.to_string_lossy().to_string();
                        if ui.text_edit_singleline(&mut path).changed() {
                            self.config.vault = path.into();
                        }
                        if ui.button("폴더 선택").clicked() {
                            if let Some(p) = rfd::FileDialog::new().pick_folder() {
                                self.config.vault = p;
                            }
                        }
                    });
                    let previous = self.config.backend.clone();
                    egui::ComboBox::from_label("연결 대상 / Storage")
                        .selected_text(&self.config.backend)
                        .show_ui(ui, |ui| {
                            for (id, name) in [
                                ("fs", "Local folder / NAS / OneDrive folder"),
                                ("webdav", "Synology WebDAV"),
                                ("s3", "NAVER Object Storage / Google Cloud / S3"),
                                ("onedrive", "OneDrive API"),
                                ("gdrive", "Google Drive API"),
                            ] {
                                ui.selectable_value(&mut self.config.backend, id.into(), name);
                            }
                        });
                    if previous != self.config.backend {
                        let id = self.config.backend.clone();
                        self.preset(&id);
                    }
                    if self.config.backend == "s3"
                        && ui.button("Google Cloud Storage 설정 사용").clicked()
                    {
                        self.config
                            .options
                            .insert("endpoint".into(), "https://storage.googleapis.com".into());
                        self.config.options.insert("region".into(), "auto".into());
                    }
                    let mut keys: Vec<_> = self.config.options.keys().cloned().collect();
                    keys.sort();
                    for key in keys {
                        ui.horizontal(|ui| {
                            ui.label(&key);
                            let secret = key.contains("token")
                                || key.contains("secret")
                                || key == "password"
                                || key == "access_key_id";
                            ui.add(
                                egui::TextEdit::singleline(
                                    self.config.options.get_mut(&key).unwrap(),
                                )
                                .password(secret && !self.show_secret)
                                .desired_width(390.0),
                            );
                        });
                    }
                    if self.config.backend == "fs" && ui.button("저장소 폴더 선택").clicked()
                    {
                        if let Some(p) = rfd::FileDialog::new().pick_folder() {
                            self.config
                                .options
                                .insert("root".into(), p.to_string_lossy().into());
                        }
                    }
                    ui.checkbox(&mut self.show_secret, "인증 정보 표시 / Reveal credentials");
                    ui.collapsing("고급 연결 옵션 / Advanced options", |ui| {
                        ui.label("JSON에는 비밀번호가 포함될 수 있습니다. 공유하지 마세요.");
                        if ui.button("현재 옵션 불러오기").clicked() {
                            self.options_json =
                                serde_json::to_string_pretty(&self.config.options).unwrap();
                        }
                        ui.add(
                            egui::TextEdit::multiline(&mut self.options_json)
                                .desired_rows(6)
                                .desired_width(f32::INFINITY),
                        );
                        if ui.button("옵션 적용").clicked() {
                            match serde_json::from_str(&self.options_json) {
                                Ok(options) => self.config.options = options,
                                Err(_) => self.status = "Invalid options JSON".into(),
                            }
                        }
                    });
                    ui.checkbox(
                        &mut self.config.propagate_deletes,
                        "삭제 전파 허용 (기본값: 삭제한 파일 복원)",
                    );
                    ui.add(
                        egui::Slider::new(&mut self.config.interval_seconds, 5..=300)
                            .text("원격 확인 간격 (초)"),
                    );
                    if ui.button("설정 저장 / Save securely").clicked() {
                        self.status = if save_config(&self.config).is_ok() {
                            "Windows 자격 증명 관리자에 저장했습니다".into()
                        } else {
                            "설정 저장 실패. 인증 정보 길이 또는 자격 증명 관리자 상태를 확인하세요"
                                .into()
                        };
                    }
                });
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!self.busy, egui::Button::new("지금 동기화 / Sync now"))
                        .clicked()
                    {
                        self.start_job();
                    }
                    if ui
                        .button(if self.running {
                            "자동 동기화 중지"
                        } else {
                            "자동 동기화 시작"
                        })
                        .clicked()
                    {
                        self.running = !self.running;
                        if self.running {
                            let tx = self.event_tx.clone();
                            self.watcher = notify::recommended_watcher(move |e| {
                                let _ = tx.send(e);
                            })
                            .ok();
                            if let Some(watcher) = &mut self.watcher {
                                let _ = watcher
                                    .watch(&self.config.vault, notify::RecursiveMode::Recursive);
                            }
                            self.next = Instant::now();
                        } else {
                            self.watcher = None;
                        }
                    }
                });
                ui.label("충돌본: .sync-conflicts   이전 파일: .sync-backups");
                ui.label("창을 닫으면 트레이에 유지됩니다. 트레이 메뉴의 Quit으로 종료하세요.");
                ui.hyperlink_to(
                    "연결 및 복구 안내",
                    "https://github.com/bluewind201/obsidian-sync/blob/main/docs/SETUP.md",
                );
                ui.separator();
                ui.collapsing("최근 동기화 기록", |ui| {
                    for entry in &self.history {
                        ui.label(entry);
                        ui.separator();
                    }
                });
            });
        });
    }
}
fn main() -> eframe::Result<()> {
    eframe::run_native(
        "Obsidian Sync",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([800.0, 800.0])
                .with_min_inner_size([560.0, 560.0]),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
