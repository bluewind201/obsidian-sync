//! Obsidian 호환 로컬 볼트(Vault) 엔진 및 UI 워크스페이스 상태 관리.
//! - YAML 프론트매터(아이콘, 커버, Notion 스타일 속성) 무손실 파싱/직렬화
//! - 위키링크(`[[노트]]`, `[[노트|별칭]]`, `[[노트#헤딩]]`), 백링크(연결됨/연결 안 됨), 리네임 리팩토링
//! - 볼트 트리 스캔, 전문/태그/경로 검색, 지식 그래프(BFS 서브그래프 포함), 템플릿, 데일리 노트, 휴지통(`.trash`), 첨부파일

use crate::crypto::{self, VaultCryptoSession};
use crate::platform;
use anyhow::{bail, Context, Result};
use base64::Engine as _;
use chrono::{DateTime, Local};
use notify::{EventKind, RecursiveMode, Watcher};
use obsidian_sync_core::{AppConfig, ProfileKind};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

const SKIP_SYSTEM_DIRS: &[&str] = &[
    "@eaDir",
    "#recycle",
    "#snapshot",
    ".SynologyWorkingDirectory",
    "node_modules",
];

const MEDIA_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico", "pdf", "mp3", "mp4", "wav", "ogg",
    "webm", "mov",
];

const TRASH_MANIFEST: &str = ".manifest.json";

pub fn default_theme() -> String {
    "light".into()
}
pub fn default_editor_mode() -> String {
    "wysiwyg".into()
}
pub fn default_daily_folder() -> String {
    "Daily".into()
}
pub fn default_templates_folder() -> String {
    "Templates".into()
}
pub fn default_show_tags() -> bool {
    false
}
pub fn default_page_layout() -> String {
    "a4".into()
}
pub fn default_page_orientation() -> String {
    "portrait".into()
}
pub fn default_margin_top_cm() -> f64 {
    2.0
}
pub fn default_margin_bottom_cm() -> f64 {
    1.0
}
pub fn default_margin_left_cm() -> f64 {
    3.0
}
pub fn default_margin_right_cm() -> f64 {
    3.0
}
pub fn default_true() -> bool {
    true
}
pub fn default_editor_zoom() -> u32 {
    100
}

fn clamp_margin_cm(v: f64) -> f64 {
    if !v.is_finite() {
        return 2.0;
    }
    (v.clamp(0.0, 6.0) * 100.0).round() / 100.0
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct VaultRef {
    pub path: String,
    pub name: String,
    #[serde(default)]
    pub is_sync_profile: bool,
    #[serde(default)]
    pub profile_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WorkspaceState {
    #[serde(default)]
    pub vaults: Vec<VaultRef>,
    #[serde(default)]
    pub active_vault: Option<String>,
    #[serde(default)]
    pub open_tabs: Vec<String>,
    #[serde(default)]
    pub active_tab: Option<String>,
    #[serde(default)]
    pub split_tab: Option<String>,
    #[serde(default)]
    pub favorites: Vec<String>,
    #[serde(default)]
    pub recent_notes: Vec<String>,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_editor_mode")]
    pub editor_mode: String,
    #[serde(default = "default_daily_folder")]
    pub daily_folder: String,
    #[serde(default = "default_templates_folder")]
    pub templates_folder: String,
    #[serde(default = "default_show_tags")]
    pub show_tags: bool,
    #[serde(default = "default_page_layout")]
    pub page_layout: String,
    #[serde(default = "default_page_orientation")]
    pub page_orientation: String,
    #[serde(default = "default_margin_top_cm")]
    pub margin_top_cm: f64,
    #[serde(default = "default_margin_bottom_cm")]
    pub margin_bottom_cm: f64,
    #[serde(default = "default_margin_left_cm")]
    pub margin_left_cm: f64,
    #[serde(default = "default_margin_right_cm")]
    pub margin_right_cm: f64,
    #[serde(default = "default_true")]
    pub auto_fit_a4: bool,
    #[serde(default = "default_true")]
    pub show_page_guides: bool,
    #[serde(default = "default_true")]
    pub print_include_title: bool,
    #[serde(default)]
    pub print_include_properties: bool,
    #[serde(default = "default_editor_zoom")]
    pub editor_zoom: u32,
    #[serde(default)]
    pub daily_folders_by_vault: HashMap<String, String>,
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            vaults: Vec::new(),
            active_vault: None,
            open_tabs: Vec::new(),
            active_tab: None,
            split_tab: None,
            favorites: Vec::new(),
            recent_notes: Vec::new(),
            theme: default_theme(),
            editor_mode: default_editor_mode(),
            daily_folder: default_daily_folder(),
            templates_folder: default_templates_folder(),
            show_tags: default_show_tags(),
            page_layout: default_page_layout(),
            page_orientation: default_page_orientation(),
            margin_top_cm: default_margin_top_cm(),
            margin_bottom_cm: default_margin_bottom_cm(),
            margin_left_cm: default_margin_left_cm(),
            margin_right_cm: default_margin_right_cm(),
            auto_fit_a4: true,
            show_page_guides: true,
            print_include_title: true,
            print_include_properties: false,
            editor_zoom: default_editor_zoom(),
            daily_folders_by_vault: HashMap::new(),
        }
    }
}

fn norm_fs_key(p: &str) -> String {
    let mut s = p.trim().replace('\\', "/");
    while s.len() > 1 && s.ends_with('/') {
        s.pop();
    }
    if cfg!(windows) {
        s = s.to_lowercase();
    }
    s
}

fn folder_display_name(path: &str) -> String {
    let trimmed = path.trim().trim_end_matches(['/', '\\']);
    trimmed
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("Vault")
        .to_string()
}

impl WorkspaceState {
    pub fn load(data_dir: &Path) -> Self {
        let p = data_dir.join("workspace_ui.json");
        if let Ok(text) = fs::read_to_string(&p) {
            if let Ok(st) = serde_json::from_str::<Self>(&text) {
                return st;
            }
        }
        Self::default()
    }

    pub fn save(&self, data_dir: &Path) -> Result<()> {
        fs::create_dir_all(data_dir)?;
        let p = data_dir.join("workspace_ui.json");
        let tmp = p.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        fs::rename(&tmp, &p)?;
        Ok(())
    }

    pub fn merge_json(&mut self, patch: &serde_json::Value) {
        let Some(obj) = patch.as_object() else {
            return;
        };
        if let Some(v) = obj.get("vaults") {
            if let Ok(list) = serde_json::from_value::<Vec<VaultRef>>(v.clone()) {
                self.vaults = list;
            }
        }
        if let Some(v) = obj.get("active_vault") {
            self.active_vault = v.as_str().map(str::to_string);
        }
        if let Some(v) = obj.get("open_tabs") {
            if let Ok(list) = serde_json::from_value::<Vec<String>>(v.clone()) {
                self.open_tabs = list;
            }
        }
        if let Some(v) = obj.get("active_tab") {
            self.active_tab = v.as_str().map(str::to_string);
        }
        if let Some(v) = obj.get("split_tab") {
            self.split_tab = v.as_str().map(str::to_string);
        }
        if let Some(v) = obj.get("favorites") {
            if let Ok(list) = serde_json::from_value::<Vec<String>>(v.clone()) {
                self.favorites = list;
            }
        }
        if let Some(v) = obj.get("recent_notes") {
            if let Ok(list) = serde_json::from_value::<Vec<String>>(v.clone()) {
                self.recent_notes = list;
            }
        }
        if let Some(s) = obj.get("theme").and_then(|v| v.as_str()) {
            if !s.trim().is_empty() {
                self.theme = s.trim().to_string();
            }
        }
        if let Some(s) = obj.get("editor_mode").and_then(|v| v.as_str()) {
            if !s.trim().is_empty() {
                self.editor_mode = s.trim().to_string();
            }
        }
        if let Some(s) = obj.get("daily_folder").and_then(|v| v.as_str()) {
            self.daily_folder = s.trim().trim_matches(['/', '\\']).to_string();
        }
        if let Some(s) = obj.get("templates_folder").and_then(|v| v.as_str()) {
            if !s.trim().is_empty() {
                self.templates_folder = s.trim().trim_matches(['/', '\\']).to_string();
            }
        }
        if let Some(b) = obj.get("show_tags").and_then(|v| v.as_bool()) {
            self.show_tags = b;
        }
        if let Some(s) = obj.get("page_layout").and_then(|v| v.as_str()) {
            let clean = s.trim().to_ascii_lowercase();
            if clean == "a4" || clean == "full" {
                self.page_layout = clean;
            }
        }
        if let Some(s) = obj.get("page_orientation").and_then(|v| v.as_str()) {
            let clean = s.trim().to_ascii_lowercase();
            if clean == "portrait" || clean == "landscape" {
                self.page_orientation = clean;
            }
        }
        if let Some(n) = obj.get("margin_top_cm").and_then(|v| v.as_f64()) {
            self.margin_top_cm = clamp_margin_cm(n);
        }
        if let Some(n) = obj.get("margin_bottom_cm").and_then(|v| v.as_f64()) {
            self.margin_bottom_cm = clamp_margin_cm(n);
        }
        if let Some(n) = obj.get("margin_left_cm").and_then(|v| v.as_f64()) {
            self.margin_left_cm = clamp_margin_cm(n);
        }
        if let Some(n) = obj.get("margin_right_cm").and_then(|v| v.as_f64()) {
            self.margin_right_cm = clamp_margin_cm(n);
        }
        if let Some(b) = obj.get("auto_fit_a4").and_then(|v| v.as_bool()) {
            self.auto_fit_a4 = b;
        }
        if let Some(b) = obj.get("show_page_guides").and_then(|v| v.as_bool()) {
            self.show_page_guides = b;
        }
        if let Some(b) = obj.get("print_include_title").and_then(|v| v.as_bool()) {
            self.print_include_title = b;
        }
        if let Some(b) = obj
            .get("print_include_properties")
            .and_then(|v| v.as_bool())
        {
            self.print_include_properties = b;
        }
        if let Some(z) = obj.get("editor_zoom").and_then(|v| v.as_u64()) {
            self.editor_zoom = (z as u32).clamp(80, 150);
        }
        if let Some(v) = obj.get("daily_folders_by_vault") {
            if let Ok(map) = serde_json::from_value::<HashMap<String, String>>(v.clone()) {
                let cleaned = map
                    .into_iter()
                    .map(|(k, val)| (k, val.trim().trim_matches(['/', '\\']).to_string()))
                    .collect();
                self.daily_folders_by_vault = cleaned;
            }
        }
    }

    pub fn resolve_daily_folder(&self, vault_path: &str) -> String {
        let target_key = norm_fs_key(vault_path);
        for (k, v) in &self.daily_folders_by_vault {
            if norm_fs_key(k) == target_key {
                return v.trim().trim_matches(['/', '\\']).to_string();
            }
        }
        self.daily_folder
            .trim()
            .trim_matches(['/', '\\'])
            .to_string()
    }

    pub fn sync_vaults_with_config(&mut self, cfg: &AppConfig) {
        let obsidian_profiles: Vec<_> = cfg
            .profiles
            .iter()
            .filter(|p| p.kind == ProfileKind::Obsidian && !p.local_path.trim().is_empty())
            .collect();

        let active_profile_ids: HashSet<&str> =
            obsidian_profiles.iter().map(|p| p.id.as_str()).collect();

        for v in &mut self.vaults {
            if v.is_sync_profile {
                let keep = v
                    .profile_id
                    .as_deref()
                    .is_some_and(|id| active_profile_ids.contains(id));
                if !keep {
                    v.is_sync_profile = false;
                    v.profile_id = None;
                }
            }
        }

        for p in obsidian_profiles {
            let clean_path = p.local_path.trim().to_string();
            let key = norm_fs_key(&clean_path);
            let display = p.display_name();
            if let Some(existing) = self.vaults.iter_mut().find(|v| {
                v.profile_id.as_deref() == Some(p.id.as_str()) || norm_fs_key(&v.path) == key
            }) {
                existing.path = clean_path;
                existing.name = display;
                existing.is_sync_profile = true;
                existing.profile_id = Some(p.id.clone());
            } else {
                self.vaults.push(VaultRef {
                    path: clean_path,
                    name: display,
                    is_sync_profile: true,
                    profile_id: Some(p.id.clone()),
                });
            }
        }

        if self.active_vault.is_none() {
            if let Some(first_existing) = self
                .vaults
                .iter()
                .find(|v| Path::new(&v.path).is_dir())
                .map(|v| v.path.clone())
            {
                self.active_vault = Some(first_existing);
            }
        }
    }

    pub fn add_or_select_vault(&mut self, path: &str) {
        let clean = path.trim().to_string();
        let key = norm_fs_key(&clean);
        if !self.vaults.iter().any(|v| norm_fs_key(&v.path) == key) {
            self.vaults.push(VaultRef {
                name: folder_display_name(&clean),
                path: clean.clone(),
                is_sync_profile: false,
                profile_id: None,
            });
        }
        if self.active_vault.as_deref().map(norm_fs_key).as_deref() != Some(key.as_str()) {
            self.active_vault = Some(clean);
            self.open_tabs.clear();
            self.active_tab = None;
            self.split_tab = None;
        }
    }

    pub fn remove_vault(&mut self, path: &str) {
        let key = norm_fs_key(path);
        self.vaults.retain(|v| norm_fs_key(&v.path) != key);
        if self.active_vault.as_deref().map(norm_fs_key).as_deref() == Some(key.as_str()) {
            self.active_vault = self
                .vaults
                .iter()
                .find(|v| Path::new(&v.path).is_dir())
                .map(|v| v.path.clone());
            self.open_tabs.clear();
            self.active_tab = None;
            self.split_tab = None;
        }
    }
}

pub struct VaultWatcherState {
    watched_vault: Option<String>,
    revision: Arc<AtomicU64>,
    _watcher: Option<notify::RecommendedWatcher>,
}

impl VaultWatcherState {
    pub fn new() -> Self {
        Self {
            watched_vault: None,
            revision: Arc::new(AtomicU64::new(1)),
            _watcher: None,
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::Relaxed)
    }

    pub fn bump(&self) -> u64 {
        self.revision.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn watch(&mut self, vault_path: Option<&str>) {
        let target = vault_path
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        if self.watched_vault == target {
            return;
        }
        self._watcher = None;
        self.watched_vault = target.clone();
        self.bump();

        let Some(vp) = target else { return };
        let root = PathBuf::from(&vp);
        if !root.is_dir() {
            return;
        }
        let rev = self.revision.clone();
        let watch_root = root.clone();
        let Ok(mut watcher) =
            notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                let Ok(ev) = res else { return };
                if matches!(ev.kind, EventKind::Access(_)) {
                    return;
                }
                let relevant = ev.paths.iter().any(|p| {
                    let rel = p
                        .strip_prefix(&watch_root)
                        .unwrap_or(p)
                        .to_string_lossy()
                        .replace('\\', "/");
                    if rel.starts_with(".git/")
                        || rel == ".git"
                        || rel.starts_with(".trash/")
                        || rel == ".trash"
                        || rel.starts_with(".obsidian/workspace")
                    {
                        return false;
                    }
                    true
                });
                if relevant {
                    rev.fetch_add(1, Ordering::Relaxed);
                }
            })
        else {
            return;
        };
        if watcher.watch(&root, RecursiveMode::Recursive).is_ok() {
            self._watcher = Some(watcher);
        }
    }
}

pub struct WorkspaceManager {
    pub data_dir: PathBuf,
    pub state: WorkspaceState,
    pub watcher: VaultWatcherState,
}

impl WorkspaceManager {
    pub fn new(data_dir: PathBuf, cfg: &AppConfig) -> Self {
        let mut state = WorkspaceState::load(&data_dir);
        state.sync_vaults_with_config(cfg);
        let _ = state.save(&data_dir);
        let mut watcher = VaultWatcherState::new();
        let active = state.active_vault.clone();
        watcher.watch(active.as_deref());
        Self {
            data_dir,
            state,
            watcher,
        }
    }

    pub fn save_and_refresh_watcher(&mut self) -> Result<()> {
        self.state.save(&self.data_dir)?;
        let active = self.state.active_vault.clone();
        self.watcher.watch(active.as_deref());
        Ok(())
    }
}

