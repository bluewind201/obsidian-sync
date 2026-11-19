//! 실시간 동기화 센터 UI (Synology NAS, Google Drive, OneDrive, S3 호환, 로컬/클라우드 드라이브 폴더)
//! 모든 상태 변경·연결 테스트·NAS 폴더 탐색·OAuth 인증(Device Code + 보안망용 수동 토큰 입력)·자동 업데이트는
//! 로컬 포트/소켓 바인딩 없이 프로세스 인메모리(`BackendState`) 직접 호출로 동작합니다.

use crate::api::BackendState;
use crate::platform;
use crate::sync_controller::{self, DetectedCloudFolderDto, SyncSnapshotDto, SETUP_DOC};
use crate::ui::{self, ThemePalette};
use eframe::egui::{self, Color32, Margin, RichText, Rounding, Stroke};
use obsidian_sync_core::{AppConfig, BackendConfig, ConflictPolicy, ProfileKind, SyncMode};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

const BACKENDS: &[(&str, &str, Color32, &str, &str)] = &[
    (
        "synology",
        "NAS",
        Color32::from_rgb(40, 90, 170),
        "Synology NAS",
        "DSM 계정으로 로그인 (File Station)",
    ),
    (
        "local_folder",
        "폴더",
        Color32::from_rgb(110, 114, 128),
        "폴더 / 클라우드 드라이브",
        "NAS 공유폴더(SMB), Synology Drive, 외장 디스크 등",
    ),
    (
        "one_drive",
        "OD",
        Color32::from_rgb(0, 120, 212),
        "OneDrive",
        "Microsoft 계정으로 로그인",
    ),
    (
        "google_drive",
        "GD",
        Color32::from_rgb(30, 142, 62),
        "Google Drive",
        "Google 계정으로 로그인",
    ),
    (
        "s3",
        "S3",
        Color32::from_rgb(3, 199, 90),
        "네이버 클라우드 / GCS",
        "S3 호환 스토리지 (액세스 키 필요)",
    ),
];

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

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum SyncSubTab {
    Home,
    Settings,
    Logs,
}

pub enum SyncUiAction {
    OpenVaultInEditor(String),
    Toast(String, bool),
    ConfigSaved,
    ApplyUpdateAndRestart,
}

pub struct SyncCenterState {
    pub sub_tab: SyncSubTab,
    pub draft_cfg: AppConfig,
    pub selected_idx: usize,
    pub ignore_texts: HashMap<String, String>,
    pub backend_cache: HashMap<(String, &'static str), BackendConfig>,
    pub show_secret: bool,
    pub confirm_delete: bool,
    pub advanced_open: bool,
    pub otp_input: String,
    pub nas_new_folder: String,
    pub nas_otp_input: String,
    pub show_manual_oauth: bool,
    pub manual_access_token: String,
    pub manual_refresh_token: String,
    pub detected_clouds: Vec<DetectedCloudFolderDto>,
}

impl SyncCenterState {
    pub fn new(initial_cfg: &AppConfig) -> Self {
        let mut s = Self {
            sub_tab: if initial_cfg.profiles.is_empty() {
                SyncSubTab::Settings
            } else {
                SyncSubTab::Home
            },
            draft_cfg: initial_cfg.clone(),
            selected_idx: 0,
            ignore_texts: HashMap::new(),
            backend_cache: HashMap::new(),
            show_secret: false,
            confirm_delete: false,
            advanced_open: false,
            otp_input: String::new(),
            nas_new_folder: String::new(),
            nas_otp_input: String::new(),
            show_manual_oauth: false,
            manual_access_token: String::new(),
            manual_refresh_token: String::new(),
            detected_clouds: sync_controller::detect_cloud_folders(),
        };
        s.sync_ignore_texts();
        s
    }

    pub fn sync_ignore_texts(&mut self) {
        for p in &self.draft_cfg.profiles {
            self.ignore_texts
                .entry(p.id.clone())
                .or_insert_with(|| p.ignore_patterns.join("\n"));
        }
    }

    pub fn is_dirty(&self, snap: &SyncSnapshotDto) -> bool {
        snap.dirty || self.draft_cfg != snap.saved
    }

