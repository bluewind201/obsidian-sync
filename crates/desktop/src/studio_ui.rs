//! Onyx Studio — 100% 순수 네이티브 Rust 데스크톱 GUI (`eframe`/`egui` + `glow`).
//! - WebView2(`wry`/`tao`/`WebView2Loader.dll`) 및 로컬 포트 바인딩을 완전히 제거한 순수 Win32 + OpenGL(`glow`) 실행 환경
//! - 외부 텍스트 복사·붙여넣기 시 HTML 태그/속성/제로폭 문자 100% 제거(순수 일반 텍스트) + 클립보드 이미지(`Win+Shift+S`, `PNG`, `CF_DIB`, 파일, 웹 혼합 `<img>`) 자동 첨부 및 인라인 이미지 미리보기
//! - 노트·볼트 선택적 AES-256-GCM(PBKDF2 210,000회) 암호화, 폴더 접기/펼치기 및 커스텀 폴더 아이콘·색상, GitHub 릴리즈 무중단 자동 업데이트 지원

use crate::api::BackendState;
use crate::graph_view::{render_markdown_preview, ForceGraphWidget, ImageTextureCache};
use crate::platform;
use crate::sync_controller::SyncSnapshotDto;
use crate::sync_ui::{SyncCenterState, SyncUiAction};
use crate::ui::{self, ThemePalette};
use crate::vault::{
    self, GraphDto, NoteDocumentDto, NotePropertyDto, SearchHitDto, TemplateItemDto, TrashItemDto,
    TreeNodeDto, VaultScanDto, WorkspaceState,
};
use crate::win_clipboard::{self, CleanPasteSegment, ClipboardPasteAction};
use base64::Engine as _;
use eframe::egui::text::{CCursor, CCursorRange};
use eframe::egui::text_edit::TextEditState;
use eframe::egui::{self, Align, Color32, Id, Key, Layout, Margin, RichText, Rounding, Stroke};
use std::collections::HashSet;
use std::io::Read as _;
use std::path::Path;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const BODY_EDITOR_ID_STR: &str = "onyx_body_editor";

const EMOJI_PRESETS: &[&str] = &[
    "📝", "📄", "📚", "📌", "💡", "🚀", "🎯", "🔥", "✅", "📅", "🗂️", "🧠", "💻", "⚙️", "🔬", "📊",
    "🎨", "🌱", "⭐", "💎", "🧭", "🏆", "💬", "🤝", "📖", "🔖", "🛠️", "🧪", "🌐", "🔒",
];

const FOLDER_ICON_PRESETS: &[&str] = &[
    "📁", "🗂️", "📚", "💼", "🚀", "🧠", "🔬", "🎨", "🛠️", "📌", "⭐", "🔒", "📅", "🌱", "💎", "📦",
];

const FOLDER_COLOR_PRESETS: &[(&str, &str)] = &[
    ("기본", ""),
    ("퍼플", "#7C3AED"),
    ("블루", "#2383E2"),
    ("그린", "#0F7B6C"),
    ("앰버", "#D97706"),
    ("레드", "#E03E3E"),
    ("핑크", "#DB2777"),
    ("슬레이트", "#64748B"),
];