// ───────────────────────────── DTO 정의 ─────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TreeNodeDto {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub icon: Option<String>,
    pub children: Vec<TreeNodeDto>,
    pub modified: i64,
    #[serde(default)]
    pub encrypted: bool,
    #[serde(default)]
    pub locked: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct NoteMetaDto {
    pub path: String,
    pub title: String,
    pub icon: Option<String>,
    pub tags: Vec<String>,
    pub updated_at: String,
    pub word_count: usize,
    #[serde(default)]
    pub encrypted: bool,
    #[serde(default)]
    pub locked: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TagCountDto {
    pub tag: String,
    pub count: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VaultScanDto {
    pub tree: Vec<TreeNodeDto>,
    pub notes: Vec<NoteMetaDto>,
    pub tags: Vec<TagCountDto>,
    pub total_notes: usize,
    pub total_folders: usize,
    pub revision: u64,
    #[serde(default)]
    pub encrypted_notes: usize,
    #[serde(default)]
    pub vault_auto_encrypt: bool,
    #[serde(default)]
    pub vault_unlocked: bool,
    #[serde(default)]
    pub vault_has_crypto_config: bool,
    #[serde(default)]
    pub vault_crypto_hint: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct NotePropertyDto {
    pub key: String,
    pub value: String,
    pub prop_type: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HeadingDto {
    pub level: u8,
    pub text: String,
    pub line: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct BacklinkDto {
    pub path: String,
    pub title: String,
    pub icon: Option<String>,
    pub snippet: String,
    pub linked: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NoteDocumentDto {
    pub path: String,
    pub title: String,
    pub icon: Option<String>,
    pub cover: Option<String>,
    pub properties: Vec<NotePropertyDto>,
    pub raw_frontmatter: String,
    pub body: String,
    pub headings: Vec<HeadingDto>,
    pub outgoing_links: Vec<String>,
    pub backlinks: Vec<BacklinkDto>,
    pub tags: Vec<String>,
    pub word_count: usize,
    pub char_count: usize,
    pub updated_at: String,
    #[serde(default)]
    pub encrypted: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub encryption_hint: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VaultBatchCryptoResult {
    pub encrypted_count: usize,
    pub decrypted_count: usize,
    pub unlocked_count: usize,
    pub skipped_locked_count: usize,
    pub total_notes: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RenameResult {
    pub new_path: String,
    pub updated_links: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TrashItemDto {
    pub trash_name: String,
    pub display_name: String,
    pub modified_at: String,
    pub is_dir: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SearchMatchDto {
    pub line: usize,
    pub snippet: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchHitDto {
    pub path: String,
    pub title: String,
    pub icon: Option<String>,
    pub matches: Vec<SearchMatchDto>,
    pub score: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GraphNodeDto {
    pub id: String,
    pub label: String,
    pub path: Option<String>,
    pub kind: String,
    pub group: String,
    pub degree: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GraphEdgeDto {
    pub source: String,
    pub target: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GraphDto {
    pub nodes: Vec<GraphNodeDto>,
    pub edges: Vec<GraphEdgeDto>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TemplateItemDto {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub description: String,
    pub body: String,
    pub properties: Vec<NotePropertyDto>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AttachmentDto {
    pub rel_path: String,
    pub media_url: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PickedAttachmentDto {
    pub picked: bool,
    pub rel_path: Option<String>,
    pub media_url: Option<String>,
    pub filename: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PickedFileAttachmentDto {
    pub picked: bool,
    pub rel_path: Option<String>,
    pub media_url: Option<String>,
    pub file_name: Option<String>,
    pub is_image: bool,
}

// ───────────────────────────── 경로 검증 및 유틸리티 ─────────────────────────────

pub fn normalize_rel_path(rel: &str) -> Result<String> {
    let s = rel.trim().replace('\\', "/");
    let s = s.trim_matches('/');
    if s.is_empty() {
        return Ok(String::new());
    }
    if s.contains('\0') || s.contains(':') {
        bail!("허용되지 않는 경로 문자입니다: {rel}");
    }
    let mut parts = Vec::new();
    for seg in s.split('/') {
        if seg.is_empty() {
            continue;
        }
        if seg == "." || seg == ".." {
            bail!("상위 폴더(..) 경로는 사용할 수 없습니다: {rel}");
        }
        parts.push(seg);
    }
    Ok(parts.join("/"))
}

pub fn resolve_safe_path(vault_root: &Path, rel: &str) -> Result<PathBuf> {
    if !vault_root.is_dir() {
        bail!("볼트 폴더가 존재하지 않습니다: {}", vault_root.display());
    }
    let norm = normalize_rel_path(rel)?;
    if norm.is_empty() {
        return Ok(vault_root.to_path_buf());
    }
    let mut out = vault_root.to_path_buf();
    for seg in norm.split('/') {
        out.push(seg);
    }
    Ok(out)
}

fn sanitize_file_stem(name: &str) -> Result<String> {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '-',
            _ => c,
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim().to_string();
    if cleaned.is_empty() {
        bail!("유효한 이름을 입력해 주세요");
    }
    Ok(cleaned)
}

fn is_ignored_dir_name(name: &str) -> bool {
    name.starts_with('.') || SKIP_SYSTEM_DIRS.contains(&name)
}

fn format_mtime(meta: Option<&fs::Metadata>) -> (i64, String) {
    let Some(st) = meta.and_then(|m| m.modified().ok()) else {
        return (0, String::new());
    };
    let secs = st
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let dt: DateTime<Local> = st.into();
    (secs, dt.format("%Y-%m-%d %H:%M").to_string())
}

fn note_title_from_rel(rel: &str) -> String {
    let fname = rel.rsplit('/').next().unwrap_or(rel);
    fname.strip_suffix(".md").unwrap_or(fname).to_string()
}

fn strip_md_ext(rel: &str) -> &str {
    rel.strip_suffix(".md")
        .or_else(|| rel.strip_suffix(".MD"))
        .unwrap_or(rel)
}

fn count_words(body: &str) -> usize {
    body.split_whitespace().count()
}

pub fn make_media_url(vault_root: &Path, rel_path: &str) -> String {
    let v = utf8_percent_encode(&vault_root.to_string_lossy(), NON_ALPHANUMERIC).to_string();
    let p = utf8_percent_encode(rel_path, NON_ALPHANUMERIC).to_string();
    format!("/vault-media?vault={v}&path={p}")
}

// ───────────────────────────── 프론트매터 & 마크다운 파서 ─────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct ParsedFrontmatter {
    pub icon: Option<String>,
    pub cover: Option<String>,
    pub properties: Vec<NotePropertyDto>,
    pub raw_frontmatter: String,
}

fn unquote_yaml(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2
        && ((t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\'')))
    {
        let inner = &t[1..t.len() - 1];
        if t.starts_with('"') {
            return inner
                .replace("\\\"", "\"")
                .replace("\\\\", "\\")
                .replace("\\n", "\n");
        }
        return inner.replace("''", "'");
    }
    t.to_string()
}

fn quote_yaml_scalar(s: &str) -> String {
    if s.is_empty() {
        return "\"\"".into();
    }
    let needs_quote = s.starts_with(' ')
        || s.ends_with(' ')
        || s.contains(':')
        || s.contains('#')
        || s.contains('[')
        || s.contains(']')
        || s.contains('{')
        || s.contains('}')
        || s.contains('"')
        || s.contains('\'')
        || s.contains('\n')
        || s.starts_with('-')
        || s.starts_with('@')
        || s.starts_with('&')
        || s.starts_with('*')
        || s.starts_with('!');
    if needs_quote {
        let escaped = s
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n");
        format!("\"{escaped}\"")
    } else {
        s.to_string()
    }
}

fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    b[0..4].iter().all(u8::is_ascii_digit)
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[8..10].iter().all(u8::is_ascii_digit)
}

fn infer_prop_type(key: &str, val: &str, was_list: bool) -> String {
    let k_lower = key.to_ascii_lowercase();
    if was_list || k_lower == "tags" || k_lower == "tag" {
        return "tags".into();
    }
    if val.eq_ignore_ascii_case("true") || val.eq_ignore_ascii_case("false") {
        return "checkbox".into();
    }
    if is_iso_date(val) {
        return "date".into();
    }
    if val.starts_with("http://") || val.starts_with("https://") {
        return "url".into();
    }
    if !val.is_empty() && val.parse::<f64>().is_ok() {
        return "number".into();
    }
    "text".into()
}

fn normalize_tag_token(t: &str) -> String {
    t.trim().trim_start_matches('#').trim().to_string()
}

pub fn parse_frontmatter_and_body(content: &str) -> (ParsedFrontmatter, String) {
    let normalized = content.strip_prefix('\u{feff}').unwrap_or(content);
    let first_newline = match normalized.find('\n') {
        Some(idx) => idx,
        None => return (ParsedFrontmatter::default(), normalized.to_string()),
    };
    let first_line = normalized[..first_newline].trim_end_matches('\r');
    if first_line != "---" {
        return (ParsedFrontmatter::default(), normalized.to_string());
    }

    let rest = &normalized[first_newline + 1..];
    let mut end_offset: Option<(usize, usize)> = None;
    let mut cursor = 0usize;
    for raw_line in rest.split_inclusive('\n') {
        let trimmed = raw_line.trim_end_matches(['\r', '\n']);
        if trimmed == "---" || trimmed == "..." {
            end_offset = Some((cursor, cursor + raw_line.len()));
            break;
        }
        cursor += raw_line.len();
    }
    // 마지막 줄에 개행이 없이 `---`로 끝나는 경우도 처리
    if end_offset.is_none() && cursor < rest.len() {
        let tail = rest[cursor..].trim_end_matches('\r');
        if tail == "---" || tail == "..." {
            end_offset = Some((cursor, rest.len()));
        }
    }

    let Some((yaml_end, body_start)) = end_offset else {
        return (ParsedFrontmatter::default(), normalized.to_string());
    };

    let raw_yaml = rest[..yaml_end].trim_end_matches(['\r', '\n']).to_string();
    let mut body = rest[body_start..].to_string();
    if body.starts_with("\r\n") {
        body = body[2..].to_string();
    } else if body.starts_with('\n') {
        body = body[1..].to_string();
    }

    let mut icon: Option<String> = None;
    let mut cover: Option<String> = None;
    let mut properties: Vec<NotePropertyDto> = Vec::new();

    let lines: Vec<&str> = raw_yaml.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim_end_matches('\r');
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }
        let Some((k_raw, v_raw)) = line.split_once(':') else {
            i += 1;
            continue;
        };
        let key = k_raw.trim().to_string();
        if key.is_empty() {
            i += 1;
            continue;
        }
        let val_trimmed = v_raw.trim();

        if val_trimmed.is_empty() {
            // 다중 줄 리스트 (`- item`) 확인
            let mut items = Vec::new();
            let mut j = i + 1;
            while j < lines.len() {
                let sub = lines[j].trim();
                if sub.is_empty() {
                    j += 1;
                    continue;
                }
                if let Some(rest_item) = sub.strip_prefix('-') {
                    let item_val = unquote_yaml(rest_item.trim());
                    if !item_val.is_empty() {
                        items.push(item_val);
                    }
                    j += 1;
                } else {
                    break;
                }
            }
            if !items.is_empty() {
                let joined = if key.eq_ignore_ascii_case("tags") || key.eq_ignore_ascii_case("tag")
                {
                    items
                        .iter()
                        .map(|x| normalize_tag_token(x))
                        .filter(|x| !x.is_empty())
                        .collect::<Vec<_>>()
                        .join(", ")
                } else {
                    items.join(", ")
                };
                let prop_type = infer_prop_type(&key, &joined, true);
                properties.push(NotePropertyDto {
                    key,
                    value: joined,
                    prop_type,
                });
                i = j;
                continue;
            }
        }

        if val_trimmed.starts_with('[') && val_trimmed.ends_with(']') {
            let inner = &val_trimmed[1..val_trimmed.len() - 1];
            let items: Vec<String> = inner
                .split(',')
                .map(unquote_yaml)
                .map(|x| {
                    if key.eq_ignore_ascii_case("tags") || key.eq_ignore_ascii_case("tag") {
                        normalize_tag_token(&x)
                    } else {
                        x
                    }
                })
                .filter(|x| !x.is_empty())
                .collect();
            let joined = items.join(", ");
            let prop_type = infer_prop_type(&key, &joined, true);
            properties.push(NotePropertyDto {
                key,
                value: joined,
                prop_type,
            });
            i += 1;
            continue;
        }

        let unquoted = unquote_yaml(val_trimmed);
        if key == "icon" {
            if !unquoted.is_empty() {
                icon = Some(unquoted);
            }
        } else if key == "cover" {
            if !unquoted.is_empty() {
                cover = Some(unquoted);
            }
        } else {
            let prop_type = infer_prop_type(&key, &unquoted, false);
            let final_val = if prop_type == "tags" {
                unquoted
                    .split(',')
                    .map(normalize_tag_token)
                    .filter(|x| !x.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ")
            } else if prop_type == "checkbox" {
                if unquoted.eq_ignore_ascii_case("true") {
                    "true".into()
                } else {
                    "false".into()
                }
            } else {
                unquoted
            };
            properties.push(NotePropertyDto {
                key,
                value: final_val,
                prop_type,
            });
        }
        i += 1;
    }

    (
        ParsedFrontmatter {
            icon,
            cover,
            properties,
            raw_frontmatter: raw_yaml,
        },
        body,
    )
}

pub fn serialize_note_content(
    icon: Option<&str>,
    cover: Option<&str>,
    properties: &[NotePropertyDto],
    body: &str,
) -> String {
    let clean_icon = icon.map(str::trim).filter(|s| !s.is_empty());
    let clean_cover = cover.map(str::trim).filter(|s| !s.is_empty());
    let valid_props: Vec<&NotePropertyDto> = properties
        .iter()
        .filter(|p| !p.key.trim().is_empty())
        .collect();

    if clean_icon.is_none() && clean_cover.is_none() && valid_props.is_empty() {
        return body.to_string();
    }

    let mut yaml_lines = Vec::new();
    yaml_lines.push("---".to_string());
    if let Some(ic) = clean_icon {
        yaml_lines.push(format!("icon: {}", quote_yaml_scalar(ic)));
    }
    if let Some(cv) = clean_cover {
        yaml_lines.push(format!("cover: {}", quote_yaml_scalar(cv)));
    }
    for prop in valid_props {
        let key = prop.key.trim();
        if key == "icon" || key == "cover" {
            continue;
        }
        match prop.prop_type.as_str() {
            "tags" => {
                let items: Vec<String> = prop
                    .value
                    .split(',')
                    .map(normalize_tag_token)
                    .filter(|s| !s.is_empty())
                    .map(|s| quote_yaml_scalar(&s))
                    .collect();
                yaml_lines.push(format!("{key}: [{}]", items.join(", ")));
            }
            "checkbox" => {
                let b = prop.value.trim().eq_ignore_ascii_case("true");
                yaml_lines.push(format!("{key}: {b}"));
            }
            "number" if prop.value.trim().parse::<f64>().is_ok() => {
                yaml_lines.push(format!("{key}: {}", prop.value.trim()));
            }
            _ => {
                yaml_lines.push(format!("{key}: {}", quote_yaml_scalar(prop.value.trim())));
            }
        }
    }
    yaml_lines.push("---".to_string());

    if body.is_empty() {
        format!("{}\n", yaml_lines.join("\n"))
    } else {
        format!("{}\n{}", yaml_lines.join("\n"), body)
    }
}

fn strip_inline_code(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_code = false;
    for ch in line.chars() {
        if ch == '`' {
            in_code = !in_code;
            out.push(' ');
        } else if in_code {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}

pub fn extract_headings(body: &str) -> Vec<HeadingDto> {
    let mut headings = Vec::new();
    let mut in_fence = false;
    let mut fence_marker = ' ';

    for (idx, raw_line) in body.lines().enumerate() {
        let trimmed = raw_line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let marker = trimmed.chars().next().unwrap();
            if !in_fence {
                in_fence = true;
                fence_marker = marker;
            } else if marker == fence_marker {
                in_fence = false;
            }
            continue;
        }
        if in_fence {
            continue;
        }
        let hashes = trimmed.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&hashes) {
            let after = &trimmed[hashes..];
            if after.starts_with(' ') || after.starts_with('\t') {
                let text = after.trim().trim_end_matches('#').trim().to_string();
                if !text.is_empty() {
                    headings.push(HeadingDto {
                        level: hashes as u8,
                        text,
                        line: idx + 1,
                    });
                }
            }
        }
    }
    headings
}

fn is_media_target(target: &str) -> bool {
    let Some((_, ext)) = target.rsplit_once('.') else {
        return false;
    };
    MEDIA_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())
}

pub fn extract_outgoing_links(body: &str) -> Vec<String> {
    let mut links = Vec::new();
    let mut seen = HashSet::new();
    let mut in_fence = false;
    let mut fence_marker = ' ';

    for raw_line in body.lines() {
        let trimmed = raw_line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let marker = trimmed.chars().next().unwrap();
            if !in_fence {
                in_fence = true;
                fence_marker = marker;
            } else if marker == fence_marker {
                in_fence = false;
            }
            continue;
        }
        if in_fence {
            continue;
        }

        let clean = strip_inline_code(raw_line);
        let bytes = clean.as_bytes();
        let mut i = 0usize;
        while i + 1 < bytes.len() {
            if bytes[i] == b'[' && bytes[i + 1] == b'[' {
                let is_embed = i > 0 && bytes[i - 1] == b'!';
                if let Some(end_rel) = clean[i + 2..].find("]]") {
                    let inner = &clean[i + 2..i + 2 + end_rel];
                    let before_alias = inner.split('|').next().unwrap_or(inner);
                    let before_heading = before_alias.split('#').next().unwrap_or(before_alias);
                    let target = before_heading.trim().replace('\\', "/");
                    if !target.is_empty() && !(is_embed && is_media_target(&target)) {
                        let norm_target = strip_md_ext(&target).trim().to_string();
                        if !norm_target.is_empty() && seen.insert(norm_target.clone()) {
                            links.push(norm_target);
                        }
                    }
                    i += 2 + end_rel + 2;
                    continue;
                }
            }
            i += 1;
        }
    }
    links
}

fn is_hex_color(tag: &str) -> bool {
    (tag.len() == 3 || tag.len() == 6) && tag.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn extract_tags_from_properties(properties: &[NotePropertyDto], body: &str) -> Vec<String> {
    let mut tags = Vec::new();
    let mut seen = HashSet::new();

    for p in properties {
        if p.prop_type == "tags"
            || p.key.eq_ignore_ascii_case("tags")
            || p.key.eq_ignore_ascii_case("tag")
        {
            for t in p.value.split(',') {
                let clean = normalize_tag_token(t);
                if !clean.is_empty() && seen.insert(clean.to_lowercase()) {
                    tags.push(clean);
                }
            }
        }
    }

    let mut in_fence = false;
    let mut fence_marker = ' ';
    for raw_line in body.lines() {
        let trimmed = raw_line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let marker = trimmed.chars().next().unwrap();
            if !in_fence {
                in_fence = true;
                fence_marker = marker;
            } else if marker == fence_marker {
                in_fence = false;
            }
            continue;
        }
        if in_fence {
            continue;
        }
        // 헤딩 라인 스킵
        let hashes = trimmed.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with([' ', '\t']) {
            continue;
        }

        let clean = strip_inline_code(raw_line);
        let chars: Vec<(usize, char)> = clean.char_indices().collect();
        let mut idx = 0usize;
        while idx < chars.len() {
            let (_, ch) = chars[idx];
            if ch == '#' {
                let prev_ok = idx == 0 || {
                    let prev = chars[idx - 1].1;
                    prev.is_whitespace() || matches!(prev, '(' | '[' | ',' | ';' | ':')
                };
                // URL 내부 `#anchor` 방지
                let prev_colon_slash = idx >= 2 && chars[idx - 1].1 == '/';
                if prev_ok && !prev_colon_slash {
                    let mut end = idx + 1;
                    while end < chars.len() {
                        let c = chars[end].1;
                        if c.is_alphanumeric() || matches!(c, '_' | '-' | '/') {
                            end += 1;
                        } else {
                            break;
                        }
                    }
                    if end > idx + 1 {
                        let start_byte = chars[idx + 1].0;
                        let end_byte = if end < chars.len() {
                            chars[end].0
                        } else {
                            clean.len()
                        };
                        let candidate = clean[start_byte..end_byte].trim_matches('/');
                        let has_letter = candidate.chars().any(|c| !c.is_ascii_digit());
                        if !candidate.is_empty()
                            && has_letter
                            && !is_hex_color(candidate)
                            && seen.insert(candidate.to_lowercase())
                        {
                            tags.push(candidate.to_string());
                        }
                        idx = end;
                        continue;
                    }
                }
            }
            idx += 1;
        }
    }

    tags
}

pub fn extract_tags(fm: &ParsedFrontmatter, body: &str) -> Vec<String> {
    extract_tags_from_properties(&fm.properties, body)
}

fn properties_from_encrypted_payload(
    payload: &crypto::EncryptedNotePayload,
) -> Vec<NotePropertyDto> {
    if !payload.property_list.is_empty() {
        return payload
            .property_list
            .iter()
            .map(|e| NotePropertyDto {
                key: e.key.clone(),
                value: e.value.clone(),
                prop_type: e.prop_type.clone(),
            })
            .collect();
    }
    payload
        .properties
        .iter()
        .map(|(k, v)| NotePropertyDto {
            key: k.clone(),
            value: match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            },
            prop_type: "text".to_string(),
        })
        .collect()
}

fn payload_from_note_fields(
    icon: Option<&str>,
    cover: Option<&str>,
    properties: &[NotePropertyDto],
    body: &str,
) -> crypto::EncryptedNotePayload {
    let property_list: Vec<crypto::EncryptedPropEntry> = properties
        .iter()
        .filter(|p| !p.key.trim().is_empty())
        .map(|p| crypto::EncryptedPropEntry {
            key: p.key.trim().to_string(),
            value: p.value.clone(),
            prop_type: p.prop_type.clone(),
        })
        .collect();
    let prop_map = crypto::props_map_from_entries(&property_list);
    crypto::EncryptedNotePayload {
        icon: icon
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        cover: cover
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        properties: prop_map,
        property_list,
        body: body.to_string(),
    }
}

// ───────────────────────────── 볼트 인덱싱 및 백링크 ─────────────────────────────

struct IndexedNote {
    rel_path: String,
    stem: String,
    rel_no_ext: String,
    icon: Option<String>,
    tags: Vec<String>,
    outgoing: Vec<String>,
    body: String,
    updated_at: String,
    word_count: usize,
    encrypted: bool,
    locked: bool,
}

fn collect_indexed_notes_with_session(
    vault_root: &Path,
    session: Option<&VaultCryptoSession>,
) -> Vec<IndexedNote> {
    let mut notes = Vec::new();
    if !vault_root.is_dir() {
        return notes;
    }

    let walker = WalkDir::new(vault_root).into_iter().filter_entry(|e| {
        if e.depth() == 0 {
            return true;
        }
        if e.file_type().is_dir() {
            let name = e.file_name().to_string_lossy();
            return !is_ignored_dir_name(&name);
        }
        true
    });

    for entry in walker.flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if !path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        {
            continue;
        }
        let Ok(rel) = path.strip_prefix(vault_root) else {
            continue;
        };
        let rel_path = rel.to_string_lossy().replace('\\', "/");
        let Ok(raw) = fs::read_to_string(path) else {
            continue;
        };
        let meta = entry.metadata().ok();
        let (_, updated_at) = format_mtime(meta.as_ref());
        let stem = note_title_from_rel(&rel_path);
        let rel_no_ext = strip_md_ext(&rel_path).to_string();

        if crypto::is_encrypted_markdown(&raw).is_some() {
            if let Some((payload, _, _)) =
                session.and_then(|s| s.try_decrypt_with_session(vault_root, &rel_path, &raw))
            {
                let props = properties_from_encrypted_payload(&payload);
                let tags = extract_tags_from_properties(&props, &payload.body);
                let outgoing = extract_outgoing_links(&payload.body);
                let word_count = count_words(&payload.body);
                notes.push(IndexedNote {
                    rel_path,
                    stem,
                    rel_no_ext,
                    icon: payload.icon,
                    tags,
                    outgoing,
                    body: payload.body,
                    updated_at,
                    word_count,
                    encrypted: true,
                    locked: false,
                });
            } else {
                // 잠긴 암호화 노트는 복호화 전까지 본문/태그/링크를 인덱스에 노출하지 않음
                notes.push(IndexedNote {
                    rel_path,
                    stem,
                    rel_no_ext,
                    icon: Some("🔒".into()),
                    tags: Vec::new(),
                    outgoing: Vec::new(),
                    body: String::new(),
                    updated_at,
                    word_count: 0,
                    encrypted: true,
                    locked: true,
                });
            }
            continue;
        }

        let (fm, body) = parse_frontmatter_and_body(&raw);
        let tags = extract_tags(&fm, &body);
        let outgoing = extract_outgoing_links(&body);
        let word_count = count_words(&body);

        notes.push(IndexedNote {
            rel_path,
            stem,
            rel_no_ext,
            icon: fm.icon,
            tags,
            outgoing,
            body,
            updated_at,
            word_count,
            encrypted: false,
            locked: false,
        });
    }

    notes.sort_by(|a, b| a.rel_path.to_lowercase().cmp(&b.rel_path.to_lowercase()));
    notes
}

fn link_matches_target(link: &str, target_stem_lower: &str, target_rel_no_ext_lower: &str) -> bool {
    let l = strip_md_ext(link.trim()).trim_matches('/').to_lowercase();
    if l.is_empty() {
        return false;
    }
    if l.contains('/') {
        l == target_rel_no_ext_lower || target_rel_no_ext_lower.ends_with(&format!("/{l}"))
    } else {
        l == target_stem_lower
    }
}

fn mask_wikilinks(line: &str) -> Vec<bool> {
    let bytes = line.as_bytes();
    let mut masked = vec![false; bytes.len()];
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        if bytes[i] == b'[' && bytes[i + 1] == b'[' {
            if let Some(end_rel) = line[i + 2..].find("]]") {
                let start = if i > 0 && bytes[i - 1] == b'!' {
                    i - 1
                } else {
                    i
                };
                let finish = i + 2 + end_rel + 2;
                for m in &mut masked[start..finish] {
                    *m = true;
                }
                i = finish;
                continue;
            }
        }
        i += 1;
    }
    masked
}

fn find_unlinked_ci_in_line(line: &str, needle: &str) -> Option<(usize, usize)> {
    if needle.is_empty() {
        return None;
    }
    let masked = mask_wikilinks(line);
    let needle_lower = needle.to_lowercase();
    for (byte_idx, _) in line.char_indices() {
        if masked.get(byte_idx).copied().unwrap_or(false) {
            continue;
        }
        let rest = &line[byte_idx..];
        let mut matched_bytes = 0usize;
        let mut rest_chars = rest.chars();
        let mut needle_chars = needle_lower.chars();
        let mut ok = true;
        loop {
            match needle_chars.next() {
                None => break,
                Some(nc) => {
                    let Some(rc) = rest_chars.next() else {
                        ok = false;
                        break;
                    };
                    if rc.to_lowercase().to_string() != nc.to_string() {
                        ok = false;
                        break;
                    }
                    matched_bytes += rc.len_utf8();
                }
            }
        }
        if ok && matched_bytes > 0 {
            let end_byte = byte_idx + matched_bytes;
            if !masked[byte_idx..end_byte].iter().any(|&m| m) {
                return Some((byte_idx, end_byte));
            }
        }
    }
    None
}

fn trim_snippet(line: &str) -> String {
    let t = line.trim();
    if t.chars().count() <= 140 {
        t.to_string()
    } else {
        let s: String = t.chars().take(140).collect();
        format!("{s}…")
    }
}

fn find_linked_snippet(
    body: &str,
    target_stem_lower: &str,
    target_rel_no_ext_lower: &str,
) -> String {
    for line in body.lines() {
        let links = extract_outgoing_links(line);
        if links
            .iter()
            .any(|l| link_matches_target(l, target_stem_lower, target_rel_no_ext_lower))
        {
            return trim_snippet(line);
        }
    }
    String::new()
}

fn find_unlinked_snippet(body: &str, target_title: &str) -> Option<String> {
    if target_title.chars().count() < 2 {
        return None;
    }
    let mut in_fence = false;
    let mut fence_marker = ' ';
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let marker = trimmed.chars().next().unwrap();
            if !in_fence {
                in_fence = true;
                fence_marker = marker;
            } else if marker == fence_marker {
                in_fence = false;
            }
            continue;
        }
        if in_fence {
            continue;
        }
        if find_unlinked_ci_in_line(line, target_title).is_some() {
            return Some(trim_snippet(line));
        }
    }
    None
}

pub fn compute_backlinks_with_session(
    vault_root: &Path,
    target_rel_path: &str,
    target_title: &str,
    session: Option<&VaultCryptoSession>,
) -> Vec<BacklinkDto> {
    let indexed = collect_indexed_notes_with_session(vault_root, session);
    let target_stem_lower = target_title.to_lowercase();
    let target_rel_no_ext_lower = strip_md_ext(target_rel_path).to_lowercase();

    let mut linked_list = Vec::new();
    let mut unlinked_list = Vec::new();

    for n in indexed {
        if n.locked || n.rel_path.eq_ignore_ascii_case(target_rel_path) {
            continue;
        }
        let is_linked = n
            .outgoing
            .iter()
            .any(|l| link_matches_target(l, &target_stem_lower, &target_rel_no_ext_lower));

        if is_linked {
            let snippet =
                find_linked_snippet(&n.body, &target_stem_lower, &target_rel_no_ext_lower);
            linked_list.push(BacklinkDto {
                path: n.rel_path,
                title: n.stem,
                icon: n.icon,
                snippet,
                linked: true,
            });
        } else if let Some(snippet) = find_unlinked_snippet(&n.body, target_title) {
            unlinked_list.push(BacklinkDto {
                path: n.rel_path,
                title: n.stem,
                icon: n.icon,
                snippet,
                linked: false,
            });
        }
    }

    linked_list.extend(unlinked_list);
    linked_list
}

// ───────────────────────────── 볼트 스캔 ─────────────────────────────

fn build_tree_recursive(
    vault_root: &Path,
    dir: &Path,
    note_map: &HashMap<String, (Option<String>, bool, bool)>,
    folder_count: &mut usize,
) -> Vec<TreeNodeDto> {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut dirs = Vec::new();
    let mut files = Vec::new();

    for entry in read_dir.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if is_ignored_dir_name(&name) || name == "attachments" {
                continue;
            }
            dirs.push((name, path, entry.metadata().ok()));
        } else if path.is_file()
            && path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        {
            files.push((name, path, entry.metadata().ok()));
        }
    }

    dirs.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    files.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

    let mut nodes = Vec::with_capacity(dirs.len() + files.len());
    for (name, path, meta) in dirs {
        let Ok(rel) = path.strip_prefix(vault_root) else {
            continue;
        };
        *folder_count += 1;
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let (modified, _) = format_mtime(meta.as_ref());
        let children = build_tree_recursive(vault_root, &path, note_map, folder_count);
        nodes.push(TreeNodeDto {
            name,
            path: rel_str,
            kind: "folder".into(),
            icon: None,
            children,
            modified,
            encrypted: false,
            locked: false,
        });
    }

    for (name, path, meta) in files {
        let Ok(rel) = path.strip_prefix(vault_root) else {
            continue;
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let (modified, _) = format_mtime(meta.as_ref());
        let display = name.strip_suffix(".md").unwrap_or(&name).to_string();
        let (icon, encrypted, locked) = note_map
            .get(&rel_str)
            .cloned()
            .unwrap_or((None, false, false));
        nodes.push(TreeNodeDto {
            name: display,
            path: rel_str,
            kind: "note".into(),
            icon,
            children: Vec::new(),
            modified,
            encrypted,
            locked,
        });
    }

    nodes
}

pub fn scan_vault_with_session(
    vault_root: &Path,
    revision: u64,
    session: Option<&VaultCryptoSession>,
) -> Result<VaultScanDto> {
    if !vault_root.is_dir() {
        bail!("볼트 폴더를 찾을 수 없습니다: {}", vault_root.display());
    }
    let indexed = collect_indexed_notes_with_session(vault_root, session);
    let mut note_map: HashMap<String, (Option<String>, bool, bool)> =
        HashMap::with_capacity(indexed.len());
    let mut tag_counts: BTreeMap<String, (String, usize)> = BTreeMap::new();
    let mut notes = Vec::with_capacity(indexed.len());
    let mut encrypted_notes = 0usize;

    for n in &indexed {
        if n.encrypted {
            encrypted_notes += 1;
        }
        note_map.insert(n.rel_path.clone(), (n.icon.clone(), n.encrypted, n.locked));
        for t in &n.tags {
            let key = t.to_lowercase();
            let entry = tag_counts.entry(key).or_insert_with(|| (t.clone(), 0));
            entry.1 += 1;
        }
        notes.push(NoteMetaDto {
            path: n.rel_path.clone(),
            title: n.stem.clone(),
            icon: n.icon.clone(),
            tags: n.tags.clone(),
            updated_at: n.updated_at.clone(),
            word_count: n.word_count,
            encrypted: n.encrypted,
            locked: n.locked,
        });
    }

    let mut total_folders = 0usize;
    let tree = build_tree_recursive(vault_root, vault_root, &note_map, &mut total_folders);

    let mut tags: Vec<TagCountDto> = tag_counts
        .into_values()
        .map(|(tag, count)| TagCountDto { tag, count })
        .collect();
    tags.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.tag.cmp(&b.tag)));

    let total_notes = notes.len();
    let crypto_cfg = crypto::load_vault_crypto_config(vault_root);
    let vault_auto_encrypt = crypto_cfg.as_ref().is_some_and(|c| c.auto_encrypt);
    let vault_has_crypto_config = crypto_cfg.is_some();
    let vault_crypto_hint = crypto_cfg.and_then(|c| c.hint);
    let vault_unlocked = session.is_some_and(|s| s.is_vault_unlocked(vault_root));

    Ok(VaultScanDto {
        tree,
        notes,
        tags,
        total_notes,
        total_folders,
        revision,
        encrypted_notes,
        vault_auto_encrypt,
        vault_unlocked,
        vault_has_crypto_config,
        vault_crypto_hint,
    })
}

pub fn scan_vault(vault_root: &Path, revision: u64) -> Result<VaultScanDto> {
    scan_vault_with_session(vault_root, revision, None)
}

// ───────────────────────────── 노트 읽기/쓰기/생성/복제 ─────────────────────────────

pub fn read_note_with_session(
    vault_root: &Path,
    rel_path: &str,
    session: Option<&VaultCryptoSession>,
) -> Result<NoteDocumentDto> {
    let norm = normalize_rel_path(rel_path)?;
    if norm.is_empty() {
        bail!("노트 경로가 비어 있습니다");
    }
    let abs = resolve_safe_path(vault_root, &norm)?;
    if !abs.is_file() {
        bail!("노트 파일을 찾을 수 없습니다: {norm}");
    }
    let raw = fs::read_to_string(&abs)
        .with_context(|| format!("노트 파일을 읽을 수 없습니다: {norm}"))?;
    let meta = fs::metadata(&abs).ok();
    let (_, updated_at) = format_mtime(meta.as_ref());
    let title = note_title_from_rel(&norm);

    if let Some(hdr) = crypto::is_encrypted_markdown(&raw) {
        if let Some((payload, _, sess_hint)) =
            session.and_then(|s| s.try_decrypt_with_session(vault_root, &norm, &raw))
        {
            let props = properties_from_encrypted_payload(&payload);
            let body = payload.body;
            let headings = extract_headings(&body);
            let outgoing_links = extract_outgoing_links(&body);
            let tags = extract_tags_from_properties(&props, &body);
            let backlinks = compute_backlinks_with_session(vault_root, &norm, &title, session);
            let word_count = count_words(&body);
            let char_count = body.chars().count();

            return Ok(NoteDocumentDto {
                path: norm,
                title,
                icon: payload.icon,
                cover: payload.cover,
                properties: props,
                raw_frontmatter: String::new(),
                body,
                headings,
                outgoing_links,
                backlinks,
                tags,
                word_count,
                char_count,
                updated_at,
                encrypted: true,
                locked: false,
                encryption_hint: hdr.hint.or(sess_hint),
            });
        }

        // 잠긴 상태: 평문이나 메타데이터를 노출하지 않고 잠금 카드 정보만 반환
        return Ok(NoteDocumentDto {
            path: norm,
            title,
            icon: Some("🔒".into()),
            cover: None,
            properties: Vec::new(),
            raw_frontmatter: String::new(),
            body: String::new(),
            headings: Vec::new(),
            outgoing_links: Vec::new(),
            backlinks: Vec::new(),
            tags: Vec::new(),
            word_count: 0,
            char_count: 0,
            updated_at,
            encrypted: true,
            locked: true,
            encryption_hint: hdr.hint,
        });
    }

    let (fm, body) = parse_frontmatter_and_body(&raw);
    let headings = extract_headings(&body);
    let outgoing_links = extract_outgoing_links(&body);
    let tags = extract_tags(&fm, &body);
    let backlinks = compute_backlinks_with_session(vault_root, &norm, &title, session);
    let word_count = count_words(&body);
    let char_count = body.chars().count();

    Ok(NoteDocumentDto {
        path: norm,
        title,
        icon: fm.icon,
        cover: fm.cover,
        properties: fm.properties,
        raw_frontmatter: fm.raw_frontmatter,
        body,
        headings,
        outgoing_links,
        backlinks,
        tags,
        word_count,
        char_count,
        updated_at,
        encrypted: false,
        locked: false,
        encryption_hint: None,
    })
}

pub fn read_note(vault_root: &Path, rel_path: &str) -> Result<NoteDocumentDto> {
    read_note_with_session(vault_root, rel_path, None)
}

pub fn save_note_with_session(
    vault_root: &Path,
    rel_path: &str,
    icon: Option<&str>,
    cover: Option<&str>,
    properties: &[NotePropertyDto],
    body: &str,
    session: Option<&VaultCryptoSession>,
) -> Result<NoteDocumentDto> {
    let norm = normalize_rel_path(rel_path)?;
    if norm.is_empty() {
        bail!("노트 경로가 비어 있습니다");
    }
    let abs = resolve_safe_path(vault_root, &norm)?;
    if let Some(parent) = abs.parent() {
        fs::create_dir_all(parent)?;
    }

    if abs.is_file() {
        if let Ok(existing_raw) = fs::read_to_string(&abs) {
            if let Some(hdr) = crypto::is_encrypted_markdown(&existing_raw) {
                let Some((_, pw, sess_hint)) = session
                    .and_then(|s| s.try_decrypt_with_session(vault_root, &norm, &existing_raw))
                else {
                    bail!(
                        "잠긴 암호화 노트는 비밀번호 잠금 해제 후에만 저장할 수 있습니다: {norm}"
                    );
                };
                let hint = hdr.hint.or(sess_hint);
                let payload = payload_from_note_fields(icon, cover, properties, body);
                let armored = crypto::encrypt_note_payload(&payload, &pw, hint.as_deref())?;
                crypto::atomic_write_string(&abs, &armored)
                    .with_context(|| format!("암호화 노트를 저장하지 못했습니다: {norm}"))?;
                return read_note_with_session(vault_root, &norm, session);
            }
        }
    }

    let full = serialize_note_content(icon, cover, properties, body);
    crypto::atomic_write_string(&abs, &full)
        .with_context(|| format!("노트를 저장하지 못했습니다: {norm}"))?;
    read_note_with_session(vault_root, &norm, session)
}

pub fn save_note(
    vault_root: &Path,
    rel_path: &str,
    icon: Option<&str>,
    cover: Option<&str>,
    properties: &[NotePropertyDto],
    body: &str,
) -> Result<NoteDocumentDto> {
    save_note_with_session(vault_root, rel_path, icon, cover, properties, body, None)
}

pub fn create_note_with_session(
    vault_root: &Path,
    folder: &str,
    title: &str,
    body: Option<&str>,
    icon: Option<&str>,
    properties: &[NotePropertyDto],
    session: Option<&VaultCryptoSession>,
) -> Result<(String, NoteDocumentDto)> {
    let folder_norm = normalize_rel_path(folder)?;
    let raw_title = if title.trim().is_empty() {
        "제목 없음"
    } else {
        title.trim()
    };
    let stem = sanitize_file_stem(raw_title.strip_suffix(".md").unwrap_or(raw_title))?;

    let mut candidate_stem = stem.clone();
    let mut seq = 1usize;
    let rel_path = loop {
        let rel = if folder_norm.is_empty() {
            format!("{candidate_stem}.md")
        } else {
            format!("{folder_norm}/{candidate_stem}.md")
        };
        let abs = resolve_safe_path(vault_root, &rel)?;
        if !abs.exists() {
            break rel;
        }
        seq += 1;
        candidate_stem = format!("{stem} {seq}");
    };

    let initial_body = body.unwrap_or("");
    if let Some(cfg) = crypto::load_vault_crypto_config(vault_root) {
        if cfg.auto_encrypt {
            let Some(pw) = session.and_then(|s| s.vault_password(vault_root)) else {
                bail!(
                    "볼트 자동 암호화가 활성화되어 있습니다. 먼저 볼트 암호를 잠금 해제해 주세요."
                );
            };
            let abs = resolve_safe_path(vault_root, &rel_path)?;
            if let Some(parent) = abs.parent() {
                fs::create_dir_all(parent)?;
            }
            let payload = payload_from_note_fields(icon, None, properties, initial_body);
            let armored = crypto::encrypt_note_payload(&payload, &pw, cfg.hint.as_deref())?;
            crypto::atomic_write_string(&abs, &armored)?;
            let doc = read_note_with_session(vault_root, &rel_path, session)?;
            return Ok((rel_path, doc));
        }
    }

    let doc = save_note_with_session(
        vault_root,
        &rel_path,
        icon,
        None,
        properties,
        initial_body,
        session,
    )?;
    Ok((rel_path, doc))
}

pub fn create_note(
    vault_root: &Path,
    folder: &str,
    title: &str,
    body: Option<&str>,
    icon: Option<&str>,
    properties: &[NotePropertyDto],
) -> Result<(String, NoteDocumentDto)> {
    create_note_with_session(vault_root, folder, title, body, icon, properties, None)
}

pub fn duplicate_note(vault_root: &Path, rel_path: &str) -> Result<String> {
    let norm = normalize_rel_path(rel_path)?;
    let abs = resolve_safe_path(vault_root, &norm)?;
    if !abs.is_file() {
        bail!("복제할 노트를 찾을 수 없습니다: {norm}");
    }
    let content = fs::read_to_string(&abs)?;
    let (parent_folder, fname) = match norm.rsplit_once('/') {
        Some((dir, f)) => (dir, f),
        None => ("", norm.as_str()),
    };
    let stem = fname.strip_suffix(".md").unwrap_or(fname);

    let mut idx = 1usize;
    let dup_rel = loop {
        let new_stem = if idx == 1 {
            format!("{stem} (사본)")
        } else {
            format!("{stem} (사본 {idx})")
        };
        let candidate = if parent_folder.is_empty() {
            format!("{new_stem}.md")
        } else {
            format!("{parent_folder}/{new_stem}.md")
        };
        let dup_abs = resolve_safe_path(vault_root, &candidate)?;
        if !dup_abs.exists() {
            fs::write(&dup_abs, &content)?;
            break candidate;
        }
        idx += 1;
    };
    Ok(dup_rel)
}

pub fn create_folder(vault_root: &Path, parent: &str, name: &str) -> Result<String> {
    let parent_norm = normalize_rel_path(parent)?;
    let clean_name = sanitize_file_stem(name)?;
    let rel = if parent_norm.is_empty() {
        clean_name
    } else {
        format!("{parent_norm}/{clean_name}")
    };
    let abs = resolve_safe_path(vault_root, &rel)?;
    if abs.exists() {
        bail!("같은 이름의 폴더나 파일이 이미 존재합니다: {rel}");
    }
    fs::create_dir_all(&abs)?;
    Ok(rel)
}

// ───────────────────────────── 리네임(위키링크 리팩토링 포함) & 이동 ─────────────────────────────

fn rewrite_wikilinks_in_markdown(
    content: &str,
    old_stem: &str,
    old_rel_no_ext: &str,
    new_stem: &str,
    new_rel_no_ext: &str,
) -> (String, usize) {
    let old_stem_lower = old_stem.to_lowercase();
    let old_rel_lower = old_rel_no_ext.to_lowercase();

    let mut out = String::with_capacity(content.len());
    let mut updated = 0usize;
    let mut i = 0usize;
    let bytes = content.as_bytes();

    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'[' && bytes[i + 1] == b'[' {
            if let Some(end_rel) = content[i + 2..].find("]]") {
                let inner = &content[i + 2..i + 2 + end_rel];
                let (before_alias, alias_part) = match inner.split_once('|') {
                    Some((l, a)) => (l, Some(a)),
                    None => (inner, None),
                };
                let (target_raw, heading_part) = match before_alias.split_once('#') {
                    Some((t, h)) => (t, Some(h)),
                    None => (before_alias, None),
                };
                let target_norm = strip_md_ext(target_raw.trim()).replace('\\', "/");
                let target_lower = target_norm.to_lowercase();

                let replacement_target = if target_norm.contains('/') {
                    if target_lower == old_rel_lower {
                        Some(new_rel_no_ext)
                    } else {
                        None
                    }
                } else if target_lower == old_stem_lower {
                    Some(new_stem)
                } else {
                    None
                };

                if let Some(rep) = replacement_target {
                    updated += 1;
                    out.push_str("[[");
                    out.push_str(rep);
                    if let Some(h) = heading_part {
                        out.push('#');
                        out.push_str(h);
                    }
                    if let Some(a) = alias_part {
                        out.push('|');
                        out.push_str(a);
                    }
                    out.push_str("]]");
                    i += 2 + end_rel + 2;
                    continue;
                }
            }
        }
        let ch = content[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }

    (out, updated)
}

pub fn rename_entry_with_session(
    vault_root: &Path,
    rel_path: &str,
    new_name: &str,
    mut session: Option<&mut VaultCryptoSession>,
) -> Result<RenameResult> {
    let norm = normalize_rel_path(rel_path)?;
    if norm.is_empty() {
        bail!("이름을 변경할 대상이 지정되지 않았습니다");
    }
    let abs = resolve_safe_path(vault_root, &norm)?;
    if !abs.exists() {
        bail!("대상을 찾을 수 없습니다: {norm}");
    }

    let is_dir = abs.is_dir();
    let is_note = abs.is_file()
        && abs
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("md"));

    let parent_dir = norm.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let raw_stem = if is_note {
        new_name
            .trim()
            .strip_suffix(".md")
            .unwrap_or(new_name.trim())
    } else {
        new_name.trim()
    };
    let clean_stem = sanitize_file_stem(raw_stem)?;
    let new_leaf = if is_note {
        format!("{clean_stem}.md")
    } else {
        clean_stem.clone()
    };
    let new_rel = if parent_dir.is_empty() {
        new_leaf
    } else {
        format!("{parent_dir}/{new_leaf}")
    };

    if new_rel == norm {
        return Ok(RenameResult {
            new_path: norm,
            updated_links: 0,
        });
    }

    let new_abs = resolve_safe_path(vault_root, &new_rel)?;
    if new_abs.exists() && !norm.eq_ignore_ascii_case(&new_rel) {
        bail!("같은 이름의 항목이 이미 존재합니다: {new_rel}");
    }

    fs::rename(&abs, &new_abs)?;

    if let Some(sess) = session.as_deref_mut() {
        if is_dir {
            sess.rename_folder_prefix(vault_root, &norm, &new_rel);
        } else if is_note {
            sess.rename_note_key(vault_root, &norm, &new_rel);
        }
    }

    let mut updated_links = 0usize;
    if is_note {
        let old_stem = note_title_from_rel(&norm);
        let old_rel_no_ext = strip_md_ext(&norm).to_string();
        let new_stem = note_title_from_rel(&new_rel);
        let new_rel_no_ext = strip_md_ext(&new_rel).to_string();

        let walker = WalkDir::new(vault_root).into_iter().filter_entry(|e| {
            if e.depth() == 0 {
                return true;
            }
            if e.file_type().is_dir() {
                return !is_ignored_dir_name(&e.file_name().to_string_lossy());
            }
            true
        });

        let sess_ro = session.as_deref();
        for entry in walker.flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            let p = entry.path();
            if !p
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("md"))
            {
                continue;
            }
            let Ok(raw) = fs::read_to_string(p) else {
                continue;
            };
            if let Some(hdr) = crypto::is_encrypted_markdown(&raw) {
                let Ok(rel_p) = p.strip_prefix(vault_root) else {
                    continue;
                };
                let p_rel = rel_p.to_string_lossy().replace('\\', "/");
                if let Some((mut payload, pw, sess_hint)) =
                    sess_ro.and_then(|s| s.try_decrypt_with_session(vault_root, &p_rel, &raw))
                {
                    let (rewritten, count) = rewrite_wikilinks_in_markdown(
                        &payload.body,
                        &old_stem,
                        &old_rel_no_ext,
                        &new_stem,
                        &new_rel_no_ext,
                    );
                    if count > 0 {
                        payload.body = rewritten;
                        let hint = hdr.hint.or(sess_hint);
                        let armored = crypto::encrypt_note_payload(&payload, &pw, hint.as_deref())?;
                        crypto::atomic_write_string(p, &armored)?;
                        updated_links += count;
                    }
                }
                continue;
            }
            let (rewritten, count) = rewrite_wikilinks_in_markdown(
                &raw,
                &old_stem,
                &old_rel_no_ext,
                &new_stem,
                &new_rel_no_ext,
            );
            if count > 0 {
                crypto::atomic_write_string(p, &rewritten)?;
                updated_links += count;
            }
        }
    }

    Ok(RenameResult {
        new_path: new_rel,
        updated_links,
    })
}

pub fn rename_entry(vault_root: &Path, rel_path: &str, new_name: &str) -> Result<RenameResult> {
    rename_entry_with_session(vault_root, rel_path, new_name, None)
}

pub fn move_entry_with_session(
    vault_root: &Path,
    rel_path: &str,
    target_folder: &str,
    session: Option<&mut VaultCryptoSession>,
) -> Result<String> {
    let norm = normalize_rel_path(rel_path)?;
    if norm.is_empty() {
        bail!("이동할 경로가 비어 있습니다");
    }
    let target_norm = normalize_rel_path(target_folder)?;
    let abs = resolve_safe_path(vault_root, &norm)?;
    if !abs.exists() {
        bail!("이동할 대상을 찾을 수 없습니다: {norm}");
    }
    let is_dir = abs.is_dir();
    if is_dir && (target_norm == norm || target_norm.starts_with(&format!("{norm}/"))) {
        bail!("폴더를 자기 자신이나 하위 폴더 안으로 이동할 수 없습니다");
    }
    let leaf = norm.rsplit('/').next().unwrap_or(&norm);
    let new_rel = if target_norm.is_empty() {
        leaf.to_string()
    } else {
        format!("{target_norm}/{leaf}")
    };
    if new_rel == norm {
        return Ok(norm);
    }
    let new_abs = resolve_safe_path(vault_root, &new_rel)?;
    if new_abs.exists() {
        bail!("대상 위치에 같은 이름이 이미 존재합니다: {new_rel}");
    }
    if let Some(parent) = new_abs.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&abs, &new_abs)?;
    if let Some(sess) = session {
        if is_dir {
            sess.rename_folder_prefix(vault_root, &norm, &new_rel);
        } else {
            sess.rename_note_key(vault_root, &norm, &new_rel);
        }
    }
    Ok(new_rel)
}