    fn create_profile(&mut self, backend: &Arc<Mutex<BackendState>>, kind: ProfileKind) {
        let copy_from = self
            .draft_cfg
            .profiles
            .get(self.selected_idx)
            .and_then(|p| match &p.backend {
                b @ BackendConfig::Synology { .. } => Some(b.clone()),
                _ => None,
            });
        let p = backend.lock().unwrap().sync.new_profile(kind, copy_from);
        self.ignore_texts
            .insert(p.id.clone(), p.ignore_patterns.join("\n"));
        self.draft_cfg.profiles.push(p);
        self.selected_idx = self.draft_cfg.profiles.len().saturating_sub(1);
        self.confirm_delete = false;
        self.sub_tab = SyncSubTab::Settings;
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        pal: &ThemePalette,
        backend: &Arc<Mutex<BackendState>>,
        snap: &mut SyncSnapshotDto,
    ) -> Option<SyncUiAction> {
        let mut action: Option<SyncUiAction> = None;

        egui::ScrollArea::vertical()
            .id_salt("sync_center_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(12.0);
                egui::Frame::none()
                    .inner_margin(Margin::symmetric(24.0, 8.0))
                    .show(ui, |ui| {
                        ui.set_max_width(960.0);

                        // 상단 헤더
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new("🔄 Onyx 동기화 센터")
                                            .size(21.0)
                                            .strong()
                                            .color(pal.text_primary),
                                    );
                                    ui::pill_badge(
                                        ui,
                                        &format!("v{}", snap.version),
                                        pal.accent_blue,
                                        pal.accent_blue_soft,
                                    );
                                });
                                ui.label(
                                    RichText::new("내 Obsidian 볼트를 Synology NAS, 클라우드 드라이브(OneDrive · Google Drive · 네이버 MYBOX/클라우드), 공유 폴더와 실시간 동기화합니다.")
                                        .size(12.5)
                                        .color(pal.text_secondary),
                                );
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button("❓ 설정 도움말").clicked() {
                                    let _ = open::that(SETUP_DOC);
                                }
                            });
                        });

                        ui.add_space(10.0);

                        // 메시지 배너
                        if let Some(msg) = &snap.message {
                            let (fg, bg) = if msg.error {
                                (pal.danger, pal.danger_bg)
                            } else {
                                (pal.ok, pal.ok_bg)
                            };
                            let mut clear = false;
                            egui::Frame::none()
                                .fill(bg)
                                .stroke(Stroke::new(1.0_f32, fg))
                                .rounding(Rounding::same(8.0))
                                .inner_margin(Margin::symmetric(14.0, 10.0))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(&msg.text).color(fg).strong());
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui.small_button("✕").clicked() {
                                                    clear = true;
                                                }
                                            },
                                        );
                                    });
                                });
                            if clear {
                                let mut st = backend.lock().unwrap();
                                st.sync.clear_message();
                                *snap = st.sync.snapshot();
                            }
                            ui.add_space(10.0);
                        }

                        // 서브탭 바
                        egui::Frame::none()
                            .fill(pal.bg_inset)
                            .stroke(Stroke::new(1.0_f32, pal.border_subtle))
                            .rounding(Rounding::same(8.0))
                            .inner_margin(Margin::same(4.0))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    for (tab, label) in [
                                        (SyncSubTab::Home, "🏠 홈 (상태 · 업데이트)"),
                                        (SyncSubTab::Settings, "⚙️ 프로필 및 저장소 설정"),
                                        (SyncSubTab::Logs, "📜 동기화 기록 (로그)"),
                                    ] {
                                        let active = self.sub_tab == tab;
                                        let btn = egui::Button::new(
                                            RichText::new(label)
                                                .size(13.5)
                                                .strong()
                                                .color(if active {
                                                    pal.accent_blue
                                                } else {
                                                    pal.text_secondary
                                                }),
                                        )
                                        .fill(if active {
                                            pal.bg_card
                                        } else {
                                            Color32::TRANSPARENT
                                        })
                                        .stroke(if active {
                                            Stroke::new(1.0_f32, pal.border_subtle)
                                        } else {
                                            Stroke::NONE
                                        })
                                        .rounding(Rounding::same(6.0))
                                        .min_size(egui::vec2(160.0, 30.0));
                                        if ui.add(btn).clicked() {
                                            self.sub_tab = tab;
                                        }
                                    }
                                });
                            });

                        ui.add_space(12.0);

                        match self.sub_tab {
                            SyncSubTab::Home => {
                                if let Some(act) = self.render_home_tab(ui, pal, backend, snap) {
                                    action = Some(act);
                                }
                            }
                            SyncSubTab::Settings => {
                                if let Some(act) = self.render_settings_tab(ui, pal, backend, snap)
                                {
                                    action = Some(act);
                                }
                            }
                            SyncSubTab::Logs => {
                                if let Some(act) = self.render_logs_tab(ui, pal, snap) {
                                    action = Some(act);
                                }
                            }
                        }

                        ui.add_space(28.0);
                    });
            });

        self.render_nas_browser_modal(ui.ctx(), pal, backend, snap);

        action
    }

    fn render_home_tab(
        &mut self,
        ui: &mut egui::Ui,
        pal: &ThemePalette,
        backend: &Arc<Mutex<BackendState>>,
        snap: &mut SyncSnapshotDto,
    ) -> Option<SyncUiAction> {
        let mut action: Option<SyncUiAction> = None;
        let st_owned = snap.status.clone();
        let st = st_owned.as_ref();
        let running = snap.service_running;
        let transitioning = snap.transitioning;

        let (dot_color, hero_title, hero_sub) = if running && st.is_some_and(|s| s.syncing) {
            (
                pal.accent_blue,
                "동기화 중…".to_string(),
                "변경된 파일을 주고받고 있습니다".to_string(),
            )
        } else if running && st.and_then(|s| s.last_error.as_ref()).is_some() {
            (
                pal.danger,
                "확인이 필요합니다".to_string(),
                st.and_then(|s| s.last_error.clone()).unwrap_or_default(),
            )
        } else if running {
            (
                pal.ok,
                "실시간 동기화 켜짐".to_string(),
                st.and_then(|s| s.last_sync.as_ref())
                    .map(|t| format!("마지막 동기화 {t}"))
                    .unwrap_or_else(|| "첫 동기화를 준비하고 있습니다".into()),
            )
        } else if transitioning {
            (
                pal.warn,
                "동기화 전환 중…".to_string(),
                "진행 중인 작업을 마친 뒤 요청을 처리합니다".to_string(),
            )
        } else if self.draft_cfg.profiles.is_empty() {
            (
                pal.warn,
                "설정이 필요합니다".to_string(),
                "동기화할 폴더(프로필)를 먼저 추가해 주세요".to_string(),
            )
        } else {
            (
                pal.text_muted,
                "동기화 꺼짐".to_string(),
                "시작 버튼을 누르면 실시간 동기화가 켜집니다".to_string(),
            )
        };

        let pending_deletes: usize = st
            .map(|s| s.profiles.iter().filter_map(|p| p.pending_deletes).sum())
            .unwrap_or(0);

        // 1. 상태 히어로 카드
        ui::card(ui, pal, |ui| {
            ui.horizontal(|ui| {
                ui::status_dot(ui, dot_color, 14.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(hero_title).size(19.0).strong());
                    ui.label(RichText::new(hero_sub).size(13.0).color(pal.text_secondary));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !running {
                        let btn = ui.add_enabled(
                            !transitioning,
                            egui::Button::new(
                                RichText::new("▶ 동기화 시작")
                                    .color(Color32::WHITE)
                                    .strong(),
                            )
                            .fill(pal.accent_blue)
                            .stroke(Stroke::NONE)
                            .rounding(Rounding::same(8.0))
                            .min_size(egui::vec2(120.0, 34.0)),
                        );
                        if btn.clicked() {
                            let dirty = self.is_dirty(snap);
                            let mut st_guard = backend.lock().unwrap();
                            if dirty {
                                let saved_ok =
                                    st_guard.sync.save_config(self.draft_cfg.clone(), true);
                                if saved_ok {
                                    let cfg_clone = st_guard.sync.cfg.clone();
                                    st_guard.workspace.state.sync_vaults_with_config(&cfg_clone);
                                    let _ = st_guard.workspace.save_and_refresh_watcher();
                                    action = Some(SyncUiAction::ConfigSaved);
                                }
                            } else {
                                st_guard.sync.start_service();
                            }
                            *snap = st_guard.sync.snapshot();
                            if !snap.service_running
                                && snap.message.as_ref().is_some_and(|m| m.error)
                            {
                                self.sub_tab = SyncSubTab::Settings;
                            }
                        }
                    } else {
                        if ui
                            .add(
                                egui::Button::new("⏸ 일시 중지")
                                    .rounding(Rounding::same(8.0))
                                    .min_size(egui::vec2(96.0, 34.0)),
                            )
                            .clicked()
                        {
                            let mut st_guard = backend.lock().unwrap();
                            st_guard.sync.stop_service();
                            *snap = st_guard.sync.snapshot();
                        }
                        let syncing_now = st.is_some_and(|s| s.syncing);
                        if ui
                            .add_enabled(
                                !syncing_now,
                                egui::Button::new(
                                    RichText::new("⟳ 지금 동기화")
                                        .color(Color32::WHITE)
                                        .strong(),
                                )
                                .fill(pal.accent_blue)
                                .stroke(Stroke::NONE)
                                .rounding(Rounding::same(8.0))
                                .min_size(egui::vec2(114.0, 34.0)),
                            )
                            .clicked()
                        {
                            let mut st_guard = backend.lock().unwrap();
                            st_guard.sync.sync_now();
                            *snap = st_guard.sync.snapshot();
                        }
                    }
                });
            });

            if pending_deletes > 0 {
                ui.add_space(10.0);
                egui::Frame::none()
                    .fill(pal.warn_bg)
                    .stroke(Stroke::new(1.0_f32, pal.warn))
                    .rounding(Rounding::same(8.0))
                    .inner_margin(Margin::same(12.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new("⚠ 대량 삭제를 보류했습니다.")
                                        .color(pal.warn)
                                        .strong(),
                                );
                                ui.label(
                                    RichText::new(format!(
                                        "모든 프로필의 보류된 삭제 합계({pending_deletes}개)입니다. 의도한 삭제인지 확인한 후 진행하세요."
                                    ))
                                    .size(12.5),
                                );
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui::primary_button(
                                        ui,
                                        pal,
                                        &format!("삭제 진행 ({pending_deletes}개)"),
                                    )
                                    .clicked()
                                    {
                                        let mut st_guard = backend.lock().unwrap();
                                        st_guard.sync.allow_deletes("");
                                        *snap = st_guard.sync.snapshot();
                                        action = Some(SyncUiAction::Toast(
                                            "보류된 삭제 작업을 승인했습니다.".into(),
                                            false,
                                        ));
                                    }
                                },
                            );
                        });
                    });
            }
        });

        ui.add_space(12.0);

        // 2. 동기화 프로필 목록 카드
        let st_clone = snap.status.clone();
        ui::card(ui, pal, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "연결된 Obsidian 볼트 ({})",
                        self.draft_cfg.profiles.len()
                    ))
                    .size(15.5)
                    .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("＋ Obsidian 볼트 추가").clicked() {
                        self.create_profile(backend, ProfileKind::Obsidian);
                    }
                });
            });
            ui.add_space(8.0);

            if self.draft_cfg.profiles.is_empty() {
                ui.label(
                    RichText::new(
                        "아직 등록된 동기화 프로필이 없습니다. 위의 버튼으로 Obsidian 볼트를 추가해 보세요.",
                    )
                    .color(pal.text_muted),
                );
            } else {
                let mut disconnect_idx: Option<usize> = None;
                let profiles_snapshot = self.draft_cfg.profiles.clone();
                for (idx, p) in profiles_snapshot.iter().enumerate() {
                    let pst = st_clone
                        .as_ref()
                        .and_then(|s| s.profiles.iter().find(|x| x.id == p.id));
                    let (p_dot, status_line, is_err) = if !p.enabled {
                        (pal.text_muted, "꺼짐".to_string(), false)
                    } else if pst.is_some_and(|x| x.syncing) {
                        (pal.accent_blue, "동기화 중…".to_string(), false)
                    } else if let Some(err) = pst.and_then(|x| x.last_error.as_ref()) {
                        (
                            pal.danger,
                            err.lines().next().unwrap_or(err).to_string(),
                            true,
                        )
                    } else if let Some(ps) = pst {
                        let s = ps
                            .last_sync
                            .as_ref()
                            .map(|t| format!("{} · {}", t.get(5..).unwrap_or(t), ps.last_summary))
                            .unwrap_or_else(|| "준비 중".into());
                        (pal.ok, s, false)
                    } else {
                        (pal.text_muted, "대기".to_string(), false)
                    };

                    ui::sub_card(ui, pal, |ui| {
                        ui.horizontal(|ui| {
                            ui::status_dot(ui, p_dot, 9.0);
                            ui.label(RichText::new(p.display_name()).strong().size(14.5));
                            ui.label(
                                RichText::new(format!("Obsidian · {}", p.mode.display_name()))
                                    .size(12.0)
                                    .color(pal.text_muted),
                            );

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .small_button("🔌 연결 해제")
                                        .on_hover_text(
                                            "이 볼트 동기화 연결을 해제합니다 (PC 파일은 유지됨)",
                                        )
                                        .clicked()
                                    {
                                        disconnect_idx = Some(idx);
                                    }
                                    if ui.button("설정").clicked() {
                                        self.selected_idx = idx;
                                        self.confirm_delete = false;
                                        self.sub_tab = SyncSubTab::Settings;
                                    }
                                    if !p.local_path.trim().is_empty()
                                        && ui.button("폴더 열기").clicked()
                                    {
                                        let _ = open::that(p.local_path.trim());
                                    }
                                    if p.kind == ProfileKind::Obsidian
                                        && !p.local_path.trim().is_empty()
                                        && ui::primary_blue_button(ui, pal, "📝 노트 열기")
                                            .clicked()
                                    {
                                        action = Some(SyncUiAction::OpenVaultInEditor(
                                            p.local_path.trim().to_string(),
                                        ));
                                    }
                                    if let Some(del_cnt) = pst.and_then(|x| x.pending_deletes) {
                                        if ui::danger_button(
                                            ui,
                                            pal,
                                            &format!("삭제 승인 ({del_cnt})"),
                                        )
                                        .clicked()
                                        {
                                            let mut st_guard = backend.lock().unwrap();
                                            st_guard.sync.allow_deletes(&p.id);
                                            *snap = st_guard.sync.snapshot();
                                        }
                                    }
                                },
                            );
                        });

                        ui.add_space(2.0);
                        let local_disp = if p.local_path.trim().is_empty() {
                            "(내 PC 폴더 미선택)"
                        } else {
                            p.local_path.trim()
                        };
                        ui.label(
                            RichText::new(format!("{local_disp}  ⇄  {}", p.backend.summary()))
                                .size(12.5)
                                .color(pal.text_secondary),
                        );
                        ui.label(RichText::new(status_line).size(12.0).color(if is_err {
                            pal.danger
                        } else {
                            pal.text_muted
                        }));
                    });
                    ui.add_space(6.0);
                }
                if let Some(d_idx) = disconnect_idx {
                    if d_idx < self.draft_cfg.profiles.len() {
                        let removed = self.draft_cfg.profiles.remove(d_idx);
                        let removed_name = removed.display_name();
                        self.ignore_texts.remove(&removed.id);
                        self.selected_idx = self.selected_idx.saturating_sub(1);
                        let mut st_guard = backend.lock().unwrap();
                        let has_service = st_guard.sync.service.is_some();
                        st_guard.sync.reset_profile(removed.id);
                        let saved_ok = st_guard.sync.save_config(
                            self.draft_cfg.clone(),
                            !self.draft_cfg.profiles.is_empty() && has_service,
                        );
                        if saved_ok {
                            let cfg_clone = st_guard.sync.cfg.clone();
                            st_guard.workspace.state.sync_vaults_with_config(&cfg_clone);
                            let _ = st_guard.workspace.save_and_refresh_watcher();
                        }
                        *snap = st_guard.sync.snapshot();
                        action = Some(SyncUiAction::Toast(
                            format!(
                                "'{removed_name}' 동기화 연결을 해제했습니다 (로컬 파일 유지)."
                            ),
                            false,
                        ));
                    }
                }
            }

            let watch_summary = if self.draft_cfg.watch_local {
                if self.draft_cfg.quick_check_secs > 0 {
                    format!(
                        "저장 즉시 업로드 · 다른 기기 변경 {}초마다 확인",
                        self.draft_cfg.quick_check_secs.max(2)
                    )
                } else {
                    format!(
                        "저장 즉시 업로드 · {}초마다 원격 확인",
                        self.draft_cfg.poll_interval_secs
                    )
                }
            } else {
                format!("{}초마다 확인", self.draft_cfg.poll_interval_secs)
            };
            ui.label(
                RichText::new(watch_summary)
                    .size(12.0)
                    .color(pal.text_muted),
            );
        });

        ui.add_space(12.0);

        // 3. PC 자동 실행 및 앱 자동 업데이트 카드
        let updater = backend.lock().unwrap().updater.clone();
        let up_status = updater.status();
        ui::card(ui, pal, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("🚀 실행 환경 및 앱 업데이트")
                        .size(15.5)
                        .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !up_status.checking && !up_status.downloading,
                            egui::Button::new("🔍 업데이트 확인"),
                        )
                        .clicked()
                    {
                        updater.check_async(true);
                    }
                    if up_status.ready_to_install || up_status.available_tag.is_some() {
                        let tag_lbl = up_status.available_tag.as_deref().unwrap_or("최신");
                        if ui::primary_button(
                            ui,
                            pal,
                            &format!("⬆️ {tag_lbl} 지금 업데이트 후 재시작"),
                        )
                        .clicked()
                        {
                            action = Some(SyncUiAction::ApplyUpdateAndRestart);
                        }
                    }
                });
            });
            ui.add_space(6.0);

            let mut auto_start = snap.autostart;
            if ui
                .checkbox(
                    &mut auto_start,
                    "Windows 시작 시 자동 실행 (시스템 트레이에서 조용히 실행)",
                )
                .changed()
            {
                let mut st_guard = backend.lock().unwrap();
                st_guard.sync.set_autostart(auto_start);
                *snap = st_guard.sync.snapshot();
            }

            let mut auto_upd = up_status.auto_update;
            if ui
                .checkbox(
                    &mut auto_upd,
                    "GitHub 새 릴리즈 자동 다운로드 및 트레이 대기 시 무중단 업데이트",
                )
                .changed()
            {
                updater.set_auto_update(auto_upd);
            }

            let status_txt = if !up_status.message.is_empty() {
                up_status.message.clone()
            } else {
                format!(
                    "현재 버전: v{} (순수 네이티브 Rust · Zero WebView2)",
                    up_status.current_version
                )
            };
            ui.label(
                RichText::new(status_txt)
                    .size(12.0)
                    .color(if up_status.error.is_some() {
                        pal.danger
                    } else if up_status.ready_to_install {
                        pal.ok
                    } else {
                        pal.text_muted
                    }),
            );
        });

        ui.add_space(12.0);

        // 4. 최근 활동 로그 10건 카드
        let recent_logs: Vec<_> = snap
            .status
            .as_ref()
            .map(|s| s.logs.iter().rev().take(10).cloned().collect())
            .unwrap_or_default();

        ui::card(ui, pal, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("최근 활동").size(15.5).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("전체 기록 보기").clicked() {
                        self.sub_tab = SyncSubTab::Logs;
                    }
                });
            });
            ui.add_space(6.0);
            if recent_logs.is_empty() {
                ui.label(RichText::new("아직 기록이 없습니다.").color(pal.text_muted));
            } else {
                for l in recent_logs {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(&l.time)
                                .monospace()
                                .size(12.0)
                                .color(pal.text_muted),
                        );
                        ui.label(RichText::new(&l.text).size(12.5).color(if l.error {
                            pal.danger
                        } else {
                            pal.text_primary
                        }));
                    });
                }
            }
        });

        ui.add_space(8.0);
        ui.label(
            RichText::new(
                "충돌이 나면 두 버전을 모두 남깁니다 (예: 노트 (충돌 PC 2025-01-01 120000).md). 다른 곳에서 지워진 파일은 폴더 안 .trash 로 옮겨집니다.",
            )
            .size(12.0)
            .color(pal.text_muted),
        );

        action
    }

    fn render_settings_tab(
        &mut self,
        ui: &mut egui::Ui,
        pal: &ThemePalette,
        backend: &Arc<Mutex<BackendState>>,
        snap: &mut SyncSnapshotDto,
    ) -> Option<SyncUiAction> {
        let mut action: Option<SyncUiAction> = None;

        // 1. 프로필 선택 카드
        ui::card(ui, pal, |ui| {
            ui.label(RichText::new("동기화 프로필").size(15.5).strong());
            ui.label(
                RichText::new(
                    "프로필 하나가 '내 PC 폴더 ↔ 저장소 폴더' 1:1 연결입니다. Obsidian 볼트, 사진 폴더 등을 따로 만들 수 있습니다.",
                )
                .size(12.5)
                .color(pal.text_secondary),
            );
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                for i in 0..self.draft_cfg.profiles.len() {
                    let selected = i == self.selected_idx;
                    let name = self.draft_cfg.profiles[i].display_name();
                    let enabled = self.draft_cfg.profiles[i].enabled;
                    let label = if enabled {
                        name
                    } else {
                        format!("{name} (꺼짐)")
                    };
                    let btn = egui::Button::new(RichText::new(label).strong().color(if selected {
                        Color32::WHITE
                    } else {
                        pal.text_primary
                    }))
                    .fill(if selected {
                        pal.accent_blue
                    } else {
                        pal.bg_inset
                    })
                    .stroke(Stroke::NONE)
                    .rounding(Rounding::same(99.0));
                    if ui.add(btn).clicked() && self.selected_idx != i {
                        self.selected_idx = i;
                        self.confirm_delete = false;
                        self.otp_input.clear();
                    }
                }
                if ui.button("＋ Obsidian 볼트 추가").clicked() {
                    self.create_profile(backend, ProfileKind::Obsidian);
                }
            });
        });

        ui.add_space(10.0);

        if self.draft_cfg.profiles.is_empty() {
            ui::card(ui, pal, |ui| {
                ui.label(
                    RichText::new("위의 '＋' 버튼으로 동기화할 폴더를 추가하세요.")
                        .color(pal.text_muted),
                );
            });
        } else {
            self.selected_idx = self
                .selected_idx
                .min(self.draft_cfg.profiles.len().saturating_sub(1));
            if let Some(act) = self.render_profile_editor(ui, pal, backend, snap) {
                action = Some(act);
            }
        }

        ui.add_space(10.0);

        let dirty = self.is_dirty(snap);
        ui::card(ui, pal, |ui| {
            ui.horizontal_wrapped(|ui| {
                let start_label = if snap.service_running {
                    "저장 후 다시 시작"
                } else {
                    "저장하고 동기화 시작"
                };
                if ui
                    .add_enabled(
                        !snap.transitioning,
                        egui::Button::new(
                            RichText::new(start_label).color(Color32::WHITE).strong(),
                        )
                        .fill(pal.accent_blue)
                        .stroke(Stroke::NONE)
                        .rounding(Rounding::same(8.0))
                        .min_size(egui::vec2(150.0, 34.0)),
                    )
                    .clicked()
                {
                    let mut st_guard = backend.lock().unwrap();
                    let ok = st_guard.sync.save_config(self.draft_cfg.clone(), true);
                    if ok {
                        let cfg_clone = st_guard.sync.cfg.clone();
                        st_guard.workspace.state.sync_vaults_with_config(&cfg_clone);
                        let _ = st_guard.workspace.save_and_refresh_watcher();
                        action = Some(SyncUiAction::ConfigSaved);
                    }
                    *snap = st_guard.sync.snapshot();
                    if snap.service_running && !snap.message.as_ref().is_some_and(|m| m.error) {
                        self.sub_tab = SyncSubTab::Home;
                    }
                }

                if ui
                    .add_enabled(
                        dirty,
                        egui::Button::new("저장만 하기")
                            .rounding(Rounding::same(8.0))
                            .min_size(egui::vec2(100.0, 34.0)),
                    )
                    .clicked()
                {
                    let mut st_guard = backend.lock().unwrap();
                    let ok = st_guard.sync.save_config(self.draft_cfg.clone(), false);
                    if ok {
                        let cfg_clone = st_guard.sync.cfg.clone();
                        st_guard.workspace.state.sync_vaults_with_config(&cfg_clone);
                        let _ = st_guard.workspace.save_and_refresh_watcher();
                        action = Some(SyncUiAction::ConfigSaved);
                    }
                    *snap = st_guard.sync.snapshot();
                }

                if dirty {
                    ui.label(
                        RichText::new("● 저장하지 않은 변경 사항이 있습니다")
                            .color(pal.warn)
                            .strong(),
                    );
                } else {
                    ui.label(RichText::new("모든 설정이 저장되어 있습니다").color(pal.text_muted));
                }
            });
        });

        ui.add_space(10.0);

        // 고급 설정 (모든 프로필 공통)
        ui::card(ui, pal, |ui| {
            let toggle_txt = if self.advanced_open {
                "⚙ 고급 설정 (모든 프로필 공통)  ▲"
            } else {
                "⚙ 고급 설정 (모든 프로필 공통)  ▼"
            };
            if ui
                .add(
                    egui::Button::new(RichText::new(toggle_txt).strong())
                        .fill(Color32::TRANSPARENT)
                        .stroke(Stroke::NONE),
                )
                .clicked()
            {
                self.advanced_open = !self.advanced_open;
            }
            if self.advanced_open {
                ui.add_space(8.0);
                ui.checkbox(
                    &mut self.draft_cfg.watch_local,
                    "저장하자마자 바로 업로드 (실시간 파일 감시)",
                );
                ui.checkbox(
                    &mut self.draft_cfg.start_on_launch,
                    "앱을 켜면 자동으로 동기화 시작",
                );
                let mut auto = snap.autostart;
                if ui
                    .checkbox(
                        &mut auto,
                        "Windows 시작 시 자동 실행 (트레이에서 조용히 실행)",
                    )
                    .changed()
                {
                    let mut st_guard = backend.lock().unwrap();
                    st_guard.sync.set_autostart(auto);
                    *snap = st_guard.sync.snapshot();
                }
                ui.add_space(6.0);
                egui::Grid::new("sync_adv_grid")
                    .num_columns(2)
                    .spacing([14.0, 8.0])
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("다른 기기 변경 확인(초)").color(pal.text_secondary),
                        );
                        ui.add(
                            egui::DragValue::new(&mut self.draft_cfg.quick_check_secs)
                                .range(0..=600)
                                .suffix("초"),
                        );
                        ui.end_row();

                        ui.label(
                            RichText::new("전체 목록 확인 주기(초)").color(pal.text_secondary),
                        );
                        ui.add(
                            egui::DragValue::new(&mut self.draft_cfg.poll_interval_secs)
                                .range(10..=3600)
                                .suffix("초"),
                        );
                        ui.end_row();

                        ui.label(RichText::new("저장 후 대기(ms)").color(pal.text_secondary));
                        ui.add(
                            egui::DragValue::new(&mut self.draft_cfg.debounce_ms)
                                .range(200..=60_000)
                                .speed(50)
                                .suffix("ms"),
                        );
                        ui.end_row();

                        ui.label(RichText::new("기기 이름").color(pal.text_secondary));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.draft_cfg.device_name)
                                .hint_text(&snap.default_device_name)
                                .desired_width(240.0),
                        );
                        ui.end_row();
                    });
                ui.add_space(8.0);
                if ui.button("📂 설정 데이터 폴더 열기").clicked() {
                    let dir = backend.lock().unwrap().sync.data_dir.clone();
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = open::that(&dir);
                }
            }
        });

        action
    }

    fn render_profile_editor(
        &mut self,
        ui: &mut egui::Ui,
        pal: &ThemePalette,
        backend: &Arc<Mutex<BackendState>>,
        snap: &mut SyncSnapshotDto,
    ) -> Option<SyncUiAction> {
        let mut action: Option<SyncUiAction> = None;
        let idx = self.selected_idx;

        // ① 이름과 종류
        let step1_done = !self.draft_cfg.profiles[idx].name.trim().is_empty();
        ui::step_card(ui, pal, 1, "이름과 종류", step1_done, |ui| {
            let p = &mut self.draft_cfg.profiles[idx];
            egui::Grid::new("step1_grid")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("프로필 이름").color(pal.text_secondary));
                    ui.add(
                        egui::TextEdit::singleline(&mut p.name)
                            .hint_text("예: Obsidian, 사진")
                            .desired_width(320.0),
                    );
                    ui.end_row();

                    ui.label(RichText::new("종류").color(pal.text_secondary));
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut p.kind, ProfileKind::Obsidian, "Obsidian 볼트");
                        ui.radio_value(
                            &mut p.kind,
                            ProfileKind::Folder,
                            "일반 폴더 (사진·문서 등)",
                        );
                    });
                    ui.end_row();

                    ui.label(RichText::new("활성화").color(pal.text_secondary));
                    ui.checkbox(&mut p.enabled, "이 프로필 동기화하기");
                    ui.end_row();
                });
        });

        ui.add_space(10.0);

        // ② 내 PC Obsidian 볼트 폴더
        let step2_done = !self.draft_cfg.profiles[idx].local_path.trim().is_empty();
        ui::step_card(ui, pal, 2, "내 PC 볼트 폴더", step2_done, |ui| {
            let pid_for_reset = self.draft_cfg.profiles[idx].id.clone();
            let p = &mut self.draft_cfg.profiles[idx];
            ui.label(
                RichText::new(
                    "Obsidian 볼트(보관함) 폴더를 선택하세요. 폴더 변경 시 이전 동기화 상태 기록이 자동으로 초기화되어 안전하게 새 볼트로 전환됩니다.",
                )
                .size(12.5)
                .color(pal.text_secondary),
            );
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut p.local_path)
                        .hint_text(r"C:\Users\나\Documents\MyVault")
                        .desired_width((ui.available_width() - 210.0).max(160.0)),
                );
                if ui.button("📂 폴더 변경…").clicked() {
                    if let Some(picked) = platform::pick_folder() {
                        let changed = p.local_path.trim() != picked.trim();
                        if p.name.trim().is_empty() || p.name == "새 폴더" || changed {
                            if let Some(last) = picked
                                .trim_end_matches(['/', '\\'])
                                .rsplit(['/', '\\'])
                                .next()
                            {
                                p.name = last.to_string();
                            }
                        }
                        p.local_path = picked;
                        if changed {
                            backend
                                .lock()
                                .unwrap()
                                .sync
                                .reset_profile(pid_for_reset.clone());
                        }
                    }
                }
                if !p.local_path.trim().is_empty()
                    && ui
                        .button("🔌 연결 해제")
                        .on_hover_text("연결된 로컬 볼트 폴더 경로를 지우고 상태를 초기화합니다")
                        .clicked()
                {
                    p.local_path.clear();
                    backend.lock().unwrap().sync.reset_profile(pid_for_reset);
                }
            });
        });

        ui.add_space(10.0);

        // ③ 저장소
        let step3_done = self.draft_cfg.profiles[idx].backend.validate().is_ok();
        let mut open_nas_browser_for: Option<(BackendConfig, String)> = None;

        ui::step_card(ui, pal, 3, "저장소", step3_done, |ui| {
            let p_id = self.draft_cfg.profiles[idx].id.clone();
            let detected_top = self.detected_clouds.clone();
            if !detected_top.is_empty() {
                ui::sub_card(ui, pal, |ui| {
                    ui.label(
                        RichText::new("☁️ API 설정 없이 PC 클라우드 드라이브로 1클릭 간편 연결")
                            .strong()
                            .size(12.5)
                            .color(pal.accent_blue),
                    );
                    ui.label(
                        RichText::new(
                            "OneDrive · Google Drive · Synology Drive · 네이버 MYBOX가 PC에 설치되어 있으면 복잡한 OAuth/API 키 없이 바로 동기화할 수 있습니다.",
                        )
                        .size(11.5)
                        .color(pal.text_secondary),
                    );
                    ui.horizontal_wrapped(|ui| {
                        for dc in &detected_top {
                            if ui
                                .small_button(format!("☁️ {} 바로 연결", dc.label))
                                .clicked()
                            {
                                let target =
                                    sync_controller::prepare_folder_target(&dc.path, "OnyxVault")
                                        .unwrap_or_else(|_| dc.path.clone());
                                self.draft_cfg.profiles[idx].backend =
                                    BackendConfig::LocalFolder { path: target };
                            }
                        }
                    });
                });
                ui.add_space(8.0);
            }
            let cur_backend_kind = self.draft_cfg.profiles[idx].backend.kind();
            let cur_kind = cur_backend_kind;
            let card_w = ((ui.available_width() - 10.0) / 2.0).max(200.0);

            egui::Grid::new("backend_choice_grid")
                .num_columns(2)
                .spacing([10.0, 8.0])
                .show(ui, |ui| {
                    for (i, (k, badge, color, name, desc)) in BACKENDS.iter().enumerate() {
                        if ui::backend_choice_card(
                            ui,
                            pal,
                            card_w,
                            badge,
                            *color,
                            name,
                            desc,
                            cur_kind == *k,
                        )
                        .clicked()
                            && cur_kind != *k
                        {
                            let old = self.draft_cfg.profiles[idx].backend.clone();
                            self.backend_cache.insert((p_id.clone(), cur_kind), old);
                            let restored = self
                                .backend_cache
                                .get(&(p_id.clone(), *k))
                                .cloned()
                                .unwrap_or_else(|| default_backend(k));
                            self.draft_cfg.profiles[idx].backend = restored;
                        }
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });

            ui.add_space(10.0);
            let show_secret = self.show_secret;
            let detected_clouds = self.detected_clouds.clone();
            let p = &mut self.draft_cfg.profiles[idx];

            match &mut p.backend {
                BackendConfig::Synology {
                    url,
                    username,
                    password,
                    folder,
                } => {
                    egui::Grid::new("syn_fields_grid")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            ui.label(RichText::new("NAS 주소").color(pal.text_secondary));
                            ui.add(
                                egui::TextEdit::singleline(url)
                                    .hint_text("https://내NAS주소:5001")
                                    .desired_width(380.0),
                            );
                            ui.end_row();

                            ui.label(RichText::new("아이디").color(pal.text_secondary));
                            ui.add(
                                egui::TextEdit::singleline(username)
                                    .hint_text("DSM 로그인 아이디")
                                    .desired_width(380.0),
                            );
                            ui.end_row();

                            ui.label(RichText::new("비밀번호").color(pal.text_secondary));
                            ui.add(
                                egui::TextEdit::singleline(password)
                                    .password(!show_secret)
                                    .desired_width(380.0),
                            );
                            ui.end_row();

                            ui.label(RichText::new("NAS 폴더").color(pal.text_secondary));
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(folder)
                                        .hint_text("/home/Obsidian")
                                        .desired_width(260.0),
                                );
                                let can_browse =
                                    !url.trim().is_empty() && !username.trim().is_empty();
                                if ui
                                    .add_enabled(can_browse, egui::Button::new("NAS에서 고르기…"))
                                    .clicked()
                                {
                                    open_nas_browser_for = Some((
                                        BackendConfig::Synology {
                                            url: url.clone(),
                                            username: username.clone(),
                                            password: password.clone(),
                                            folder: folder.clone(),
                                        },
                                        folder.clone(),
                                    ));
                                }
                            });
                            ui.end_row();
                        });
                    ui.label(
                        RichText::new(
                            "[NAS에서 고르기…]로 목록에서 고르면 오타가 없습니다. 직접 적을 때는 공유 폴더 이름부터 적습니다 (예: /home/Obsidian).",
                        )
                        .size(11.5)
                        .color(pal.text_muted),
                    );
                }
                BackendConfig::LocalFolder { path } => {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("저장 위치").color(pal.text_secondary));
                        ui.add(
                            egui::TextEdit::singleline(path)
                                .hint_text(r"\\NAS\home\Obsidian 또는 D:\SynologyDrive\OnyxVault")
                                .desired_width(ui.available_width() - 105.0),
                        );
                        if ui.button("폴더 선택…").clicked() {
                            if let Some(picked) = platform::pick_folder() {
                                *path = picked;
                            }
                        }
                    });
                    if !detected_clouds.is_empty() {
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new("탐지된 드라이브 빠른 선택:")
                                    .size(11.5)
                                    .color(pal.text_muted),
                            );
                            for dc in &detected_clouds {
                                if ui.small_button(format!("☁️ {}", dc.label)).clicked() {
                                    if let Ok(ready) = sync_controller::prepare_folder_target(
                                        &dc.path,
                                        "OnyxVault",
                                    ) {
                                        *path = ready;
                                    } else {
                                        *path = dc.path.clone();
                                    }
                                }
                            }
                        });
                    }
                }
                BackendConfig::S3 {
                    endpoint,
                    region,
                    bucket,
                    prefix,
                    access_key,
                    secret_key,
                } => {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("빠른 프리셋:")
                                .size(12.0)
                                .color(pal.text_muted),
                        );
                        if ui.small_button("네이버 클라우드").clicked() {
                            *endpoint = "https://kr.object.ncloudstorage.com".into();
                            *region = "kr-standard".into();
                        }
                        if ui.small_button("Google Cloud Storage").clicked() {
                            *endpoint = "https://storage.googleapis.com".into();
                            *region = "auto".into();
                        }
                    });
                    egui::Grid::new("s3_fields_grid")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            ui.label(RichText::new("엔드포인트").color(pal.text_secondary));
                            ui.add(
                                egui::TextEdit::singleline(endpoint)
                                    .hint_text("https://kr.object.ncloudstorage.com")
                                    .desired_width(380.0),
                            );
                            ui.end_row();

                            ui.label(RichText::new("리전").color(pal.text_secondary));
                            ui.add(
                                egui::TextEdit::singleline(region)
                                    .hint_text("kr-standard")
                                    .desired_width(380.0),
                            );
                            ui.end_row();

                            ui.label(RichText::new("버킷 이름").color(pal.text_secondary));
                            ui.add(
                                egui::TextEdit::singleline(bucket)
                                    .hint_text("my-obsidian")
                                    .desired_width(380.0),
                            );
                            ui.end_row();

                            ui.label(RichText::new("저장 폴더(Prefix)").color(pal.text_secondary));
                            ui.add(
                                egui::TextEdit::singleline(prefix)
                                    .hint_text("obsidian")
                                    .desired_width(380.0),
                            );
                            ui.end_row();

                            ui.label(RichText::new("Access Key").color(pal.text_secondary));
                            ui.add(egui::TextEdit::singleline(access_key).desired_width(380.0));
                            ui.end_row();

                            ui.label(RichText::new("Secret Key").color(pal.text_secondary));
                            ui.add(
                                egui::TextEdit::singleline(secret_key)
                                    .password(!show_secret)
                                    .desired_width(380.0),
                            );
                            ui.end_row();
                        });
                }
                BackendConfig::OneDrive {
                    client_id,
                    tenant,
                    folder,
                } => {
                    egui::Grid::new("od_fields_grid")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            ui.label(RichText::new("앱 Client ID").color(pal.text_secondary));
                            ui.add(
                                egui::TextEdit::singleline(client_id)
                                    .hint_text("Azure 앱 등록에서 받은 애플리케이션 ID")
                                    .desired_width(380.0),
                            );
                            ui.end_row();

                            ui.label(RichText::new("테넌트(Tenant)").color(pal.text_secondary));
                            ui.add(
                                egui::TextEdit::singleline(tenant)
                                    .hint_text("common")
                                    .desired_width(380.0),
                            );
                            ui.end_row();

                            ui.label(RichText::new("OneDrive 폴더").color(pal.text_secondary));
                            ui.add(
                                egui::TextEdit::singleline(folder)
                                    .hint_text("ObsidianSync")
                                    .desired_width(380.0),
                            );
                            ui.end_row();
                        });
                }
                BackendConfig::GoogleDrive {
                    client_id,
                    client_secret,
                    folder,
                } => {
                    egui::Grid::new("gd_fields_grid")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            ui.label(RichText::new("Client ID").color(pal.text_secondary));
                            ui.add(
                                egui::TextEdit::singleline(client_id)
                                    .hint_text("OAuth 클라이언트 ID (TV 및 입력 제한 기기)")
                                    .desired_width(380.0),
                            );
                            ui.end_row();

                            ui.label(RichText::new("Client Secret").color(pal.text_secondary));
                            ui.add(
                                egui::TextEdit::singleline(client_secret)
                                    .password(!show_secret)
                                    .desired_width(380.0),
                            );
                            ui.end_row();

                            ui.label(RichText::new("Drive 폴더").color(pal.text_secondary));
                            ui.add(
                                egui::TextEdit::singleline(folder)
                                    .hint_text("ObsidianSync")
                                    .desired_width(380.0),
                            );
                            ui.end_row();
                        });
                }
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.show_secret, "비밀번호 / 시크릿 키 표시");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let help_url =
                        sync_controller::backend_help(&self.draft_cfg.profiles[idx].backend);
                    if ui.button("❓ 이 저장소 설정 방법 보기").clicked() {
                        let _ = open::that(help_url);
                    }
                });
            });
        });

        if let Some((b_cfg, init_folder)) = open_nas_browser_for {
            let mut st_guard = backend.lock().unwrap();
            st_guard.sync.nas_browse_open(b_cfg, init_folder);
            *snap = st_guard.sync.snapshot();
        }

        ui.add_space(10.0);

        // ④ 연결 확인 (OAuth 디바이스 코드 + 보안망용 수동 토큰 입력 + 연결 테스트)
        let cur_backend = self.draft_cfg.profiles[idx].backend.clone();
        let cur_pid = self.draft_cfg.profiles[idx].id.clone();
        let extra = snap
            .profile_extras
            .iter()
            .find(|x| x.id == cur_pid)
            .cloned();
        let needs_oauth = cur_backend.needs_oauth();
        let data_dir = backend.lock().unwrap().sync.data_dir.clone();
        let oauth_logged_in =
            !needs_oauth || obsidian_sync_core::auth::is_logged_in(&data_dir, &cur_backend);
        let backend_ok = cur_backend.validate().is_ok();
        let step4_done =
            oauth_logged_in && snap.test_job.as_ref().is_some_and(|j| j.status == "ok");

        ui::step_card(ui, pal, 4, "연결 확인", step4_done, |ui| {
            if needs_oauth {
                if let Some(oj) = snap.oauth_job.clone() {
                    match oj.status.as_str() {
                        "requesting" => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("로그인 코드 발급 중…");
                            });
                        }
                        "code" => {
                            ui::sub_card(ui, pal, |ui| {
                                ui.label("브라우저가 열리면 아래 코드를 입력하고 계정을 승인해 주세요 (인바운드 포트 미사용):");
                                if let Some(code) = &oj.user_code {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(code)
                                                .size(20.0)
                                                .strong()
                                                .monospace()
                                                .color(pal.accent_blue),
                                        );
                                        if ui.button("📋 코드 복사").clicked() {
                                            ui.output_mut(|o| o.copied_text = code.clone());
                                            action = Some(SyncUiAction::Toast(
                                                "인증 코드를 복사했습니다.".into(),
                                                false,
                                            ));
                                        }
                                    });
                                }
                                if let Some(uri) = &oj.uri {
                                    if ui
                                        .hyperlink_to(format!("브라우저에서 열기: {uri}"), uri)
                                        .clicked()
                                    {
                                        let _ = open::that(uri);
                                    }
                                }
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(
                                        RichText::new("승인을 기다리는 중…")
                                            .size(12.5)
                                            .color(pal.text_secondary),
                                    );
                                    if ui.button("취소").clicked() {
                                        let mut st_guard = backend.lock().unwrap();
                                        st_guard.sync.oauth_cancel();
                                        *snap = st_guard.sync.snapshot();
                                    }
                                });
                            });
                        }
                        _ => {}
                    }
                }

                if oauth_logged_in {
                    ui.horizontal(|ui| {
                        ui::status_dot(ui, pal.ok, 9.0);
                        ui.label(RichText::new("계정 연결됨").strong());
                        if ui.button("다시 로그인").clicked() {
                            let mut st_guard = backend.lock().unwrap();
                            st_guard.sync.oauth_start(cur_backend.clone());
                            *snap = st_guard.sync.snapshot();
                        }
                        if ui.button("로그아웃").clicked() {
                            let mut st_guard = backend.lock().unwrap();
                            st_guard.sync.oauth_logout(&cur_backend);
                            *snap = st_guard.sync.snapshot();
                        }
                        if ui.small_button("🔐 수동 토큰 입력").clicked() {
                            self.show_manual_oauth = !self.show_manual_oauth;
                        }
                    });
                } else {
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                backend_ok,
                                egui::Button::new(
                                    RichText::new("🔑 디바이스 코드 로그인")
                                        .color(Color32::WHITE)
                                        .strong(),
                                )
                                .fill(pal.accent_blue)
                                .stroke(Stroke::NONE)
                                .rounding(Rounding::same(7.0)),
                            )
                            .clicked()
                        {
                            let mut st_guard = backend.lock().unwrap();
                            st_guard.sync.oauth_start(cur_backend.clone());
                            *snap = st_guard.sync.snapshot();
                        }
                        if ui.button("🔐 수동 토큰 직접 입력 (보안망용)").clicked() {
                            self.show_manual_oauth = !self.show_manual_oauth;
                        }
                        ui.label(
                            RichText::new(if backend_ok {
                                "로컬 포트 리스닝 없이 아웃바운드 HTTPS로 인증합니다"
                            } else {
                                "위 저장소 항목을 먼저 입력하세요"
                            })
                            .size(12.0)
                            .color(pal.text_secondary),
                        );
                    });
                }

                if self.show_manual_oauth {
                    ui.add_space(6.0);
                    ui::sub_card(ui, pal, |ui| {
                        ui.label(
                            RichText::new(
                                "🔐 수동 OAuth 토큰 등록 (내부 보안망/브라우저 격리 환경)",
                            )
                            .strong()
                            .size(13.0),
                        );
                        ui.label(
                            RichText::new("외부 기기에서 발급받은 Access Token 및 Refresh Token(선택)을 직접 등록합니다.")
                                .size(12.0)
                                .color(pal.text_secondary),
                        );
                        egui::Grid::new("manual_oauth_grid")
                            .num_columns(2)
                            .spacing([10.0, 6.0])
                            .show(ui, |ui| {
                                ui.label("Access Token");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.manual_access_token)
                                        .password(!self.show_secret)
                                        .desired_width(340.0),
                                );
                                ui.end_row();

                                ui.label("Refresh Token");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.manual_refresh_token)
                                        .password(!self.show_secret)
                                        .hint_text("선택 사항 (자동 갱신용)")
                                        .desired_width(340.0),
                                );
                                ui.end_row();
                            });
                        ui.horizontal(|ui| {
                            if ui::primary_button(ui, pal, "토큰 저장").clicked() {
                                let mut st_guard = backend.lock().unwrap();
                                match st_guard.sync.oauth_save_manual_token(
                                    &cur_backend,
                                    &self.manual_access_token,
                                    Some(&self.manual_refresh_token),
                                ) {
                                    Ok(()) => {
                                        self.manual_access_token.clear();
                                        self.manual_refresh_token.clear();
                                        self.show_manual_oauth = false;
                                    }
                                    Err(e) => {
                                        st_guard.sync.set_msg(format!("{e:#}"), true);
                                    }
                                }
                                *snap = st_guard.sync.snapshot();
                            }
                            if ui.button("닫기").clicked() {
                                self.show_manual_oauth = false;
                            }
                        });
                    });
                }
            }

            if matches!(cur_backend, BackendConfig::Synology { .. })
                && extra.as_ref().is_some_and(|e| e.synology_device_remembered)
            {
                ui.horizontal(|ui| {
                    ui::status_dot(ui, pal.ok, 8.0);
                    ui.label(RichText::new("2단계 인증 완료 (이 PC 기억됨)").size(12.5));
                    if ui.small_button("기억 해제").clicked() {
                        let mut st_guard = backend.lock().unwrap();
                        st_guard.sync.forget_device(&cur_backend);
                        *snap = st_guard.sync.snapshot();
                    }
                });
            }

            let testing = snap
                .test_job
                .as_ref()
                .is_some_and(|j| j.status == "running");
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        backend_ok && oauth_logged_in && !testing,
                        egui::Button::new("🔌 연결 테스트"),
                    )
                    .clicked()
                {
                    let mut st_guard = backend.lock().unwrap();
                    st_guard.sync.test_connection(cur_backend.clone());
                    *snap = st_guard.sync.snapshot();
                }
                if testing {
                    ui.spinner();
                    ui.label("확인하는 중…");
                }
            });

            if let Some(tj) = snap.test_job.clone() {
                match tj.status.as_str() {
                    "ok" => {
                        ui.label(
                            RichText::new(format!("✔ {}", tj.message))
                                .color(pal.ok)
                                .strong(),
                        );
                    }
                    "failed" => {
                        ui.label(RichText::new(format!("✖ {}", tj.message)).color(pal.danger));
                    }
                    "need_otp" => {
                        ui.add_space(6.0);
                        ui::sub_card(ui, pal, |ui| {
                            ui.label(RichText::new("🔐 Synology 2단계 인증 (OTP)").strong());
                            ui.label(
                                RichText::new("OTP 앱(Synology Secure SignIn, Google Authenticator 등)의 6자리 코드를 입력하세요. 한 번 인증하면 이 PC를 기억합니다.")
                                    .size(12.0)
                                    .color(pal.text_secondary),
                            );
                            ui.horizontal(|ui| {
                                let resp = ui.add(
                                    egui::TextEdit::singleline(&mut self.otp_input)
                                        .hint_text("123456")
                                        .desired_width(140.0),
                                );
                                let submit = ui::primary_button(ui, pal, "인증").clicked()
                                    || (resp.lost_focus()
                                        && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                                if submit && !self.otp_input.trim().is_empty() {
                                    let code = std::mem::take(&mut self.otp_input);
                                    let mut st_guard = backend.lock().unwrap();
                                    st_guard.sync.submit_otp(cur_backend.clone(), code);
                                    *snap = st_guard.sync.snapshot();
                                }
                            });
                        });
                    }
                    _ => {}
                }
            }
        });

        ui.add_space(10.0);

        // ⑤ 동기화 방식
        let mut delete_this_profile = false;
        ui::step_card(ui, pal, 5, "동기화 방식", true, |ui| {
            let p = &mut self.draft_cfg.profiles[idx];
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
                "내려받기만 — 저장소의 변경만 이 PC로 받습니다",
            );

            ui.add_space(6.0);
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
                        "둘 다 보존 — 이 PC 쪽은 (충돌 …) 사본으로 남깁니다 (권장)",
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
                }
            }

            if p.kind == ProfileKind::Obsidian {
                ui.add_space(4.0);
                ui.checkbox(
                    &mut p.sync_obsidian_folder,
                    ".obsidian 설정 폴더도 동기화 (플러그인, 테마, 단축키)",
                );
            }

            ui.add_space(8.0);
            ui.label(
                RichText::new("동기화하지 않을 파일/폴더 (한 줄에 하나, 예: Private/**, *.mp4)")
                    .size(12.5)
                    .color(pal.text_secondary),
            );
            let text_buf = self
                .ignore_texts
                .entry(p.id.clone())
                .or_insert_with(|| p.ignore_patterns.join("\n"));
            if ui
                .add(
                    egui::TextEdit::multiline(text_buf)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY)
                        .hint_text("Private/**\n*.tmp"),
                )
                .changed()
            {
                p.ignore_patterns = text_buf
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect();
            }

            ui.add_space(10.0);
            ui.separator();
            ui.horizontal(|ui| {
                let p_id = p.id.clone();
                if ui
                    .add_enabled(
                        !snap.transitioning,
                        egui::Button::new("이 프로필 동기화 기록 초기화"),
                    )
                    .clicked()
                {
                    let mut st_guard = backend.lock().unwrap();
                    st_guard.sync.reset_profile(p_id);
                    *snap = st_guard.sync.snapshot();
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !self.confirm_delete {
                        if ui::danger_button(ui, pal, "프로필 삭제").clicked() {
                            self.confirm_delete = true;
                        }
                    } else {
                        if ui.button("취소").clicked() {
                            self.confirm_delete = false;
                        }
                        if ui::danger_button(ui, pal, "삭제 확인").clicked() {
                            delete_this_profile = true;
                        }
                        ui.label(
                            RichText::new("프로필을 지울까요? (실제 파일은 지워지지 않습니다)")
                                .size(12.0)
                                .color(pal.danger),
                        );
                    }
                });
            });
        });

        if delete_this_profile {
            let removed = self.draft_cfg.profiles.remove(idx);
            self.ignore_texts.remove(&removed.id);
            self.confirm_delete = false;
            self.selected_idx = self.selected_idx.saturating_sub(1);
            action = Some(SyncUiAction::Toast(
                "프로필을 목록에서 지웠습니다. '저장'을 눌러야 반영됩니다.".into(),
                false,
            ));
        }

        action
    }

    fn render_logs_tab(
        &mut self,
        ui: &mut egui::Ui,
        pal: &ThemePalette,
        snap: &SyncSnapshotDto,
    ) -> Option<SyncUiAction> {
        let mut action = None;
        let logs = snap
            .status
            .as_ref()
            .map(|s| s.logs.as_slice())
            .unwrap_or(&[]);

        ui::card(ui, pal, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("동기화 기록 ({}건)", logs.len()))
                        .size(15.5)
                        .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(!logs.is_empty(), egui::Button::new("📋 전체 복사"))
                        .clicked()
                    {
                        let all_txt = logs
                            .iter()
                            .map(|l| format!("{} {}", l.time, l.text))
                            .collect::<Vec<_>>()
                            .join("\n");
                        ui.output_mut(|o| o.copied_text = all_txt);
                        action = Some(SyncUiAction::Toast(
                            "동기화 기록 전체를 복사했습니다.".into(),
                            false,
                        ));
                    }
                });
            });
            ui.add_space(8.0);
            if logs.is_empty() {
                ui.label(
                    RichText::new("동기화를 시작하면 여기에 기록이 표시됩니다.")
                        .color(pal.text_muted),
                );
            } else {
                egui::ScrollArea::vertical()
                    .id_salt("sync_logs_scroll_box")
                    .max_height(500.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for l in logs {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(&l.time)
                                        .monospace()
                                        .size(12.0)
                                        .color(pal.text_muted),
                                );
                                ui.label(RichText::new(&l.text).size(12.5).color(if l.error {
                                    pal.danger
                                } else {
                                    pal.text_primary
                                }));
                            });
                        }
                    });
            }
        });

        action
    }

    fn render_nas_browser_modal(
        &mut self,
        ctx: &egui::Context,
        pal: &ThemePalette,
        backend: &Arc<Mutex<BackendState>>,
        snap: &mut SyncSnapshotDto,
    ) {
        let Some(nb) = snap.nas_browser.clone() else {
            return;
        };
        let Some(prof) = self.draft_cfg.profiles.get(self.selected_idx) else {
            return;
        };
        let prof_backend = prof.backend.clone();
        if !matches!(prof_backend, BackendConfig::Synology { .. }) {
            return;
        }

        let at_root = nb.path == "/" || nb.path.is_empty();
        let ready = nb.status == "ready";
        let mut close_modal = false;
        let mut chosen_path: Option<String> = None;

        egui::Window::new("📁 Synology NAS 폴더 선택")
            .collapsible(false)
            .resizable(false)
            .fixed_size([460.0, 380.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!at_root, egui::Button::new("⬆ 위로"))
                        .clicked()
                    {
                        let up = match nb.path.rfind('/') {
                            Some(0) | None => "/".to_string(),
                            Some(i) => nb.path[..i].to_string(),
                        };
                        let mut st_guard = backend.lock().unwrap();
                        st_guard
                            .sync
                            .nas_browse_step(prof_backend.clone(), up, None, None);
                        *snap = st_guard.sync.snapshot();
                    }
                    ui.label(
                        RichText::new(if at_root {
                            "공유 폴더 목록 (/)".to_string()
                        } else {
                            nb.path.clone()
                        })
                        .strong(),
                    );
                });

                ui.add_space(6.0);
                egui::Frame::none()
                    .fill(pal.bg_inset)
                    .stroke(Stroke::new(1.0_f32, pal.border_subtle))
                    .rounding(Rounding::same(8.0))
                    .inner_margin(Margin::same(8.0))
                    .show(ui, |ui| {
                        ui.set_min_height(210.0);
                        ui.set_max_height(210.0);
                        match nb.status.as_str() {
                            "loading" => {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label("NAS 폴더를 불러오는 중…");
                                });
                            }
                            "failed" => {
                                ui.label(
                                    RichText::new(format!(
                                        "✖ {}",
                                        nb.message.as_deref().unwrap_or("오류")
                                    ))
                                    .color(pal.danger),
                                );
                                if ui.button("다시 시도").clicked() {
                                    let mut st_guard = backend.lock().unwrap();
                                    st_guard.sync.nas_browse_step(
                                        prof_backend.clone(),
                                        nb.path.clone(),
                                        None,
                                        None,
                                    );
                                    *snap = st_guard.sync.snapshot();
                                }
                            }
                            "need_otp" => {
                                ui.label(RichText::new("🔐 2단계 인증 필요").strong());
                                ui.label("OTP 앱의 6자리 코드를 입력하세요.");
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.nas_otp_input)
                                            .hint_text("123456")
                                            .desired_width(140.0),
                                    );
                                    if ui::primary_button(ui, pal, "인증").clicked()
                                        && !self.nas_otp_input.trim().is_empty()
                                    {
                                        let otp = std::mem::take(&mut self.nas_otp_input);
                                        let mut st_guard = backend.lock().unwrap();
                                        st_guard.sync.nas_browse_step(
                                            prof_backend.clone(),
                                            nb.path.clone(),
                                            None,
                                            Some(otp),
                                        );
                                        *snap = st_guard.sync.snapshot();
                                    }
                                });
                            }
                            _ => {
                                egui::ScrollArea::vertical().show(ui, |ui| {
                                    if nb.folders.is_empty() {
                                        ui.label(
                                            RichText::new("하위 폴더가 없습니다.")
                                                .color(pal.text_muted),
                                        );
                                    } else {
                                        for f in &nb.folders {
                                            if ui
                                                .add(
                                                    egui::Button::new(format!("📁 {}", f.name))
                                                        .min_size(egui::vec2(
                                                            ui.available_width(),
                                                            28.0,
                                                        )),
                                                )
                                                .clicked()
                                            {
                                                let mut st_guard = backend.lock().unwrap();
                                                st_guard.sync.nas_browse_step(
                                                    prof_backend.clone(),
                                                    f.path.clone(),
                                                    None,
                                                    None,
                                                );
                                                *snap = st_guard.sync.snapshot();
                                            }
                                        }
                                    }
                                });
                            }
                        }
                    });

                if !at_root {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.nas_new_folder)
                                .hint_text("새 폴더 이름")
                                .desired_width(280.0),
                        );
                        if ui
                            .add_enabled(ready, egui::Button::new("＋ 새 폴더"))
                            .clicked()
                            && !self.nas_new_folder.trim().is_empty()
                        {
                            let new_name = std::mem::take(&mut self.nas_new_folder);
                            let mut st_guard = backend.lock().unwrap();
                            st_guard.sync.nas_browse_step(
                                prof_backend.clone(),
                                nb.path.clone(),
                                Some(new_name),
                                None,
                            );
                            *snap = st_guard.sync.snapshot();
                        }
                    });
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("취소").clicked() {
                        close_modal = true;
                    }
                    if ui
                        .add_enabled(
                            !at_root && ready,
                            egui::Button::new(
                                RichText::new(format!("이 폴더 사용: {}", nb.path))
                                    .color(Color32::WHITE)
                                    .strong(),
                            )
                            .fill(pal.accent_blue)
                            .stroke(Stroke::NONE),
                        )
                        .clicked()
                    {
                        chosen_path = Some(nb.path.clone());
                        close_modal = true;
                    }
                });
            });

        if let Some(chosen) = chosen_path {
            if let Some(p) = self.draft_cfg.profiles.get_mut(self.selected_idx) {
                if let BackendConfig::Synology { folder, .. } = &mut p.backend {
                    *folder = chosen;
                }
            }
        }
        if close_modal {
            let mut st_guard = backend.lock().unwrap();
            st_guard.sync.nas_browse_close();
            *snap = st_guard.sync.snapshot();
        }
    }
}