const COVER_PRESETS: &[(&str, Color32)] = &[
    ("노션 선셋", Color32::from_rgb(246, 165, 108)),
    ("오션 블루", Color32::from_rgb(35, 131, 226)),
    ("오로라 퍼플", Color32::from_rgb(124, 58, 237)),
    ("포레스트 민트", Color32::from_rgb(17, 153, 142)),
    ("로즈 쿼츠", Color32::from_rgb(244, 114, 182)),
    ("미드나잇 슬레이트", Color32::from_rgb(45, 50, 62)),
    ("어텀 앰버", Color32::from_rgb(217, 115, 13)),
    ("노르딕 세이지", Color32::from_rgb(68, 131, 97)),
];

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum ActiveMainView {
    Editor,
    Graph,
    Sync,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum RightInspectorTab {
    Backlinks,
    Outline,
    LocalGraph,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum SaveStatus {
    Saved,
    Dirty,
    Error,
}

struct ToastItem {
    text: String,
    is_error: bool,
    expires_at: Instant,
}

pub enum PromptKind {
    NewNote { folder: String },
    NewFolder { parent: String },
    Rename { old_path: String },
}

#[derive(Clone, Debug)]
pub struct FolderStyleModalState {
    pub folder_path: String,
    pub folder_name: String,
    pub icon: String,
    pub color: String,
}

pub struct StudioApp {
    pub backend: Arc<Mutex<BackendState>>,
    pub workspace: WorkspaceState,
    pub scan: Option<VaultScanDto>,
    pub sync_snap: SyncSnapshotDto,
    pub vault_revision: u64,

    pub main_view: ActiveMainView,
    pub sidebar_collapsed: bool,
    pub inspector_open: bool,
    pub inspector_tab: RightInspectorTab,
    pub expanded_folders: HashSet<String>,

    // 현재 편집 중인 노트 상태
    pub current_doc: Option<NoteDocumentDto>,
    pub draft_title: String,
    pub draft_body: String,
    pub draft_icon: Option<String>,
    pub draft_cover: Option<String>,
    pub draft_properties: Vec<NotePropertyDto>,
    pub new_tag_inputs: Vec<String>,
    pub show_raw_yaml: bool,
    pub save_status: SaveStatus,
    pub last_edit_at: Option<Instant>,
    pub unlock_password_input: String,

    // 우측 분할 보기 상태
    pub split_doc: Option<NoteDocumentDto>,

    // 이미지 텍스처 캐시 (마크다운 미리보기 인라인 이미지)
    pub img_cache: ImageTextureCache,

    // 그래프 뷰 상태
    pub local_graph_depth: usize,
    pub local_graph_data: Option<GraphDto>,
    pub local_graph_widget: ForceGraphWidget,
    pub global_graph_include_tags: bool,
    pub global_graph_local_only: bool,
    pub global_graph_data: Option<GraphDto>,
    pub global_graph_widget: ForceGraphWidget,

    // 동기화 센터 상태
    pub sync_ui: SyncCenterState,

    // 모달 상태
    pub show_command_palette: bool,
    pub palette_query: String,
    pub palette_hits: Vec<SearchHitDto>,
    pub show_emoji_picker: bool,
    pub show_cover_picker: bool,
    pub show_templates_modal: bool,
    pub loaded_templates: Vec<TemplateItemDto>,
    pub show_trash_modal: bool,
    pub trash_items: Vec<TrashItemDto>,
    pub show_wikilink_modal: bool,
    pub wikilink_query: String,
    pub prompt_modal: Option<(PromptKind, String, String)>,
    pub folder_style_modal: Option<FolderStyleModalState>,
    pub show_crypto_modal: bool,
    pub crypto_pw_input: String,
    pub crypto_pw_confirm: String,
    pub crypto_hint_input: String,
    pub crypto_auto_encrypt: bool,

    // 트레이 & 윈도우 핸들
    pub hwnd: isize,
    pub shared_hwnd: Arc<AtomicIsize>,
    pub hide_on_start: bool,
    pub tray: Option<platform::Tray>,
    pub has_tray: bool,

    // 주기적 폴링 및 토스트
    last_poll: Instant,
    toasts: Vec<ToastItem>,
    last_theme: String,
}

fn parent_folder_of(rel: &str) -> String {
    match rel.rfind('/') {
        Some(idx) if idx > 0 => rel[..idx].to_string(),
        _ => String::new(),
    }
}

fn strip_md_ext(s: &str) -> &str {
    s.strip_suffix(".md")
        .or_else(|| s.strip_suffix(".MD"))
        .unwrap_or(s)
}

fn char_idx_to_byte_idx(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

fn collect_all_folder_paths(nodes: &[TreeNodeDto], out: &mut HashSet<String>) {
    for n in nodes {
        if n.kind == "folder" {
            out.insert(n.path.clone());
            collect_all_folder_paths(&n.children, out);
        }
    }
}

impl StudioApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        backend: Arc<Mutex<BackendState>>,
        trigger: platform::SharedTrigger,
        hide_on_start: bool,
    ) -> Self {
        ui::setup_fonts(&cc.egui_ctx);

        let mut hwnd = 0isize;
        #[cfg(windows)]
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(h) = cc.window_handle() {
                if let RawWindowHandle::Win32(w) = h.as_raw() {
                    hwnd = w.hwnd.get();
                }
            }
        }

        let shared_hwnd = Arc::new(AtomicIsize::new(hwnd));
        {
            let ctx_activate = cc.egui_ctx.clone();
            let hwnd_activate = shared_hwnd.clone();
            platform::spawn_activation_callback(move || {
                let h = hwnd_activate.load(Ordering::SeqCst);
                if h != 0 {
                    platform::present_hwnd(h);
                }
                ctx_activate.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx_activate.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx_activate.send_viewport_cmd(egui::ViewportCommand::Focus);
                ctx_activate.request_repaint();
            });
        }

        let tray = platform::create_tray(hwnd, trigger);
        let has_tray = tray.is_some();

        let (workspace, vault_revision, sync_snap, scan) = {
            let mut st = backend.lock().unwrap();
            st.updater.set_main_hwnd(hwnd);
            let cfg_clone = st.sync.cfg.clone();
            st.workspace.state.sync_vaults_with_config(&cfg_clone);
            let _ = st.workspace.save_and_refresh_watcher();
            let rev = st.workspace.watcher.revision();
            let snap = st.sync.snapshot();
            let ws = st.workspace.state.clone();
            let initial_scan = ws
                .active_vault
                .as_deref()
                .map(Path::new)
                .filter(|p| p.is_dir())
                .and_then(|p| vault::scan_vault_with_session(p, rev, Some(&st.crypto)).ok());
            (ws, rev, snap, initial_scan)
        };

        let theme = if workspace.theme.trim().is_empty() {
            "light".to_string()
        } else {
            workspace.theme.clone()
        };
        ui::apply_theme(&cc.egui_ctx, &theme);

        let sync_ui = SyncCenterState::new(&sync_snap.cfg);
        let vault_auto_enc = scan.as_ref().is_some_and(|s| s.vault_auto_encrypt);

        let mut app = Self {
            backend,
            workspace,
            scan,
            sync_snap,
            vault_revision,
            main_view: ActiveMainView::Editor,
            sidebar_collapsed: false,
            inspector_open: true,
            inspector_tab: RightInspectorTab::Backlinks,
            // 시작 시 모든 폴더는 깔끔하게 접힌 상태로 시작 (열린 노트의 상위 폴더만 필요 시 펼침)
            expanded_folders: HashSet::new(),
            current_doc: None,
            draft_title: String::new(),
            draft_body: String::new(),
            draft_icon: None,
            draft_cover: None,
            draft_properties: Vec::new(),
            new_tag_inputs: Vec::new(),
            show_raw_yaml: false,
            save_status: SaveStatus::Saved,
            last_edit_at: None,
            unlock_password_input: String::new(),
            split_doc: None,
            img_cache: ImageTextureCache::default(),
            local_graph_depth: 2,
            local_graph_data: None,
            local_graph_widget: ForceGraphWidget::new(true),
            global_graph_include_tags: true,
            global_graph_local_only: false,
            global_graph_data: None,
            global_graph_widget: ForceGraphWidget::new(false),
            sync_ui,
            show_command_palette: false,
            palette_query: String::new(),
            palette_hits: Vec::new(),
            show_emoji_picker: false,
            show_cover_picker: false,
            show_templates_modal: false,
            loaded_templates: Vec::new(),
            show_trash_modal: false,
            trash_items: Vec::new(),
            show_wikilink_modal: false,
            wikilink_query: String::new(),
            prompt_modal: None,
            folder_style_modal: None,
            show_crypto_modal: false,
            crypto_pw_input: String::new(),
            crypto_pw_confirm: String::new(),
            crypto_hint_input: String::new(),
            crypto_auto_encrypt: vault_auto_enc,
            hwnd,
            shared_hwnd,
            hide_on_start: hide_on_start && has_tray,
            tray,
            has_tray,
            last_poll: Instant::now(),
            toasts: Vec::new(),
            last_theme: theme,
        };

        if let Some(first_note) = app
            .workspace
            .active_tab
            .clone()
            .or_else(|| app.workspace.open_tabs.first().cloned())
            .or_else(|| {
                app.scan
                    .as_ref()
                    .and_then(|s| s.notes.first().map(|n| n.path.clone()))
            })
        {
            app.open_note_internal(&first_note, false);
        }

        if let Some(split_path) = app.workspace.split_tab.clone() {
            app.open_split_note(&split_path);
        }

        app
    }

    fn show_toast(&mut self, text: impl Into<String>, is_error: bool) {
        self.toasts.push(ToastItem {
            text: text.into(),
            is_error,
            expires_at: Instant::now() + Duration::from_millis(3400),
        });
    }

    fn expand_ancestors_of(&mut self, rel_path: &str) {
        let parts: Vec<&str> = rel_path.split('/').collect();
        let mut acc = String::new();
        for p in parts.iter().take(parts.len().saturating_sub(1)) {
            if acc.is_empty() {
                acc = (*p).to_string();
            } else {
                acc = format!("{acc}/{p}");
            }
            self.expanded_folders.insert(acc.clone());
        }
    }

    fn persist_workspace(&mut self) {
        let mut st = self.backend.lock().unwrap();
        st.workspace.state = self.workspace.clone();
        let _ = st.workspace.save_and_refresh_watcher();
    }

    fn rescan_active_vault(&mut self) {
        let Some(vp) = self.workspace.active_vault.clone() else {
            self.scan = None;
            return;
        };
        let st = self.backend.lock().unwrap();
        let rev = st.workspace.watcher.revision();
        if let Ok(s) = vault::scan_vault_with_session(Path::new(&vp), rev, Some(&st.crypto)) {
            self.vault_revision = rev;
            self.crypto_auto_encrypt = s.vault_auto_encrypt;
            self.scan = Some(s);
        }
    }

    fn select_vault(&mut self, path: &str) {
        self.flush_save(false);
        let clean = path.trim().to_string();
        if clean.is_empty() || !Path::new(&clean).is_dir() {
            self.show_toast(format!("볼트 폴더가 존재하지 않습니다: {clean}"), true);
            return;
        }
        {
            let mut st = self.backend.lock().unwrap();
            st.workspace.state.add_or_select_vault(&clean);
            let _ = st.workspace.save_and_refresh_watcher();
            self.workspace = st.workspace.state.clone();
            self.vault_revision = st.workspace.watcher.revision();
            self.scan = vault::scan_vault_with_session(
                Path::new(&clean),
                self.vault_revision,
                Some(&st.crypto),
            )
            .ok();
        }
        self.img_cache.clear();
        self.current_doc = None;
        self.expanded_folders.clear();
        self.main_view = ActiveMainView::Editor;

        if let Some(first) = self
            .workspace
            .active_tab
            .clone()
            .or_else(|| self.workspace.open_tabs.first().cloned())
            .or_else(|| {
                self.scan
                    .as_ref()
                    .and_then(|s| s.notes.first().map(|n| n.path.clone()))
            })
        {
            self.open_note_internal(&first, false);
        }
    }

    fn handle_open_vault_dialog(&mut self) {
        if let Some(picked) = platform::pick_folder() {
            self.select_vault(&picked);
            self.show_toast(format!("볼트를 열었습니다: {picked}"), false);
        }
    }

    fn handle_create_sample_vault(&mut self) {
        self.flush_save(false);
        let data_dir = self.backend.lock().unwrap().workspace.data_dir.clone();
        match vault::create_sample_vault(&data_dir) {
            Ok(sample_path) => {
                let s = sample_path.to_string_lossy().to_string();
                self.select_vault(&s);
                self.show_toast("샘플 볼트가 생성되었습니다! 바로 편집해 보세요.", false);
            }
            Err(e) => self.show_toast(format!("샘플 볼트 생성 실패: {e:#}"), true),
        }
    }

    pub fn open_note(&mut self, rel_path: &str) {
        self.open_note_internal(rel_path, true);
    }

    fn open_note_internal(&mut self, rel_path: &str, expand_parents: bool) {
        let Some(vault_path) = self.workspace.active_vault.clone() else {
            return;
        };
        if self.main_view != ActiveMainView::Editor {
            self.main_view = ActiveMainView::Editor;
        }
        if self
            .current_doc
            .as_ref()
            .is_some_and(|d| d.path == rel_path)
        {
            return;
        }

        self.flush_save(false);

        let read_res = {
            let st = self.backend.lock().unwrap();
            vault::read_note_with_session(Path::new(&vault_path), rel_path, Some(&st.crypto))
        };

        match read_res {
            Ok(doc) => {
                self.apply_loaded_doc(doc, expand_parents);
            }
            Err(e) => {
                self.show_toast(format!("노트를 열 수 없습니다: {e:#}"), true);
            }
        }
    }

    fn apply_loaded_doc(&mut self, doc: NoteDocumentDto, expand_parents: bool) {
        let norm_path = doc.path.clone();
        self.draft_title = doc.title.clone();
        self.draft_body = doc.body.clone();
        self.draft_icon = doc.icon.clone();
        self.draft_cover = doc.cover.clone();
        self.draft_properties = doc.properties.clone();
        self.new_tag_inputs = vec![String::new(); self.draft_properties.len()];
        self.save_status = SaveStatus::Saved;
        self.last_edit_at = None;
        self.unlock_password_input.clear();
        self.current_doc = Some(doc);

        if expand_parents {
            self.expand_ancestors_of(&norm_path);
        }
        if !self.workspace.open_tabs.contains(&norm_path) {
            self.workspace.open_tabs.push(norm_path.clone());
        }
        self.workspace.active_tab = Some(norm_path.clone());
        self.workspace.recent_notes.retain(|x| x != &norm_path);
        self.workspace.recent_notes.insert(0, norm_path);
        if self.workspace.recent_notes.len() > 20 {
            self.workspace.recent_notes.truncate(20);
        }
        self.persist_workspace();
        self.refresh_local_graph();
    }

    fn close_tab(&mut self, rel_path: &str) {
        if self
            .current_doc
            .as_ref()
            .is_some_and(|d| d.path == rel_path)
        {
            self.flush_save(false);
        }
        let idx = self
            .workspace
            .open_tabs
            .iter()
            .position(|t| t == rel_path)
            .unwrap_or(0);
        self.workspace.open_tabs.retain(|t| t != rel_path);

        if self
            .current_doc
            .as_ref()
            .is_some_and(|d| d.path == rel_path)
        {
            let next = self
                .workspace
                .open_tabs
                .get(idx.saturating_sub(1))
                .cloned()
                .or_else(|| self.workspace.open_tabs.first().cloned());
            self.workspace.active_tab = next.clone();
            self.persist_workspace();
            if let Some(n) = next {
                self.current_doc = None;
                self.open_note(&n);
            } else {
                self.current_doc = None;
            }
        } else {
            self.persist_workspace();
        }
    }

    fn mark_dirty(&mut self) {
        self.save_status = SaveStatus::Dirty;
        self.last_edit_at = Some(Instant::now());
    }

    pub fn flush_save(&mut self, manual_toast: bool) {
        let Some(vault_path) = self.workspace.active_vault.clone() else {
            return;
        };
        let Some(cur_doc) = &self.current_doc else {
            return;
        };
        if cur_doc.locked {
            return;
        }
        let path = cur_doc.path.clone();

        // 제목이 바뀌었으면 리네임 처리 (위키링크 자동 갱신)
        let trimmed_title = self.draft_title.trim().to_string();
        if !trimmed_title.is_empty() && trimmed_title != cur_doc.title {
            let old_title = cur_doc.title.clone();
            let rename_res = {
                let mut st = self.backend.lock().unwrap();
                vault::rename_entry_with_session(
                    Path::new(&vault_path),
                    &path,
                    &trimmed_title,
                    Some(&mut st.crypto),
                )
            };
            if let Ok(res) = rename_res {
                if let Some(d) = &mut self.current_doc {
                    d.path = res.new_path.clone();
                    d.title = trimmed_title;
                }
                for t in &mut self.workspace.open_tabs {
                    if *t == path {
                        *t = res.new_path.clone();
                    }
                }
                if self.workspace.active_tab.as_deref() == Some(path.as_str()) {
                    self.workspace.active_tab = Some(res.new_path.clone());
                }
                self.persist_workspace();
                if res.updated_links > 0 {
                    self.show_toast(
                        format!(
                            "'{}' 이름 변경 완료 (위키링크 {}곳 자동 갱신)",
                            old_title, res.updated_links
                        ),
                        false,
                    );
                }
            }
        }

        if self.save_status != SaveStatus::Dirty && !manual_toast {
            return;
        }

        let target_path = self
            .current_doc
            .as_ref()
            .map(|d| d.path.clone())
            .unwrap_or(path);

        let save_res = {
            let st = self.backend.lock().unwrap();
            vault::save_note_with_session(
                Path::new(&vault_path),
                &target_path,
                self.draft_icon.as_deref(),
                self.draft_cover.as_deref(),
                &self.draft_properties,
                &self.draft_body,
                Some(&st.crypto),
            )
        };

        match save_res {
            Ok(updated) => {
                self.current_doc = Some(updated);
                self.save_status = SaveStatus::Saved;
                self.last_edit_at = None;
                {
                    let st = self.backend.lock().unwrap();
                    let rev = st.workspace.watcher.bump();
                    if let Ok(s) = vault::scan_vault_with_session(
                        Path::new(&vault_path),
                        rev,
                        Some(&st.crypto),
                    ) {
                        self.vault_revision = rev;
                        self.scan = Some(s);
                    }
                }
                self.refresh_local_graph();
                if manual_toast {
                    self.show_toast("노트를 저장했습니다.", false);
                }
            }
            Err(e) => {
                self.save_status = SaveStatus::Error;
                self.show_toast(format!("저장 실패: {e:#}"), true);
            }
        }
    }

    /// 본문 편집기의 현재 커서(또는 선택 영역) 위치에 텍스트/마크다운을 정확히 삽입합니다.
    fn insert_at_body_cursor(&mut self, ctx: &egui::Context, snippet: &str) {
        if snippet.is_empty() {
            return;
        }
        let editor_id = Id::new(BODY_EDITOR_ID_STR);
        if let Some(mut state) = TextEditState::load(ctx, editor_id) {
            if let Some(range) = state.cursor.char_range() {
                let start_c = range.primary.index.min(range.secondary.index);
                let end_c = range.primary.index.max(range.secondary.index);
                let start_b = char_idx_to_byte_idx(&self.draft_body, start_c);
                let end_b = char_idx_to_byte_idx(&self.draft_body, end_c);
                self.draft_body.replace_range(start_b..end_b, snippet);
                let new_c = start_c + snippet.chars().count();
                state
                    .cursor
                    .set_char_range(Some(CCursorRange::one(CCursor::new(new_c))));
                state.store(ctx, editor_id);
                ctx.memory_mut(|m| m.request_focus(editor_id));
                self.mark_dirty();
                return;
            }
        }

        if !self.draft_body.is_empty()
            && !self.draft_body.ends_with('\n')
            && snippet.starts_with('\n')
        {
            self.draft_body.push_str(snippet);
        } else {
            self.draft_body.push_str(snippet);
        }
        self.mark_dirty();
    }

    fn try_save_remote_image_to_vault(&self, vault_root: &Path, url: &str) -> Option<String> {
        let resp = ureq::get(url)
            .set("User-Agent", "OnyxStudio-Desktop")
            .timeout(Duration::from_secs(4))
            .call()
            .ok()?;
        let ct = resp
            .header("Content-Type")
            .unwrap_or("")
            .to_ascii_lowercase();
        let ext = if ct.contains("jpeg") || ct.contains("jpg") {
            "jpg"
        } else if ct.contains("gif") {
            "gif"
        } else if ct.contains("webp") {
            "webp"
        } else if ct.contains("bmp") {
            "bmp"
        } else if ct.contains("png") {
            "png"
        } else {
            return None;
        };
        let mut bytes = Vec::new();
        resp.into_reader()
            .take(15 * 1024 * 1024)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() < 16 {
            return None;
        }
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        vault::upload_attachment(vault_root, &format!("pasted.{ext}"), &b64)
            .ok()
            .map(|a| a.rel_path)
    }

    /// Win32 클립보드를 읽어 외부 텍스트의 서식/속성 코드는 100% 제거하고,
    /// 캡처·복사된 이미지는 볼트 `attachments/`에 저장하여 깔끔한 마크다운 문자열을 생성합니다.
    fn build_clipboard_paste_text(
        &mut self,
        force_plain_text: bool,
        fallback_text: Option<String>,
    ) -> String {
        let vault_path = self.workspace.active_vault.clone();

        if let Some(action) = win_clipboard::read_windows_clipboard(force_plain_text) {
            match action {
                ClipboardPasteAction::ImageFiles(paths) => {
                    if let Some(ref vp) = vault_path {
                        let vr = Path::new(vp);
                        let mut parts = Vec::new();
                        for p in paths {
                            let label = p
                                .file_stem()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_else(|| "이미지".into());
                            if let Ok(saved) = vault::save_picked_image_attachment(vr, Some(p)) {
                                if let Some(rel) = saved.rel_path {
                                    parts.push(format!("![{label}]({rel})"));
                                }
                            }
                        }
                        if !parts.is_empty() {
                            self.img_cache.clear();
                            self.show_toast(
                                format!("이미지 {}개를 붙여넣었습니다.", parts.len()),
                                false,
                            );
                            return format!("\n{}\n", parts.join("\n"));
                        }
                    }
                }
                ClipboardPasteAction::ImageBytes { bytes, ext } => {
                    if let Some(ref vp) = vault_path {
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                        match vault::upload_attachment(
                            Path::new(vp),
                            &format!("clipboard.{ext}"),
                            &b64,
                        ) {
                            Ok(att) => {
                                self.img_cache.clear();
                                self.show_toast(
                                    format!("클립보드 이미지를 붙여넣었습니다 ({})", att.rel_path),
                                    false,
                                );
                                return format!("\n![이미지]({})\n", att.rel_path);
                            }
                            Err(e) => {
                                self.show_toast(format!("이미지 저장 실패: {e:#}"), true);
                            }
                        }
                    }
                }
                ClipboardPasteAction::Segments(segs) => {
                    let mut out = String::new();
                    let mut saved_imgs = 0usize;
                    for seg in segs {
                        match seg {
                            CleanPasteSegment::Text(t) => {
                                if !out.is_empty() && !out.ends_with('\n') {
                                    out.push_str("\n\n");
                                }
                                out.push_str(&t);
                            }
                            CleanPasteSegment::Image { src, alt } => {
                                let resolved_target = if let Some(ref vp) = vault_path {
                                    let vr = Path::new(vp);
                                    if src.starts_with("data:image/") {
                                        win_clipboard::decode_data_image_uri(&src).and_then(
                                            |(bytes, ext)| {
                                                let b64 = base64::engine::general_purpose::STANDARD
                                                    .encode(&bytes);
                                                vault::upload_attachment(
                                                    vr,
                                                    &format!("pasted.{ext}"),
                                                    &b64,
                                                )
                                                .ok()
                                                .map(|a| {
                                                    saved_imgs += 1;
                                                    a.rel_path
                                                })
                                            },
                                        )
                                    } else if src.starts_with("http://")
                                        || src.starts_with("https://")
                                    {
                                        self.try_save_remote_image_to_vault(vr, &src)
                                            .map(|rel| {
                                                saved_imgs += 1;
                                                rel
                                            })
                                            .or(Some(src))
                                    } else {
                                        Some(src)
                                    }
                                } else if !src.starts_with("data:image/") {
                                    Some(src)
                                } else {
                                    None
                                };

                                if let Some(target) = resolved_target {
                                    if !out.is_empty() && !out.ends_with('\n') {
                                        out.push_str("\n\n");
                                    }
                                    out.push_str(&format!("![{alt}]({target})\n"));
                                }
                            }
                        }
                    }
                    if saved_imgs > 0 {
                        self.img_cache.clear();
                        self.show_toast(
                            format!("일반 텍스트로 정리하고 이미지 {saved_imgs}개를 첨부했습니다."),
                            false,
                        );
                    }
                    return out;
                }
                ClipboardPasteAction::PlainText(clean) => {
                    return clean;
                }
            }
        }

        fallback_text
            .map(|t| win_clipboard::sanitize_external_plain_text(&t))
            .unwrap_or_default()
    }

    fn open_wikilink_by_title(&mut self, raw_title: &str) {
        let Some(vault_path) = self.workspace.active_vault.clone() else {
            return;
        };
        let clean = raw_title.split('#').next().unwrap_or(raw_title).trim();
        if clean.is_empty() {
            return;
        }
        if let Some(scan) = &self.scan {
            if let Some(found) = scan.notes.iter().find(|n| {
                n.title.eq_ignore_ascii_case(clean)
                    || strip_md_ext(&n.path).eq_ignore_ascii_case(clean)
            }) {
                let p = found.path.clone();
                self.open_note(&p);
                return;
            }
        }

        let folder = self
            .current_doc
            .as_ref()
            .map(|d| parent_folder_of(&d.path))
            .unwrap_or_default();
        let created = {
            let st = self.backend.lock().unwrap();
            vault::create_note_with_session(
                Path::new(&vault_path),
                &folder,
                clean,
                None,
                None,
                &[],
                Some(&st.crypto),
            )
        };
        match created {
            Ok((new_path, _)) => {
                self.backend.lock().unwrap().workspace.watcher.bump();
                self.rescan_active_vault();
                self.open_note(&new_path);
                self.show_toast(format!("새 연결 노트를 생성했습니다: {clean}"), false);
            }
            Err(e) => self.show_toast(format!("노트 생성 실패: {e:#}"), true),
        }
    }

    fn open_today_daily_note(&mut self) {
        let Some(vault_path) = self.workspace.active_vault.clone() else {
            self.show_toast("먼저 볼트 폴더를 열어 주세요.", true);
            return;
        };
        let daily_folder = if self.workspace.daily_folder.trim().is_empty() {
            "Daily".to_string()
        } else {
            self.workspace.daily_folder.clone()
        };
        let opened = {
            let st = self.backend.lock().unwrap();
            vault::open_daily_note_with_session(
                Path::new(&vault_path),
                &daily_folder,
                Some(&st.crypto),
            )
        };
        match opened {
            Ok((path, _)) => {
                self.backend.lock().unwrap().workspace.watcher.bump();
                self.rescan_active_vault();
                self.open_note(&path);
            }
            Err(e) => self.show_toast(format!("일일 노트 열기 실패: {e:#}"), true),
        }
    }

    fn open_split_note(&mut self, rel_path: &str) {
        let Some(vault_path) = self.workspace.active_vault.clone() else {
            return;
        };
        let doc_res = {
            let st = self.backend.lock().unwrap();
            vault::read_note_with_session(Path::new(&vault_path), rel_path, Some(&st.crypto))
        };
        if let Ok(doc) = doc_res {
            self.split_doc = Some(doc);
            self.workspace.split_tab = Some(rel_path.to_string());
            self.persist_workspace();
        }
    }

    fn close_split_note(&mut self) {
        self.split_doc = None;
        self.workspace.split_tab = None;
        self.persist_workspace();
    }

    fn toggle_favorite(&mut self, rel_path: &str) {
        if let Some(pos) = self.workspace.favorites.iter().position(|x| x == rel_path) {
            self.workspace.favorites.remove(pos);
        } else {
            self.workspace.favorites.push(rel_path.to_string());
        }
        self.persist_workspace();
    }

    fn refresh_local_graph(&mut self) {
        let Some(vp) = self.workspace.active_vault.as_deref() else {
            self.local_graph_data = None;
            return;
        };
        let Some(center) = self.current_doc.as_ref().map(|d| d.path.clone()) else {
            self.local_graph_data = None;
            return;
        };
        let g_res = {
            let st = self.backend.lock().unwrap();
            vault::build_graph_with_session(
                Path::new(vp),
                Some(&center),
                Some(self.local_graph_depth),
                true,
                Some(&st.crypto),
            )
        };
        if let Ok(g) = g_res {
            self.local_graph_widget.set_data(&g, Some(&center));
            self.local_graph_data = Some(g);
        }
    }

    fn refresh_global_graph(&mut self) {
        let Some(vp) = self.workspace.active_vault.as_deref() else {
            self.global_graph_data = None;
            return;
        };
        let center = if self.global_graph_local_only {
            self.current_doc.as_ref().map(|d| d.path.as_str())
        } else {
            None
        };
        let depth = if self.global_graph_local_only {
            Some(2)
        } else {
            None
        };
        let g_res = {
            let st = self.backend.lock().unwrap();
            vault::build_graph_with_session(
                Path::new(vp),
                center,
                depth,
                self.global_graph_include_tags,
                Some(&st.crypto),
            )
        };
        if let Ok(g) = g_res {
            self.global_graph_widget
                .set_data(&g, self.current_doc.as_ref().map(|d| d.path.as_str()));
            self.global_graph_data = Some(g);
        }
    }

    fn apply_update_and_restart(&mut self) {
        self.flush_save(false);
        let updater = self.backend.lock().unwrap().updater.clone();
        match updater.prepare_staged_for_apply() {
            Ok(staged) => {
                let stop_join = {
                    let mut st = self.backend.lock().unwrap();
                    st.sync.stop_service();
                    st.sync.stopping.take()
                };
                if let Some(j) = stop_join {
                    let _ = j.join();
                }
                match updater.swap_and_restart(&staged) {
                    Ok(()) => {
                        std::thread::sleep(Duration::from_millis(120));
                        std::process::exit(0);
                    }
                    Err(e) => self.show_toast(format!("업데이트 적용 실패: {e:#}"), true),
                }
            }
            Err(e) => self.show_toast(format!("업데이트 준비 실패: {e:#}"), true),
        }
    }

    fn sync_status_summary(&self, pal: &ThemePalette) -> (Color32, String, usize) {
        let st = self.sync_snap.status.as_ref();
        let pending: usize = st
            .map(|s| s.profiles.iter().filter_map(|p| p.pending_deletes).sum())
            .unwrap_or(0);

        if self.sync_snap.service_running && st.is_some_and(|s| s.syncing) {
            (pal.accent_purple, "동기화 중…".into(), pending)
        } else if self.sync_snap.service_running && st.and_then(|s| s.last_error.as_ref()).is_some()
        {
            (pal.danger, "확인 필요".into(), pending)
        } else if self.sync_snap.service_running {
            (pal.ok, "동기화 켜짐".into(), pending)
        } else if self.sync_snap.transitioning {
            (pal.warn, "전환 중…".into(), pending)
        } else {
            (pal.text_muted, "동기화 꺼짐".into(), pending)
        }
    }

    fn handle_shortcuts_and_clipboard(&mut self, ctx: &egui::Context) {
        let mut open_palette = false;
        let mut save_now = false;
        let mut open_daily = false;
        let mut toggle_graph = false;
        let mut toggle_sidebar = false;
        let mut new_note_shortcut = false;

        let body_focused = ctx.memory(|m| m.has_focus(Id::new(BODY_EDITOR_ID_STR)))
            && self.main_view == ActiveMainView::Editor
            && self.current_doc.as_ref().is_some_and(|d| !d.locked);

        let mut trigger_body_paste = false;
        let mut body_paste_shift = false;
        let mut fallback_paste_str: Option<String> = None;

        ctx.input_mut(|i| {
            if body_focused {
                body_paste_shift = i.modifiers.shift;
                let mut extracted = Vec::new();
                i.events.retain(|ev| {
                    if let egui::Event::Paste(s) = ev {
                        extracted.push(s.clone());
                        false
                    } else {
                        true
                    }
                });
                if !extracted.is_empty() {
                    trigger_body_paste = true;
                    fallback_paste_str = Some(extracted.join(""));
                }
                if i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::V) {
                    trigger_body_paste = true;
                    body_paste_shift = true;
                }
                if i.consume_key(egui::Modifiers::COMMAND, Key::V) {
                    trigger_body_paste = true;
                }
            } else {
                // 제목·검색창 등 일반 입력창에서도 붙여넣기 시 보이지 않는 제어 문자/제로폭 공백을 정리
                for ev in &mut i.events {
                    if let egui::Event::Paste(s) = ev {
                        *s = win_clipboard::sanitize_external_plain_text(s);
                    }
                }
            }

            if i.consume_key(egui::Modifiers::COMMAND, Key::K)
                || i.consume_key(egui::Modifiers::COMMAND, Key::P)
            {
                open_palette = true;
            }
            if i.consume_key(egui::Modifiers::COMMAND, Key::S) {
                save_now = true;
            }
            if i.consume_key(egui::Modifiers::COMMAND, Key::N) {
                new_note_shortcut = true;
            }
            if i.consume_key(egui::Modifiers::COMMAND, Key::D) {
                open_daily = true;
            }
            if i.consume_key(egui::Modifiers::COMMAND, Key::G) {
                toggle_graph = true;
            }
            if i.consume_key(egui::Modifiers::COMMAND, Key::Backslash) {
                toggle_sidebar = true;
            }
        });

        if trigger_body_paste {
            let clean_paste = self.build_clipboard_paste_text(body_paste_shift, fallback_paste_str);
            if !clean_paste.is_empty() {
                ctx.input_mut(|i| {
                    i.events.push(egui::Event::Paste(clean_paste));
                });
            }
        }

        if open_palette {
            self.show_command_palette = true;
            self.palette_query.clear();
            self.palette_hits.clear();
        }
        if save_now {
            self.flush_save(true);
        }
        if new_note_shortcut && self.workspace.active_vault.is_some() {
            let folder = self
                .current_doc
                .as_ref()
                .map(|d| parent_folder_of(&d.path))
                .unwrap_or_default();
            self.prompt_modal = Some((
                PromptKind::NewNote { folder },
                "📝 새 노트 만들기".into(),
                String::new(),
            ));
        }
        if open_daily {
            self.open_today_daily_note();
        }
        if toggle_graph {
            if self.main_view == ActiveMainView::Graph {
                self.main_view = ActiveMainView::Editor;
            } else {
                self.flush_save(false);
                self.main_view = ActiveMainView::Graph;
                self.refresh_global_graph();
            }
        }
        if toggle_sidebar {
            self.sidebar_collapsed = !self.sidebar_collapsed;
        }
    }

    fn poll_background(&mut self) {
        if let Some(ref t) = self.tray {
            t.sync_checked();
        }

        // 자동 저장 (900ms 디바운스)
        if self.save_status == SaveStatus::Dirty {
            if let Some(t) = self.last_edit_at {
                if t.elapsed() >= Duration::from_millis(900) {
                    self.flush_save(false);
                }
            }
        }

        // 백그라운드 동기화 및 볼트 변경 폴링
        if self.last_poll.elapsed() >= Duration::from_millis(450) {
            self.last_poll = Instant::now();
            let (rev, snap) = {
                let mut st = self.backend.lock().unwrap();
                if let Some(ref vp) = self.workspace.active_vault {
                    st.workspace.watcher.watch(Some(vp));
                }
                (st.workspace.watcher.revision(), st.sync.snapshot())
            };
            self.sync_snap = snap;
            if rev != self.vault_revision && self.save_status != SaveStatus::Dirty {
                self.rescan_active_vault();
            }
        }

        self.toasts.retain(|t| Instant::now() < t.expires_at);
    }
}