pub fn move_entry(vault_root: &Path, rel_path: &str, target_folder: &str) -> Result<String> {
    move_entry_with_session(vault_root, rel_path, target_folder, None)
}

// ───────────────────────────── 연결 안 된 멘션 링크 변환 ─────────────────────────────

pub fn link_unlinked_mention_with_session(
    vault_root: &Path,
    source_path: &str,
    target_title: &str,
    session: Option<&VaultCryptoSession>,
) -> Result<()> {
    let norm = normalize_rel_path(source_path)?;
    let abs = resolve_safe_path(vault_root, &norm)?;
    if !abs.is_file() {
        bail!("원본 노트를 찾을 수 없습니다: {norm}");
    }
    let title = target_title.trim();
    if title.is_empty() {
        bail!("링크할 제목이 비어 있습니다");
    }
    let raw = fs::read_to_string(&abs)?;
    let encrypted_ctx = if let Some(hdr) = crypto::is_encrypted_markdown(&raw) {
        let Some((payload, pw, sess_hint)) =
            session.and_then(|s| s.try_decrypt_with_session(vault_root, &norm, &raw))
        else {
            bail!("잠긴 암호화 노트는 잠금 해제 후 링크를 변환할 수 있습니다: {norm}");
        };
        Some((payload, pw, hdr.hint.or(sess_hint)))
    } else {
        None
    };

    let (fm, body) = if let Some((ref payload, _, _)) = encrypted_ctx {
        (ParsedFrontmatter::default(), payload.body.clone())
    } else {
        parse_frontmatter_and_body(&raw)
    };

    let mut in_fence = false;
    let mut fence_marker = ' ';
    let mut replaced = false;
    let mut new_body = String::with_capacity(body.len() + 8);

    for chunk in body.split_inclusive('\n') {
        let (line_no_nl, nl) = if let Some(stripped) = chunk.strip_suffix("\r\n") {
            (stripped, "\r\n")
        } else if let Some(stripped) = chunk.strip_suffix('\n') {
            (stripped, "\n")
        } else {
            (chunk, "")
        };

        let trimmed = line_no_nl.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let marker = trimmed.chars().next().unwrap();
            if !in_fence {
                in_fence = true;
                fence_marker = marker;
            } else if marker == fence_marker {
                in_fence = false;
            }
            new_body.push_str(chunk);
            continue;
        }

        if !replaced && !in_fence {
            if let Some((start, end)) = find_unlinked_ci_in_line(line_no_nl, title) {
                new_body.push_str(&line_no_nl[..start]);
                new_body.push_str("[[");
                new_body.push_str(title);
                new_body.push_str("]]");
                new_body.push_str(&line_no_nl[end..]);
                new_body.push_str(nl);
                replaced = true;
                continue;
            }
        }
        new_body.push_str(chunk);
    }

    if replaced {
        if let Some((mut payload, pw, hint)) = encrypted_ctx {
            payload.body = new_body;
            let armored = crypto::encrypt_note_payload(&payload, &pw, hint.as_deref())?;
            crypto::atomic_write_string(&abs, &armored)?;
        } else {
            let updated = if !fm.raw_frontmatter.is_empty()
                || fm.icon.is_some()
                || fm.cover.is_some()
                || !fm.properties.is_empty()
            {
                format!("---\n{}\n---\n{}", fm.raw_frontmatter, new_body)
            } else {
                new_body
            };
            crypto::atomic_write_string(&abs, &updated)?;
        }
    }

    Ok(())
}

pub fn link_unlinked_mention(
    vault_root: &Path,
    source_path: &str,
    target_title: &str,
) -> Result<()> {
    link_unlinked_mention_with_session(vault_root, source_path, target_title, None)
}

// ───────────────────────────── 휴지통 (.trash) ─────────────────────────────

fn load_trash_manifest(trash_dir: &Path) -> HashMap<String, String> {
    let p = trash_dir.join(TRASH_MANIFEST);
    fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_trash_manifest(trash_dir: &Path, map: &HashMap<String, String>) -> Result<()> {
    fs::create_dir_all(trash_dir)?;
    let p = trash_dir.join(TRASH_MANIFEST);
    fs::write(p, serde_json::to_string_pretty(map)?)?;
    Ok(())
}

pub fn delete_to_trash_with_session(
    vault_root: &Path,
    rel_path: &str,
    session: Option<&mut VaultCryptoSession>,
) -> Result<()> {
    let norm = normalize_rel_path(rel_path)?;
    if norm.is_empty() {
        bail!("삭제할 대상이 지정되지 않았습니다");
    }
    let abs = resolve_safe_path(vault_root, &norm)?;
    if !abs.exists() {
        bail!("삭제할 파일을 찾을 수 없습니다: {norm}");
    }
    let trash_dir = vault_root.join(".trash");
    fs::create_dir_all(&trash_dir)?;

    let leaf = norm.rsplit('/').next().unwrap_or(&norm);
    let mut trash_name = leaf.to_string();
    let mut seq = 1usize;
    while trash_dir.join(&trash_name).exists() {
        let ts = Local::now().format("%Y%m%d-%H%M%S");
        trash_name = match leaf.rsplit_once('.') {
            Some((s, ext)) => format!("{s}-{ts}-{seq}.{ext}"),
            None => format!("{leaf}-{ts}-{seq}"),
        };
        seq += 1;
    }

    fs::rename(&abs, trash_dir.join(&trash_name))?;
    if let Some(sess) = session {
        sess.remove_note_key(vault_root, &norm);
    }
    let mut manifest = load_trash_manifest(&trash_dir);
    manifest.insert(trash_name, norm);
    let _ = save_trash_manifest(&trash_dir, &manifest);
    Ok(())
}

pub fn delete_to_trash(vault_root: &Path, rel_path: &str) -> Result<()> {
    delete_to_trash_with_session(vault_root, rel_path, None)
}

pub fn list_trash(vault_root: &Path) -> Result<Vec<TrashItemDto>> {
    let trash_dir = vault_root.join(".trash");
    if !trash_dir.is_dir() {
        return Ok(Vec::new());
    }
    let manifest = load_trash_manifest(&trash_dir);
    let mut items = Vec::new();

    for entry in fs::read_dir(&trash_dir)?.flatten() {
        let trash_name = entry.file_name().to_string_lossy().to_string();
        if trash_name == TRASH_MANIFEST {
            continue;
        }
        let meta = entry.metadata().ok();
        let is_dir = meta.as_ref().is_some_and(|m| m.is_dir());
        let (secs, modified_at) = format_mtime(meta.as_ref());
        let display_name = manifest
            .get(&trash_name)
            .cloned()
            .unwrap_or_else(|| trash_name.clone());
        items.push((
            secs,
            TrashItemDto {
                trash_name,
                display_name,
                modified_at,
                is_dir,
            },
        ));
    }

    items.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1.trash_name.cmp(&b.1.trash_name))
    });
    Ok(items.into_iter().map(|(_, item)| item).collect())
}