impl eframe::App for StudioApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        #[cfg(windows)]
        if self.hwnd == 0 {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(h) = frame.window_handle() {
                if let RawWindowHandle::Win32(w) = h.as_raw() {
                    let found = w.hwnd.get();
                    if found != 0 {
                        self.hwnd = found;
                        self.shared_hwnd.store(found, Ordering::SeqCst);
                        self.backend.lock().unwrap().updater.set_main_hwnd(found);
                    }
                }
            }
        }
        #[cfg(not(windows))]
        let _ = frame;

        if self.hide_on_start {
            self.hide_on_start = false;
            if !platform::hide_window(self.hwnd) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
        }

        // 창 닫기 버튼(X)을 누르면 종료하지 않고 시스템 트레이로 숨김
        if ctx.input(|i| i.viewport().close_requested()) && self.has_tray {
            if platform::hide_window(self.hwnd) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }

        if self.workspace.theme != self.last_theme {
            self.last_theme = self.workspace.theme.clone();
            ui::apply_theme(ctx, &self.last_theme);
        }
        let pal = ThemePalette::for_theme(&self.workspace.theme);

        self.handle_shortcuts_and_clipboard(ctx);
        self.poll_background();

        // 좌측 사이드바
        if !self.sidebar_collapsed {
            egui::SidePanel::left("studio_left_sidebar")
                .resizable(true)
                .default_width(268.0)
                .width_range(215.0..=390.0)
                .frame(
                    egui::Frame::none()
                        .fill(pal.bg_sidebar)
                        .stroke(Stroke::new(1.0_f32, pal.border_subtle))
                        .inner_margin(Margin::same(10.0)),
                )
                .show(ctx, |ui| {
                    self.render_sidebar(ui, &pal);
                });
        }

        // 우측 인스펙터 패널
        if self.main_view == ActiveMainView::Editor && self.inspector_open {
            egui::SidePanel::right("studio_right_inspector")
                .resizable(true)
                .default_width(292.0)
                .width_range(240.0..=420.0)
                .frame(
                    egui::Frame::none()
                        .fill(pal.bg_secondary)
                        .stroke(Stroke::new(1.0_f32, pal.border_subtle))
                        .inner_margin(Margin::same(10.0)),
                )
                .show(ctx, |ui| {
                    self.render_inspector(ui, &pal);
                });
        }

        // 중앙 메인 워크스페이스
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(pal.bg_main))
            .show(ctx, |ui| {
                self.render_tabs_strip(ui, &pal);
                self.render_topbar(ui, &pal);

                match self.main_view {
                    ActiveMainView::Editor => self.render_editor_workspace(ui, &pal),
                    ActiveMainView::Graph => self.render_global_graph_view(ui, &pal),
                    ActiveMainView::Sync => {
                        let backend_clone = self.backend.clone();
                        let mut snap_clone = self.sync_snap.clone();
                        let act = self.sync_ui.ui(ui, &pal, &backend_clone, &mut snap_clone);
                        self.sync_snap = snap_clone;
                        if let Some(a) = act {
                            match a {
                                SyncUiAction::OpenVaultInEditor(vp) => self.select_vault(&vp),
                                SyncUiAction::Toast(m, err) => self.show_toast(m, err),
                                SyncUiAction::ConfigSaved => {
                                    let ws = self.backend.lock().unwrap().workspace.state.clone();
                                    self.workspace = ws;
                                    self.rescan_active_vault();
                                }
                                SyncUiAction::ApplyUpdateAndRestart => {
                                    self.apply_update_and_restart();
                                }
                            }
                        }
                    }
                }
            });

        self.render_modals(ctx, &pal);
        self.render_toasts(ctx, &pal);

        ctx.request_repaint_after(Duration::from_millis(500));
    }
}

impl StudioApp {
    fn render_sidebar(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        let active_vault = self.workspace.active_vault.clone();
        let vault_name = self
            .workspace
            .vaults
            .iter()
            .find(|v| Some(&v.path) == active_vault.as_ref())
            .map(|v| v.name.clone())
            .or_else(|| {
                active_vault.as_ref().map(|p| {
                    p.trim_end_matches(['/', '\\'])
                        .rsplit(['/', '\\'])
                        .next()
                        .unwrap_or("볼트")
                        .to_string()
                })
            })
            .unwrap_or_else(|| "볼트 선택…".into());
        let avatar_ch = vault_name
            .chars()
            .next()
            .unwrap_or('O')
            .to_uppercase()
            .to_string();

        // 상단 볼트 스위처 헤더
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(rect, Rounding::same(6.0), pal.accent_purple);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                &avatar_ch,
                egui::FontId::proportional(12.5),
                Color32::WHITE,
            );