pub fn restore_trash(vault_root: &Path, trash_name: &str) -> Result<String> {
    if trash_name.contains(['/', '\\']) || trash_name == ".." || trash_name == "." {
        bail!("올바르지 않은 휴지통 항목입니다");
    }
    let trash_dir = vault_root.join(".trash");
    let src = trash_dir.join(trash_name);
    if !src.exists() {
        bail!("휴지통에서 항목을 찾을 수 없습니다: {trash_name}");
    }
    let mut manifest = load_trash_manifest(&trash_dir);
    let preferred_rel = manifest
        .remove(trash_name)
        .and_then(|r| normalize_rel_path(&r).ok())
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| trash_name.to_string());

    let mut dest_rel = preferred_rel.clone();
    let mut seq = 1usize;
    while resolve_safe_path(vault_root, &dest_rel)?.exists() {
        seq += 1;
        let (dir, leaf) = match preferred_rel.rsplit_once('/') {
            Some((d, l)) => (format!("{d}/"), l),
            None => (String::new(), preferred_rel.as_str()),
        };
        dest_rel = match leaf.rsplit_once('.') {
            Some((s, ext)) => format!("{dir}{s} (복원 {seq}).{ext}"),
            None => format!("{dir}{leaf} (복원 {seq})"),
        };
    }

    let dest_abs = resolve_safe_path(vault_root, &dest_rel)?;
    if let Some(parent) = dest_abs.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&src, &dest_abs)?;
    let _ = save_trash_manifest(&trash_dir, &manifest);
    Ok(dest_rel)
}

pub fn empty_trash(vault_root: &Path) -> Result<()> {
    let trash_dir = vault_root.join(".trash");
    if trash_dir.is_dir() {
        fs::remove_dir_all(&trash_dir)?;
    }
    Ok(())
}

// ───────────────────────────── 볼트 전문 검색 ─────────────────────────────

pub fn search_vault_with_session(
    vault_root: &Path,
    query: &str,
    session: Option<&VaultCryptoSession>,
) -> Result<Vec<SearchHitDto>> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }

    let mut required_tags: Vec<String> = Vec::new();
    let mut required_paths: Vec<String> = Vec::new();
    let mut free_terms: Vec<String> = Vec::new();

    for token in q.split_whitespace() {
        if let Some(tag_val) = token.strip_prefix("tag:") {
            let t = normalize_tag_token(tag_val).to_lowercase();
            if !t.is_empty() {
                required_tags.push(t);
            }
        } else if let Some(tag_val) = token.strip_prefix('#') {
            let t = normalize_tag_token(tag_val).to_lowercase();
            if !t.is_empty() {
                required_tags.push(t);
            }
        } else if let Some(path_val) = token.strip_prefix("path:") {
            let p = path_val
                .trim()
                .trim_matches('/')
                .replace('\\', "/")
                .to_lowercase();
            if !p.is_empty() {
                required_paths.push(p);
            }
        } else {
            free_terms.push(token.to_lowercase());
        }
    }

    let full_free_lower = free_terms.join(" ");
    let indexed = collect_indexed_notes_with_session(vault_root, session);
    let mut hits = Vec::new();

    for n in indexed {
        let rel_lower = n.rel_path.to_lowercase();
        if !required_paths.iter().all(|rp| rel_lower.contains(rp)) {
            continue;
        }
        let note_tags_lower: Vec<String> = n.tags.iter().map(|t| t.to_lowercase()).collect();
        if !required_tags.iter().all(|rt| {
            note_tags_lower
                .iter()
                .any(|nt| nt == rt || nt.starts_with(&format!("{rt}/")))
        }) {
            continue;
        }

        let mut score = 0i64;
        let mut matches = Vec::new();

        if !required_tags.is_empty() {
            score += 40;
        }
        if !required_paths.is_empty() {
            score += 20;
        }

        if !free_terms.is_empty() {
            let title_lower = n.stem.to_lowercase();
            let body_lower = if n.locked {
                String::new()
            } else {
                n.body.to_lowercase()
            };

            let all_terms_present = free_terms
                .iter()
                .all(|term| title_lower.contains(term) || (!n.locked && body_lower.contains(term)));
            if !all_terms_present {
                continue;
            }

            if title_lower == full_free_lower {
                score += 200;
            } else if title_lower.starts_with(&full_free_lower) {
                score += 120;
            } else if title_lower.contains(&full_free_lower) {
                score += 80;
            }

            if n.locked {
                matches.push(SearchMatchDto {
                    line: 1,
                    snippet: "🔒 암호화된 노트 (제목 일치)".into(),
                });
            } else {
                for (idx, line) in n.body.lines().enumerate() {
                    let ll = line.to_lowercase();
                    if free_terms.iter().any(|term| ll.contains(term)) {
                        score += 10;
                        if matches.len() < 5 {
                            matches.push(SearchMatchDto {
                                line: idx + 1,
                                snippet: trim_snippet(line),
                            });
                        }
                    }
                }
            }
        } else if matches.is_empty() {
            if n.locked {
                matches.push(SearchMatchDto {
                    line: 1,
                    snippet: "🔒 암호화된 노트".into(),
                });
            } else if let Some(first_non_empty) = n.body.lines().find(|l| !l.trim().is_empty()) {
                matches.push(SearchMatchDto {
                    line: 1,
                    snippet: trim_snippet(first_non_empty),
                });
            }
        }

        if score > 0 {
            hits.push(SearchHitDto {
                path: n.rel_path,
                title: n.stem,
                icon: n.icon,
                matches,
                score,
            });
        }
    }

    hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.title.cmp(&b.title)));
    Ok(hits)
}

pub fn search_vault(vault_root: &Path, query: &str) -> Result<Vec<SearchHitDto>> {
    search_vault_with_session(vault_root, query, None)
}

// ───────────────────────────── 지식 그래프 빌더 ─────────────────────────────

pub fn build_graph_with_session(
    vault_root: &Path,
    center_path: Option<&str>,
    depth: Option<usize>,
    include_tags: bool,
    session: Option<&VaultCryptoSession>,
) -> Result<GraphDto> {
    let indexed = collect_indexed_notes_with_session(vault_root, session);

    let mut stem_to_id: HashMap<String, String> = HashMap::new();
    let mut rel_to_id: HashMap<String, String> = HashMap::new();
    let mut nodes_map: BTreeMap<String, GraphNodeDto> = BTreeMap::new();

    for n in &indexed {
        let id = n.rel_path.clone();
        let group = n
            .rel_path
            .split_once('/')
            .map(|(top, _)| top.to_string())
            .unwrap_or_else(|| "root".into());
        stem_to_id
            .entry(n.stem.to_lowercase())
            .or_insert_with(|| id.clone());
        rel_to_id.insert(n.rel_no_ext.to_lowercase(), id.clone());
        nodes_map.insert(
            id.clone(),
            GraphNodeDto {
                id,
                label: n.stem.clone(),
                path: Some(n.rel_path.clone()),
                kind: "note".into(),
                group,
                degree: 0,
            },
        );
    }

    let mut edge_set: HashSet<(String, String)> = HashSet::new();
    let mut edges: Vec<GraphEdgeDto> = Vec::new();

    for n in &indexed {
        if n.locked {
            continue;
        }
        let src_id = n.rel_path.clone();
        for link in &n.outgoing {
            let norm_link = strip_md_ext(link.trim()).trim_matches('/');
            if norm_link.is_empty() {
                continue;
            }
            let lower = norm_link.to_lowercase();
            let resolved = if lower.contains('/') {
                rel_to_id.get(&lower).cloned()
            } else {
                stem_to_id.get(&lower).cloned()
            };

            let dst_id = match resolved {
                Some(existing_id) => existing_id,
                None => {
                    let ghost_id = format!("ghost:{lower}");
                    let ghost_label = norm_link
                        .rsplit('/')
                        .next()
                        .unwrap_or(norm_link)
                        .to_string();
                    nodes_map.entry(ghost_id.clone()).or_insert(GraphNodeDto {
                        id: ghost_id.clone(),
                        label: ghost_label,
                        path: None,
                        kind: "ghost".into(),
                        group: "ghost".into(),
                        degree: 0,
                    });
                    ghost_id
                }
            };

            if src_id != dst_id && edge_set.insert((src_id.clone(), dst_id.clone())) {
                edges.push(GraphEdgeDto {
                    source: src_id.clone(),
                    target: dst_id,
                });
            }
        }

        if include_tags {
            for t in &n.tags {
                let tag_id = format!("tag:{}", t.to_lowercase());
                nodes_map.entry(tag_id.clone()).or_insert(GraphNodeDto {
                    id: tag_id.clone(),
                    label: format!("#{t}"),
                    path: None,
                    kind: "tag".into(),
                    group: "tag".into(),
                    degree: 0,
                });
                if edge_set.insert((src_id.clone(), tag_id.clone())) {
                    edges.push(GraphEdgeDto {
                        source: src_id.clone(),
                        target: tag_id,
                    });
                }
            }
        }
    }

    // center_path가 주어지면 BFS로 주변 서브그래프만 필터링
    if let Some(cp) = center_path.map(str::trim).filter(|s| !s.is_empty()) {
        let cp_norm = normalize_rel_path(cp).unwrap_or_else(|_| cp.to_string());
        if nodes_map.contains_key(&cp_norm) {
            let max_hops = depth.unwrap_or(2);
            let mut adj: HashMap<String, Vec<String>> = HashMap::new();
            for e in &edges {
                adj.entry(e.source.clone())
                    .or_default()
                    .push(e.target.clone());
                adj.entry(e.target.clone())
                    .or_default()
                    .push(e.source.clone());
            }

            let mut visited: HashSet<String> = HashSet::new();
            let mut q: VecDeque<(String, usize)> = VecDeque::new();
            visited.insert(cp_norm.clone());
            q.push_back((cp_norm, 0));

            while let Some((cur, d)) = q.pop_front() {
                if d >= max_hops {
                    continue;
                }
                if let Some(neighbors) = adj.get(&cur) {
                    for nb in neighbors {
                        if visited.insert(nb.clone()) {
                            q.push_back((nb.clone(), d + 1));
                        }
                    }
                }
            }

            nodes_map.retain(|id, _| visited.contains(id));
            edges.retain(|e| visited.contains(&e.source) && visited.contains(&e.target));
        }
    }

    for e in &edges {
        if let Some(n) = nodes_map.get_mut(&e.source) {
            n.degree += 1;
        }
        if let Some(n) = nodes_map.get_mut(&e.target) {
            n.degree += 1;
        }
    }

    Ok(GraphDto {
        nodes: nodes_map.into_values().collect(),
        edges,
    })
}

pub fn build_graph(
    vault_root: &Path,
    center_path: Option<&str>,
    depth: Option<usize>,
    include_tags: bool,
) -> Result<GraphDto> {
    build_graph_with_session(vault_root, center_path, depth, include_tags, None)
}

// ───────────────────────────── 데일리 노트 & 템플릿 ─────────────────────────────

pub fn open_daily_note_with_session(
    vault_root: &Path,
    daily_folder: &str,
    session: Option<&VaultCryptoSession>,
) -> Result<(String, NoteDocumentDto)> {
    let folder = normalize_rel_path(daily_folder)?;
    let today = Local::now().format("%Y-%m-%d").to_string();
    let rel_path = if folder.is_empty() {
        format!("{today}.md")
    } else {
        format!("{folder}/{today}.md")
    };
    let abs = resolve_safe_path(vault_root, &rel_path)?;

    if !abs.exists() {
        let props = vec![
            NotePropertyDto {
                key: "date".into(),
                value: today.clone(),
                prop_type: "date".into(),
            },
            NotePropertyDto {
                key: "tags".into(),
                value: "daily".into(),
                prop_type: "tags".into(),
            },
        ];
        let body = format!(
            "# {today} 데일리 노트\n\n## 오늘의 할 일\n- [ ] 가장 중요한 핵심 작업 1가지 집중하기\n- [ ] 받은 메시지 및 일정 점검\n\n## 메모 & 아이디어\n- \n\n## 오늘의 회고\n- **잘한 점**: \n- **내일 개선할 점**: \n"
        );
        if let Some(cfg) = crypto::load_vault_crypto_config(vault_root) {
            if cfg.auto_encrypt {
                let Some(pw) = session.and_then(|s| s.vault_password(vault_root)) else {
                    bail!("볼트 자동 암호화가 활성화되어 있습니다. 먼저 볼트 암호를 잠금 해제해 주세요.");
                };
                if let Some(parent) = abs.parent() {
                    fs::create_dir_all(parent)?;
                }
                let payload = payload_from_note_fields(Some("📅"), None, &props, &body);
                let armored = crypto::encrypt_note_payload(&payload, &pw, cfg.hint.as_deref())?;
                crypto::atomic_write_string(&abs, &armored)?;
                let doc = read_note_with_session(vault_root, &rel_path, session)?;
                return Ok((rel_path, doc));
            }
        }
        let doc = save_note_with_session(
            vault_root,
            &rel_path,
            Some("📅"),
            None,
            &props,
            &body,
            session,
        )?;
        return Ok((rel_path, doc));
    }

    let doc = read_note_with_session(vault_root, &rel_path, session)?;
    Ok((rel_path, doc))
}

pub fn open_daily_note(vault_root: &Path, daily_folder: &str) -> Result<(String, NoteDocumentDto)> {
    open_daily_note_with_session(vault_root, daily_folder, None)
}

fn substitute_template_vars(text: &str, today: &str, now_time: &str) -> String {
    text.replace("{{date}}", today)
        .replace("{{DATE}}", today)
        .replace("{{time}}", now_time)
        .replace("{{TIME}}", now_time)
}

pub fn list_templates(vault_root: &Path, templates_folder: &str) -> Result<Vec<TemplateItemDto>> {
    let today = Local::now().format("%Y-%m-%d").to_string();
    let now_time = Local::now().format("%H:%M").to_string();

    let mut templates = vec![
        TemplateItemDto {
            id: "builtin-meeting".into(),
            name: "회의록".into(),
            icon: "🤝".into(),
            description: "참석자, 안건, 결정 사항 및 액션 아이템 정리".into(),
            properties: vec![
                NotePropertyDto {
                    key: "date".into(),
                    value: today.clone(),
                    prop_type: "date".into(),
                },
                NotePropertyDto {
                    key: "tags".into(),
                    value: "meeting".into(),
                    prop_type: "tags".into(),
                },
                NotePropertyDto {
                    key: "참석자".into(),
                    value: "".into(),
                    prop_type: "text".into(),
                },
            ],
            body: format!(
                "# 회의록 ({today} {now_time})\n\n> **목적**: 오늘 회의의 핵심 목표를 한 줄로 요약하세요.\n\n## 주요 안건\n1. \n2. \n\n## 논의 및 결정 사항\n- \n\n## 실행 과제 (Action Items)\n| 담당자 | 할 일 | 기한 | 완료 |\n| --- | --- | --- | --- |\n| @나 | 후속 자료 공유 | {today} | 미완료 |\n"
            ),
        },
        TemplateItemDto {
            id: "builtin-project".into(),
            name: "프로젝트 기획서".into(),
            icon: "🚀".into(),
            description: "배경, 목표, 마일스톤, 리스크를 구조화하는 기획 문서".into(),
            properties: vec![
                NotePropertyDto {
                    key: "status".into(),
                    value: "기획 중".into(),
                    prop_type: "text".into(),
                },
                NotePropertyDto {
                    key: "start_date".into(),
                    value: today.clone(),
                    prop_type: "date".into(),
                },
                NotePropertyDto {
                    key: "tags".into(),
                    value: "project, spec".into(),
                    prop_type: "tags".into(),
                },
            ],
            body: "# 프로젝트 개요\n\n## 1. 문제 정의 & 배경\n- 왜 이 프로젝트가 지금 필요한가요?\n\n## 2. 핵심 목표 (Goals)\n- [ ] 목표 1:\n- [ ] 목표 2:\n\n## 3. 마일스톤 일정\n| 단계 | 주요 산출물 | 목표일 | 상태 |\n| --- | --- | --- | --- |\n| 1단계 | 요구사항 정의 | | 진행 중 |\n| 2단계 | 프로토타입 구현 | | 대기 |\n| 3단계 | 배포 및 검증 | | 대기 |\n\n## 4. 참고 문서\n- \n".into(),
        },
        TemplateItemDto {
            id: "builtin-journal".into(),
            name: "일일 저널".into(),
            icon: "📅".into(),
            description: "할 일 체크리스트, 기록, 하루 회고를 담는 데일리 노트".into(),
            properties: vec![
                NotePropertyDto {
                    key: "date".into(),
                    value: today.clone(),
                    prop_type: "date".into(),
                },
                NotePropertyDto {
                    key: "tags".into(),
                    value: "daily, journal".into(),
                    prop_type: "tags".into(),
                },
            ],
            body: format!(
                "# {today} 일일 저널\n\n## 오늘의 우선순위\n- [ ] \n- [ ] \n\n## 기록 & 생각\n- \n\n## 감사한 일 3가지\n1. \n2. \n3. \n"
            ),
        },
        TemplateItemDto {
            id: "builtin-weekly".into(),
            name: "주간 회고".into(),
            icon: "🧭".into(),
            description: "Keep / Problem / Try 프레임워크 기반 주간 돌아보기".into(),
            properties: vec![
                NotePropertyDto {
                    key: "date".into(),
                    value: today.clone(),
                    prop_type: "date".into(),
                },
                NotePropertyDto {
                    key: "tags".into(),
                    value: "weekly, review".into(),
                    prop_type: "tags".into(),
                },
            ],
            body: "# 주간 회고 (KPT)\n\n## Keep (이번 주 잘한 점 / 계속 유지할 것)\n- \n\n## Problem (아쉬웠던 점 / 병목)\n- \n\n## Try (다음 주에 시도할 구체적 행동)\n- [ ] \n".into(),
        },
        TemplateItemDto {
            id: "builtin-tech".into(),
            name: "기술 문서 / 트러블슈팅".into(),
            icon: "🛠️".into(),
            description: "아키텍처 결정 기록(ADR) 및 기술 이슈 해결 과정 정리".into(),
            properties: vec![
                NotePropertyDto {
                    key: "tags".into(),
                    value: "tech, engineering".into(),
                    prop_type: "tags".into(),
                },
                NotePropertyDto {
                    key: "검토완료".into(),
                    value: "false".into(),
                    prop_type: "checkbox".into(),
                },
            ],
            body: "# 기술 요약\n\n## 현상 및 맥락\n- \n\n## 원인 분석\n```rust\n// 관련 코드 또는 로그 스니펫\n```\n\n## 해결 방안 비교\n| 대안 | 장점 | 단점 |\n| --- | --- | --- |\n| 방안 A | | |\n| 방안 B | | |\n\n## 최종 결론\n- \n".into(),
        },
        TemplateItemDto {
            id: "builtin-book".into(),
            name: "독서 노트".into(),
            icon: "📚".into(),
            description: "인상 깊은 문장 발췌와 내 삶에 적용할 인사이트 정리".into(),
            properties: vec![
                NotePropertyDto {
                    key: "저자".into(),
                    value: "".into(),
                    prop_type: "text".into(),
                },
                NotePropertyDto {
                    key: "평점".into(),
                    value: "5".into(),
                    prop_type: "number".into(),
                },
                NotePropertyDto {
                    key: "완독".into(),
                    value: "false".into(),
                    prop_type: "checkbox".into(),
                },
                NotePropertyDto {
                    key: "tags".into(),
                    value: "book, reading".into(),
                    prop_type: "tags".into(),
                },
            ],
            body: "# 핵심 한 줄 요약\n\n> 이 책에서 얻은 가장 큰 통찰을 적어 보세요.\n\n## 인상 깊은 문장\n- \n\n## 내 생각 & 적용할 점\n- [ ] \n".into(),
        },
    ];

    let folder_norm = normalize_rel_path(templates_folder).unwrap_or_default();
    if !folder_norm.is_empty() && vault_root.is_dir() {
        if let Ok(tpl_dir) = resolve_safe_path(vault_root, &folder_norm) {
            if tpl_dir.is_dir() {
                for entry in WalkDir::new(&tpl_dir).max_depth(3).into_iter().flatten() {
                    if !entry.file_type().is_file() {
                        continue;
                    }
                    let p = entry.path();
                    if !p
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("md"))
                    {
                        continue;
                    }
                    let Ok(raw) = fs::read_to_string(p) else {
                        continue;
                    };
                    let substituted = substitute_template_vars(&raw, &today, &now_time);
                    let (fm, body) = parse_frontmatter_and_body(&substituted);
                    let name = p
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "사용자 템플릿".into());
                    let rel = p
                        .strip_prefix(vault_root)
                        .unwrap_or(p)
                        .to_string_lossy()
                        .replace('\\', "/");
                    templates.push(TemplateItemDto {
                        id: format!("vault:{rel}"),
                        name,
                        icon: fm.icon.unwrap_or_else(|| "📄".into()),
                        description: format!("{folder_norm} 폴더의 사용자 정의 템플릿"),
                        body,
                        properties: fm.properties,
                    });
                }
            }
        }
    }

    Ok(templates)
}

// ───────────────────────────── 첨부파일 업로드 / 선택 / 미디어 조회 ─────────────────────────────

fn sanitize_attachment_filename(filename: &str) -> String {
    let base = filename.rsplit(['/', '\\']).next().unwrap_or("image.png");
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() {
        "image.png".into()
    } else {
        trimmed.to_string()
    }
}

pub fn upload_attachment(
    vault_root: &Path,
    filename: &str,
    data_base64: &str,
) -> Result<AttachmentDto> {
    if !vault_root.is_dir() {
        bail!("볼트 폴더가 존재하지 않습니다");
    }
    let b64 = match data_base64.find(";base64,") {
        Some(idx) => &data_base64[idx + 8..],
        None => data_base64.trim(),
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .context("첨부 이미지 데이터를 디코딩할 수 없습니다")?;

    let attach_dir = vault_root.join("attachments");
    fs::create_dir_all(&attach_dir)?;

    let ts = Local::now().format("%Y%m%d-%H%M%S-%3f");
    let safe_name = sanitize_attachment_filename(filename);
    let final_name = format!("{ts}-{safe_name}");
    let dest = attach_dir.join(&final_name);
    fs::write(&dest, bytes)?;

    let rel_path = format!("attachments/{final_name}");
    let media_url = make_media_url(vault_root, &rel_path);
    Ok(AttachmentDto {
        rel_path,
        media_url,
    })
}

fn copy_file_to_attachments(
    vault_root: &Path,
    picked_path: &Path,
    fallback_name: &str,
) -> Result<(String, String, String)> {
    let bytes = fs::read(picked_path).context("선택한 파일을 읽을 수 없습니다")?;
    let orig_name = picked_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| fallback_name.to_string());

    let attach_dir = vault_root.join("attachments");
    fs::create_dir_all(&attach_dir)?;
    let ts = Local::now().format("%Y%m%d-%H%M%S-%3f");
    let safe_name = sanitize_attachment_filename(&orig_name);
    let final_name = format!("{ts}-{safe_name}");
    fs::write(attach_dir.join(&final_name), bytes)?;

    let rel_path = format!("attachments/{final_name}");
    let media_url = make_media_url(vault_root, &rel_path);
    Ok((rel_path, media_url, orig_name))
}

fn is_image_filename(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" | "avif"
            )
        })
}

pub fn save_picked_image_attachment(
    vault_root: &Path,
    picked_path: Option<PathBuf>,
) -> Result<PickedAttachmentDto> {
    if !vault_root.is_dir() {
        bail!("볼트 폴더가 존재하지 않습니다");
    }
    let Some(path) = picked_path else {
        return Ok(PickedAttachmentDto {
            picked: false,
            rel_path: None,
            media_url: None,
            filename: None,
        });
    };
    let (rel_path, media_url, orig_name) =
        copy_file_to_attachments(vault_root, &path, "image.png")?;
    Ok(PickedAttachmentDto {
        picked: true,
        rel_path: Some(rel_path),
        media_url: Some(media_url),
        filename: Some(orig_name),
    })
}

pub fn pick_attachment(vault_root: &Path) -> Result<PickedAttachmentDto> {
    if !vault_root.is_dir() {
        bail!("볼트 폴더가 존재하지 않습니다");
    }
    let picked = platform::pick_image_file();
    save_picked_image_attachment(vault_root, picked)
}

pub fn save_picked_file_attachment(
    vault_root: &Path,
    picked_path: Option<PathBuf>,
) -> Result<PickedFileAttachmentDto> {
    if !vault_root.is_dir() {
        bail!("볼트 폴더가 존재하지 않습니다");
    }
    let Some(path) = picked_path else {
        return Ok(PickedFileAttachmentDto {
            picked: false,
            rel_path: None,
            media_url: None,
            file_name: None,
            is_image: false,
        });
    };
    let (rel_path, media_url, orig_name) =
        copy_file_to_attachments(vault_root, &path, "attachment.bin")?;
    let is_image = is_image_filename(&orig_name);
    Ok(PickedFileAttachmentDto {
        picked: true,
        rel_path: Some(rel_path),
        media_url: Some(media_url),
        file_name: Some(orig_name),
        is_image,
    })
}

pub fn pick_file_attachment(vault_root: &Path) -> Result<PickedFileAttachmentDto> {
    if !vault_root.is_dir() {
        bail!("볼트 폴더가 존재하지 않습니다");
    }
    let picked = platform::pick_any_file();
    save_picked_file_attachment(vault_root, picked)
}

// ───────────────────────────── 노트 / 볼트 암호화 워크플로 ─────────────────────────────

fn walk_vault_markdown_files(vault_root: &Path) -> Vec<(String, PathBuf)> {
    let mut files = Vec::new();
    if !vault_root.is_dir() {
        return files;
    }
    let walker = WalkDir::new(vault_root).into_iter().filter_entry(|e| {
        if e.depth() == 0 {
            return true;
        }
        if e.file_type().is_dir() {
            let name = e.file_name().to_string_lossy();
            return !is_ignored_dir_name(&name);
        }
        true
    });
    for entry in walker.flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if !path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        {
            continue;
        }
        let Ok(rel) = path.strip_prefix(vault_root) else {
            continue;
        };
        files.push((rel.to_string_lossy().replace('\\', "/"), path.to_path_buf()));
    }
    files.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    files
}

fn payload_from_plaintext_markdown(raw: &str) -> crypto::EncryptedNotePayload {
    let (fm, body) = parse_frontmatter_and_body(raw);
    payload_from_note_fields(
        fm.icon.as_deref(),
        fm.cover.as_deref(),
        &fm.properties,
        &body,
    )
}

fn plaintext_markdown_from_payload(payload: &crypto::EncryptedNotePayload) -> String {
    let props = properties_from_encrypted_payload(payload);
    serialize_note_content(
        payload.icon.as_deref(),
        payload.cover.as_deref(),
        &props,
        &payload.body,
    )
}

pub fn encrypt_single_note(
    vault_root: &Path,
    rel_path: &str,
    password: &str,
    hint: Option<&str>,
    session: &mut VaultCryptoSession,
) -> Result<NoteDocumentDto> {
    if password.is_empty() {
        bail!("암호화 비밀번호를 입력하세요.");
    }
    let norm = normalize_rel_path(rel_path)?;
    if norm.is_empty() {
        bail!("노트 경로가 비어 있습니다");
    }
    let abs = resolve_safe_path(vault_root, &norm)?;
    if !abs.is_file() {
        bail!("노트 파일을 찾을 수 없습니다: {norm}");
    }
    let raw = fs::read_to_string(&abs)?;
    let clean_hint = hint.map(str::trim).filter(|s| !s.is_empty());

    let payload = if crypto::is_encrypted_markdown(&raw).is_some() {
        if let Some((p, _, _)) = session.try_decrypt_with_session(vault_root, &norm, &raw) {
            p
        } else {
            crypto::decrypt_note_markdown(&raw, password)
                .context("이미 암호화된 노트입니다. 기존 암호를 해제한 뒤 변경하세요")?
        }
    } else {
        payload_from_plaintext_markdown(&raw)
    };

    let armored = crypto::encrypt_note_payload(&payload, password, clean_hint)?;
    crypto::atomic_write_string(&abs, &armored)?;
    session.unlock_note(vault_root, &norm, password, clean_hint.map(str::to_string));
    read_note_with_session(vault_root, &norm, Some(session))
}