            egui::ComboBox::from_id_salt("vault_switcher_combo")
                .selected_text(RichText::new(&vault_name).strong().size(13.5))
                .width(168.0)
                .show_ui(ui, |ui| {
                    let vaults_list = self.workspace.vaults.clone();
                    for v in vaults_list {
                        let is_cur = active_vault.as_deref() == Some(v.path.as_str());
                        let label = format!(
                            "{} {} ({})",
                            if v.is_sync_profile { "🔄" } else { "📁" },
                            v.name,
                            v.path
                        );
                        if ui.selectable_label(is_cur, label).clicked() && !is_cur {
                            self.select_vault(&v.path);
                        }
                    }
                    ui.separator();
                    if ui.button("📂 폴더 열기… (기존 볼트 추가)").clicked() {
                        self.handle_open_vault_dialog();
                        ui.close_menu();
                    }
                    if ui.button("✨ 샘플 볼트 시작하기").clicked() {
                        self.handle_create_sample_vault();
                        ui.close_menu();
                    }
                    if let Some(ref vp) = active_vault {
                        ui.separator();
                        if ui.button("🔒 볼트 · 노트 암호화 관리").clicked() {
                            self.show_crypto_modal = true;
                            ui.close_menu();
                        }
                        if ui.button("💻 탐색기에서 볼트 폴더 열기").clicked() {
                            let _ = open::that(vp);
                            ui.close_menu();
                        }
                    }
                });

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .button("✏️")
                    .on_hover_text("새 노트 작성 (Ctrl+N)")
                    .clicked()
                {
                    self.prompt_modal = Some((
                        PromptKind::NewNote {
                            folder: String::new(),
                        },
                        "📝 새 노트 만들기".into(),
                        String::new(),
                    ));
                }
            });
        });

        ui.add_space(6.0);

        let (sync_dot, sync_label, pending_del) = self.sync_status_summary(pal);

        if Self::sidebar_nav_btn(ui, pal, "🔍", "빠른 검색 / 명령어", Some("Ctrl+K"), false)
            .clicked()
        {
            self.show_command_palette = true;
            self.palette_query.clear();
        }
        if Self::sidebar_nav_btn(ui, pal, "📅", "오늘의 일일 노트", Some("Ctrl+D"), false).clicked()
        {
            self.open_today_daily_note();
        }
        if Self::sidebar_nav_btn(
            ui,
            pal,
            "🕸️",
            "그래프 뷰",
            Some("Ctrl+G"),
            self.main_view == ActiveMainView::Graph,
        )
        .clicked()
        {
            if self.main_view == ActiveMainView::Graph {
                self.main_view = ActiveMainView::Editor;
            } else {
                self.flush_save(false);
                self.main_view = ActiveMainView::Graph;
                self.refresh_global_graph();
            }
        }

        // 동기화 센터 버튼
        let sync_active = self.main_view == ActiveMainView::Sync;
        egui::Frame::none()
            .fill(if sync_active {
                pal.bg_active
            } else {
                Color32::TRANSPARENT
            })
            .rounding(Rounding::same(6.0))
            .inner_margin(Margin::symmetric(8.0, 5.0))
            .show(ui, |ui| {
                let resp = ui
                    .horizontal(|ui| {
                        ui.label(RichText::new("🔄").size(14.0));
                        ui.label(
                            RichText::new("동기화 센터")
                                .size(13.5)
                                .color(if sync_active {
                                    pal.accent_purple
                                } else {
                                    pal.text_primary
                                })
                                .strong(),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui::status_dot(ui, sync_dot, 8.0);
                            if pending_del > 0 {
                                ui::pill_badge(
                                    ui,
                                    &format!("삭제보류 {pending_del}"),
                                    pal.warn,
                                    pal.warn_bg,
                                );
                            }
                        });
                    })
                    .response
                    .interact(egui::Sense::click());
                if resp.clicked() {
                    self.flush_save(false);
                    self.main_view = ActiveMainView::Sync;
                }
            });

        ui.separator();

        // 볼트 트리 영역
        let footer_h = 36.0;
        let avail_h = (ui.available_height() - footer_h).max(100.0);
        egui::ScrollArea::vertical()
            .id_salt("sidebar_tree_scroll")
            .max_height(avail_h)
            .show(ui, |ui| {
                let Some(scan) = self.scan.clone() else {
                    ui.add_space(16.0);
                    ui.label(
                        RichText::new("볼트 폴더를 열거나 샘플 볼트를 만들어 노트를 시작하세요.")
                            .size(12.5)
                            .color(pal.text_muted),
                    );
                    return;
                };

                // 1) 즐겨찾기
                let fav_notes: Vec<_> = self
                    .workspace
                    .favorites
                    .iter()
                    .filter_map(|fp| scan.notes.iter().find(|n| &n.path == fp).cloned())
                    .collect();

                if !fav_notes.is_empty() {
                    ui.label(
                        RichText::new("⭐ 즐겨찾기")
                            .size(11.5)
                            .strong()
                            .color(pal.text_muted),
                    );
                    for meta in fav_notes {
                        let is_cur = self.main_view == ActiveMainView::Editor
                            && self
                                .current_doc
                                .as_ref()
                                .is_some_and(|d| d.path == meta.path);
                        let label =
                            format!("{} {}", meta.icon.as_deref().unwrap_or("📄"), meta.title);
                        let resp = ui.selectable_label(is_cur, label);
                        if resp.clicked() {
                            self.open_note(&meta.path);
                        }
                        resp.context_menu(|ui| {
                            if ui.button("☆ 즐겨찾기 해제").clicked() {
                                self.toggle_favorite(&meta.path);
                                ui.close_menu();
                            }
                        });
                    }
                    ui.add_space(8.0);
                }

                // 2) 페이지 트리 헤더 (모두 펼치기 ⊞ / 모두 접기 ⊟ 포함)
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("📄 페이지 ({})", scan.total_notes))
                            .size(11.5)
                            .strong()
                            .color(pal.text_muted),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("📁").on_hover_text("새 폴더").clicked() {
                            self.prompt_modal = Some((
                                PromptKind::NewFolder {
                                    parent: String::new(),
                                },
                                "📁 새 폴더 만들기".into(),
                                String::new(),
                            ));
                        }
                        if ui.small_button("＋").on_hover_text("새 노트").clicked() {
                            self.prompt_modal = Some((
                                PromptKind::NewNote {
                                    folder: String::new(),
                                },
                                "📝 새 노트 만들기".into(),
                                String::new(),
                            ));
                        }
                        if ui
                            .small_button("⊟")
                            .on_hover_text("모든 폴더 접기")
                            .clicked()
                        {
                            self.expanded_folders.clear();
                        }
                        if ui
                            .small_button("⊞")
                            .on_hover_text("모든 폴더 펼치기")
                            .clicked()
                        {
                            collect_all_folder_paths(&scan.tree, &mut self.expanded_folders);
                        }
                    });
                });

                if scan.tree.is_empty() {
                    ui.label(
                        RichText::new("아직 노트가 없습니다. ＋ 버튼으로 첫 노트를 작성하세요.")
                            .size(12.0)
                            .color(pal.text_muted),
                    );
                } else {
                    for node in &scan.tree {
                        self.render_tree_node(ui, pal, node, 0);
                    }
                }

                // 3) 태그 클라우드 (토글 가능)
                if !scan.tags.is_empty() {
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("🏷️ 태그 ({})", scan.tags.len()))
                                .size(11.5)
                                .strong()
                                .color(pal.text_muted),
                        );
                        if ui
                            .small_button(if self.workspace.show_tags {
                                "숨기기"
                            } else {
                                "펼치기"
                            })
                            .clicked()
                        {
                            self.workspace.show_tags = !self.workspace.show_tags;
                            self.persist_workspace();
                        }
                    });
                    if self.workspace.show_tags {
                        ui.horizontal_wrapped(|ui| {
                            for t in &scan.tags {
                                let clean = t.tag.trim_start_matches('#');
                                if ui.small_button(format!("#{clean} ({})", t.count)).clicked() {
                                    self.show_command_palette = true;
                                    self.palette_query = format!("tag:#{clean}");
                                    if let Some(ref vp) = self.workspace.active_vault {
                                        let st = self.backend.lock().unwrap();
                                        self.palette_hits = vault::search_vault_with_session(
                                            Path::new(vp),
                                            &self.palette_query,
                                            Some(&st.crypto),
                                        )
                                        .unwrap_or_default();
                                    }
                                }
                            }
                        });
                    }
                }
            });

        ui.separator();

        // 하단 푸터 (휴지통 + 암호화 + 실시간 동기화 상태)
        ui.horizontal(|ui| {
            if ui
                .add_enabled(active_vault.is_some(), egui::Button::new("🗑️"))
                .on_hover_text("볼트 휴지통 (.trash)")
                .clicked()
            {
                if let Some(ref vp) = active_vault {
                    self.trash_items = vault::list_trash(Path::new(vp)).unwrap_or_default();
                    self.show_trash_modal = true;
                }
            }
            if ui
                .add_enabled(active_vault.is_some(), egui::Button::new("🔒"))
                .on_hover_text("노트 · 볼트 AES-256 암호화 관리")
                .clicked()
            {
                self.show_crypto_modal = true;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button(&sync_label).clicked() {
                    self.flush_save(false);
                    self.main_view = ActiveMainView::Sync;
                }
                ui::status_dot(ui, sync_dot, 8.0);
            });
        });
    }

    fn sidebar_nav_btn(
        ui: &mut egui::Ui,
        pal: &ThemePalette,
        icon: &str,
        label: &str,
        shortcut: Option<&str>,
        active: bool,
    ) -> egui::Response {
        let frame = egui::Frame::none()
            .fill(if active {
                pal.bg_active
            } else {
                Color32::TRANSPARENT
            })
            .rounding(Rounding::same(6.0))
            .inner_margin(Margin::symmetric(8.0, 4.0));
        let resp = frame
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(icon).size(14.0));
                    ui.label(
                        RichText::new(label)
                            .size(13.5)
                            .color(if active {
                                pal.accent_purple
                            } else {
                                pal.text_primary
                            })
                            .strong(),
                    );
                    if let Some(sc) = shortcut {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(sc).size(11.0).color(pal.text_muted));
                        });
                    }
                });
            })
            .response
            .interact(egui::Sense::click());
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        resp
    }

    fn render_tree_node(
        &mut self,
        ui: &mut egui::Ui,
        pal: &ThemePalette,
        node: &TreeNodeDto,
        depth: usize,
    ) {
        let is_folder = node.kind == "folder";
        let expanded = is_folder && self.expanded_folders.contains(&node.path);
        let is_active = !is_folder
            && self.main_view == ActiveMainView::Editor
            && self
                .current_doc
                .as_ref()
                .is_some_and(|d| d.path == node.path);

        ui.horizontal(|ui| {
            ui.add_space(depth as f32 * 13.0);
            let icon = if is_folder {
                if let Some(custom_ic) = node.icon.as_deref() {
                    custom_ic
                } else if expanded {
                    "📂"
                } else {
                    "📁"
                }
            } else if node.locked {
                "🔒"
            } else {
                node.icon.as_deref().unwrap_or("📄")
            };
            let display_name = if is_folder {
                node.name.as_str()
            } else {
                strip_md_ext(&node.name)
            };
            let lock_suffix = if !is_folder && node.encrypted && !node.locked {
                " 🔓"
            } else {
                ""
            };
            let text = format!("{icon} {display_name}{lock_suffix}");

            let custom_folder_color = if is_folder {
                node.color.as_deref().and_then(ui::parse_hex_color)
            } else {
                None
            };
            let fg_color = if is_active {
                pal.accent_purple
            } else if let Some(fc) = custom_folder_color {
                fc
            } else {
                pal.text_primary
            };

            let resp =
                ui.selectable_label(is_active, RichText::new(text).size(13.5).color(fg_color));
            if resp.clicked() {
                if is_folder {
                    if expanded {
                        self.expanded_folders.remove(&node.path);
                    } else {
                        self.expanded_folders.insert(node.path.clone());
                    }
                } else {
                    self.open_note(&node.path);
                }
            }

            resp.context_menu(|ui| {
                if is_folder {
                    if ui.button("📝 이 폴더에 새 노트").clicked() {
                        self.expanded_folders.insert(node.path.clone());
                        self.prompt_modal = Some((
                            PromptKind::NewNote {
                                folder: node.path.clone(),
                            },
                            format!("📝 새 노트 ({})", node.path),
                            String::new(),
                        ));
                        ui.close_menu();
                    }
                    if ui.button("📁 새 하위 폴더").clicked() {
                        self.expanded_folders.insert(node.path.clone());
                        self.prompt_modal = Some((
                            PromptKind::NewFolder {
                                parent: node.path.clone(),
                            },
                            format!("📁 새 하위 폴더 ({})", node.path),
                            String::new(),
                        ));
                        ui.close_menu();
                    }
                    if ui.button("🎨 폴더 아이콘 · 색상 변경").clicked() {
                        self.folder_style_modal = Some(FolderStyleModalState {
                            folder_path: node.path.clone(),
                            folder_name: node.name.clone(),
                            icon: node.icon.clone().unwrap_or_default(),
                            color: node.color.clone().unwrap_or_default(),
                        });
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("✏️ 이름 변경").clicked() {
                        self.prompt_modal = Some((
                            PromptKind::Rename {
                                old_path: node.path.clone(),
                            },
                            "✏️ 폴더 이름 변경".into(),
                            node.name.clone(),
                        ));
                        ui.close_menu();
                    }
                    if ui.button("🗑️ 휴지통으로 이동").clicked() {
                        if let Some(ref vp) = self.workspace.active_vault.clone() {
                            let ok = {
                                let mut st = self.backend.lock().unwrap();
                                vault::delete_to_trash_with_session(
                                    Path::new(vp),
                                    &node.path,
                                    Some(&mut st.crypto),
                                )
                                .is_ok()
                            };
                            if ok {
                                self.rescan_active_vault();
                                self.show_toast("휴지통으로 이동했습니다.", false);
                            }
                        }
                        ui.close_menu();
                    }
                } else {
                    let is_fav = self.workspace.favorites.contains(&node.path);
                    if ui
                        .button(if is_fav {
                            "☆ 즐겨찾기 해제"
                        } else {
                            "⭐ 즐겨찾기에 고정"
                        })
                        .clicked()
                    {
                        self.toggle_favorite(&node.path);
                        ui.close_menu();
                    }
                    if ui.button("◫ 우측 분할 보기로 열기").clicked() {
                        self.open_split_note(&node.path);
                        ui.close_menu();
                    }
                    if ui.button("🔒 암호화 설정").clicked() {
                        self.open_note(&node.path);
                        self.show_crypto_modal = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("✏️ 이름 변경 (위키링크 자동 갱신)").clicked() {
                        self.prompt_modal = Some((
                            PromptKind::Rename {
                                old_path: node.path.clone(),
                            },
                            "✏️ 노트 이름 변경".into(),
                            strip_md_ext(&node.name).to_string(),
                        ));
                        ui.close_menu();
                    }
                    if ui.button("📑 노트 복제").clicked() {
                        if let Some(ref vp) = self.workspace.active_vault.clone() {
                            if let Ok(dup_path) = vault::duplicate_note(Path::new(vp), &node.path) {
                                self.rescan_active_vault();
                                self.open_note(&dup_path);
                                self.show_toast(format!("노트 복제됨: {dup_path}"), false);
                            }
                        }
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("🗑️ 휴지통으로 이동").clicked() {
                        let p_clone = node.path.clone();
                        if let Some(ref vp) = self.workspace.active_vault.clone() {
                            let ok = {
                                let mut st = self.backend.lock().unwrap();
                                vault::delete_to_trash_with_session(
                                    Path::new(vp),
                                    &p_clone,
                                    Some(&mut st.crypto),
                                )
                                .is_ok()
                            };
                            if ok {
                                self.close_tab(&p_clone);
                                self.rescan_active_vault();
                                self.show_toast("휴지통(.trash)으로 이동했습니다.", false);
                            }
                        }
                        ui.close_menu();
                    }
                }
            });
        });

        if is_folder && expanded {
            for child in &node.children {
                self.render_tree_node(ui, pal, child, depth + 1);
            }
        }
    }

    fn render_tabs_strip(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        egui::Frame::none()
            .fill(pal.bg_sidebar)
            .stroke(Stroke::new(1.0_f32, pal.border_subtle))
            .inner_margin(Margin::symmetric(8.0, 4.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .button("☰")
                        .on_hover_text("사이드바 접기/펼치기 (Ctrl+\\)")
                        .clicked()
                    {
                        self.sidebar_collapsed = !self.sidebar_collapsed;
                    }

                    egui::ScrollArea::horizontal()
                        .id_salt("open_tabs_scroll")
                        .show(ui, |ui| {
                            let tabs = self.workspace.open_tabs.clone();
                            for tab_path in tabs {
                                let (icon, title) = {
                                    let meta = self
                                        .scan
                                        .as_ref()
                                        .and_then(|s| s.notes.iter().find(|n| n.path == tab_path));
                                    let ic = meta
                                        .and_then(|m| {
                                            if m.locked {
                                                Some("🔒".to_string())
                                            } else {
                                                m.icon.clone()
                                            }
                                        })
                                        .unwrap_or_else(|| "📄".to_string());
                                    let ti = meta.map(|m| m.title.clone()).unwrap_or_else(|| {
                                        strip_md_ext(
                                            tab_path.rsplit('/').next().unwrap_or(&tab_path),
                                        )
                                        .to_string()
                                    });
                                    (ic, ti)
                                };
                                let active = self.main_view == ActiveMainView::Editor
                                    && self
                                        .current_doc
                                        .as_ref()
                                        .is_some_and(|d| d.path == tab_path);

                                egui::Frame::none()
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
                                    .inner_margin(Margin::symmetric(8.0, 3.0))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            if ui
                                                .add(
                                                    egui::Button::new(
                                                        RichText::new(format!("{icon} {title}"))
                                                            .size(12.5)
                                                            .strong()
                                                            .color(if active {
                                                                pal.accent_purple
                                                            } else {
                                                                pal.text_secondary
                                                            }),
                                                    )
                                                    .fill(Color32::TRANSPARENT)
                                                    .stroke(Stroke::NONE),
                                                )
                                                .clicked()
                                            {
                                                self.open_note(&tab_path);
                                            }
                                            if ui.small_button("✕").clicked() {
                                                self.close_tab(&tab_path);
                                            }
                                        });
                                    });
                            }
                        });

                    if ui
                        .button("＋")
                        .on_hover_text("새 노트 만들기 (Ctrl+N)")
                        .clicked()
                    {
                        let folder = self
                            .current_doc
                            .as_ref()
                            .map(|d| parent_folder_of(&d.path))
                            .unwrap_or_default();
                        self.prompt_modal = Some((
                            PromptKind::NewNote { folder },
                            "📝 새 노트 만들기".into(),
                            String::new(),
                        ));
                    }
                });
            });
    }

    fn render_topbar(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        let up_status = self.backend.lock().unwrap().updater.status();

        egui::Frame::none()
            .fill(pal.bg_main)
            .stroke(Stroke::new(1.0_f32, pal.border_subtle))
            .inner_margin(Margin::symmetric(14.0, 6.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    match self.main_view {
                        ActiveMainView::Graph => {
                            ui.label(
                                RichText::new("🕸️ 전체 볼트 지식 그래프")
                                    .strong()
                                    .size(13.5),
                            );
                        }
                        ActiveMainView::Sync => {
                            ui.label(RichText::new("🔄 실시간 동기화 센터").strong().size(13.5));
                        }
                        ActiveMainView::Editor => {
                            if let Some(doc) = &self.current_doc {
                                let icon = if doc.locked {
                                    "🔒"
                                } else {
                                    self.draft_icon.as_deref().unwrap_or("📄")
                                };
                                ui.label(
                                    RichText::new(format!("{icon} {}", doc.path))
                                        .strong()
                                        .size(13.0)
                                        .color(pal.text_secondary),
                                );
                                if doc.encrypted && !doc.locked {
                                    ui::pill_badge(
                                        ui,
                                        "🔓 AES-256 암호화됨",
                                        pal.accent_purple,
                                        pal.accent_purple_soft,
                                    );
                                }
                            } else {
                                ui.label(
                                    RichText::new("노트를 선택하거나 새로 작성하세요")
                                        .size(13.0)
                                        .color(pal.text_muted),
                                );
                            }
                        }
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.main_view == ActiveMainView::Editor {
                            if ui
                                .selectable_label(self.inspector_open, "◨ 인스펙터")
                                .on_hover_text("백링크·목차·로컬 그래프")
                                .clicked()
                            {
                                self.inspector_open = !self.inspector_open;
                            }
                        }

                        let is_dark = self.workspace.theme.eq_ignore_ascii_case("dark");
                        if ui
                            .button(if is_dark {
                                "☀️ 라이트"
                            } else {
                                "🌙 다크"
                            })
                            .clicked()
                        {
                            self.workspace.theme = if is_dark {
                                "light".into()
                            } else {
                                "dark".into()
                            };
                            self.persist_workspace();
                        }

                        // GitHub 자동 업데이트 준비 배지
                        if up_status.ready_to_install {
                            let tag = up_status.available_tag.as_deref().unwrap_or("최신");
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new(format!("⬆️ {tag} 업데이트 재시작"))
                                            .size(12.0)
                                            .color(Color32::WHITE)
                                            .strong(),
                                    )
                                    .fill(pal.ok)
                                    .rounding(Rounding::same(99.0)),
                                )
                                .clicked()
                            {
                                self.apply_update_and_restart();
                            }
                        }

                        if self.main_view == ActiveMainView::Editor && self.current_doc.is_some() {
                            if ui
                                .button("🔒 암호화")
                                .on_hover_text("이 노트 또는 볼트 전체 AES-256 암호화/잠금")
                                .clicked()
                            {
                                self.show_crypto_modal = true;
                            }

                            if ui
                                .selectable_label(self.split_doc.is_some(), "◫ 분할 보기")
                                .clicked()
                            {
                                if self.split_doc.is_some() {
                                    self.close_split_note();
                                } else if let Some(p) = self
                                    .workspace
                                    .open_tabs
                                    .iter()
                                    .find(|t| {
                                        Some(t.as_str())
                                            != self.current_doc.as_ref().map(|d| d.path.as_str())
                                    })
                                    .cloned()
                                    .or_else(|| self.current_doc.as_ref().map(|d| d.path.clone()))
                                {
                                    self.open_split_note(&p);
                                }
                            }

                            let is_md_split = self.workspace.editor_mode == "markdown";
                            if ui.selectable_label(!is_md_split, "통합 편집").clicked()
                                && is_md_split
                            {
                                self.workspace.editor_mode = "wysiwyg".into();
                                self.persist_workspace();
                            }
                            if ui
                                .selectable_label(is_md_split, "좌우 분할 프리뷰")
                                .clicked()
                                && !is_md_split
                            {
                                self.workspace.editor_mode = "markdown".into();
                                self.persist_workspace();
                            }

                            match self.save_status {
                                SaveStatus::Saved => {
                                    ui::pill_badge(ui, "✓ 저장됨", pal.ok, pal.ok_bg);
                                }
                                SaveStatus::Dirty => {
                                    if ui
                                        .button(
                                            RichText::new("● 변경됨 (저장)")
                                                .size(11.5)
                                                .color(pal.warn),
                                        )
                                        .clicked()
                                    {
                                        self.flush_save(true);
                                    }
                                }
                                SaveStatus::Error => {
                                    ui::pill_badge(ui, "⚠ 저장 오류", pal.danger, pal.danger_bg);
                                }
                            }
                        }
                    });
                });
            });
    }

    fn render_editor_workspace(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        if self.workspace.active_vault.is_none() {
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(80.0);
                    ui.label(RichText::new("💎").size(48.0));
                    ui.label(
                        RichText::new("Onyx Studio에 오신 것을 환영합니다")
                            .size(22.0)
                            .strong(),
                    );
                    ui.label(
                        RichText::new(
                            "100% 순수 네이티브 Rust · Zero WebView2 · Zero 로컬 포트 환경에서\n노션 스타일 속성·이미지가 살아있는 마크다운 편집기, 지식 그래프, 5종 클라우드/NAS 동기화를 경험하세요.",
                        )
                        .size(14.0)
                        .color(pal.text_secondary),
                    );
                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        if ui::primary_button(ui, pal, "✨ 샘플 볼트 시작하기").clicked() {
                            self.handle_create_sample_vault();
                        }
                        if ui.button("📂 내 PC 볼트 폴더 열기…").clicked() {
                            self.handle_open_vault_dialog();
                        }
                        if ui.button("🔄 동기화 센터 설정").clicked() {
                            self.main_view = ActiveMainView::Sync;
                        }
                    });
                });
            });
            return;
        }

        if self.current_doc.is_none() {
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(100.0);
                    ui.label(RichText::new("✍️").size(42.0));
                    ui.label(
                        RichText::new("열려 있는 노트가 없습니다")
                            .size(18.0)
                            .strong(),
                    );
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui::primary_button(ui, pal, "＋ 새 노트 만들기").clicked() {
                            self.prompt_modal = Some((
                                PromptKind::NewNote {
                                    folder: String::new(),
                                },
                                "📝 새 노트 만들기".into(),
                                String::new(),
                            ));
                        }
                        if ui.button("📅 오늘의 일일 노트").clicked() {
                            self.open_today_daily_note();
                        }
                    });
                });
            });
            return;
        }

        // 잠긴 암호화 노트일 경우 잠금 해제 화면 표시
        if self.current_doc.as_ref().is_some_and(|d| d.locked) {
            self.render_locked_note_screen(ui, pal);
            return;
        }

        let has_split = self.split_doc.is_some();
        let total_w = ui.available_width();
        let left_w = if has_split { total_w * 0.56 } else { total_w };

        ui.horizontal_top(|ui| {
            ui.allocate_ui(egui::vec2(left_w, ui.available_height()), |ui| {
                self.render_main_note_pane(ui, pal);
            });

            if has_split {
                ui.separator();
                ui.allocate_ui(
                    egui::vec2(ui.available_width(), ui.available_height()),
                    |ui| {
                        self.render_split_note_pane(ui, pal);
                    },
                );
            }
        });
    }

    fn render_locked_note_screen(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        let Some(doc) = self.current_doc.clone() else {
            return;
        };
        let Some(vp) = self.workspace.active_vault.clone() else {
            return;
        };

        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            egui::Frame::none()
                .fill(pal.bg_card)
                .stroke(Stroke::new(1.0_f32, pal.border_subtle))
                .rounding(Rounding::same(12.0))
                .inner_margin(Margin::same(24.0))
                .show(ui, |ui| {
                    ui.set_max_width(420.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("🔒").size(40.0));
                        ui.label(
                            RichText::new(format!("'{}' 노트가 암호화되어 있습니다", doc.title))
                                .size(17.5)
                                .strong(),
                        );
                        ui.label(
                            RichText::new("AES-256-GCM 보호 노트를 열려면 비밀번호를 입력하세요.")
                                .size(12.5)
                                .color(pal.text_secondary),
                        );
                        if let Some(hint) = &doc.encryption_hint {
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(format!("💡 힌트: {hint}"))
                                    .size(12.0)
                                    .color(pal.accent_purple),
                            );
                        }
                        ui.add_space(12.0);
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.unlock_password_input)
                                .password(true)
                                .hint_text("비밀번호 입력 후 Enter")
                                .desired_width(280.0),
                        );
                        let enter_pressed =
                            resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if (ui::primary_button(ui, pal, "🔓 이 노트 잠금 해제").clicked()
                                || enter_pressed)
                                && !self.unlock_password_input.is_empty()
                            {
                                let pw = self.unlock_password_input.clone();
                                let res = {
                                    let mut st = self.backend.lock().unwrap();
                                    vault::unlock_single_note(
                                        Path::new(&vp),
                                        &doc.path,
                                        &pw,
                                        &mut st.crypto,
                                    )
                                };
                                match res {
                                    Ok(unlocked_doc) => {
                                        self.apply_loaded_doc(unlocked_doc, false);
                                        self.rescan_active_vault();
                                        self.show_toast("노트 잠금을 해제했습니다.", false);
                                    }
                                    Err(e) => {
                                        self.show_toast(format!("잠금 해제 실패: {e:#}"), true);
                                    }
                                }
                            }
                            if ui.button("볼트 전체 잠금 해제").clicked()
                                && !self.unlock_password_input.is_empty()
                            {
                                let pw = self.unlock_password_input.clone();
                                let res = {
                                    let mut st = self.backend.lock().unwrap();
                                    vault::unlock_entire_vault(Path::new(&vp), &pw, &mut st.crypto)
                                };
                                match res {
                                    Ok(batch) => {
                                        self.rescan_active_vault();
                                        self.current_doc = None;
                                        self.open_note(&doc.path);
                                        self.show_toast(
                                            format!(
                                                "볼트 전체 잠금 해제 완료 ({}개 노트)",
                                                batch.unlocked_count
                                            ),
                                            false,
                                        );
                                    }
                                    Err(e) => {
                                        self.show_toast(format!("잠금 해제 실패: {e:#}"), true);
                                    }
                                }
                            }
                        });
                    });
                });
        });
    }

    fn render_main_note_pane(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        let mut clicked_wikilink: Option<String> = None;
        let vault_path_opt = self.workspace.active_vault.clone();
        let vault_root_ref = vault_path_opt.as_deref().map(Path::new);

        egui::ScrollArea::vertical()
            .id_salt("main_note_editor_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // 커버 배너
                if let Some(cover_str) = self.draft_cover.clone() {
                    let cover_color = COVER_PRESETS
                        .iter()
                        .find(|(name, _)| cover_str.contains(name))
                        .map(|(_, c)| *c)
                        .unwrap_or(pal.accent_purple);
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 88.0),
                        egui::Sense::hover(),
                    );
                    ui.painter().rect_filled(rect, Rounding::ZERO, cover_color);
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        &cover_str,
                        egui::FontId::proportional(13.0),
                        Color32::from_rgba_unmultiplied(255, 255, 255, 190),
                    );
                }

                egui::Frame::none()
                    .inner_margin(Margin::symmetric(28.0, 16.0))
                    .show(ui, |ui| {
                        // 상단 노션 스타일 컨트롤 바
                        ui.horizontal_wrapped(|ui| {
                            let icon_txt = self.draft_icon.as_deref().unwrap_or("😀 아이콘");
                            if ui
                                .button(RichText::new(icon_txt).size(15.0))
                                .on_hover_text("페이지 아이콘 변경")
                                .clicked()
                            {
                                self.show_emoji_picker = true;
                            }
                            if ui.button("🖼️ 커버").clicked() {
                                self.show_cover_picker = true;
                            }
                            if self.draft_cover.is_some() && ui.small_button("커버 제거").clicked()
                            {
                                self.draft_cover = None;
                                self.mark_dirty();
                            }
                            if ui.button("✨ 템플릿").clicked() {
                                if let Some(ref vp) = self.workspace.active_vault {
                                    self.loaded_templates = vault::list_templates(
                                        Path::new(vp),
                                        &self.workspace.templates_folder,
                                    )
                                    .unwrap_or_default();
                                    self.show_templates_modal = true;
                                }
                            }
                            if let Some(cur_path) =
                                self.current_doc.as_ref().map(|d| d.path.clone())
                            {
                                let is_fav = self.workspace.favorites.contains(&cur_path);
                                if ui
                                    .button(if is_fav {
                                        "⭐ 즐겨찾기됨"
                                    } else {
                                        "☆ 즐겨찾기"
                                    })
                                    .clicked()
                                {
                                    self.toggle_favorite(&cur_path);
                                }
                            }
                            if self.current_doc.as_ref().is_some_and(|d| d.encrypted)
                                && ui
                                    .button("🔒 지금 잠그기")
                                    .on_hover_text("편집 내용을 저장하고 이 노트를 다시 잠급니다")
                                    .clicked()
                            {
                                self.flush_save(false);
                                if let (Some(vp), Some(cur)) = (
                                    self.workspace.active_vault.clone(),
                                    self.current_doc.as_ref().map(|d| d.path.clone()),
                                ) {
                                    let res = {
                                        let mut st = self.backend.lock().unwrap();
                                        vault::lock_single_note(Path::new(&vp), &cur, &mut st.crypto)
                                    };
                                    if let Ok(locked_doc) = res {
                                        self.apply_loaded_doc(locked_doc, false);
                                        self.rescan_active_vault();
                                        self.show_toast("노트를 잠갔습니다.", false);
                                        return;
                                    }
                                }
                            }
                        });

                        ui.add_space(6.0);

                        // 노트 대제목 입력
                        let title_resp = ui.add(
                            egui::TextEdit::singleline(&mut self.draft_title)
                                .font(egui::FontId::proportional(26.0))
                                .hint_text("제목 없음")
                                .desired_width(f32::INFINITY),
                        );
                        if title_resp.changed() {
                            self.mark_dirty();
                        }

                        ui.add_space(8.0);

                        self.render_properties_editor(ui, pal);

                        ui.separator();

                        self.render_markdown_toolbar(ui, pal);

                        ui.add_space(6.0);

                        // 본문 편집기 + 실시간 인라인 이미지/위키링크 미리보기
                        let is_split_md = self.workspace.editor_mode == "markdown";
                        let body_id = Id::new(BODY_EDITOR_ID_STR);

                        if is_split_md {
                            ui.columns(2, |cols| {
                                cols[0].label(
                                    RichText::new("✍️ 본문 편집 (Ctrl+V: 일반 텍스트 · 이미지 자동 정리)")
                                        .size(11.5)
                                        .strong()
                                        .color(pal.text_muted),
                                );
                                let body_resp = cols[0].add(
                                    egui::TextEdit::multiline(&mut self.draft_body)
                                        .id(body_id)
                                        .font(egui::FontId::monospace(13.5))
                                        .desired_rows(24)
                                        .desired_width(f32::INFINITY)
                                        .lock_focus(true),
                                );
                                if body_resp.changed() {
                                    self.mark_dirty();
                                }
                                self.attach_body_context_menu(&body_resp);

                                cols[1].label(
                                    RichText::new("👁️ 실시간 미리보기 (인라인 이미지 · 위키링크)")
                                        .size(11.5)
                                        .strong()
                                        .color(pal.text_muted),
                                );
                                egui::Frame::none()
                                    .fill(pal.bg_card)
                                    .stroke(Stroke::new(1.0_f32, pal.border_subtle))
                                    .rounding(Rounding::same(8.0))
                                    .inner_margin(Margin::same(12.0))
                                    .show(&mut cols[1], |ui| {
                                        if let Some(w) = render_markdown_preview(
                                            ui,
                                            pal,
                                            &self.draft_body,
                                            vault_root_ref,
                                            &mut self.img_cache,
                                        ) {
                                            clicked_wikilink = Some(w);
                                        }
                                    });
                            });
                        } else {
                            let body_resp = ui.add(
                                egui::TextEdit::multiline(&mut self.draft_body)
                                    .id(body_id)
                                    .font(egui::FontId::proportional(14.5))
                                    .desired_rows(15)
                                    .desired_width(f32::INFINITY)
                                    .hint_text(
                                        "내용을 입력하세요… (외부 글 복사·붙여넣기 시 HTML 코드는 제거되고 일반 텍스트와 그림만 깔끔하게 들어갑니다)",
                                    ),
                            );
                            if body_resp.changed() {
                                self.mark_dirty();
                            }
                            self.attach_body_context_menu(&body_resp);

                            ui.add_space(10.0);
                            ui.label(
                                RichText::new("📖 문서 · 첨부 이미지 미리보기 & [[위키링크]] 클릭 이동")
                                    .size(12.0)
                                    .strong()
                                    .color(pal.text_muted),
                            );
                            ui::card(ui, pal, |ui| {
                                if let Some(w) = render_markdown_preview(
                                    ui,
                                    pal,
                                    &self.draft_body,
                                    vault_root_ref,
                                    &mut self.img_cache,
                                ) {
                                    clicked_wikilink = Some(w);
                                }
                            });
                        }

                        ui.add_space(10.0);
                        if let Some(doc) = &self.current_doc {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "단어 {} · 글자 {} · 수정 {}",
                                        doc.word_count, doc.char_count, doc.updated_at
                                    ))
                                    .size(12.0)
                                    .color(pal.text_muted),
                                );
                            });
                        }
                    });
            });

        if let Some(target) = clicked_wikilink {
            self.open_wikilink_by_title(&target);
        }
    }

    fn attach_body_context_menu(&mut self, resp: &egui::Response) {
        let ctx = resp.ctx.clone();
        resp.context_menu(|ui| {
            if ui
                .button("📋 붙여넣기 (일반 텍스트 · 이미지 유지)  Ctrl+V")
                .clicked()
            {
                let pasted = self.build_clipboard_paste_text(false, None);
                self.insert_at_body_cursor(&ctx, &pasted);
                ui.close_menu();
            }
            if ui
                .button("📄 순수 텍스트만 붙여넣기  Ctrl+Shift+V")
                .clicked()
            {
                let pasted = self.build_clipboard_paste_text(true, None);
                self.insert_at_body_cursor(&ctx, &pasted);
                ui.close_menu();
            }
            ui.separator();
            if ui
                .button("🧹 본문 내 외부 HTML 속성 코드 일괄 정리")
                .clicked()
            {
                let cleaned =
                    win_clipboard::strip_html_wrappers_preserving_images(&self.draft_body);
                if cleaned != self.draft_body {
                    self.draft_body = cleaned;
                    self.mark_dirty();
                    self.show_toast("본문의 불필요한 HTML 태그·속성 코드를 정리했습니다.", false);
                } else {
                    self.show_toast("정리할 외부 HTML 태그가 없습니다.", false);
                }
                ui.close_menu();
            }
        });
    }

    fn render_properties_editor(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        let mut remove_idx: Option<usize> = None;
        let mut changed = false;

        while self.new_tag_inputs.len() < self.draft_properties.len() {
            self.new_tag_inputs.push(String::new());
        }

        for (idx, prop) in self.draft_properties.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt(format!("prop_type_{idx}"))
                    .selected_text(match prop.prop_type.as_str() {
                        "tags" => "🏷️ 태그",
                        "date" => "📅 날짜",
                        "checkbox" => "☑️ 체크",
                        "url" => "🔗 URL",
                        "number" => "🔢 숫자",
                        _ => "≡ 텍스트",
                    })
                    .width(84.0)
                    .show_ui(ui, |ui| {
                        for (k, lbl) in [
                            ("text", "≡ 텍스트"),
                            ("tags", "🏷️ 태그"),
                            ("date", "📅 날짜"),
                            ("checkbox", "☑️ 체크"),
                            ("url", "🔗 URL"),
                            ("number", "🔢 숫자"),
                        ] {
                            if ui
                                .selectable_value(&mut prop.prop_type, k.to_string(), lbl)
                                .changed()
                            {
                                changed = true;
                            }
                        }
                    });

                if ui
                    .add(
                        egui::TextEdit::singleline(&mut prop.key)
                            .hint_text("속성 이름")
                            .desired_width(110.0),
                    )
                    .changed()
                {
                    changed = true;
                }

                if prop.prop_type == "checkbox" {
                    let mut checked = prop.value.trim().eq_ignore_ascii_case("true");
                    if ui.checkbox(&mut checked, "").changed() {
                        prop.value = if checked {
                            "true".into()
                        } else {
                            "false".into()
                        };
                        changed = true;
                    }
                } else if ui
                    .add(
                        egui::TextEdit::singleline(&mut prop.value)
                            .hint_text(if prop.prop_type == "tags" {
                                "쉼표로 구분 (예: 업무, 회의)"
                            } else {
                                "값 입력"
                            })
                            .desired_width(ui.available_width() - 40.0),
                    )
                    .changed()
                {
                    changed = true;
                }

                if ui.small_button("✕").on_hover_text("속성 삭제").clicked() {
                    remove_idx = Some(idx);
                }
            });
        }

        if let Some(r) = remove_idx {
            self.draft_properties.remove(r);
            changed = true;
        }

        ui.horizontal(|ui| {
            if ui.small_button("＋ 속성 추가").clicked() {
                let has_tags = self.draft_properties.iter().any(|p| p.key == "tags");
                self.draft_properties.push(if has_tags {
                    NotePropertyDto {
                        key: format!("속성{}", self.draft_properties.len() + 1),
                        value: String::new(),
                        prop_type: "text".into(),
                    }
                } else {
                    NotePropertyDto {
                        key: "tags".into(),
                        value: String::new(),
                        prop_type: "tags".into(),
                    }
                });
                changed = true;
            }
            if ui
                .small_button(if self.show_raw_yaml {
                    "▲ 원본 YAML 접기"
                } else {
                    "▼ 원본 YAML 보기"
                })
                .clicked()
            {
                self.show_raw_yaml = !self.show_raw_yaml;
            }
        });

        if self.show_raw_yaml {
            if let Some(doc) = &self.current_doc {
                let yaml_txt = if doc.raw_frontmatter.trim().is_empty() {
                    "(저장된 Frontmatter YAML 없음)".to_string()
                } else {
                    format!("---\n{}\n---", doc.raw_frontmatter.trim())
                };
                egui::Frame::none()
                    .fill(pal.code_bg)
                    .rounding(Rounding::same(6.0))
                    .inner_margin(Margin::same(8.0))
                    .show(ui, |ui| {
                        ui.label(RichText::new(yaml_txt).monospace().size(12.0));
                    });
            }
        }

        if changed {
            self.mark_dirty();
        }
    }

    fn render_markdown_toolbar(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        let ctx = ui.ctx().clone();
        egui::Frame::none()
            .fill(pal.bg_inset)
            .stroke(Stroke::new(1.0_f32, pal.border_subtle))
            .rounding(Rounding::same(7.0))
            .inner_margin(Margin::symmetric(8.0, 4.0))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .small_button("📋 깔끔 붙여넣기")
                        .on_hover_text(
                            "외부 글의 서식·HTML 속성 코드는 제거하고 일반 텍스트와 이미지만 붙여넣습니다 (Ctrl+V)",
                        )
                        .clicked()
                    {
                        let pasted = self.build_clipboard_paste_text(false, None);
                        self.insert_at_body_cursor(&ctx, &pasted);
                    }

                    if ui
                        .small_button("🧹 코드·태그 정리")
                        .on_hover_text("이미 붙여넣어진 <span ...> 등 외부 HTML 속성 태그를 일괄 제거합니다 (이미지는 보존)")
                        .clicked()
                    {
                        let cleaned =
                            win_clipboard::strip_html_wrappers_preserving_images(&self.draft_body);
                        if cleaned != self.draft_body {
                            self.draft_body = cleaned;
                            self.mark_dirty();
                            self.show_toast("외부 HTML 속성 코드를 깔끔하게 정리했습니다.", false);
                        } else {
                            self.show_toast("제거할 외부 HTML 태그가 없습니다.", false);
                        }
                    }

                    ui.separator();

                    for (btn_lbl, snippet) in [
                        ("H1", "\n# 제목 1\n"),
                        ("H2", "\n## 제목 2\n"),
                        ("H3", "\n### 제목 3\n"),
                        ("B 굵게", "**굵은 텍스트**"),
                        ("I 기울임", "*기울임*"),
                        ("==형광펜==", "==형광펜 강조=="),
                        ("☑️ 할 일", "\n- [ ] 할 일 항목\n"),
                        ("• 목록", "\n- 항목\n"),
                        ("💡 콜아웃", "\n> [!note] 메모\n> 핵심 내용을 입력하세요.\n"),
                        ("📊 표", "\n| 항목 | 내용 |\n| --- | --- |\n| 예시 | 값 |\n"),
                        ("💻 코드블록", "\n```rust\n// 코드\n```\n"),
                    ] {
                        if ui.small_button(btn_lbl).clicked() {
                            self.insert_at_body_cursor(&ctx, snippet);
                        }
                    }

                    if ui.small_button("[[노트 연결]]").clicked() {
                        self.wikilink_query.clear();
                        self.show_wikilink_modal = true;
                    }

                    if ui.small_button("🖼️ 이미지 첨부").clicked() {
                        if let Some(ref vp) = self.workspace.active_vault.clone() {
                            if let Ok(picked) = vault::pick_attachment(Path::new(vp)) {
                                if picked.picked {
                                    if let Some(rel) = picked.rel_path {
                                        self.img_cache.clear();
                                        self.insert_at_body_cursor(
                                            &ctx,
                                            &format!("\n![이미지]({rel})\n"),
                                        );
                                        self.show_toast(
                                            format!("이미지를 첨부했습니다: {rel}"),
                                            false,
                                        );
                                    }
                                }
                            }
                        }
                    }

                    if ui.small_button("📎 파일 첨부").clicked() {
                        if let Some(ref vp) = self.workspace.active_vault.clone() {
                            if let Ok(picked) = vault::pick_file_attachment(Path::new(vp)) {
                                if picked.picked {
                                    if let Some(rel) = picked.rel_path {
                                        let fname =
                                            picked.file_name.unwrap_or_else(|| "첨부파일".into());
                                        let md = if picked.is_image {
                                            self.img_cache.clear();
                                            format!("\n![{fname}]({rel})\n")
                                        } else {
                                            format!(" [📎 {fname}]({rel}) ")
                                        };
                                        self.insert_at_body_cursor(&ctx, &md);
                                        self.show_toast(
                                            format!("파일을 첨부했습니다: {fname}"),
                                            false,
                                        );
                                    }
                                }
                            }
                        }
                    }
                });
            });
    }

    fn render_split_note_pane(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        let Some(split) = self.split_doc.clone() else {
            return;
        };
        let mut clicked_wiki: Option<String> = None;
        let vault_path_opt = self.workspace.active_vault.clone();
        let vault_root_ref = vault_path_opt.as_deref().map(Path::new);

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "{} {}",
                    split.icon.as_deref().unwrap_or("📄"),
                    split.title
                ))
                .strong(),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("✕").clicked() {
                    self.close_split_note();
                }
                if ui.small_button("편집기로 열기").clicked() {
                    self.open_note(&split.path);
                }
            });
        });
        ui.separator();

        egui::ScrollArea::vertical()
            .id_salt("split_viewer_scroll")
            .show(ui, |ui| {
                egui::Frame::none()
                    .inner_margin(Margin::same(16.0))
                    .show(ui, |ui| {
                        if split.locked {
                            ui.label(
                                RichText::new(
                                    "🔒 잠긴 암호화 노트입니다. 편집기에서 잠금을 해제하세요.",
                                )
                                .color(pal.text_muted),
                            );
                        } else if let Some(w) = render_markdown_preview(
                            ui,
                            pal,
                            &split.body,
                            vault_root_ref,
                            &mut self.img_cache,
                        ) {
                            clicked_wiki = Some(w);
                        }
                    });
            });

        if let Some(target) = clicked_wiki {
            self.open_wikilink_by_title(&target);
        }
    }

    fn render_inspector(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        ui.horizontal(|ui| {
            for (tab, label) in [
                (RightInspectorTab::Backlinks, "🔗 백링크"),
                (RightInspectorTab::Outline, "📑 목차"),
                (RightInspectorTab::LocalGraph, "🕸️ 로컬 그래프"),
            ] {
                if ui
                    .selectable_label(self.inspector_tab == tab, label)
                    .clicked()
                {
                    self.inspector_tab = tab;
                    if tab == RightInspectorTab::LocalGraph {
                        self.refresh_local_graph();
                    }
                }
            }
        });
        ui.separator();

        let Some(doc) = self.current_doc.clone() else {
            ui.label(RichText::new("열려 있는 노트가 없습니다.").color(pal.text_muted));
            return;
        };

        egui::ScrollArea::vertical()
            .id_salt("inspector_body_scroll")
            .show(ui, |ui| match self.inspector_tab {
                RightInspectorTab::Backlinks => {
                    let linked: Vec<_> = doc.backlinks.iter().filter(|b| b.linked).collect();
                    let unlinked: Vec<_> = doc.backlinks.iter().filter(|b| !b.linked).collect();

                    ui.label(
                        RichText::new(format!("연결된 언급 ({})", linked.len()))
                            .size(12.0)
                            .strong()
                            .color(pal.text_muted),
                    );
                    if linked.is_empty() {
                        ui.label(
                            RichText::new("이 노트를 [[링크]]한 다른 노트가 없습니다.")
                                .size(12.0)
                                .color(pal.text_muted),
                        );
                    } else {
                        for bl in linked {
                            ui::sub_card(ui, pal, |ui| {
                                if ui
                                    .link(format!(
                                        "{} {}",
                                        bl.icon.as_deref().unwrap_or("📄"),
                                        bl.title
                                    ))
                                    .clicked()
                                {
                                    self.open_note(&bl.path);
                                }
                                ui.label(
                                    RichText::new(&bl.snippet)
                                        .size(11.5)
                                        .color(pal.text_secondary),
                                );
                            });
                            ui.add_space(4.0);
                        }
                    }

                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!("연결되지 않은 언급 ({})", unlinked.len()))
                            .size(12.0)
                            .strong()
                            .color(pal.text_muted),
                    );
                    for bl in unlinked {
                        ui::sub_card(ui, pal, |ui| {
                            ui.horizontal(|ui| {
                                if ui
                                    .link(format!(
                                        "{} {}",
                                        bl.icon.as_deref().unwrap_or("📄"),
                                        bl.title
                                    ))
                                    .clicked()
                                {
                                    self.open_note(&bl.path);
                                }
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.small_button("[[연결]]").clicked() {
                                        if let Some(ref vp) = self.workspace.active_vault.clone() {
                                            let linked_ok = {
                                                let st = self.backend.lock().unwrap();
                                                vault::link_unlinked_mention_with_session(
                                                    Path::new(vp),
                                                    &bl.path,
                                                    &doc.title,
                                                    Some(&st.crypto),
                                                )
                                                .is_ok()
                                            };
                                            if linked_ok {
                                                let refreshed = {
                                                    let st = self.backend.lock().unwrap();
                                                    vault::read_note_with_session(
                                                        Path::new(vp),
                                                        &doc.path,
                                                        Some(&st.crypto),
                                                    )
                                                    .ok()
                                                };
                                                if let Some(r) = refreshed {
                                                    self.current_doc = Some(r);
                                                }
                                                self.show_toast(
                                                    format!(
                                                        "'{}' 노트에 [[{}]] 링크를 연결했습니다.",
                                                        bl.title, doc.title
                                                    ),
                                                    false,
                                                );
                                            }
                                        }
                                    }
                                });
                            });
                            ui.label(
                                RichText::new(&bl.snippet)
                                    .size(11.5)
                                    .color(pal.text_secondary),
                            );
                        });
                        ui.add_space(4.0);
                    }

                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!("나가는 링크 ({})", doc.outgoing_links.len()))
                            .size(12.0)
                            .strong()
                            .color(pal.text_muted),
                    );
                    for out in &doc.outgoing_links {
                        if ui.button(format!("↗ [[{out}]]")).clicked() {
                            self.open_wikilink_by_title(out);
                        }
                    }
                }
                RightInspectorTab::Outline => {
                    if doc.headings.is_empty() {
                        ui.label(
                            RichText::new("제목(H1~H6)을 추가하면 목차가 자동으로 생성됩니다.")
                                .size(12.0)
                                .color(pal.text_muted),
                        );
                    } else {
                        for h in &doc.headings {
                            ui.horizontal(|ui| {
                                ui.add_space((h.level.saturating_sub(1) as f32) * 10.0);
                                ui.label(
                                    RichText::new(format!("• {}", h.text))
                                        .size(13.0)
                                        .color(pal.text_primary),
                                );
                            });
                        }
                    }
                }
                RightInspectorTab::LocalGraph => {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("연결 깊이").size(12.0));
                        for d in [1usize, 2usize] {
                            if ui
                                .selectable_label(self.local_graph_depth == d, format!("{d}단계"))
                                .clicked()
                            {
                                self.local_graph_depth = d;
                                self.refresh_local_graph();
                            }
                        }
                    });
                    ui.add_space(6.0);
                    if self.local_graph_data.is_none() {
                        self.refresh_local_graph();
                    }
                    let clicked = self.local_graph_widget.ui(
                        ui,
                        pal,
                        egui::vec2(ui.available_width(), 260.0),
                    );
                    if let Some(node) = clicked {
                        if node.kind == "note" {
                            if let Some(p) = node.path {
                                self.open_note(&p);
                            }
                        } else if node.kind == "ghost" {
                            self.open_wikilink_by_title(&node.label);
                        }
                    }
                }
            });
    }

    fn render_global_graph_view(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        if self.global_graph_data.is_none() {
            self.refresh_global_graph();
        }

        egui::Frame::none()
            .inner_margin(Margin::same(16.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("🕸️ 지식 그래프 필터").strong().size(15.0));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.global_graph_widget.search_query)
                            .hint_text("노드 이름 검색 하이라이트…")
                            .desired_width(220.0),
                    );
                    if ui
                        .checkbox(&mut self.global_graph_include_tags, "#태그 노드 표시")
                        .changed()
                    {
                        self.refresh_global_graph();
                    }
                    if ui
                        .checkbox(
                            &mut self.global_graph_local_only,
                            "현재 노트 주변(2단계)만 보기",
                        )
                        .changed()
                    {
                        self.refresh_global_graph();
                    }
                    if ui.button("카메라 초기화").clicked() {
                        self.global_graph_widget.reset_camera();
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui::primary_button(ui, pal, "← 노트 편집기로 돌아가기").clicked()
                        {
                            self.main_view = ActiveMainView::Editor;
                        }
                    });
                });

                ui.add_space(8.0);
                let clicked = self.global_graph_widget.ui(
                    ui,
                    pal,
                    egui::vec2(ui.available_width(), ui.available_height() - 10.0),
                );
                if let Some(node) = clicked {
                    if node.kind == "note" {
                        if let Some(p) = node.path {
                            self.open_note(&p);
                        }
                    } else if node.kind == "ghost" {
                        self.open_wikilink_by_title(&node.label);
                    }
                }
            });
    }

    fn render_modals(&mut self, ctx: &egui::Context, pal: &ThemePalette) {
        // 1. 빠른 검색 / 명령어 팔레트 (Ctrl+K / Ctrl+P)
        if self.show_command_palette {
            let mut close = false;
            let mut open_path: Option<String> = None;

            egui::Window::new("🔍 빠른 검색 및 명령어 팔레트")
                .collapsible(false)
                .resizable(false)
                .fixed_size([540.0, 380.0])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.palette_query)
                            .hint_text("노트 제목·본문·#태그 검색…")
                            .desired_width(f32::INFINITY),
                    );
                    if resp.changed() {
                        if let Some(ref vp) = self.workspace.active_vault {
                            let st = self.backend.lock().unwrap();
                            self.palette_hits = vault::search_vault_with_session(
                                Path::new(vp),
                                &self.palette_query,
                                Some(&st.crypto),
                            )
                            .unwrap_or_default();
                        }
                    }

                    ui.add_space(6.0);
                    egui::ScrollArea::vertical()
                        .max_height(280.0)
                        .show(ui, |ui| {
                            if self.palette_query.trim().is_empty() {
                                if let Some(scan) = &self.scan {
                                    for n in scan.notes.iter().take(25) {
                                        if ui
                                            .button(format!(
                                                "{} {} ({})",
                                                n.icon.as_deref().unwrap_or("📄"),
                                                n.title,
                                                n.path
                                            ))
                                            .clicked()
                                        {
                                            open_path = Some(n.path.clone());
                                            close = true;
                                        }
                                    }
                                }
                            } else if self.palette_hits.is_empty() {
                                ui.label(
                                    RichText::new("일치하는 노트가 없습니다.")
                                        .color(pal.text_muted),
                                );
                            } else {
                                for hit in &self.palette_hits {
                                    ui::sub_card(ui, pal, |ui| {
                                        if ui
                                            .link(format!(
                                                "{} {} ({})",
                                                hit.icon.as_deref().unwrap_or("📄"),
                                                hit.title,
                                                hit.path
                                            ))
                                            .clicked()
                                        {
                                            open_path = Some(hit.path.clone());
                                            close = true;
                                        }
                                        if let Some(m) = hit.matches.first() {
                                            ui.label(
                                                RichText::new(&m.snippet)
                                                    .size(11.5)
                                                    .color(pal.text_secondary),
                                            );
                                        }
                                    });
                                }
                            }
                        });

                    ui.separator();
                    if ui.button("닫기").clicked() {
                        close = true;
                    }
                });

            if let Some(p) = open_path {
                self.open_note(&p);
            }
            if close {
                self.show_command_palette = false;
            }
        }

        // 2. 텍스트 프롬프트 모달 (새 노트 / 새 폴더 / 이름 변경)
        if let Some((kind, title, mut val)) = self.prompt_modal.take() {
            let mut keep_open = true;
            let mut confirmed = false;

            egui::Window::new(title.clone())
                .collapsible(false)
                .resizable(false)
                .fixed_size([380.0, 140.0])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut val)
                            .hint_text("이름 입력")
                            .desired_width(f32::INFINITY),
                    );
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        confirmed = true;
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui::primary_button(ui, pal, "확인").clicked() {
                            confirmed = true;
                        }
                        if ui.button("취소").clicked() {
                            keep_open = false;
                        }
                    });
                });

            if confirmed && !val.trim().is_empty() {
                keep_open = false;
                if let Some(ref vp) = self.workspace.active_vault.clone() {
                    match &kind {
                        PromptKind::NewNote { folder } => {
                            let created = {
                                let st = self.backend.lock().unwrap();
                                vault::create_note_with_session(
                                    Path::new(vp),
                                    folder,
                                    val.trim(),
                                    None,
                                    None,
                                    &[],
                                    Some(&st.crypto),
                                )
                            };
                            match created {
                                Ok((new_path, _)) => {
                                    self.rescan_active_vault();
                                    self.open_note(&new_path);
                                }
                                Err(e) => self.show_toast(format!("{e:#}"), true),
                            }
                        }
                        PromptKind::NewFolder { parent } => {
                            match vault::create_folder(Path::new(vp), parent, val.trim()) {
                                Ok(new_folder) => {
                                    self.expanded_folders.insert(new_folder);
                                    self.rescan_active_vault();
                                }
                                Err(e) => self.show_toast(format!("{e:#}"), true),
                            }
                        }
                        PromptKind::Rename { old_path } => {
                            let renamed = {
                                let mut st = self.backend.lock().unwrap();
                                vault::rename_entry_with_session(
                                    Path::new(vp),
                                    old_path,
                                    val.trim(),
                                    Some(&mut st.crypto),
                                )
                            };
                            match renamed {
                                Ok(res) => {
                                    self.rescan_active_vault();
                                    if self
                                        .current_doc
                                        .as_ref()
                                        .is_some_and(|d| &d.path == old_path)
                                    {
                                        self.current_doc = None;
                                        self.open_note(&res.new_path);
                                    }
                                }
                                Err(e) => self.show_toast(format!("{e:#}"), true),
                            }
                        }
                    }
                }
            }

            if keep_open {
                self.prompt_modal = Some((kind, title, val));
            }
        }

        // 3. 커스텀 폴더 아이콘·색상 변경 모달
        if let Some(mut fsm) = self.folder_style_modal.take() {
            let mut keep_open = true;
            let mut apply_style = false;
            let mut reset_style = false;

            egui::Window::new(format!("🎨 폴더 스타일 ({})", fsm.folder_name))
                .collapsible(false)
                .resizable(false)
                .fixed_size([380.0, 260.0])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(RichText::new("폴더 아이콘").strong().size(12.5));
                    ui.horizontal_wrapped(|ui| {
                        for ic in FOLDER_ICON_PRESETS {
                            let selected = fsm.icon == *ic;
                            if ui
                                .selectable_label(selected, RichText::new(*ic).size(17.0))
                                .clicked()
                            {
                                fsm.icon = (*ic).to_string();
                            }
                        }
                    });
                    ui.add_space(6.0);
                    ui.label(RichText::new("폴더 이름 색상").strong().size(12.5));
                    ui.horizontal_wrapped(|ui| {
                        for (lbl, hex) in FOLDER_COLOR_PRESETS {
                            let selected = fsm.color.eq_ignore_ascii_case(hex);
                            let txt_col = ui::parse_hex_color(hex).unwrap_or(pal.text_primary);
                            if ui
                                .selectable_label(
                                    selected,
                                    RichText::new(*lbl).strong().color(txt_col),
                                )
                                .clicked()
                            {
                                fsm.color = (*hex).to_string();
                            }
                        }
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui::primary_button(ui, pal, "적용").clicked() {
                            apply_style = true;
                            keep_open = false;
                        }
                        if ui.button("초기화").clicked() {
                            reset_style = true;
                            keep_open = false;
                        }
                        if ui.button("취소").clicked() {
                            keep_open = false;
                        }
                    });
                });

            if apply_style || reset_style {
                if let Some(ref vp) = self.workspace.active_vault.clone() {
                    let ic_opt = if reset_style || fsm.icon.trim().is_empty() {
                        None
                    } else {
                        Some(fsm.icon.as_str())
                    };
                    let col_opt = if reset_style || fsm.color.trim().is_empty() {
                        None
                    } else {
                        Some(fsm.color.as_str())
                    };
                    if let Err(e) =
                        vault::set_folder_style(Path::new(vp), &fsm.folder_path, ic_opt, col_opt)
                    {
                        self.show_toast(format!("폴더 스타일 변경 실패: {e:#}"), true);
                    } else {
                        self.rescan_active_vault();
                    }
                }
            } else if keep_open {
                self.folder_style_modal = Some(fsm);
            }
        }

        // 4. 노트 · 볼트 AES-256-GCM 암호화 관리 모달
        if self.show_crypto_modal {
            let mut close = false;
            let vp_opt = self.workspace.active_vault.clone();
            let cur_doc_opt = self.current_doc.clone();
            let scan_opt = self.scan.clone();

            egui::Window::new("🔒 노트 · 볼트 AES-256-GCM 암호화 관리")
                .collapsible(false)
                .resizable(false)
                .fixed_size([460.0, 420.0])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(
                        RichText::new(
                            "AES-256-GCM + PBKDF2-HMAC-SHA256(210,000회) 표준 암호화로 민감한 노트를 보호합니다.",
                        )
                        .size(12.0)
                        .color(pal.text_secondary),
                    );
                    ui.add_space(6.0);

                    egui::Grid::new("crypto_modal_inputs")
                        .num_columns(2)
                        .spacing([10.0, 6.0])
                        .show(ui, |ui| {
                            ui.label("비밀번호");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.crypto_pw_input)
                                    .password(true)
                                    .desired_width(300.0),
                            );
                            ui.end_row();

                            ui.label("비밀번호 확인");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.crypto_pw_confirm)
                                    .password(true)
                                    .hint_text("새 암호 설정 시 동일하게 입력")
                                    .desired_width(300.0),
                            );
                            ui.end_row();

                            ui.label("비밀번호 힌트");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.crypto_hint_input)
                                    .hint_text("선택 사항 (비밀번호 자체를 적지 마세요)")
                                    .desired_width(300.0),
                            );
                            ui.end_row();
                        });

                    ui.add_space(8.0);
                    if let (Some(vp), Some(doc)) = (vp_opt.as_ref(), cur_doc_opt.as_ref()) {
                        ui::sub_card(ui, pal, |ui| {
                            ui.label(
                                RichText::new(format!("현재 노트: {}", doc.title))
                                    .strong()
                                    .size(13.5),
                            );
                            ui.horizontal_wrapped(|ui| {
                                if !doc.encrypted {
                                    if ui::primary_button(ui, pal, "🔒 이 노트 암호화").clicked()
                                    {
                                        if self.crypto_pw_input.is_empty() {
                                            self.show_toast("비밀번호를 입력하세요.", true);
                                        } else if !self.crypto_pw_confirm.is_empty()
                                            && self.crypto_pw_input != self.crypto_pw_confirm
                                        {
                                            self.show_toast("비밀번호 확인이 일치하지 않습니다.", true);
                                        } else {
                                            self.flush_save(false);
                                            let res = {
                                                let mut st = self.backend.lock().unwrap();
                                                vault::encrypt_single_note(
                                                    Path::new(vp),
                                                    &doc.path,
                                                    &self.crypto_pw_input,
                                                    Some(&self.crypto_hint_input),
                                                    &mut st.crypto,
                                                )
                                            };
                                            match res {
                                                Ok(enc_doc) => {
                                                    self.apply_loaded_doc(enc_doc, false);
                                                    self.rescan_active_vault();
                                                    self.crypto_pw_input.clear();
                                                    self.crypto_pw_confirm.clear();
                                                    self.show_toast(
                                                        "노트를 AES-256으로 암호화했습니다.",
                                                        false,
                                                    );
                                                    close = true;
                                                }
                                                Err(e) => self.show_toast(format!("{e:#}"), true),
                                            }
                                        }
                                    }
                                } else {
                                    if ui.button("🔓 암호화 영구 해제(평문 전환)").clicked() {
                                        let res = {
                                            let mut st = self.backend.lock().unwrap();
                                            vault::decrypt_single_note(
                                                Path::new(vp),
                                                &doc.path,
                                                &self.crypto_pw_input,
                                                &mut st.crypto,
                                            )
                                        };
                                        match res {
                                            Ok(plain_doc) => {
                                                self.apply_loaded_doc(plain_doc, false);
                                                self.rescan_active_vault();
                                                self.crypto_pw_input.clear();
                                                self.show_toast(
                                                    "노트 암호화를 해제하여 일반 마크다운으로 전환했습니다.",
                                                    false,
                                                );
                                                close = true;
                                            }
                                            Err(e) => self.show_toast(format!("{e:#}"), true),
                                        }
                                    }
                                }
                            });
                        });
                    }

                    ui.add_space(8.0);
                    if let Some(vp) = vp_opt.as_ref() {
                        ui::sub_card(ui, pal, |ui| {
                            let enc_cnt = scan_opt.as_ref().map(|s| s.encrypted_notes).unwrap_or(0);
                            ui.label(
                                RichText::new(format!(
                                    "볼트 전체 암호화 (현재 암호화 노트: {enc_cnt}개)"
                                ))
                                .strong()
                                .size(13.5),
                            );
                            ui.checkbox(
                                &mut self.crypto_auto_encrypt,
                                "새로 만드는 노트도 자동으로 암호화",
                            );
                            ui.horizontal_wrapped(|ui| {
                                if ui.button("🔒 볼트 전체 일괄 암호화").clicked() {
                                    if self.crypto_pw_input.is_empty() {
                                        self.show_toast("비밀번호를 입력하세요.", true);
                                    } else if !self.crypto_pw_confirm.is_empty()
                                        && self.crypto_pw_input != self.crypto_pw_confirm
                                    {
                                        self.show_toast("비밀번호 확인이 일치하지 않습니다.", true);
                                    } else {
                                        self.flush_save(false);
                                        let res = {
                                            let mut st = self.backend.lock().unwrap();
                                            vault::encrypt_entire_vault(
                                                Path::new(vp),
                                                &self.crypto_pw_input,
                                                Some(&self.crypto_hint_input),
                                                self.crypto_auto_encrypt,
                                                &mut st.crypto,
                                            )
                                        };
                                        match res {
                                            Ok(b) => {
                                                self.rescan_active_vault();
                                                if let Some(cur) =
                                                    self.current_doc.as_ref().map(|d| d.path.clone())
                                                {
                                                    self.current_doc = None;
                                                    self.open_note(&cur);
                                                }
                                                self.crypto_pw_input.clear();
                                                self.crypto_pw_confirm.clear();
                                                self.show_toast(
                                                    format!(
                                                        "볼트 암호화 완료 ({}개 신규 암호화)",
                                                        b.encrypted_count
                                                    ),
                                                    false,
                                                );
                                                close = true;
                                            }
                                            Err(e) => self.show_toast(format!("{e:#}"), true),
                                        }
                                    }
                                }
                                if ui.button("🔐 볼트 전체 잠그기").clicked() {
                                    self.flush_save(false);
                                    {
                                        let mut st = self.backend.lock().unwrap();
                                        vault::lock_entire_vault(Path::new(vp), &mut st.crypto);
                                    }
                                    self.rescan_active_vault();
                                    if let Some(cur) =
                                        self.current_doc.as_ref().map(|d| d.path.clone())
                                    {
                                        self.current_doc = None;
                                        self.open_note(&cur);
                                    }
                                    self.show_toast("볼트의 암호화 노트를 모두 잠갔습니다.", false);
                                    close = true;
                                }
                                if enc_cnt > 0
                                    && ui::danger_button(ui, pal, "볼트 전체 평문 복호화").clicked()
                                {
                                    let res = {
                                        let mut st = self.backend.lock().unwrap();
                                        vault::decrypt_entire_vault(
                                            Path::new(vp),
                                            &self.crypto_pw_input,
                                            &mut st.crypto,
                                        )
                                    };
                                    match res {
                                        Ok(b) => {
                                            self.rescan_active_vault();
                                            if let Some(cur) =
                                                self.current_doc.as_ref().map(|d| d.path.clone())
                                            {
                                                self.current_doc = None;
                                                self.open_note(&cur);
                                            }
                                            self.crypto_pw_input.clear();
                                            self.show_toast(
                                                format!(
                                                    "볼트 복호화 완료 ({}개 노트 평문 전환)",
                                                    b.decrypted_count
                                                ),
                                                false,
                                            );
                                            close = true;
                                        }
                                        Err(e) => self.show_toast(format!("{e:#}"), true),
                                    }
                                }
                            });
                        });
                    }

                    ui.separator();
                    if ui.button("닫기").clicked() {
                        close = true;
                    }
                });

            if close {
                self.show_crypto_modal = false;
            }
        }

        // 5. 이모지 아이콘 선택 모달
        if self.show_emoji_picker {
            let mut close = false;
            egui::Window::new("😀 페이지 아이콘 선택")
                .collapsible(false)
                .resizable(false)
                .fixed_size([340.0, 240.0])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for em in EMOJI_PRESETS {
                            if ui.button(RichText::new(*em).size(18.0)).clicked() {
                                self.draft_icon = Some((*em).to_string());
                                self.mark_dirty();
                                close = true;
                            }
                        }
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("아이콘 제거").clicked() {
                            self.draft_icon = None;
                            self.mark_dirty();
                            close = true;
                        }
                        if ui.button("닫기").clicked() {
                            close = true;
                        }
                    });
                });
            if close {
                self.show_emoji_picker = false;
            }
        }

        // 6. 커버 선택 모달
        if self.show_cover_picker {
            let mut close = false;
            egui::Window::new("🖼️ 노션 페이지 커버 배너")
                .collapsible(false)
                .resizable(false)
                .fixed_size([360.0, 220.0])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for (name, color) in COVER_PRESETS {
                            let btn = egui::Button::new(
                                RichText::new(*name).color(Color32::WHITE).strong(),
                            )
                            .fill(*color)
                            .min_size(egui::vec2(145.0, 32.0));
                            if ui.add(btn).clicked() {
                                self.draft_cover = Some((*name).to_string());
                                self.mark_dirty();
                                close = true;
                            }
                        }
                    });
                    ui.separator();
                    if ui.button("닫기").clicked() {
                        close = true;
                    }
                });
            if close {
                self.show_cover_picker = false;
            }
        }

        // 7. 노트 템플릿 모달
        if self.show_templates_modal {
            let mut close = false;
            let mut chosen: Option<TemplateItemDto> = None;
            egui::Window::new("✨ 노트 템플릿 적용")
                .collapsible(false)
                .resizable(false)
                .fixed_size([440.0, 320.0])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(240.0)
                        .show(ui, |ui| {
                            for tpl in &self.loaded_templates {
                                ui::sub_card(ui, pal, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(format!("{} {}", tpl.icon, tpl.name))
                                                .strong(),
                                        );
                                        ui.with_layout(
                                            Layout::right_to_left(Align::Center),
                                            |ui| {
                                                if ui::primary_button(ui, pal, "적용").clicked() {
                                                    chosen = Some(tpl.clone());
                                                    close = true;
                                                }
                                            },
                                        );
                                    });
                                    ui.label(
                                        RichText::new(&tpl.description)
                                            .size(12.0)
                                            .color(pal.text_secondary),
                                    );
                                });
                                ui.add_space(4.0);
                            }
                        });
                    if ui.button("닫기").clicked() {
                        close = true;
                    }
                });
            if let Some(tpl) = chosen {
                if self.draft_icon.is_none() {
                    self.draft_icon = Some(tpl.icon);
                }
                for p in tpl.properties {
                    if !self.draft_properties.iter().any(|x| x.key == p.key) {
                        self.draft_properties.push(p);
                    }
                }
                if self.draft_body.trim().is_empty() {
                    self.draft_body = tpl.body;
                } else {
                    self.draft_body = format!("{}\n\n{}", self.draft_body.trim(), tpl.body);
                }
                self.mark_dirty();
                self.show_toast("템플릿을 적용했습니다.", false);
            }
            if close {
                self.show_templates_modal = false;
            }
        }

        // 8. 위키링크 삽입 모달
        if self.show_wikilink_modal {
            let mut close = false;
            let mut picked_title: Option<String> = None;
            egui::Window::new("🔗 [[위키링크]] 노트 연결")
                .collapsible(false)
                .resizable(false)
                .fixed_size([400.0, 300.0])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.wikilink_query)
                            .hint_text("연결할 노트 제목 검색…")
                            .desired_width(f32::INFINITY),
                    );
                    let q = self.wikilink_query.trim().to_lowercase();
                    egui::ScrollArea::vertical()
                        .max_height(200.0)
                        .show(ui, |ui| {
                            if let Some(scan) = &self.scan {
                                for n in &scan.notes {
                                    if q.is_empty() || n.title.to_lowercase().contains(&q) {
                                        if ui
                                            .button(format!(
                                                "{} {}",
                                                n.icon.as_deref().unwrap_or("📄"),
                                                n.title
                                            ))
                                            .clicked()
                                        {
                                            picked_title = Some(n.title.clone());
                                            close = true;
                                        }
                                    }
                                }
                            }
                            if !self.wikilink_query.trim().is_empty()
                                && ui
                                    .button(format!(
                                        "✨ 새 링크 삽입: [[{}]]",
                                        self.wikilink_query.trim()
                                    ))
                                    .clicked()
                            {
                                picked_title = Some(self.wikilink_query.trim().to_string());
                                close = true;
                            }
                        });
                    if ui.button("닫기").clicked() {
                        close = true;
                    }
                });
            if let Some(t) = picked_title {
                self.insert_at_body_cursor(ctx, &format!(" [[{t}]] "));
            }
            if close {
                self.show_wikilink_modal = false;
            }
        }

        // 9. 볼트 휴지통 (.trash) 모달
        if self.show_trash_modal {
            let mut close = false;
            let mut restore_item: Option<String> = None;
            let mut empty_all = false;
            egui::Window::new("🗑️ 볼트 휴지통 (.trash)")
                .collapsible(false)
                .resizable(false)
                .fixed_size([460.0, 320.0])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    if self.trash_items.is_empty() {
                        ui.label(RichText::new("휴지통이 비어 있습니다.").color(pal.text_muted));
                    } else {
                        egui::ScrollArea::vertical()
                            .max_height(220.0)
                            .show(ui, |ui| {
                                for item in &self.trash_items {
                                    ui.horizontal(|ui| {
                                        ui.label(format!(
                                            "{} {} ({})",
                                            if item.is_dir { "📁" } else { "📄" },
                                            item.display_name,
                                            item.modified_at
                                        ));
                                        ui.with_layout(
                                            Layout::right_to_left(Align::Center),
                                            |ui| {
                                                if ui.small_button("복원").clicked() {
                                                    restore_item = Some(item.trash_name.clone());
                                                }
                                            },
                                        );
                                    });
                                }
                            });
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        if !self.trash_items.is_empty()
                            && ui::danger_button(ui, pal, "휴지통 비우기").clicked()
                        {
                            empty_all = true;
                        }
                        if ui.button("닫기").clicked() {
                            close = true;
                        }
                    });
                });

            if let Some(tname) = restore_item {
                if let Some(ref vp) = self.workspace.active_vault.clone() {
                    if let Ok(restored) = vault::restore_trash(Path::new(vp), &tname) {
                        self.trash_items = vault::list_trash(Path::new(vp)).unwrap_or_default();
                        self.rescan_active_vault();
                        self.show_toast(format!("복원 완료: {restored}"), false);
                    }
                }
            }
            if empty_all {
                if let Some(ref vp) = self.workspace.active_vault.clone() {
                    let _ = vault::empty_trash(Path::new(vp));
                    self.trash_items.clear();
                    self.show_toast("휴지통을 비웠습니다.", false);
                }
            }
            if close {
                self.show_trash_modal = false;
            }
        }
    }

    fn render_toasts(&self, ctx: &egui::Context, pal: &ThemePalette) {
        if self.toasts.is_empty() {
            return;
        }
        egui::Area::new(egui::Id::new("studio_toast_overlay"))
            .anchor(egui::Align2::RIGHT_BOTTOM, [-18.0, -18.0])
            .interactable(false)
            .show(ctx, |ui| {
                for t in &self.toasts {
                    let (fg, bg) = if t.is_error {
                        (Color32::WHITE, pal.danger)
                    } else {
                        (Color32::WHITE, pal.accent_purple)
                    };
                    egui::Frame::none()
                        .fill(bg)
                        .rounding(Rounding::same(8.0))
                        .inner_margin(Margin::symmetric(12.0, 8.0))
                        .show(ui, |ui| {
                            ui.label(RichText::new(&t.text).color(fg).strong().size(13.0));
                        });
                    ui.add_space(4.0);
                }
            });
    }
}