pub fn decrypt_single_note(
    vault_root: &Path,
    rel_path: &str,
    password: &str,
    session: &mut VaultCryptoSession,
) -> Result<NoteDocumentDto> {
    let norm = normalize_rel_path(rel_path)?;
    if norm.is_empty() {
        bail!("노트 경로가 비어 있습니다");
    }
    let abs = resolve_safe_path(vault_root, &norm)?;
    if !abs.is_file() {
        bail!("노트 파일을 찾을 수 없습니다: {norm}");
    }
    let raw = fs::read_to_string(&abs)?;
    if crypto::is_encrypted_markdown(&raw).is_none() {
        bail!("암호화된 노트가 아닙니다: {norm}");
    }

    let payload = if !password.is_empty() {
        crypto::decrypt_note_markdown(&raw, password)?
    } else if let Some((p, _, _)) = session.try_decrypt_with_session(vault_root, &norm, &raw) {
        p
    } else {
        bail!("노트 복호화를 위해 비밀번호를 입력하세요.");
    };

    let plain = plaintext_markdown_from_payload(&payload);
    crypto::atomic_write_string(&abs, &plain)?;
    session.remove_note_key(vault_root, &norm);
    read_note_with_session(vault_root, &norm, Some(session))
}

pub fn unlock_single_note(
    vault_root: &Path,
    rel_path: &str,
    password: &str,
    session: &mut VaultCryptoSession,
) -> Result<NoteDocumentDto> {
    if password.is_empty() {
        bail!("비밀번호를 입력하세요.");
    }
    let norm = normalize_rel_path(rel_path)?;
    if norm.is_empty() {
        bail!("노트 경로가 비어 있습니다");
    }
    let abs = resolve_safe_path(vault_root, &norm)?;
    if !abs.is_file() {
        bail!("노트 파일을 찾을 수 없습니다: {norm}");
    }
    let raw = fs::read_to_string(&abs)?;
    let hdr = crypto::is_encrypted_markdown(&raw)
        .ok_or_else(|| anyhow::anyhow!("암호화된 노트가 아닙니다: {norm}"))?;
    let _ = crypto::decrypt_note_markdown(&raw, password)?;
    session.unlock_note(vault_root, &norm, password, hdr.hint);
    read_note_with_session(vault_root, &norm, Some(session))
}

pub fn lock_single_note(
    vault_root: &Path,
    rel_path: &str,
    session: &mut VaultCryptoSession,
) -> Result<NoteDocumentDto> {
    let norm = normalize_rel_path(rel_path)?;
    session.lock_note(vault_root, &norm);
    read_note_with_session(vault_root, &norm, Some(session))
}

pub fn encrypt_entire_vault(
    vault_root: &Path,
    password: &str,
    hint: Option<&str>,
    auto_encrypt: bool,
    session: &mut VaultCryptoSession,
) -> Result<VaultBatchCryptoResult> {
    if !vault_root.is_dir() {
        bail!("볼트 폴더를 찾을 수 없습니다: {}", vault_root.display());
    }
    if password.is_empty() {
        bail!("볼트 암호화 비밀번호를 입력하세요.");
    }
    if let Some(existing) = crypto::load_vault_crypto_config(vault_root) {
        if !crypto::verify_vault_password(&existing, password) {
            bail!("기존 볼트 비밀번호와 일치하지 않습니다.");
        }
    }

    let clean_hint = hint.map(str::trim).filter(|s| !s.is_empty());
    crypto::save_vault_crypto_config(vault_root, password, auto_encrypt, clean_hint)?;
    session.unlock_vault(vault_root, password);

    let files = walk_vault_markdown_files(vault_root);
    let total_notes = files.len();
    let mut encrypted_count = 0usize;
    let mut unlocked_count = 0usize;
    let mut skipped_locked_count = 0usize;

    for (rel, path) in files {
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        if crypto::is_encrypted_markdown(&raw).is_some() {
            if crypto::decrypt_note_markdown(&raw, password).is_ok() {
                unlocked_count += 1;
            } else {
                skipped_locked_count += 1;
            }
            continue;
        }
        let payload = payload_from_plaintext_markdown(&raw);
        let armored = crypto::encrypt_note_payload(&payload, password, clean_hint)?;
        crypto::atomic_write_string(&path, &armored)?;
        session.unlock_note(vault_root, &rel, password, clean_hint.map(str::to_string));
        encrypted_count += 1;
    }

    Ok(VaultBatchCryptoResult {
        encrypted_count,
        decrypted_count: 0,
        unlocked_count,
        skipped_locked_count,
        total_notes,
    })
}

pub fn unlock_entire_vault(
    vault_root: &Path,
    password: &str,
    session: &mut VaultCryptoSession,
) -> Result<VaultBatchCryptoResult> {
    if !vault_root.is_dir() {
        bail!("볼트 폴더를 찾을 수 없습니다: {}", vault_root.display());
    }
    if password.is_empty() {
        bail!("비밀번호를 입력하세요.");
    }
    let cfg = crypto::load_vault_crypto_config(vault_root);
    if let Some(ref c) = cfg {
        if !crypto::verify_vault_password(c, password) {
            bail!("볼트 비밀번호가 올바르지 않습니다.");
        }
    }

    let files = walk_vault_markdown_files(vault_root);
    let total_notes = files.len();
    let mut unlocked_count = 0usize;
    let mut skipped_locked_count = 0usize;

    for (_rel, path) in files {
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        if crypto::is_encrypted_markdown(&raw).is_some() {
            if crypto::decrypt_note_markdown(&raw, password).is_ok() {
                unlocked_count += 1;
            } else {
                skipped_locked_count += 1;
            }
        }
    }

    if cfg.is_none() && unlocked_count == 0 && skipped_locked_count > 0 {
        bail!("입력한 비밀번호로 잠금 해제할 수 있는 노트가 없습니다.");
    }

    session.unlock_vault(vault_root, password);

    Ok(VaultBatchCryptoResult {
        encrypted_count: 0,
        decrypted_count: 0,
        unlocked_count,
        skipped_locked_count,
        total_notes,
    })
}

pub fn lock_entire_vault(vault_root: &Path, session: &mut VaultCryptoSession) {
    session.lock_vault(vault_root);
}

pub fn decrypt_entire_vault(
    vault_root: &Path,
    password: &str,
    session: &mut VaultCryptoSession,
) -> Result<VaultBatchCryptoResult> {
    if !vault_root.is_dir() {
        bail!("볼트 폴더를 찾을 수 없습니다: {}", vault_root.display());
    }
    if password.is_empty() {
        bail!("볼트 복호화 비밀번호를 입력하세요.");
    }
    let cfg = crypto::load_vault_crypto_config(vault_root);
    if let Some(ref c) = cfg {
        if !crypto::verify_vault_password(c, password) {
            bail!("볼트 비밀번호가 올바르지 않습니다.");
        }
    }

    let files = walk_vault_markdown_files(vault_root);
    let total_notes = files.len();
    let mut decrypted_count = 0usize;
    let mut skipped_locked_count = 0usize;

    for (rel, path) in files {
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        if crypto::is_encrypted_markdown(&raw).is_some() {
            match crypto::decrypt_note_markdown(&raw, password) {
                Ok(payload) => {
                    let plain = plaintext_markdown_from_payload(&payload);
                    crypto::atomic_write_string(&path, &plain)?;
                    session.remove_note_key(vault_root, &rel);
                    decrypted_count += 1;
                }
                Err(_) => {
                    skipped_locked_count += 1;
                }
            }
        }
    }

    if cfg.is_none() && decrypted_count == 0 && skipped_locked_count > 0 {
        bail!("입력한 비밀번호와 일치하는 암호화 노트가 없습니다.");
    }

    if skipped_locked_count == 0 {
        crypto::remove_vault_crypto_config(vault_root)?;
        session.lock_vault(vault_root);
    }

    Ok(VaultBatchCryptoResult {
        encrypted_count: 0,
        decrypted_count,
        unlocked_count: 0,
        skipped_locked_count,
        total_notes,
    })
}

pub fn set_vault_auto_encrypt(
    vault_root: &Path,
    enabled: bool,
    session: &VaultCryptoSession,
) -> Result<()> {
    if !vault_root.is_dir() {
        bail!("볼트 폴더를 찾을 수 없습니다: {}", vault_root.display());
    }
    let Some(pw) = session.vault_password(vault_root) else {
        bail!("자동 암호화 설정을 변경하려면 먼저 볼트 잠금을 해제하세요.");
    };
    let existing_hint = crypto::load_vault_crypto_config(vault_root).and_then(|c| c.hint);
    crypto::save_vault_crypto_config(vault_root, &pw, enabled, existing_hint.as_deref())?;
    Ok(())
}

pub fn resolve_vault_media_file(vault_root: &Path, rel_or_name: &str) -> Option<PathBuf> {
    if !vault_root.is_dir() {
        return None;
    }
    let norm = normalize_rel_path(rel_or_name).ok()?;
    if norm.is_empty() {
        return None;
    }
    if let Ok(direct) = resolve_safe_path(vault_root, &norm) {
        if direct.is_file() {
            return Some(direct);
        }
    }
    // Obsidian 최단 경로 방식: 슬래시가 없으면 볼트 하위 폴더에서 파일 이름으로 검색
    if !norm.contains('/') {
        let attach_candidate = vault_root.join("attachments").join(&norm);
        if attach_candidate.is_file() {
            return Some(attach_candidate);
        }
        let target_lower = norm.to_lowercase();
        let walker = WalkDir::new(vault_root).into_iter().filter_entry(|e| {
            if e.depth() == 0 {
                return true;
            }
            if e.file_type().is_dir() {
                return !is_ignored_dir_name(&e.file_name().to_string_lossy());
            }
            true
        });
        for entry in walker.flatten() {
            if entry.file_type().is_file()
                && entry.file_name().to_string_lossy().to_lowercase() == target_lower
            {
                return Some(entry.into_path());
            }
        }
    }
    None
}

// ───────────────────────────── 샘플 볼트 생성 ─────────────────────────────

pub fn create_sample_vault(base_dir: &Path) -> Result<PathBuf> {
    #[cfg(not(target_os = "android"))]
    let preferred_parent = directories::UserDirs::new()
        .and_then(|u| u.document_dir().map(Path::to_path_buf))
        .unwrap_or_else(|| base_dir.to_path_buf());
    #[cfg(target_os = "android")]
    let preferred_parent = {
        let docs = PathBuf::from("/storage/emulated/0/Documents");
        if docs.is_dir() {
            docs
        } else {
            base_dir.to_path_buf()
        }
    };

    let vault_dir = preferred_parent.join("ObsidianSyncVault");
    let target = if fs::create_dir_all(&vault_dir).is_ok() {
        vault_dir
    } else {
        let fallback = base_dir.join("SampleVault");
        fs::create_dir_all(&fallback)?;
        fallback
    };

    fs::create_dir_all(target.join(".obsidian"))?;
    fs::create_dir_all(target.join("Daily"))?;
    fs::create_dir_all(target.join("Templates"))?;
    fs::create_dir_all(target.join("Projects"))?;
    fs::create_dir_all(target.join("attachments"))?;

    let today = Local::now().format("%Y-%m-%d").to_string();

    let welcome_path = target.join("환영합니다.md");
    if !welcome_path.exists() {
        let welcome = format!(
            "---\nicon: \"✨\"\ntags: [welcome, guide, obsidian-sync]\ndate: {today}\nstatus: 완료\n---\n# Obsidian Sync 스튜디오에 오신 것을 환영합니다!\n\n이 공간은 **Notion의 직관적인 문서 편집 경험(TOAST UI Editor WYSIWYG)**과 **Obsidian의 로컬 마크다운 & 지식 그래프**, 그리고 **Synology NAS · 클라우드 양방향 동기화**를 하나로 결합한 워크스페이스입니다.\n\n> 💡 **바로 체험해 보세요**\n> - 상단 툴바에서 **WYSIWYG / 마크다운 분할 모드**를 자유롭게 전환할 수 있습니다.\n> - 상단 속성 패널에서 **아이콘, 커버 이미지, 태그, 체크박스, 날짜 속성**을 추가해 보세요.\n> - `[[빠른 시작 가이드]]` 또는 `[[Projects/동기화 아키텍처]]` 위키링크를 클릭해 연결된 노트로 이동하고 우측 **백링크 & 그래프 뷰**를 확인해 보세요!\n\n## 핵심 기능 한눈에 보기\n\n| 기능 영역 | 지원 내용 | 호환성 |\n| --- | --- | --- |\n| 블록 & 표 편집 | TOAST UI WYSIWYG, 표(Table), 체크리스트, 코드 하이라이팅 | 순수 `.md` 100% 호환 |\n| 지식 연결 | `[[위키링크]]`, 자동 백링크, 미연결 멘션 감지, 그래프 뷰 | Obsidian 볼트 그대로 사용 |\n| 원격 동기화 | Synology NAS, 로컬/SMB 공유폴더, OneDrive, Google Drive, S3 | Windows ↔ Android 양방향 |\n\n## 오늘부터 시작하기\n- [x] 샘플 볼트 생성하기\n- [ ] 좌측 상단에서 기존 Obsidian 볼트 폴더 열기\n- [ ] 우측 상단 **동기화 설정**에서 NAS 또는 클라우드 연결하기\n"
        );
        fs::write(&welcome_path, welcome)?;
    }

    let guide_path = target.join("빠른 시작 가이드.md");
    if !guide_path.exists() {
        let guide = format!(
            "---\nicon: \"🧭\"\ntags: [guide, tips]\ndate: {today}\n---\n# 빠른 시작 가이드\n\n[[환영합니다]] 문서에서 이어지는 실전 활용 팁입니다.\n\n## 1. 단축키 & 빠른 탐색\n- `Ctrl + P` / `Ctrl + K`: 모든 노트를 제목·태그·본문으로 즉시 검색\n- `Ctrl + N`: 새 노트 만들기\n- `Ctrl + S`: 즉시 저장 (자동 저장도 기본 작동합니다)\n\n## 2. 위키링크와 리네임 리팩토링\n노트 이름을 바꾸면 볼트 안의 모든 `[[빠른 시작 가이드]]` 링크가 자동으로 새 이름으로 업데이트됩니다.\n또한 본문에 환영합니다 처럼 대괄호 없이 제목을 언급해도 **연결 안 된 멘션(Unlinked Mention)**으로 찾아 줍니다.\n\n## 3. 프로젝트 예시\n실제 업무 문서 예시는 [[Projects/동기화 아키텍처]] 노트를 참고하세요.\n"
        );
        fs::write(&guide_path, guide)?;
    }

    let proj_path = target.join("Projects").join("동기화 아키텍처.md");
    if !proj_path.exists() {
        let proj = format!(
            "---\nicon: \"⚡\"\ntags: [project, sync, architecture]\ndate: {today}\n검토완료: true\n---\n# 3-Way 양방향 동기화 아키텍처\n\n[[환영합니다]] 및 [[빠른 시작 가이드]]와 연결된 기술 노트 예시입니다.\n\n## 설계 원칙\n1. **데이터 무결성 최우선**: 양쪽 기기에서 동시에 수정된 경우 덮어쓰지 않고 `(충돌 기기이름 날짜)` 사본으로 안전하게 보존합니다.\n2. **대량 삭제 보호**: 한 번에 많은 파일이 삭제되면 자동으로 보류하고 사용자 확인을 거칩니다.\n3. **휴지통(`.trash`) 보관**: 원격 또는 로컬 삭제 시 즉시 영구 삭제하지 않고 `.trash` 폴더로 이동합니다.\n\n```rust\n// 모든 마크다운 파일은 표준 UTF-8 텍스트로 로컬 디스크에 저장됩니다.\npub struct VaultNote {{\n    pub local_first: bool,\n    pub zero_lock_in: bool,\n}}\n```\n"
        );
        fs::write(&proj_path, proj)?;
    }

    let tpl_path = target.join("Templates").join("아이디어 메모.md");
    if !tpl_path.exists() {
        let tpl = "---\nicon: \"💡\"\ntags: [idea]\ndate: {{date}}\n---\n# 한 줄 아이디어 ({{date}} {{time}})\n\n## 핵심 발상\n- \n\n## 다음 실험 단계\n- [ ] \n";
        fs::write(&tpl_path, tpl)?;
    }

    let _ = open_daily_note(&target, "Daily");
    Ok(target)
}

// ───────────────────────────── 단위 테스트 ─────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_roundtrip_and_pure_markdown() {
        // 1) 속성이 전혀 없으면 --- 블록 없이 순수 마크다운 유지
        let pure = "# 순수 마크다운\n\n내용입니다.\n";
        let saved_pure = serialize_note_content(None, Some("  "), &[], pure);
        assert_eq!(saved_pure, pure);

        // 2) 아이콘, 커버, 다양한 타입의 속성 왕복 검증
        let props = vec![
            NotePropertyDto {
                key: "tags".into(),
                value: "rust, obsidian, 노트".into(),
                prop_type: "tags".into(),
            },
            NotePropertyDto {
                key: "published".into(),
                value: "true".into(),
                prop_type: "checkbox".into(),
            },
            NotePropertyDto {
                key: "due".into(),
                value: "2026-11-15".into(),
                prop_type: "date".into(),
            },
            NotePropertyDto {
                key: "homepage".into(),
                value: "https://obsidian.md".into(),
                prop_type: "url".into(),
            },
            NotePropertyDto {
                key: "rating".into(),
                value: "4.5".into(),
                prop_type: "number".into(),
            },
            NotePropertyDto {
                key: "summary".into(),
                value: "콜론: 포함 문자열 #해시".into(),
                prop_type: "text".into(),
            },
        ];
        let body = "# 제목\n\n본문 #인라인태그 텍스트";
        let serialized =
            serialize_note_content(Some("🚀"), Some("attachments/banner.png"), &props, body);
        assert!(serialized.starts_with("---\n"));

        let (fm, parsed_body) = parse_frontmatter_and_body(&serialized);
        assert_eq!(fm.icon.as_deref(), Some("🚀"));
        assert_eq!(fm.cover.as_deref(), Some("attachments/banner.png"));
        assert_eq!(parsed_body, body);
        assert_eq!(fm.properties, props);

        let all_tags = extract_tags(&fm, &parsed_body);
        assert!(all_tags.contains(&"rust".to_string()));
        assert!(all_tags.contains(&"노트".to_string()));
        assert!(all_tags.contains(&"인라인태그".to_string()));
    }

    #[test]
    fn wikilink_rename_refactoring_and_backlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path();

        create_folder(vault, "", "Projects").unwrap();
        create_note(
            vault,
            "Projects",
            "아키텍처",
            Some("# 설계\n핵심 내용"),
            Some("📐"),
            &[],
        )
        .unwrap();
        create_note(
            vault,
            "",
            "회의록",
            Some("오늘 [[아키텍처|시스템 설계]] 문서와 [[Projects/아키텍처#설계]] 섹션을 검토했습니다."),
            None,
            &[],
        )
        .unwrap();
        create_note(
            vault,
            "",
            "일지",
            Some("내일은 아키텍처 문서를 보완할 예정입니다."),
            None,
            &[],
        )
        .unwrap();

        // 백링크 확인: 회의록(linked=true) + 일지(linked=false)
        let arch_doc = read_note(vault, "Projects/아키텍처.md").unwrap();
        assert_eq!(arch_doc.backlinks.len(), 2);
        assert!(arch_doc
            .backlinks
            .iter()
            .any(|b| b.path == "회의록.md" && b.linked));
        assert!(arch_doc
            .backlinks
            .iter()
            .any(|b| b.path == "일지.md" && !b.linked));

        // 연결 안 된 멘션을 위키링크로 변환
        link_unlinked_mention(vault, "일지.md", "아키텍처").unwrap();
        let journal = read_note(vault, "일지.md").unwrap();
        assert!(journal.body.contains("[[아키텍처]]"));

        // 노트 리네임 시 볼트 전체 위키링크 자동 업데이트 검증
        let res = rename_entry(vault, "Projects/아키텍처.md", "신규 아키텍처").unwrap();
        assert_eq!(res.new_path, "Projects/신규 아키텍처.md");
        assert_eq!(res.updated_links, 3);

        let meeting_after = read_note(vault, "회의록.md").unwrap();
        assert!(meeting_after.body.contains("[[신규 아키텍처|시스템 설계]]"));
        assert!(meeting_after
            .body
            .contains("[[Projects/신규 아키텍처#설계]]"));
    }

    #[test]
    fn search_trash_and_graph_work_end_to_end() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path();

        let props = vec![NotePropertyDto {
            key: "tags".into(),
            value: "roadmap".into(),
            prop_type: "tags".into(),
        }];
        create_folder(vault, "", "Docs").unwrap();
        create_note(
            vault,
            "Docs",
            "로드맵",
            Some("2026년 동기화 엔진 개선 및 [[미작성노트]] 연결 #milestone"),
            Some("🗺️"),
            &props,
        )
        .unwrap();

        // 태그 + 경로 + 본문 검색
        let tag_hits = search_vault(vault, "tag:#roadmap").unwrap();
        assert_eq!(tag_hits.len(), 1);
        assert_eq!(tag_hits[0].path, "Docs/로드맵.md");

        let combined_hits = search_vault(vault, "path:Docs 동기화").unwrap();
        assert_eq!(combined_hits.len(), 1);

        // 그래프 빌드 (노트 + 미작성 ghost 노드 + 태그 노드)
        let graph = build_graph(vault, Some("Docs/로드맵.md"), Some(1), true).unwrap();
        assert!(graph
            .nodes
            .iter()
            .any(|n| n.kind == "note" && n.label == "로드맵"));
        assert!(graph
            .nodes
            .iter()
            .any(|n| n.kind == "ghost" && n.label == "미작성노트"));
        assert!(graph
            .nodes
            .iter()
            .any(|n| n.kind == "tag" && n.label == "#roadmap"));
        assert_eq!(graph.edges.len(), 3);

        // 휴지통 이동 -> 목록 -> 복원 -> 비우기
        delete_to_trash(vault, "Docs/로드맵.md").unwrap();
        assert!(!vault.join("Docs/로드맵.md").exists());
        let trash = list_trash(vault).unwrap();
        assert_eq!(trash.len(), 1);
        assert_eq!(trash[0].display_name, "Docs/로드맵.md");

        let restored = restore_trash(vault, &trash[0].trash_name).unwrap();
        assert_eq!(restored, "Docs/로드맵.md");
        assert!(vault.join("Docs/로드맵.md").exists());

        delete_to_trash(vault, "Docs/로드맵.md").unwrap();
        empty_trash(vault).unwrap();
        assert!(list_trash(vault).unwrap().is_empty());
    }

    #[test]
    fn path_traversal_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(resolve_safe_path(tmp.path(), "../secret.md").is_err());
        assert!(resolve_safe_path(tmp.path(), "a/../../secret.md").is_err());
        assert!(resolve_safe_path(tmp.path(), "C:/Windows/win.ini").is_err());
    }

    #[test]
    fn workspace_state_defaults_clamping_and_daily_folder_resolution() {
        let def = WorkspaceState::default();
        assert!(!def.show_tags);
        assert_eq!(def.page_layout, "a4");
        assert_eq!(def.page_orientation, "portrait");
        assert!((def.margin_top_cm - 2.0).abs() < 1e-6);
        assert!((def.margin_bottom_cm - 1.0).abs() < 1e-6);
        assert!((def.margin_left_cm - 3.0).abs() < 1e-6);
        assert!((def.margin_right_cm - 3.0).abs() < 1e-6);

        let mut ws = WorkspaceState::default();
        ws.merge_json(&serde_json::json!({
            "show_tags": true,
            "margin_top_cm": 9.9,
            "margin_bottom_cm": -1.5,
            "margin_left_cm": 3.5,
            "editor_zoom": 300,
            "daily_folders_by_vault": {
                "C:/VaultA": "일일업무/2026",
                "C:/VaultRoot": "/"
            }
        }));
        assert!(ws.show_tags);
        assert!((ws.margin_top_cm - 6.0).abs() < 1e-6);
        assert!((ws.margin_bottom_cm - 0.0).abs() < 1e-6);
        assert!((ws.margin_left_cm - 3.5).abs() < 1e-6);
        assert_eq!(ws.editor_zoom, 150);
        assert_eq!(ws.resolve_daily_folder("c:\\VaultA"), "일일업무/2026");
        assert_eq!(ws.resolve_daily_folder("C:/VaultRoot"), "");
        assert_eq!(ws.resolve_daily_folder("C:/OtherVault"), "Daily");

        // 데일리 노트가 빈 폴더("")로 설정되면 볼트 루트에 직접 생성됨
        let tmp = tempfile::tempdir().unwrap();
        let (root_rel, root_doc) = open_daily_note_with_session(tmp.path(), "", None).unwrap();
        assert!(!root_rel.contains('/'));
        assert!(tmp.path().join(&root_rel).is_file());
        assert_eq!(root_doc.icon.as_deref(), Some("📅"));

        let (sub_rel, _) = open_daily_note_with_session(tmp.path(), "일일업무", None).unwrap();
        assert!(sub_rel.starts_with("일일업무/"));
        assert!(tmp.path().join(&sub_rel).is_file());
    }

    #[test]
    fn single_note_encryption_lock_save_refusal_unlock_and_decrypt() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path();
        let mut session = VaultCryptoSession::new();

        let props = vec![NotePropertyDto {
            key: "tags".into(),
            value: "secret, finance".into(),
            prop_type: "tags".into(),
        }];
        let (rel, _) = create_note(
            vault,
            "Private",
            "비밀노트",
            Some("# 비밀 계획\n기밀 프로젝트 예산은 5000만원입니다. #internal"),
            Some("🗝️"),
            &props,
        )
        .unwrap();

        // 1) 개별 노트 암호화 -> 세션에는 잠금 해제 상태로 유지
        let enc_doc = encrypt_single_note(
            vault,
            &rel,
            "my-strong-pw!",
            Some("개인 암호 힌트"),
            &mut session,
        )
        .unwrap();
        assert!(enc_doc.encrypted);
        assert!(!enc_doc.locked);
        assert!(enc_doc.body.contains("기밀 프로젝트"));

        // 디스크 상 .md 파일에는 평문/태그가 전혀 노출되지 않음
        let raw_disk = fs::read_to_string(vault.join(&rel)).unwrap();
        assert!(raw_disk.contains("obsync_encrypted: true"));
        assert!(!raw_disk.contains("기밀 프로젝트"));
        assert!(!raw_disk.contains("finance"));

        // 2) 잠금 -> 본문/태그/검색 비노출 + 잠금 중 저장 거부
        let locked_doc = lock_single_note(vault, &rel, &mut session).unwrap();
        assert!(locked_doc.encrypted);
        assert!(locked_doc.locked);
        assert!(locked_doc.body.is_empty());
        assert!(locked_doc.tags.is_empty());

        let locked_search = search_vault_with_session(vault, "기밀", Some(&session)).unwrap();
        assert!(locked_search.is_empty());
        let title_search = search_vault_with_session(vault, "비밀노트", Some(&session)).unwrap();
        assert_eq!(title_search.len(), 1);
        assert!(title_search[0].matches[0].snippet.contains("암호화"));

        assert!(save_note_with_session(
            vault,
            &rel,
            None,
            None,
            &[],
            "덮어쓰기 시도",
            Some(&session)
        )
        .is_err());

        // 3) 잘못된 비밀번호 거부 -> 올바른 비밀번호 잠금 해제 -> 편집 저장 후에도 암호문 유지
        assert!(unlock_single_note(vault, &rel, "wrong-pw", &mut session).is_err());
        let unlocked = unlock_single_note(vault, &rel, "my-strong-pw!", &mut session).unwrap();
        assert!(unlocked.encrypted);
        assert!(!unlocked.locked);
        assert_eq!(unlocked.icon.as_deref(), Some("🗝️"));
        assert!(unlocked.tags.contains(&"finance".to_string()));

        let saved = save_note_with_session(
            vault,
            &rel,
            unlocked.icon.as_deref(),
            None,
            &unlocked.properties,
            "# 비밀 계획\n업데이트된 기밀 본문",
            Some(&session),
        )
        .unwrap();
        assert!(saved.body.contains("업데이트된 기밀 본문"));
        let disk_after_edit = fs::read_to_string(vault.join(&rel)).unwrap();
        assert!(disk_after_edit.contains("obsync_encrypted: true"));
        assert!(!disk_after_edit.contains("업데이트된 기밀 본문"));

        // 4) 영구 복호화 -> 일반 마크다운/프론트매터로 복원
        let dec = decrypt_single_note(vault, &rel, "my-strong-pw!", &mut session).unwrap();
        assert!(!dec.encrypted);
        assert!(!dec.locked);
        let disk_plain = fs::read_to_string(vault.join(&rel)).unwrap();
        assert!(!disk_plain.contains("obsync_encrypted"));
        assert!(disk_plain.contains("업데이트된 기밀 본문"));
    }

    #[test]
    fn whole_vault_encryption_unlock_auto_encrypt_and_decrypt() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path();
        let mut session = VaultCryptoSession::new();

        create_note(vault, "", "노트1", Some("첫 번째 평문"), None, &[]).unwrap();
        create_note(vault, "Sub", "노트2", Some("두 번째 평문"), None, &[]).unwrap();
        // 별도 암호로 미리 잠근 노트는 볼트 암호화 시 그대로 보호(skip)
        create_note(vault, "", "개별잠금", Some("타인 암호"), None, &[]).unwrap();
        encrypt_single_note(vault, "개별잠금.md", "other-key", None, &mut session).unwrap();
        lock_single_note(vault, "개별잠금.md", &mut session).unwrap();

        let enc_res = encrypt_entire_vault(
            vault,
            "vault-master-pw",
            Some("마스터 힌트"),
            true,
            &mut session,
        )
        .unwrap();
        assert_eq!(enc_res.encrypted_count, 2);
        assert_eq!(enc_res.skipped_locked_count, 1);
        assert!(vault.join(".obsidian-sync-crypto.json").is_file());

        // 자동 암호화 ON + 볼트 잠금 해제 상태에서 새 노트 생성 시 즉시 암호화 저장
        let (auto_rel, auto_doc) = create_note_with_session(
            vault,
            "",
            "자동암호화노트",
            Some("자동 보호 내용"),
            None,
            &[],
            Some(&session),
        )
        .unwrap();
        assert!(auto_doc.encrypted);
        assert!(!fs::read_to_string(vault.join(&auto_rel))
            .unwrap()
            .contains("자동 보호 내용"));

        // 볼트 잠금 후 스캔 -> 4개 노트 모두 암호화 상태 & 잠금, auto_encrypt=true일 때 새 노트 생성 차단
        lock_entire_vault(vault, &mut session);
        let scan_locked = scan_vault_with_session(vault, 1, Some(&session)).unwrap();
        assert_eq!(scan_locked.encrypted_notes, 4);
        assert!(!scan_locked.vault_unlocked);
        assert!(scan_locked.vault_auto_encrypt);
        assert!(create_note_with_session(
            vault,
            "",
            "잠금중생성",
            Some("실패해야함"),
            None,
            &[],
            Some(&session)
        )
        .is_err());

        // 잘못된 암호 거부 -> 올바른 볼트 암호 잠금 해제
        assert!(unlock_entire_vault(vault, "bad-pw", &mut session).is_err());
        let unlock_res = unlock_entire_vault(vault, "vault-master-pw", &mut session).unwrap();
        assert_eq!(unlock_res.unlocked_count, 3);
        assert_eq!(unlock_res.skipped_locked_count, 1);

        // 자동 암호화 OFF 변경 후 새 노트는 평문 생성
        set_vault_auto_encrypt(vault, false, &session).unwrap();
        let (plain_rel, plain_doc) = create_note_with_session(
            vault,
            "",
            "평문노트",
            Some("일반 마크다운"),
            None,
            &[],
            Some(&session),
        )
        .unwrap();
        assert!(!plain_doc.encrypted);
        assert!(fs::read_to_string(vault.join(&plain_rel))
            .unwrap()
            .contains("일반 마크다운"));

        // 개별잠금 노트를 개별 해제 후 볼트 전체 복호화 -> 설정 파일까지 완전 제거
        decrypt_single_note(vault, "개별잠금.md", "other-key", &mut session).unwrap();
        let dec_res = decrypt_entire_vault(vault, "vault-master-pw", &mut session).unwrap();
        assert_eq!(dec_res.decrypted_count, 3);
        assert_eq!(dec_res.skipped_locked_count, 0);
        assert!(!vault.join(".obsidian-sync-crypto.json").exists());
        assert!(!fs::read_to_string(vault.join("노트1.md"))
            .unwrap()
            .contains("obsync_encrypted"));
    }

    #[test]
    fn pick_file_attachment_copies_non_image_and_detects_image() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path().join("vault");
        let src_dir = tmp.path().join("src");
        fs::create_dir_all(&vault).unwrap();
        fs::create_dir_all(&src_dir).unwrap();

        let pdf_src = src_dir.join("보고서 2026.pdf");
        fs::write(&pdf_src, b"%PDF-1.4 dummy").unwrap();
        let res_pdf = save_picked_file_attachment(&vault, Some(pdf_src)).unwrap();
        assert!(res_pdf.picked);
        assert!(!res_pdf.is_image);
        assert_eq!(res_pdf.file_name.as_deref(), Some("보고서 2026.pdf"));
        assert!(vault.join(res_pdf.rel_path.unwrap()).is_file());

        let img_src = src_dir.join("diagram.PNG");
        fs::write(&img_src, b"png-bytes").unwrap();
        let res_img = save_picked_file_attachment(&vault, Some(img_src)).unwrap();
        assert!(res_img.picked);
        assert!(res_img.is_image);
        assert!(vault.join(res_img.rel_path.unwrap()).is_file());
    }
}
