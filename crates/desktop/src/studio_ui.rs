//! Onyx Studio — 100% 순수 네이티브 Rust (`eframe`/`egui` + `glow`, Zero `WebView2Loader.dll`)
//! WebView Notion 스튜디오 1:1 동일 디자인·구조 구현:
//! - 좌측 사이드바(`#F7F7F5`, `[O]` 블루 아바타, 일일노트 톱니 설정, 드래그 앤 드롭 폴더 이동, 커스텀 SVG 틴트 폴더, ⊞/⊟ 전체 접기/펼치기, 태그 숨김 토글)
//! - 상단 3단 크롬(탭 바 38px + 브레드크럼/3모드 스위치 44px `[👁️ 뷰어 | 위지윅 모드 | 마크다운 분할]` + 50% 슬림 도킹 도구함 8개 그룹 & 찾기/바꾸기 바)
//! - A4 용지/전체폭 스테이지, 상단 룰러 배지, 좌우 여백 희미한 경계 가이드, 테두리 없는 28px 노션 제목 & 단일 문서 인플레이스 블록 위지윅 편집기
//! - 벡터 플로우차트 빌더/카드, #CBD5E1 테두리 편집 표, 외부 텍스트 일반텍스트+이미지 클린 붙여넣기, A4 인쇄/PDF, 3종 레이아웃 그래프, AES-256 암호화

use crate::api::BackendState;
use crate::flowchart::{self, BuilderModalAction, DiagramCardAction, FlowBuilderState};
use crate::graph_view::{
    self, render_markdown_preview_ex, ForceGraphWidget, GraphLayoutMode, ImageTextureCache,
};
use crate::platform;
use crate::print_pdf::{self, PagePrintConfig};
use crate::sync_controller::SyncSnapshotDto;
use crate::sync_ui::{SyncCenterState, SyncUiAction};
use crate::ui::{self, ThemePalette};
use crate::vault::{
    self, GraphDto, NoteDocumentDto, NotePropertyDto, SearchHitDto, TemplateItemDto, TrashItemDto,
    TreeNodeDto, VaultScanDto, WorkspaceState,
};
use crate::win_clipboard::{self, CleanPasteSegment, ClipboardPasteAction};
use base64::Engine as _;
use chrono::Datelike as _;
use eframe::egui::text::{CCursor, CCursorRange};
use eframe::egui::text_edit::TextEditState;
use eframe::egui::{
    self, Align, Align2, Color32, Id, Key, Layout, Margin, Pos2, Rect, RichText, Rounding, Sense,
    Stroke, Vec2,
};
use std::collections::HashSet;
use std::io::Read as _;
use std::path::Path;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const BODY_EDITOR_ID_STR: &str = "onyx_body_editor";
const BLOCK_EDITOR_ID_STR: &str = "onyx_active_block_editor";

const EMOJI_PRESETS: &[&str] = &[
    "📝", "📄", "📚", "📌", "💡", "🚀", "🎯", "🔥", "✅", "📅", "🗂️", "🧠", "💻", "⚙️", "🔬", "📊",
    "🎨", "🌱", "⭐", "💎", "🧭", "🏆", "💬", "🤝", "📖", "🔖", "🛠️", "🧪", "🌐", "🔒", "📁", "📂",
    "💼", "📎", "🗒️", "🚗",
];

const FOLDER_COLOR_PRESETS: &[(&str, &str)] = &[
    ("기본", ""),
    ("블루", "#2383E2"),
    ("퍼플", "#7C3AED"),
    ("그린", "#0F7B6C"),
    ("에메랄드", "#16A34A"),
    ("시안", "#0891B2"),
    ("인디고", "#4F46E5"),
    ("앰버", "#D97706"),
    ("오렌지", "#EA580C"),
    ("레드", "#E03E3E"),
    ("핑크", "#DB2777"),
    ("슬레이트", "#64748B"),
    ("차콜", "#1E293B"),
];

const TEXT_COLOR_PRESETS: &[(&str, &str)] = &[
    ("기본 검정", "#37352F"),
    ("네이비", "#1E3A8A"),
    ("노션 블루", "#2383E2"),
    ("퍼플", "#7C3AED"),
    ("에메랄드", "#0F7B6C"),
    ("그린", "#15803D"),
    ("앰버", "#D97706"),
    ("오렌지", "#EA580C"),
    ("레드", "#E03E3E"),
    ("핑크", "#DB2777"),
    ("시안", "#0891B2"),
    ("그레이", "#6F6E69"),
];

const HIGHLIGHT_PRESETS: &[(&str, &str)] = &[
    ("노랑 ==형광펜==", "#FEF08A"),
    ("민트 형광펜", "#BBF7D0"),
    ("하늘 형광펜", "#BAE6FD"),
    ("라벤더 형광펜", "#E9D5FF"),
    ("피치 형광펜", "#FED7AA"),
    ("로즈 형광펜", "#FECDD3"),
];

const CALLOUT_TYPES: &[(&str, &str)] = &[
    ("note", "📘 메모 (Note)"),
    ("tip", "💡 팁 (Tip)"),
    ("info", "ℹ️ 정보 (Info)"),
    ("abstract", "📋 요약 (Abstract)"),
    ("todo", "☑️ 할 일 (Todo)"),
    ("success", "✅ 성공 (Success)"),
    ("question", "❓ 질문 (Question)"),
    ("warning", "⚠️ 주의 (Warning)"),
    ("failure", "❌ 실패 (Failure)"),
    ("danger", "🔥 위험 (Danger)"),
    ("bug", "🐞 버그 (Bug)"),
    ("example", "🧪 예시 (Example)"),
];

const SPECIAL_SYMBOLS: &[&str] = &[
    "※", "▶", "●", "○", "◎", "◆", "◇", "◈", "★", "☆", "☐", "☑", "✓", "✗", "→", "←", "↑", "↓", "⇒",
    "⇔", "▲", "▼", "▣", "■", "□", "§", "※", "¶", "℃", "㎢", "™", "©",
];

const MATH_SYMBOLS: &[&str] = &[
    "±", "×", "÷", "≠", "≈", "≤", "≥", "∞", "√", "∑", "∏", "∫", "∂", "∇", "∈", "∉", "⊂", "∪", "∩",
    "∧", "∨", "∀", "∃", "∴", "α", "β", "γ", "δ", "θ", "λ", "π", "Ω",
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
pub enum NoteInteractionMode {
    Read,
    Write,
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
    pub rgb: [u8; 3],
    pub use_custom_color: bool,
}

#[derive(Clone, Debug)]
pub struct MoveEntryModalState {
    pub source_path: String,
    pub display_name: String,
    pub target_folder: String,
    pub is_folder: bool,
}

#[derive(Clone, Debug)]
pub struct DailyFolderModalState {
    pub selected_folder: String,
    pub new_subfolder_name: String,
}

#[derive(Clone, Debug)]
pub struct PageSetupModalState {
    pub page_layout: String,
    pub page_orientation: String,
    pub margin_top_cm: f64,
    pub margin_bottom_cm: f64,
    pub margin_left_cm: f64,
    pub margin_right_cm: f64,
    pub auto_fit_a4: bool,
    pub show_page_guides: bool,
    pub editor_zoom: u32,
    pub print_include_title: bool,
    pub print_include_properties: bool,
}

#[derive(Clone, Debug)]
pub struct LinkInsertModalState {
    pub label: String,
    pub url: String,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
enum DocBlockKind {
    Heading {
        level: u8,
    },
    Diagram {
        code: String,
    },
    CodeBlock {
        lang: String,
        code: String,
    },
    Table {
        rows: Vec<Vec<String>>,
    },
    Callout {
        kind: String,
        title: String,
        body: String,
    },
    Quote {
        text: String,
    },
    Checklist {
        items: Vec<(usize, bool, String)>,
    }, // (absolute line idx, checked, label)
    Details {
        summary: String,
        body: String,
    },
    Columns {
        num_cols: usize,
        cols: Vec<String>,
    },
    Image {
        target: String,
        alt: String,
        width: Option<f32>,
    },
    PageBreak,
    Hr,
    Paragraph,
}

#[derive(Clone, Debug)]
struct DocBlock {
    start_line: usize,
    end_line: usize, // exclusive
    raw: String,
    kind: DocBlockKind,
}

pub struct StudioApp {
    pub backend: Arc<Mutex<BackendState>>,
    pub workspace: WorkspaceState,
    pub scan: Option<VaultScanDto>,
    pub sync_snap: SyncSnapshotDto,
    pub vault_revision: u64,

    pub main_view: ActiveMainView,
    pub note_interaction_mode: NoteInteractionMode,
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
    pub show_raw_yaml: bool,
    pub save_status: SaveStatus,
    pub last_edit_at: Option<Instant>,
    pub unlock_password_input: String,

    // 인플레이스 블록 위지윅 상태
    pub editing_block_idx: Option<usize>,
    pub editing_block_range: (usize, usize),
    pub editing_block_text: String,
    pub request_focus_block: bool,
    pub editing_table_cell: Option<(usize, usize, usize, String)>, // (block_idx, r, c, text)
    pub trailing_edit_text: String,

    // 실행 취소 / 다시 실행 및 찾기/바꾸기
    pub undo_stack: Vec<String>,
    pub redo_stack: Vec<String>,
    pub show_find_bar: bool,
    pub find_query: String,
    pub replace_query: String,

    // 드래그 앤 드롭 이동
    pub dragging_tree_path: Option<String>,
    pub dragging_tree_name: String,

    // 우측 분할 보기 상태
    pub split_doc: Option<NoteDocumentDto>,

    // 이미지 텍스처 캐시
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
    pub move_modal: Option<MoveEntryModalState>,
    pub daily_folder_modal: Option<DailyFolderModalState>,
    pub page_setup_modal: Option<PageSetupModalState>,
    pub link_modal: Option<LinkInsertModalState>,
    pub flow_builder: Option<FlowBuilderState>,
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
    screenshot_out: Option<String>,
    screenshot_frames: u32,
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

fn collect_folder_paths_vec(nodes: &[TreeNodeDto], out: &mut Vec<String>) {
    for n in nodes {
        if n.kind == "folder" {
            out.push(n.path.clone());
            collect_folder_paths_vec(&n.children, out);
        }
    }
}

fn find_tree_node<'a>(nodes: &'a [TreeNodeDto], path: &str) -> Option<&'a TreeNodeDto> {
    for n in nodes {
        if n.path == path {
            return Some(n);
        }
        if let Some(sub) = find_tree_node(&n.children, path) {
            return Some(sub);
        }
    }
    None
}

/// 마크다운 본문을 인플레이스 블록 위지윅 편집이 가능하도록 라인 범위 기반 논리 블록으로 분해합니다.
fn parse_markdown_blocks(md: &str) -> Vec<DocBlock> {
    if md.is_empty() {
        return Vec::new();
    }
    let lines: Vec<&str> = md.split('\n').collect();
    let mut blocks = Vec::new();
    let mut i = 0usize;

    while i < lines.len() {
        let t = lines[i].trim();
        if t.is_empty() {
            i += 1;
            continue;
        }

        // 페이지 나누기
        if t.contains("page-break") || t == "<!-- pagebreak -->" {
            blocks.push(DocBlock {
                start_line: i,
                end_line: i + 1,
                raw: lines[i].to_string(),
                kind: DocBlockKind::PageBreak,
            });
            i += 1;
            continue;
        }

        // 다단 컬럼
        if t.starts_with("<div class=\"doc-cols") {
            let start = i;
            let num_cols = if t.contains("doc-cols-3") { 3 } else { 2 };
            let mut col_blocks = Vec::new();
            let mut cur_col = String::new();
            let mut in_col = false;
            i += 1;
            while i < lines.len() {
                let lt = lines[i].trim();
                if lt.starts_with("<div class=\"doc-col\"") {
                    in_col = true;
                    cur_col.clear();
                } else if lt == "</div>" && in_col {
                    in_col = false;
                    col_blocks.push(cur_col.clone());
                } else if lt == "</div>" && !in_col {
                    i += 1;
                    break;
                } else if in_col {
                    cur_col.push_str(lines[i]);
                    cur_col.push('\n');
                }
                i += 1;
            }
            blocks.push(DocBlock {
                start_line: start,
                end_line: i,
                raw: lines[start..i].join("\n"),
                kind: DocBlockKind::Columns {
                    num_cols,
                    cols: col_blocks,
                },
            });
            continue;
        }

        // 접기/펼치기 <details>
        if t.starts_with("<details") {
            let start = i;
            let mut summary = "세부 내용".to_string();
            let mut body = String::new();
            i += 1;
            while i < lines.len() {
                let lt = lines[i].trim();
                if lt == "</details>" {
                    i += 1;
                    break;
                }
                if let Some(s) = lt.strip_prefix("<summary>") {
                    summary = s.trim_end_matches("</summary>").trim().to_string();
                } else {
                    if !body.is_empty() {
                        body.push('\n');
                    }
                    body.push_str(lines[i]);
                }
                i += 1;
            }
            blocks.push(DocBlock {
                start_line: start,
                end_line: i,
                raw: lines[start..i].join("\n"),
                kind: DocBlockKind::Details { summary, body },
            });
            continue;
        }

        // 펜스 코드 블록 / 플로우차트 다이어그램
        if t.starts_with("```") || t.starts_with("~~~") {
            let start = i;
            let lang = t.trim_start_matches(['`', '~']).trim().to_string();
            i += 1;
            let code_start = i;
            while i < lines.len() {
                let lt = lines[i].trim();
                if lt.starts_with("```") || lt.starts_with("~~~") {
                    break;
                }
                i += 1;
            }
            let code = lines[code_start..i].join("\n");
            if i < lines.len() {
                i += 1;
            }
            let kind = if flowchart::is_diagram_code_block(&lang, &code) {
                DocBlockKind::Diagram { code }
            } else {
                DocBlockKind::CodeBlock { lang, code }
            };
            blocks.push(DocBlock {
                start_line: start,
                end_line: i,
                raw: lines[start..i].join("\n"),
                kind,
            });
            continue;
        }

        // 펜스 없이 시작하는 bare 플로우차트 (`flowchart TB ...`)
        let lower_t = t.to_ascii_lowercase();
        if lower_t.starts_with("flowchart ")
            || lower_t.starts_with("graph ")
            || lower_t.starts_with("sequencediagram")
            || lower_t.starts_with("mindmap")
        {
            let start = i;
            i += 1;
            while i < lines.len() && !lines[i].trim().is_empty() {
                i += 1;
            }
            let code = lines[start..i].join("\n");
            blocks.push(DocBlock {
                start_line: start,
                end_line: i,
                raw: code.clone(),
                kind: DocBlockKind::Diagram { code },
            });
            continue;
        }

        // 단독 이미지
        if let Some((target, alt, width)) = graph_view::parse_standalone_image_line(t) {
            blocks.push(DocBlock {
                start_line: i,
                end_line: i + 1,
                raw: lines[i].to_string(),
                kind: DocBlockKind::Image { target, alt, width },
            });
            i += 1;
            continue;
        }

        // 구분선
        if t == "---" || t == "***" || t == "___" {
            blocks.push(DocBlock {
                start_line: i,
                end_line: i + 1,
                raw: lines[i].to_string(),
                kind: DocBlockKind::Hr,
            });
            i += 1;
            continue;
        }

        // 표 (`| ... |`)
        if t.starts_with('|') && t.ends_with('|') {
            let start = i;
            let mut rows = Vec::new();
            while i < lines.len() {
                let tl = lines[i].trim();
                if tl.starts_with('|') && tl.ends_with('|') {
                    let cells: Vec<String> = tl[1..tl.len() - 1]
                        .split('|')
                        .map(|c| c.trim().to_string())
                        .collect();
                    let is_sep = cells.iter().all(|c| {
                        !c.is_empty() && c.chars().all(|ch| matches!(ch, '-' | ':' | ' '))
                    });
                    if !is_sep {
                        rows.push(cells);
                    }
                    i += 1;
                } else {
                    break;
                }
            }
            if !rows.is_empty() {
                blocks.push(DocBlock {
                    start_line: start,
                    end_line: i,
                    raw: lines[start..i].join("\n"),
                    kind: DocBlockKind::Table { rows },
                });
                continue;
            }
        }

        // 콜아웃 또는 인용문 (`> ...`)
        if t.starts_with('>') {
            let start = i;
            let mut qlines = Vec::new();
            while i < lines.len() {
                if let Some(q) = lines[i].trim().strip_prefix('>') {
                    qlines.push(q.trim_start().to_string());
                    i += 1;
                } else {
                    break;
                }
            }
            let raw = lines[start..i].join("\n");
            let first = qlines.first().map(String::as_str).unwrap_or("");
            if first.starts_with("[!") {
                if let Some(end) = first.find(']') {
                    let kind = first[2..end].to_lowercase();
                    let title = first[end + 1..].trim().to_string();
                    let body = qlines
                        .iter()
                        .skip(1)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("\n");
                    blocks.push(DocBlock {
                        start_line: start,
                        end_line: i,
                        raw,
                        kind: DocBlockKind::Callout { kind, title, body },
                    });
                    continue;
                }
            }
            blocks.push(DocBlock {
                start_line: start,
                end_line: i,
                raw,
                kind: DocBlockKind::Quote {
                    text: qlines.join("\n"),
                },
            });
            continue;
        }

        // 제목 (`# `..`###### `)
        let hashes = t.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&hashes) && t[hashes..].starts_with(' ') {
            blocks.push(DocBlock {
                start_line: i,
                end_line: i + 1,
                raw: lines[i].to_string(),
                kind: DocBlockKind::Heading {
                    level: hashes as u8,
                },
            });
            i += 1;
            continue;
        }

        // 체크리스트 그룹 (`- [ ]` / `- [x]`)
        if t.starts_with("- [ ] ")
            || t.starts_with("- [x] ")
            || t.starts_with("- [X] ")
            || t.starts_with("* [ ] ")
            || t.starts_with("* [x] ")
        {
            let start = i;
            let mut items = Vec::new();
            while i < lines.len() {
                let lt = lines[i].trim();
                if let Some(r) = lt
                    .strip_prefix("- [ ] ")
                    .or_else(|| lt.strip_prefix("* [ ] "))
                {
                    items.push((i, false, r.to_string()));
                    i += 1;
                } else if let Some(r) = lt
                    .strip_prefix("- [x] ")
                    .or_else(|| lt.strip_prefix("- [X] "))
                    .or_else(|| lt.strip_prefix("* [x] "))
                {
                    items.push((i, true, r.to_string()));
                    i += 1;
                } else {
                    break;
                }
            }
            blocks.push(DocBlock {
                start_line: start,
                end_line: i,
                raw: lines[start..i].join("\n"),
                kind: DocBlockKind::Checklist { items },
            });
            continue;
        }

        // 일반 문단 / 리스트 묶음
        let start = i;
        i += 1;
        while i < lines.len() {
            let nt = lines[i].trim();
            if nt.is_empty()
                || nt.starts_with('#')
                || nt.starts_with('>')
                || nt.starts_with("```")
                || nt.starts_with("~~~")
                || (nt.starts_with('|') && nt.ends_with('|'))
                || nt == "---"
                || nt.starts_with("- [ ] ")
                || nt.starts_with("- [x] ")
                || nt.starts_with("<details")
                || nt.starts_with("<div class=\"doc-cols")
                || nt.contains("page-break")
                || graph_view::parse_standalone_image_line(nt).is_some()
            {
                break;
            }
            i += 1;
        }
        blocks.push(DocBlock {
            start_line: start,
            end_line: i,
            raw: lines[start..i].join("\n"),
            kind: DocBlockKind::Paragraph,
        });
    }

    blocks
}

fn serialize_table_rows(rows: &[Vec<String>]) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
    let mut out = Vec::new();
    let format_row = |r: &[String]| -> String {
        let cells: Vec<String> = (0..cols)
            .map(|c| {
                r.get(c)
                    .map(|s| s.replace('|', "\\|").replace('\n', " "))
                    .unwrap_or_default()
            })
            .collect();
        format!("| {} |", cells.join(" | "))
    };
    out.push(format_row(&rows[0]));
    let sep = (0..cols).map(|_| "---").collect::<Vec<_>>().join(" | ");
    out.push(format!("| {sep} |"));
    for r in rows.iter().skip(1) {
        out.push(format_row(r));
    }
    out.join("\n")
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
            note_interaction_mode: NoteInteractionMode::Write,
            sidebar_collapsed: false,
            inspector_open: false,
            inspector_tab: RightInspectorTab::Backlinks,
            // 시작 시 모든 폴더는 깔끔하게 접힌 상태로 시작
            expanded_folders: HashSet::new(),
            current_doc: None,
            draft_title: String::new(),
            draft_body: String::new(),
            draft_icon: None,
            draft_cover: None,
            draft_properties: Vec::new(),
            show_raw_yaml: false,
            save_status: SaveStatus::Saved,
            last_edit_at: None,
            unlock_password_input: String::new(),
            editing_block_idx: None,
            editing_block_range: (0, 0),
            editing_block_text: String::new(),
            request_focus_block: false,
            editing_table_cell: None,
            trailing_edit_text: String::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            show_find_bar: false,
            find_query: String::new(),
            replace_query: String::new(),
            dragging_tree_path: None,
            dragging_tree_name: String::new(),
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
            move_modal: None,
            daily_folder_modal: None,
            page_setup_modal: None,
            link_modal: None,
            flow_builder: None,
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
            screenshot_out: std::env::var("ONYX_SCREENSHOT_OUT")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            screenshot_frames: 0,
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

    fn get_active_daily_folder(&self) -> String {
        if let Some(vp) = &self.workspace.active_vault {
            if let Some(df) = self.workspace.daily_folders_by_vault.get(vp) {
                return df.clone();
            }
        }
        self.workspace.daily_folder.clone()
    }

    fn page_print_config(&self) -> PagePrintConfig {
        PagePrintConfig {
            landscape: self.workspace.page_orientation == "landscape",
            margin_top_cm: self.workspace.margin_top_cm,
            margin_bottom_cm: self.workspace.margin_bottom_cm,
            margin_left_cm: self.workspace.margin_left_cm,
            margin_right_cm: self.workspace.margin_right_cm,
            include_title: self.workspace.print_include_title,
            include_properties: self.workspace.print_include_properties,
        }
    }

    fn push_undo_snapshot(&mut self) {
        if self
            .undo_stack
            .last()
            .is_some_and(|s| s == &self.draft_body)
        {
            return;
        }
        self.undo_stack.push(self.draft_body.clone());
        if self.undo_stack.len() > 64 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    fn perform_undo(&mut self) {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(self.draft_body.clone());
            self.draft_body = prev;
            self.editing_block_idx = None;
            self.editing_table_cell = None;
            self.mark_dirty();
        }
    }

    fn perform_redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(self.draft_body.clone());
            self.draft_body = next;
            self.editing_block_idx = None;
            self.editing_table_cell = None;
            self.mark_dirty();
        }
    }

    fn sync_active_block_to_draft(&mut self) {
        if self.editing_block_idx.is_none() {
            return;
        }
        let mut lines: Vec<String> = if self.draft_body.is_empty() {
            Vec::new()
        } else {
            self.draft_body.split('\n').map(str::to_string).collect()
        };
        let (s, e) = self.editing_block_range;
        let start = s.min(lines.len());
        let end = e.min(lines.len()).max(start);
        let replacement: Vec<String> = self
            .editing_block_text
            .split('\n')
            .map(str::to_string)
            .collect();
        let new_len = replacement.len();
        lines.splice(start..end, replacement);
        self.draft_body = lines.join("\n");
        self.editing_block_range = (start, start + new_len);
        self.mark_dirty();
    }

    fn replace_line_range_in_draft(&mut self, start_line: usize, end_line: usize, new_text: &str) {
        self.push_undo_snapshot();
        let mut lines: Vec<String> = if self.draft_body.is_empty() {
            Vec::new()
        } else {
            self.draft_body.split('\n').map(str::to_string).collect()
        };
        let s = start_line.min(lines.len());
        let e = end_line.min(lines.len()).max(s);
        let replacement: Vec<String> = new_text.split('\n').map(str::to_string).collect();
        lines.splice(s..e, replacement);
        self.draft_body = lines.join("\n");
        self.editing_block_idx = None;
        self.editing_table_cell = None;
        self.mark_dirty();
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
        self.editing_block_idx = None;
        self.editing_table_cell = None;
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
        self.editing_block_idx = None;
        self.editing_table_cell = None;
        self.trailing_edit_text.clear();
        self.undo_stack.clear();
        self.redo_stack.clear();
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

    fn execute_move_entry(&mut self, src_path: &str, target_folder: &str) {
        let Some(vp) = self.workspace.active_vault.clone() else {
            return;
        };
        if src_path == target_folder || target_folder.starts_with(&format!("{src_path}/")) {
            self.show_toast("폴더를 자기 자신이나 하위 폴더로 이동할 수 없습니다.", true);
            return;
        }
        if parent_folder_of(src_path) == target_folder {
            return;
        }
        self.flush_save(false);
        let res = {
            let mut st = self.backend.lock().unwrap();
            vault::move_entry_with_session(
                Path::new(&vp),
                src_path,
                target_folder,
                Some(&mut st.crypto),
            )
        };
        match res {
            Ok(new_rel) => {
                let remap = |p: &str| -> String {
                    if p == src_path {
                        new_rel.clone()
                    } else if let Some(rest) = p.strip_prefix(&format!("{src_path}/")) {
                        format!("{new_rel}/{rest}")
                    } else {
                        p.to_string()
                    }
                };
                self.expanded_folders = self.expanded_folders.iter().map(|f| remap(f)).collect();
                if !target_folder.is_empty() {
                    self.expanded_folders.insert(target_folder.to_string());
                }
                self.workspace.open_tabs =
                    self.workspace.open_tabs.iter().map(|p| remap(p)).collect();
                self.workspace.favorites =
                    self.workspace.favorites.iter().map(|p| remap(p)).collect();
                if let Some(ref at) = self.workspace.active_tab {
                    self.workspace.active_tab = Some(remap(at));
                }
                if let Some(ref sp) = self.workspace.split_tab {
                    self.workspace.split_tab = Some(remap(sp));
                }
                if let Some(ref mut cur) = self.current_doc {
                    cur.path = remap(&cur.path);
                }
                self.persist_workspace();
                self.rescan_active_vault();
                self.show_toast(format!("이동 완료: {new_rel}"), false);
            }
            Err(e) => self.show_toast(format!("이동 실패: {e:#}"), true),
        }
    }

    fn execute_print_note(&mut self) {
        let Some(doc) = self.current_doc.clone() else {
            self.show_toast("인쇄할 노트를 먼저 여세요.", true);
            return;
        };
        if doc.locked {
            self.show_toast("잠긴 노트는 먼저 잠금 해제 후 인쇄할 수 있습니다.", true);
            return;
        }
        self.flush_save(false);
        let cfg = self.page_print_config();
        let vp_opt = self.workspace.active_vault.clone();
        match print_pdf::print_note_a4(
            &self.draft_title,
            self.draft_icon.as_deref(),
            &self.draft_properties,
            &self.draft_body,
            vp_opt.as_deref().map(Path::new),
            &cfg,
        ) {
            Ok(()) => self.show_toast("A4 인쇄 미리보기를 열었습니다.", false),
            Err(e) => self.show_toast(format!("인쇄 실행 실패: {e:#}"), true),
        }
    }

    fn execute_export_pdf(&mut self) {
        let Some(doc) = self.current_doc.clone() else {
            self.show_toast("PDF로 내보낼 노트를 먼저 여세요.", true);
            return;
        };
        if doc.locked {
            self.show_toast(
                "잠긴 노트는 먼저 잠금 해제 후 PDF로 내보낼 수 있습니다.",
                true,
            );
            return;
        }
        self.flush_save(false);
        let safe_name = format!(
            "{}.pdf",
            self.draft_title
                .replace(['\\', '/', ':', '*', '?', '"', '<', '>', '|'], "_")
        );
        let cfg = self.page_print_config();
        let vp_opt = self.workspace.active_vault.clone();
        match print_pdf::export_note_pdf(
            &safe_name,
            &self.draft_title,
            self.draft_icon.as_deref(),
            &self.draft_properties,
            &self.draft_body,
            vp_opt.as_deref().map(Path::new),
            &cfg,
        ) {
            Ok(Some(dest)) => {
                self.show_toast(format!("PDF 저장 완료: {}", dest.display()), false);
                let _ = open::that(&dest);
            }
            Ok(None) => {}
            Err(e) => self.show_toast(format!("PDF 저장 실패: {e:#}"), true),
        }
    }

    fn open_page_setup_modal(&mut self) {
        self.page_setup_modal = Some(PageSetupModalState {
            page_layout: self.workspace.page_layout.clone(),
            page_orientation: self.workspace.page_orientation.clone(),
            margin_top_cm: self.workspace.margin_top_cm,
            margin_bottom_cm: self.workspace.margin_bottom_cm,
            margin_left_cm: self.workspace.margin_left_cm,
            margin_right_cm: self.workspace.margin_right_cm,
            auto_fit_a4: self.workspace.auto_fit_a4,
            show_page_guides: self.workspace.show_page_guides,
            editor_zoom: self.workspace.editor_zoom,
            print_include_title: self.workspace.print_include_title,
            print_include_properties: self.workspace.print_include_properties,
        });
    }

    /// 현재 활성화된 편집 대상(블록 인플레이스 편집기 또는 마크다운 분할 편집기)에 스니펫을 삽입하거나 선택 텍스트를 감쌉니다.
    fn insert_or_wrap_in_editor(
        &mut self,
        ctx: &egui::Context,
        prefix: &str,
        suffix: &str,
        placeholder: &str,
    ) {
        self.push_undo_snapshot();
        if self.note_interaction_mode == NoteInteractionMode::Write
            && self.workspace.editor_mode != "markdown"
            && self.editing_block_idx.is_some()
        {
            let block_id = Id::new(BLOCK_EDITOR_ID_STR);
            if let Some(mut state) = TextEditState::load(ctx, block_id) {
                if let Some(range) = state.cursor.char_range() {
                    let s_c = range.primary.index.min(range.secondary.index);
                    let e_c = range.primary.index.max(range.secondary.index);
                    let s_b = char_idx_to_byte_idx(&self.editing_block_text, s_c);
                    let e_b = char_idx_to_byte_idx(&self.editing_block_text, e_c);
                    let selected = self.editing_block_text[s_b..e_b].to_string();
                    let middle = if selected.is_empty() {
                        placeholder
                    } else {
                        selected.as_str()
                    };
                    let replacement = format!("{prefix}{middle}{suffix}");
                    self.editing_block_text
                        .replace_range(s_b..e_b, &replacement);
                    let new_c = s_c + replacement.chars().count();
                    state
                        .cursor
                        .set_char_range(Some(CCursorRange::one(CCursor::new(new_c))));
                    state.store(ctx, block_id);
                    ctx.memory_mut(|m| m.request_focus(block_id));
                    self.sync_active_block_to_draft();
                    return;
                }
            }
            self.editing_block_text
                .push_str(&format!("{prefix}{placeholder}{suffix}"));
            self.sync_active_block_to_draft();
            return;
        }

        let editor_id = Id::new(BODY_EDITOR_ID_STR);
        if let Some(mut state) = TextEditState::load(ctx, editor_id) {
            if let Some(range) = state.cursor.char_range() {
                let s_c = range.primary.index.min(range.secondary.index);
                let e_c = range.primary.index.max(range.secondary.index);
                let s_b = char_idx_to_byte_idx(&self.draft_body, s_c);
                let e_b = char_idx_to_byte_idx(&self.draft_body, e_c);
                let selected = self.draft_body[s_b..e_b].to_string();
                let middle = if selected.is_empty() {
                    placeholder
                } else {
                    selected.as_str()
                };
                let replacement = format!("{prefix}{middle}{suffix}");
                self.draft_body.replace_range(s_b..e_b, &replacement);
                let new_c = s_c + replacement.chars().count();
                state
                    .cursor
                    .set_char_range(Some(CCursorRange::one(CCursor::new(new_c))));
                state.store(ctx, editor_id);
                ctx.memory_mut(|m| m.request_focus(editor_id));
                self.mark_dirty();
                return;
            }
        }

        if !self.draft_body.is_empty() && !self.draft_body.ends_with('\n') {
            self.draft_body.push('\n');
        }
        self.draft_body
            .push_str(&format!("{prefix}{placeholder}{suffix}"));
        self.mark_dirty();
    }

    fn insert_at_body_cursor(&mut self, ctx: &egui::Context, snippet: &str) {
        if snippet.is_empty() {
            return;
        }
        self.insert_or_wrap_in_editor(ctx, snippet, "", "");
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
        let daily_folder = self.get_active_daily_folder();
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

    fn toggle_show_tags(&mut self) {
        self.workspace.show_tags = !self.workspace.show_tags;
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
            (pal.accent_blue, "동기화 중…".into(), pending)
        } else if self.sync_snap.service_running && st.and_then(|s| s.last_error.as_ref()).is_some()
        {
            (pal.danger, "확인 필요".into(), pending)
        } else if self.sync_snap.service_running {
            (pal.ok, "켜짐".into(), pending)
        } else if self.sync_snap.transitioning {
            (pal.warn, "전환 중…".into(), pending)
        } else {
            (pal.text_muted, "꺼짐".into(), pending)
        }
    }

    fn handle_shortcuts_and_clipboard(&mut self, ctx: &egui::Context) {
        let mut open_palette = false;
        let mut save_now = false;
        let mut print_now = false;
        let mut open_daily = false;
        let mut toggle_graph = false;
        let mut toggle_sidebar = false;
        let mut toggle_find = false;
        let mut new_note_shortcut = false;
        let mut do_undo = false;
        let mut do_redo = false;

        let active_ed_id = if self.note_interaction_mode == NoteInteractionMode::Write
            && self.workspace.editor_mode != "markdown"
            && self.editing_block_idx.is_some()
        {
            Some(Id::new(BLOCK_EDITOR_ID_STR))
        } else if self.main_view == ActiveMainView::Editor
            && self.note_interaction_mode == NoteInteractionMode::Write
        {
            Some(Id::new(BODY_EDITOR_ID_STR))
        } else {
            None
        };

        let body_focused = active_ed_id.is_some_and(|eid| ctx.memory(|m| m.has_focus(eid)))
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
                for ev in &mut i.events {
                    if let egui::Event::Paste(s) = ev {
                        *s = win_clipboard::sanitize_external_plain_text(s);
                    }
                }
            }

            if i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::P)
                || i.consume_key(egui::Modifiers::COMMAND, Key::K)
            {
                open_palette = true;
            }
            if i.consume_key(egui::Modifiers::COMMAND, Key::P) {
                print_now = true;
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
            if i.consume_key(egui::Modifiers::COMMAND, Key::F)
                || i.consume_key(egui::Modifiers::COMMAND, Key::H)
            {
                toggle_find = true;
            }
            if self.main_view == ActiveMainView::Editor
                && self.note_interaction_mode == NoteInteractionMode::Write
                && self.editing_block_idx.is_none()
            {
                if i.consume_key(egui::Modifiers::COMMAND, Key::Z) {
                    do_undo = true;
                }
                if i.consume_key(egui::Modifiers::COMMAND, Key::Y) {
                    do_redo = true;
                }
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
        if print_now {
            self.execute_print_note();
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
        if toggle_find && self.main_view == ActiveMainView::Editor {
            self.show_find_bar = !self.show_find_bar;
        }
        if do_undo {
            self.perform_undo();
        }
        if do_redo {
            self.perform_redo();
        }
    }

    fn poll_background(&mut self) {
        if let Some(ref t) = self.tray {
            t.sync_checked();
        }

        if self.save_status == SaveStatus::Dirty {
            if let Some(t) = self.last_edit_at {
                if t.elapsed() >= Duration::from_millis(900) {
                    self.flush_save(false);
                }
            }
        }

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
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        ThemePalette::for_theme(&self.workspace.theme)
            .bg_main
            .to_normalized_gamma_f32()
    }

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

        // 좌측 사이드바 (`268px`, `#F7F7F5`)
        if !self.sidebar_collapsed {
            egui::SidePanel::left("studio_left_sidebar")
                .resizable(true)
                .default_width(268.0)
                .width_range(240.0..=360.0)
                .frame(
                    egui::Frame::none()
                        .fill(pal.bg_sidebar)
                        .stroke(Stroke::new(1.0_f32, pal.border_subtle))
                        .inner_margin(Margin::same(8.0)),
                )
                .show(ctx, |ui| {
                    let sb_w = ui.available_width().max(180.0);
                    ui.set_max_width(sb_w);
                    self.render_sidebar(ui, &pal);
                });
        }

        // 우측 인스펙터 패널 (`#FBFBFA`)
        if self.main_view == ActiveMainView::Editor && self.inspector_open {
            egui::SidePanel::right("studio_right_inspector")
                .resizable(true)
                .default_width(288.0)
                .width_range(230.0..=400.0)
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

        // 중앙 메인 워크스페이스 (3단 상단 크롬 + 캔버스)
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(pal.bg_main))
            .show(ctx, |ui| {
                self.render_tabs_strip(ui, &pal);
                self.render_topbar(ui, &pal);

                if self.main_view == ActiveMainView::Editor
                    && self.note_interaction_mode == NoteInteractionMode::Write
                    && self.current_doc.as_ref().is_some_and(|d| !d.locked)
                {
                    self.render_docked_toolbar(ui, &pal);
                    if self.show_find_bar {
                        self.render_find_replace_bar(ui, &pal);
                    }
                }

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

        // 드래그 중 플로팅 배지 및 놓기 완료 감지
        if let Some(ref drag_src) = self.dragging_tree_path {
            if let Some(pos) = ctx.pointer_latest_pos() {
                egui::Area::new(Id::new("tree_drag_ghost"))
                    .fixed_pos(pos + egui::vec2(14.0, 12.0))
                    .interactable(false)
                    .show(ctx, |ui| {
                        egui::Frame::none()
                            .fill(pal.accent_blue)
                            .rounding(Rounding::same(6.0))
                            .inner_margin(Margin::symmetric(8.0, 4.0))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "📦 이동: {} (대상 폴더에 놓기)",
                                        self.dragging_tree_name
                                    ))
                                    .size(11.5)
                                    .strong()
                                    .color(Color32::WHITE),
                                );
                            });
                    });
            }
            let _ = drag_src;
            if !ctx.input(|i| i.pointer.any_down()) {
                self.dragging_tree_path = None;
                self.dragging_tree_name.clear();
            }
        }

        self.render_modals(ctx, &pal);
        self.render_toasts(ctx, &pal);

        if self.screenshot_out.is_some() {
            let captured = ctx.input(|i| {
                i.events.iter().find_map(|ev| match ev {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(img) = captured {
                if let Some(out_path) = self.screenshot_out.take() {
                    let _ = image::save_buffer(
                        &out_path,
                        img.as_raw(),
                        img.width() as u32,
                        img.height() as u32,
                        image::ColorType::Rgba8,
                    );
                }
                std::process::exit(0);
            } else {
                self.screenshot_frames = self.screenshot_frames.saturating_add(1);
                if self.screenshot_frames == 8 {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
                }
                ctx.request_repaint();
            }
        }

        ctx.request_repaint_after(Duration::from_millis(500));
    }
}

impl StudioApp {
    // ───────────────────────── Left Sidebar ─────────────────────────
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

        // 상단 볼트 스위처 헤더 (`[O] Obsidian ▾` + `✏️`)
        let sb_w = ui.available_width().max(180.0);
        ui.allocate_ui_with_layout(
            Vec2::new(sb_w, 26.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), Sense::hover());
                ui.painter()
                    .rect_filled(rect, Rounding::same(5.0), pal.accent_blue);
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    &avatar_ch,
                    egui::FontId::proportional(12.0),
                    Color32::WHITE,
                );

                let vaults_list = self.workspace.vaults.clone();
                let switcher_btn = ui
                    .add(
                        egui::Button::new(
                            RichText::new(format!("{vault_name}  ▾"))
                                .strong()
                                .size(13.5)
                                .color(pal.text_primary),
                        )
                        .fill(Color32::TRANSPARENT)
                        .stroke(Stroke::NONE)
                        .rounding(Rounding::same(6.0)),
                    )
                    .on_hover_text("볼트 전환 및 관리");
                switcher_btn.context_menu(|_| {});
                let switcher_pop_id = Id::new("vault_switcher_popup");
                if switcher_btn.clicked() {
                    ui.memory_mut(|m| m.toggle_popup(switcher_pop_id));
                }
                egui::popup_below_widget(
                    ui,
                    switcher_pop_id,
                    &switcher_btn,
                    egui::PopupCloseBehavior::CloseOnClickOutside,
                    |ui| {
                        ui.set_min_width(230.0);
                        for v in &vaults_list {
                            let is_cur = active_vault.as_deref() == Some(v.path.as_str());
                            let label = format!(
                                "{} {}",
                                if v.is_sync_profile { "🔄" } else { "📁" },
                                v.name
                            );
                            if ui.selectable_label(is_cur, label).clicked() && !is_cur {
                                self.select_vault(&v.path);
                                ui.close_menu();
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
                    },
                );

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui::icon_btn(ui, pal, "✏️", "새 노트 작성 (Ctrl+N)", false).clicked()
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
            },
        );

        ui.add_space(4.0);

        let (sync_dot, sync_label, pending_del) = self.sync_status_summary(pal);

        if Self::sidebar_nav_btn(
            ui,
            pal,
            "🔍",
            "빠른 검색 / 명령어",
            Some("Ctrl+K"),
            false,
            sb_w,
        )
        .clicked()
        {
            self.show_command_palette = true;
            self.palette_query.clear();
        }

        // 일일 노트 행 + `Ctrl+D` + `⚙️` 일일 폴더 설정 버튼
        ui.allocate_ui_with_layout(
            Vec2::new(sb_w, 28.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let row_w = (sb_w - 32.0).max(120.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(row_w, 26.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        if Self::sidebar_nav_btn(
                            ui,
                            pal,
                            "📅",
                            "오늘의 일일 노트",
                            Some("Ctrl+D"),
                            false,
                            row_w,
                        )
                        .clicked()
                        {
                            self.open_today_daily_note();
                        }
                    },
                );
                if ui::icon_btn(ui, pal, "⚙️", "일일 문서 저장 폴더 설정", false).clicked()
                {
                    if active_vault.is_none() {
                        self.show_toast("먼저 볼트 폴더를 열어 주세요.", true);
                    } else {
                        self.daily_folder_modal = Some(DailyFolderModalState {
                            selected_folder: self.get_active_daily_folder(),
                            new_subfolder_name: String::new(),
                        });
                    }
                }
            },
        );

        if Self::sidebar_nav_btn(
            ui,
            pal,
            "🕸️",
            "그래프 뷰",
            Some("Ctrl+G"),
            self.main_view == ActiveMainView::Graph,
            sb_w,
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
        let sync_inner_w = (sb_w - 16.0).max(80.0);
        let sync_resp = egui::Frame::none()
            .fill(if sync_active {
                pal.bg_active
            } else {
                Color32::TRANSPARENT
            })
            .rounding(Rounding::same(6.0))
            .inner_margin(Margin::symmetric(8.0, 4.0))
            .show(ui, |ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(sync_inner_w, 20.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.label(RichText::new("🔄").size(13.5));
                        ui.label(
                            RichText::new("동기화 센터")
                                .size(13.0)
                                .color(if sync_active {
                                    pal.accent_blue
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
                                    &format!("보류 {pending_del}"),
                                    pal.warn,
                                    pal.warn_bg,
                                );
                            }
                        });
                    },
                );
            })
            .response
            .interact(Sense::click());
        if sync_resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if sync_resp.clicked() {
            self.flush_save(false);
            self.main_view = ActiveMainView::Sync;
        }

        ui.add_space(4.0);
        ui.separator();

        // 볼트 트리 영역
        let footer_h = 34.0;
        let avail_h = (ui.available_height() - footer_h).max(100.0);
        let tree_max_w = (sb_w - 4.0).max(140.0);
        let row_fit_w = (tree_max_w - 12.0).max(120.0);
        egui::ScrollArea::vertical()
            .id_salt("sidebar_tree_scroll")
            .max_height(avail_h)
            .max_width(tree_max_w)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_max_width(row_fit_w);
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
                    ui.add_space(6.0);
                }

                // 2) 페이지 트리 헤더 (최상위 드롭 타겟 + ⊞/⊟/＋/📁)
                let header_drop = ui.allocate_ui_with_layout(
                    Vec2::new(row_fit_w, 24.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        ui.label(
                            RichText::new(format!("📄 페이지 ({})", scan.total_notes))
                                .size(11.5)
                                .strong()
                                .color(pal.text_muted),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            let mini_btn =
                                |ui: &mut egui::Ui, lbl: &str, tip: &str| -> egui::Response {
                                    ui.add(
                                        egui::Button::new(
                                            RichText::new(lbl).size(12.5).color(pal.text_secondary),
                                        )
                                        .fill(Color32::TRANSPARENT)
                                        .stroke(Stroke::NONE)
                                        .rounding(Rounding::same(4.0))
                                        .min_size(Vec2::new(20.0, 20.0)),
                                    )
                                    .on_hover_text(tip)
                                };
                            if mini_btn(ui, "📁", "새 폴더").clicked() {
                                self.prompt_modal = Some((
                                    PromptKind::NewFolder {
                                        parent: String::new(),
                                    },
                                    "📁 새 폴더 만들기".into(),
                                    String::new(),
                                ));
                            }
                            if mini_btn(ui, "＋", "새 노트").clicked() {
                                self.prompt_modal = Some((
                                    PromptKind::NewNote {
                                        folder: String::new(),
                                    },
                                    "📝 새 노트 만들기".into(),
                                    String::new(),
                                ));
                            }
                            if mini_btn(ui, "⊟", "모든 폴더 접기").clicked() {
                                self.expanded_folders.clear();
                            }
                            if mini_btn(ui, "⊞", "모든 폴더 펼치기").clicked() {
                                collect_all_folder_paths(&scan.tree, &mut self.expanded_folders);
                            }
                        });
                    },
                );

                // 드래그 중 '페이지' 헤더 위에 놓으면 볼트 루트("")로 이동
                if let Some(drag_src) = self.dragging_tree_path.clone() {
                    if header_drop.response.hovered() {
                        ui.painter().rect_stroke(
                            header_drop.response.rect,
                            Rounding::same(5.0),
                            Stroke::new(1.5_f32, pal.accent_blue),
                        );
                        if ui.input(|i| i.pointer.any_released()) {
                            self.dragging_tree_path = None;
                            self.dragging_tree_name.clear();
                            self.execute_move_entry(&drag_src, "");
                        }
                    }
                }

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

                // 3) 태그 클라우드 (기본 숨김 — `workspace.show_tags` 켜진 경우에만 표시)
                if self.workspace.show_tags && !scan.tags.is_empty() {
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("🏷️ 태그 ({})", scan.tags.len()))
                                .size(11.5)
                                .strong()
                                .color(pal.text_muted),
                        );
                        if ui.small_button("숨기기").clicked() {
                            self.toggle_show_tags();
                        }
                    });
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
            });

        ui.separator();

        // 하단 푸터 (`🗑️` `🔒` · 상태점 + `켜짐`)
        ui.allocate_ui_with_layout(
            Vec2::new(sb_w, 24.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                if ui
                    .add_enabled(
                        active_vault.is_some(),
                        egui::Button::new("🗑️")
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::NONE),
                    )
                    .on_hover_text("볼트 휴지통 (.trash)")
                    .clicked()
                {
                    if let Some(ref vp) = active_vault {
                        self.trash_items = vault::list_trash(Path::new(vp)).unwrap_or_default();
                        self.show_trash_modal = true;
                    }
                }
                if ui
                    .add_enabled(
                        active_vault.is_some(),
                        egui::Button::new("🔒")
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::NONE),
                    )
                    .on_hover_text("노트 · 볼트 AES-256 암호화 관리")
                    .clicked()
                {
                    self.show_crypto_modal = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(&sync_label)
                                    .size(12.0)
                                    .color(pal.text_secondary),
                            )
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::NONE),
                        )
                        .clicked()
                    {
                        self.flush_save(false);
                        self.main_view = ActiveMainView::Sync;
                    }
                    ui::status_dot(ui, sync_dot, 8.0);
                });
            },
        );
    }

    fn sidebar_nav_btn(
        ui: &mut egui::Ui,
        pal: &ThemePalette,
        icon: &str,
        label: &str,
        shortcut: Option<&str>,
        active: bool,
        outer_w: f32,
    ) -> egui::Response {
        let inner_w = (outer_w - 16.0).max(80.0);
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
                ui.allocate_ui_with_layout(
                    Vec2::new(inner_w, 18.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.label(RichText::new(icon).size(13.5));
                        ui.label(RichText::new(label).size(13.0).color(if active {
                            pal.accent_blue
                        } else {
                            pal.text_primary
                        }));
                        if let Some(sc) = shortcut {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui::kbd_pill(ui, pal, sc);
                            });
                        }
                    },
                );
            })
            .response
            .interact(Sense::click());
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

        let custom_folder_color = if is_folder {
            node.color.as_deref().and_then(ui::parse_hex_color)
        } else {
            None
        };

        let row_w = ui.available_width();
        let (row_rect, row_resp) =
            ui.allocate_exact_size(Vec2::new(row_w, 25.0), Sense::click_and_drag());
        let is_hovered = row_resp.hovered();

        let is_drag_target = is_folder
            && self
                .dragging_tree_path
                .as_ref()
                .is_some_and(|src| src != &node.path && !node.path.starts_with(&format!("{src}/")));

        if is_active {
            ui.painter()
                .rect_filled(row_rect, Rounding::same(5.0), pal.bg_active);
        } else if is_drag_target && is_hovered {
            ui.painter()
                .rect_filled(row_rect, Rounding::same(5.0), pal.accent_blue_soft);
            ui.painter().rect_stroke(
                row_rect,
                Rounding::same(5.0),
                Stroke::new(1.2_f32, pal.accent_blue),
            );
        } else if is_hovered {
            ui.painter()
                .rect_filled(row_rect, Rounding::same(5.0), pal.bg_hover);
        }

        let indent = depth as f32 * 14.0;
        let chev_rect = Rect::from_min_size(
            Pos2::new(row_rect.left() + 4.0 + indent, row_rect.top() + 5.0),
            Vec2::new(14.0, 14.0),
        );
        if is_folder {
            ui::paint_chevron(ui.painter(), chev_rect, expanded, pal.text_muted);
        }

        let icon_rect = Rect::from_min_size(
            Pos2::new(chev_rect.right() + 2.0, row_rect.top() + 4.0),
            Vec2::new(17.0, 17.0),
        );
        if is_folder {
            if let Some(custom_ic) = node.icon.as_deref() {
                ui.painter().text(
                    icon_rect.center(),
                    Align2::CENTER_CENTER,
                    custom_ic,
                    egui::FontId::proportional(13.5),
                    custom_folder_color.unwrap_or(pal.text_primary),
                );
            } else {
                ui::paint_folder_icon(ui.painter(), icon_rect, custom_folder_color, expanded, pal);
            }
        } else {
            let ic = if node.locked {
                "🔒"
            } else {
                node.icon.as_deref().unwrap_or("📄")
            };
            ui.painter().text(
                icon_rect.center(),
                Align2::CENTER_CENTER,
                ic,
                egui::FontId::proportional(13.0),
                pal.text_secondary,
            );
        }

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
        let fg_color = if is_active {
            pal.text_primary
        } else if let Some(fc) = custom_folder_color {
            fc
        } else {
            pal.text_primary
        };

        let label_pos = Pos2::new(icon_rect.right() + 5.0, row_rect.center().y);
        ui.painter()
            .with_clip_rect(Rect::from_min_max(
                Pos2::new(icon_rect.right() + 3.0, row_rect.top()),
                Pos2::new(row_rect.right() - 4.0, row_rect.bottom()),
            ))
            .text(
                label_pos,
                Align2::LEFT_CENTER,
                format!("{display_name}{lock_suffix}"),
                egui::FontId::proportional(13.0),
                fg_color,
            );

        if is_hovered {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if row_resp.drag_started() {
            self.dragging_tree_path = Some(node.path.clone());
            self.dragging_tree_name = display_name.to_string();
        }
        if is_drag_target && is_hovered && ui.input(|i| i.pointer.any_released()) {
            if let Some(drag_src) = self.dragging_tree_path.take() {
                self.dragging_tree_name.clear();
                self.execute_move_entry(&drag_src, &node.path);
            }
        }
        if row_resp.clicked() {
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

        row_resp.context_menu(|ui| {
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
                ui.separator();
                if ui.button("🎨 아이콘 · 색상 변경…").clicked() {
                    let cur_hex = node.color.clone().unwrap_or_default();
                    let col =
                        ui::parse_hex_color(&cur_hex).unwrap_or(Color32::from_rgb(35, 131, 226));
                    self.folder_style_modal = Some(FolderStyleModalState {
                        folder_path: node.path.clone(),
                        folder_name: node.name.clone(),
                        icon: node.icon.clone().unwrap_or_default(),
                        color: cur_hex.clone(),
                        rgb: [col.r(), col.g(), col.b()],
                        use_custom_color: !cur_hex.is_empty(),
                    });
                    ui.close_menu();
                }
                if (node.icon.is_some() || node.color.is_some())
                    && ui.button("↺ 아이콘 · 색상 초기화").clicked()
                {
                    if let Some(ref vp) = self.workspace.active_vault.clone() {
                        if vault::set_folder_style(Path::new(vp), &node.path, None, None).is_ok() {
                            self.rescan_active_vault();
                            self.show_toast("폴더 아이콘·색상을 기본값으로 되돌렸습니다.", false);
                        }
                    }
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("📦 다른 폴더로 이동…").clicked() {
                    self.move_modal = Some(MoveEntryModalState {
                        source_path: node.path.clone(),
                        display_name: node.name.clone(),
                        target_folder: parent_folder_of(&node.path),
                        is_folder: true,
                    });
                    ui.close_menu();
                }
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
                if ui.button("📦 다른 폴더로 이동…").clicked() {
                    self.move_modal = Some(MoveEntryModalState {
                        source_path: node.path.clone(),
                        display_name: strip_md_ext(&node.name).to_string(),
                        target_folder: parent_folder_of(&node.path),
                        is_folder: false,
                    });
                    ui.close_menu();
                }
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

        if is_folder && expanded {
            for child in &node.children {
                self.render_tree_node(ui, pal, child, depth + 1);
            }
        }
    }

    // ───────────────────────── Top Bar 1: Open Tabs Strip (38px) ─────────────────────────
    fn render_tabs_strip(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        egui::Frame::none()
            .fill(pal.bg_sidebar)
            .stroke(Stroke::new(1.0_f32, pal.border_subtle))
            .inner_margin(Margin {
                left: 8.0,
                right: 8.0,
                top: 4.0,
                bottom: 0.0,
            })
            .show(ui, |ui| {
                ui.set_height(34.0);
                ui.horizontal(|ui| {
                    if ui::icon_btn(ui, pal, "☰", "사이드바 접기/펼치기 (Ctrl+\\)", false).clicked()
                    {
                        self.sidebar_collapsed = !self.sidebar_collapsed;
                    }

                    let avail = (ui.available_width() - 34.0).max(120.0);
                    ui.allocate_ui(Vec2::new(avail, 32.0), |ui| {
                        egui::ScrollArea::horizontal()
                            .id_salt("open_tabs_scroll")
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let tabs = self.workspace.open_tabs.clone();
                                    for tab_path in tabs {
                                        let (icon, title) = {
                                            let meta = self.scan.as_ref().and_then(|s| {
                                                s.notes.iter().find(|n| n.path == tab_path)
                                            });
                                            let ic = meta
                                                .and_then(|m| {
                                                    if m.locked {
                                                        Some("🔒".to_string())
                                                    } else {
                                                        m.icon.clone()
                                                    }
                                                })
                                                .unwrap_or_else(|| "📄".to_string());
                                            let ti = meta.map(|m| m.title.clone()).unwrap_or_else(
                                                || {
                                                    strip_md_ext(
                                                        tab_path
                                                            .rsplit('/')
                                                            .next()
                                                            .unwrap_or(&tab_path),
                                                    )
                                                    .to_string()
                                                },
                                            );
                                            (ic, ti)
                                        };
                                        let active = self.main_view == ActiveMainView::Editor
                                            && self
                                                .current_doc
                                                .as_ref()
                                                .is_some_and(|d| d.path == tab_path);

                                        egui::Frame::none()
                                            .fill(if active {
                                                pal.bg_main
                                            } else {
                                                Color32::TRANSPARENT
                                            })
                                            .stroke(if active {
                                                Stroke::new(1.0_f32, pal.border_subtle)
                                            } else {
                                                Stroke::NONE
                                            })
                                            .rounding(Rounding {
                                                nw: 7.0,
                                                ne: 7.0,
                                                sw: 0.0,
                                                se: 0.0,
                                            })
                                            .inner_margin(Margin::symmetric(10.0, 4.0))
                                            .show(ui, |ui| {
                                                ui.horizontal(|ui| {
                                                    let tbtn = ui.add(
                                                        egui::Button::new(
                                                            RichText::new(format!(
                                                                "{icon} {title}"
                                                            ))
                                                            .size(12.5)
                                                            .color(if active {
                                                                pal.text_primary
                                                            } else {
                                                                pal.text_secondary
                                                            }),
                                                        )
                                                        .fill(Color32::TRANSPARENT)
                                                        .stroke(Stroke::NONE),
                                                    );
                                                    if tbtn.clicked() {
                                                        self.open_note(&tab_path);
                                                    }
                                                    if tbtn.middle_clicked() {
                                                        self.close_tab(&tab_path);
                                                    }
                                                    if ui
                                                        .add(
                                                            egui::Button::new(
                                                                RichText::new("✕")
                                                                    .size(11.0)
                                                                    .color(pal.text_muted),
                                                            )
                                                            .fill(Color32::TRANSPARENT)
                                                            .stroke(Stroke::NONE),
                                                        )
                                                        .clicked()
                                                    {
                                                        self.close_tab(&tab_path);
                                                    }
                                                });
                                            });
                                    }
                                });
                            });
                    });

                    if ui::icon_btn(ui, pal, "＋", "새 노트 만들기 (Ctrl+N)", false).clicked()
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

    // ───────────────────────── Top Bar 2: Breadcrumbs + 3-Mode Pill + Controls (44px) ─────────────────────────
    fn render_topbar(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        let up_status = self.backend.lock().unwrap().updater.status();

        egui::Frame::none()
            .fill(pal.bg_main)
            .stroke(Stroke::new(1.0_f32, pal.border_subtle))
            .inner_margin(Margin::symmetric(14.0, 7.0))
            .show(ui, |ui| {
                ui.set_height(30.0);
                ui.horizontal(|ui| {
                    // Left: Folder-by-folder clickable breadcrumbs (clipped safely before right controls)
                    let reserve_right = if self.main_view == ActiveMainView::Editor {
                        565.0
                    } else {
                        170.0
                    };
                    let left_max_w = (ui.available_width() - reserve_right).max(120.0);
                    ui.allocate_ui(Vec2::new(left_max_w, 28.0), |ui| {
                        ui.set_clip_rect(ui.max_rect());
                        ui.horizontal(|ui| match self.main_view {
                            ActiveMainView::Graph => {
                                ui.label(
                                    RichText::new("🕸️ 전체 볼트 지식 그래프")
                                        .strong()
                                        .size(13.0),
                                );
                            }
                            ActiveMainView::Sync => {
                                ui.label(
                                    RichText::new("🔄 실시간 동기화 센터").strong().size(13.0),
                                );
                            }
                            ActiveMainView::Editor => {
                                if let Some(doc) = self.current_doc.clone() {
                                    let parts: Vec<&str> = doc.path.split('/').collect();
                                    let mut acc = String::new();
                                    for (idx, seg) in parts.iter().enumerate() {
                                        if idx > 0 {
                                            ui.label(
                                                RichText::new("›").size(13.0).color(pal.text_muted),
                                            );
                                        }
                                        if idx + 1 == parts.len() {
                                            let ic = if doc.locked {
                                                "🔒"
                                            } else {
                                                self.draft_icon.as_deref().unwrap_or("📄")
                                            };
                                            ui.label(
                                                RichText::new(format!(
                                                    "{ic} {}",
                                                    strip_md_ext(seg)
                                                ))
                                                .strong()
                                                .size(13.0)
                                                .color(pal.text_primary),
                                            );
                                        } else {
                                            acc = if acc.is_empty() {
                                                (*seg).to_string()
                                            } else {
                                                format!("{acc}/{seg}")
                                            };
                                            let fnode = self
                                                .scan
                                                .as_ref()
                                                .and_then(|s| find_tree_node(&s.tree, &acc));
                                            let fic = fnode
                                                .and_then(|n| n.icon.as_deref())
                                                .unwrap_or("📁");
                                            let fcol = fnode
                                                .and_then(|n| n.color.as_deref())
                                                .and_then(ui::parse_hex_color)
                                                .unwrap_or(pal.text_secondary);
                                            if ui
                                                .add(
                                                    egui::Button::new(
                                                        RichText::new(format!("{fic} {seg}"))
                                                            .size(12.5)
                                                            .color(fcol),
                                                    )
                                                    .fill(Color32::TRANSPARENT)
                                                    .stroke(Stroke::NONE),
                                                )
                                                .clicked()
                                            {
                                                self.sidebar_collapsed = false;
                                                self.expanded_folders.insert(acc.clone());
                                            }
                                        }
                                    }
                                    if doc.encrypted && !doc.locked {
                                        ui::pill_badge(
                                            ui,
                                            "🔓 암호해제",
                                            pal.accent_blue,
                                            pal.accent_blue_soft,
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
                        });
                    });

                    // Right Controls (Right-to-Left layout)
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // 더보기 메뉴 (⋯)
                        let more_btn = ui::icon_btn(ui, pal, "⋯", "더보기 메뉴", false);
                        let more_pop_id = Id::new("topbar_more_popup");
                        if more_btn.clicked() {
                            ui.memory_mut(|m| m.toggle_popup(more_pop_id));
                        }
                        egui::popup_below_widget(
                            ui,
                            more_pop_id,
                            &more_btn,
                            egui::PopupCloseBehavior::CloseOnClickOutside,
                            |ui| {
                                ui.set_min_width(210.0);
                                if ui.button("✨ 템플릿 적용").clicked() {
                                    if let Some(ref vp) = self.workspace.active_vault {
                                        self.loaded_templates = vault::list_templates(
                                            Path::new(vp),
                                            &self.workspace.templates_folder,
                                        )
                                        .unwrap_or_default();
                                        self.show_templates_modal = true;
                                    }
                                    ui.close_menu();
                                }
                                if ui.button("📐 A4 용지 및 여백 설정…").clicked() {
                                    self.open_page_setup_modal();
                                    ui.close_menu();
                                }
                                if ui.button("📅 일일 문서 저장 폴더 설정…").clicked()
                                {
                                    self.daily_folder_modal = Some(DailyFolderModalState {
                                        selected_folder: self.get_active_daily_folder(),
                                        new_subfolder_name: String::new(),
                                    });
                                    ui.close_menu();
                                }
                                ui.separator();
                                if ui.button("🔒 볼트 전체 암호화 관리…").clicked() {
                                    self.show_crypto_modal = true;
                                    ui.close_menu();
                                }
                                if ui.button("🗑️ 볼트 휴지통 (.trash)…").clicked() {
                                    if let Some(ref vp) = self.workspace.active_vault {
                                        self.trash_items =
                                            vault::list_trash(Path::new(vp)).unwrap_or_default();
                                        self.show_trash_modal = true;
                                    }
                                    ui.close_menu();
                                }
                            },
                        );

                        if self.main_view == ActiveMainView::Editor {
                            if ui::icon_btn(
                                ui,
                                pal,
                                "◨",
                                "우측 인스펙터 (백링크 · 목차 · 로컬 그래프)",
                                self.inspector_open,
                            )
                            .clicked()
                            {
                                self.inspector_open = !self.inspector_open;
                            }
                        }

                        let is_dark = self.workspace.theme.eq_ignore_ascii_case("dark");
                        if ui::icon_btn(
                            ui,
                            pal,
                            if is_dark { "☀️" } else { "🌙" },
                            "라이트 / 다크 테마 전환",
                            false,
                        )
                        .clicked()
                        {
                            self.workspace.theme = if is_dark {
                                "light".into()
                            } else {
                                "dark".into()
                            };
                            self.persist_workspace();
                        }

                        if self.main_view == ActiveMainView::Editor {
                            if ui::icon_btn(
                                ui,
                                pal,
                                "🏷️",
                                if self.workspace.show_tags {
                                    "태그 숨기기"
                                } else {
                                    "태그 보기 (사이드바 및 문서 속성)"
                                },
                                self.workspace.show_tags,
                            )
                            .clicked()
                            {
                                self.toggle_show_tags();
                            }
                        }

                        // GitHub 자동 업데이트 준비 배지
                        if up_status.ready_to_install {
                            let tag = up_status.available_tag.as_deref().unwrap_or("최신");
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new(format!("🚀 {tag} 업데이트"))
                                            .size(11.5)
                                            .color(Color32::WHITE)
                                            .strong(),
                                    )
                                    .fill(pal.ok)
                                    .stroke(Stroke::NONE)
                                    .rounding(Rounding::same(99.0)),
                                )
                                .clicked()
                            {
                                self.apply_update_and_restart();
                            }
                        }

                        if self.main_view == ActiveMainView::Editor && self.current_doc.is_some() {
                            if ui::icon_btn(
                                ui,
                                pal,
                                "◫",
                                "우측 분할 보기 토글",
                                self.split_doc.is_some(),
                            )
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

                            let lock_ic = if self.current_doc.as_ref().is_some_and(|d| d.locked) {
                                "🔒"
                            } else if self.current_doc.as_ref().is_some_and(|d| d.encrypted) {
                                "🔓"
                            } else {
                                "🔐"
                            };
                            if ui::icon_btn(
                                ui,
                                pal,
                                lock_ic,
                                "노트 · 볼트 AES-256 암호화/잠금",
                                self.current_doc.as_ref().is_some_and(|d| d.encrypted),
                            )
                            .clicked()
                            {
                                self.show_crypto_modal = true;
                            }

                            if self.current_doc.as_ref().is_some_and(|d| !d.locked) {
                                // `PDF` + `🖨️` + `📐`
                                let pdf_btn = ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new("PDF")
                                                .size(11.0)
                                                .strong()
                                                .color(pal.text_secondary),
                                        )
                                        .fill(Color32::TRANSPARENT)
                                        .stroke(Stroke::NONE)
                                        .rounding(Rounding::same(5.0))
                                        .min_size(Vec2::new(30.0, 26.0)),
                                    )
                                    .on_hover_text("A4 PDF 문서로 내보내기");
                                if pdf_btn.clicked() {
                                    self.execute_export_pdf();
                                }

                                if ui::icon_btn(ui, pal, "🖨️", "A4 인쇄 (Ctrl+P)", false).clicked()
                                {
                                    self.execute_print_note();
                                }

                                if ui::icon_btn(
                                    ui,
                                    pal,
                                    "📐",
                                    "A4 용지 및 인쇄/PDF 여백 설정",
                                    self.workspace.page_layout != "full",
                                )
                                .clicked()
                                {
                                    self.open_page_setup_modal();
                                }

                                // 3-Mode Segmented Pill: [ 👁️ 뷰어 | 위지윅 모드 | 마크다운 분할 ]
                                egui::Frame::none()
                                    .fill(pal.bg_inset)
                                    .stroke(Stroke::new(1.0_f32, pal.border_subtle))
                                    .rounding(Rounding::same(7.0))
                                    .inner_margin(Margin::same(2.0))
                                    .show(ui, |ui| {
                                        ui.with_layout(
                                            Layout::left_to_right(Align::Center),
                                            |ui| {
                                                let mode_read = self.note_interaction_mode
                                                    == NoteInteractionMode::Read;
                                                let mode_ww = self.note_interaction_mode
                                                    == NoteInteractionMode::Write
                                                    && self.workspace.editor_mode != "markdown";
                                                let mode_md = self.note_interaction_mode
                                                    == NoteInteractionMode::Write
                                                    && self.workspace.editor_mode == "markdown";

                                                for (active, label, kind) in [
                                                    (mode_read, "👁️ 뷰어", 0),
                                                    (mode_ww, "위지윅 모드", 1),
                                                    (mode_md, "마크다운 분할", 2),
                                                ] {
                                                    let b = egui::Button::new(
                                                        RichText::new(label)
                                                            .size(11.8)
                                                            .strong()
                                                            .color(if active {
                                                                pal.text_primary
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
                                                    .rounding(Rounding::same(5.0))
                                                    .min_size(Vec2::new(0.0, 22.0));
                                                    if ui.add(b).clicked() {
                                                        match kind {
                                                            0 => {
                                                                self.commit_all_block_edits();
                                                                self.flush_save(false);
                                                                self.note_interaction_mode =
                                                                    NoteInteractionMode::Read;
                                                            }
                                                            1 => {
                                                                self.note_interaction_mode =
                                                                    NoteInteractionMode::Write;
                                                                if self.workspace.editor_mode
                                                                    != "wysiwyg"
                                                                {
                                                                    self.workspace.editor_mode =
                                                                        "wysiwyg".into();
                                                                    self.persist_workspace();
                                                                }
                                                            }
                                                            _ => {
                                                                self.commit_all_block_edits();
                                                                self.note_interaction_mode =
                                                                    NoteInteractionMode::Write;
                                                                if self.workspace.editor_mode
                                                                    != "markdown"
                                                                {
                                                                    self.workspace.editor_mode =
                                                                        "markdown".into();
                                                                    self.persist_workspace();
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            },
                                        );
                                    });
                            }

                            match self.save_status {
                                SaveStatus::Saved => {
                                    ui::pill_badge(ui, "✓ 저장됨", pal.text_muted, pal.bg_inset);
                                }
                                SaveStatus::Dirty => {
                                    if ui
                                        .button(
                                            RichText::new("● 변경됨").size(11.5).color(pal.warn),
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

    fn commit_all_block_edits(&mut self) {
        self.editing_block_idx = None;
        self.editing_table_cell = None;
        self.request_focus_block = false;
    }

    // ───────────────────────── Top Bar 3: Docked 50%-Slim Formatting Toolbar ─────────────────────────
    fn render_docked_toolbar(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        let ctx = ui.ctx().clone();
        egui::Frame::none()
            .fill(pal.bg_main)
            .stroke(Stroke::new(1.0_f32, pal.border_subtle))
            .inner_margin(Margin::symmetric(12.0, 4.0))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(2.0, 3.0);

                    // Group 1: Undo / Redo / Find
                    if ui::slim_tool_btn(ui, pal, "↩", "실행 취소 (Ctrl+Z)", false).clicked() {
                        self.perform_undo();
                    }
                    if ui::slim_tool_btn(ui, pal, "↪", "다시 실행 (Ctrl+Y)", false).clicked() {
                        self.perform_redo();
                    }
                    if ui::slim_tool_btn(
                        ui,
                        pal,
                        "🔍찾기",
                        "본문 찾기 및 바꾸기 (Ctrl+F)",
                        self.show_find_bar,
                    )
                    .clicked()
                    {
                        self.show_find_bar = !self.show_find_bar;
                    }

                    ui::toolbar_divider(ui, pal);

                    // Group 2: Text Color / H / Heading dropdown / Size dropdown
                    let color_btn =
                        ui::slim_tool_btn(ui, pal, "A▾", "글자 색상", false);
                    let color_pop_id = Id::new("tb_text_color_pop");
                    if color_btn.clicked() {
                        ui.memory_mut(|m| m.toggle_popup(color_pop_id));
                    }
                    egui::popup_below_widget(
                        ui,
                        color_pop_id,
                        &color_btn,
                        egui::PopupCloseBehavior::CloseOnClickOutside,
                        |ui| {
                            ui.label(RichText::new("강조 색상 태그 삽입").size(11.5).strong());
                            ui.horizontal_wrapped(|ui| {
                                for (cname, hex) in TEXT_COLOR_PRESETS {
                                    let c = ui::parse_hex_color(hex).unwrap_or(pal.text_primary);
                                    if ui
                                        .button(RichText::new("●").size(15.0).color(c))
                                        .on_hover_text(*cname)
                                        .clicked()
                                    {
                                        self.insert_or_wrap_in_editor(&ctx, "**", "**", "강조 텍스트");
                                        ui.close_menu();
                                    }
                                }
                            });
                        },
                    );

                    if ui::slim_tool_btn(ui, pal, "H", "제목(H2) 삽입", false).clicked() {
                        self.insert_at_body_cursor(&ctx, "\n## ");
                    }
                    let h_btn = ui::slim_tool_btn(ui, pal, "H▾", "H1~H6 제목 크기", false);
                    let h_pop_id = Id::new("tb_heading_pop");
                    if h_btn.clicked() {
                        ui.memory_mut(|m| m.toggle_popup(h_pop_id));
                    }
                    egui::popup_below_widget(
                        ui,
                        h_pop_id,
                        &h_btn,
                        egui::PopupCloseBehavior::CloseOnClickOutside,
                        |ui| {
                            for (lbl, prefix) in [
                                ("H1 대제목", "\n# "),
                                ("H2 중제목", "\n## "),
                                ("H3 소제목", "\n### "),
                                ("H4 단계제목", "\n#### "),
                                ("H5 세부제목", "\n##### "),
                                ("H6 작은제목", "\n###### "),
                            ] {
                                if ui.button(lbl).clicked() {
                                    self.insert_at_body_cursor(&ctx, prefix);
                                    ui.close_menu();
                                }
                            }
                        },
                    );

                    let sz_btn = ui::slim_tool_btn(ui, pal, "크기▾", "제목/본문 단계", false);
                    let sz_pop_id = Id::new("tb_size_pop");
                    if sz_btn.clicked() {
                        ui.memory_mut(|m| m.toggle_popup(sz_pop_id));
                    }
                    egui::popup_below_widget(
                        ui,
                        sz_pop_id,
                        &sz_btn,
                        egui::PopupCloseBehavior::CloseOnClickOutside,
                        |ui| {
                            for (lbl, wrap_p, wrap_s) in [
                                ("큰 제목 (H1)", "\n# ", "\n"),
                                ("부제목 (H2)", "\n## ", "\n"),
                                ("중간 강조 (10pt 기본)", "**", "**"),
                                ("작은 각주 표시", "`", "`"),
                            ] {
                                if ui.button(lbl).clicked() {
                                    self.insert_or_wrap_in_editor(&ctx, wrap_p, wrap_s, "텍스트");
                                    ui.close_menu();
                                }
                            }
                        },
                    );

                    ui::toolbar_divider(ui, pal);

                    // Group 3: Inline marks
                    if ui::slim_tool_btn(ui, pal, "B", "굵게 (**)", false).clicked() {
                        self.insert_or_wrap_in_editor(&ctx, "**", "**", "굵은 글씨");
                    }
                    if ui::slim_tool_btn(ui, pal, "I", "기울임 (*)", false).clicked() {
                        self.insert_or_wrap_in_editor(&ctx, "*", "*", "기울임");
                    }
                    if ui::slim_tool_btn(ui, pal, "U", "밑줄 강조", false).clicked() {
                        self.insert_or_wrap_in_editor(&ctx, "__", "__", "밑줄");
                    }
                    if ui::slim_tool_btn(ui, pal, "S", "취소선 (~~)", false).clicked() {
                        self.insert_or_wrap_in_editor(&ctx, "~~", "~~", "취소선");
                    }
                    if ui::slim_tool_btn(ui, pal, "X²", "위 첨자 기호", false).clicked() {
                        self.insert_at_body_cursor(&ctx, "²");
                    }
                    if ui::slim_tool_btn(ui, pal, "X₂", "아래 첨자 기호", false).clicked() {
                        self.insert_at_body_cursor(&ctx, "₂");
                    }
                    if ui::slim_tool_btn(ui, pal, "⌨", "키보드 단축키 스타일", false).clicked() {
                        self.insert_or_wrap_in_editor(&ctx, "`", "`", "Ctrl+S");
                    }

                    let hl_btn = ui::slim_tool_btn_colored(
                        ui,
                        pal,
                        "형광펜",
                        "형광펜 강조 (==텍스트==)",
                        false,
                        Some(pal.mark_bg),
                    );
                    let hl_pop_id = Id::new("tb_highlight_pop");
                    if hl_btn.clicked() {
                        ui.memory_mut(|m| m.toggle_popup(hl_pop_id));
                    }
                    egui::popup_below_widget(
                        ui,
                        hl_pop_id,
                        &hl_btn,
                        egui::PopupCloseBehavior::CloseOnClickOutside,
                        |ui| {
                            for (lbl, _) in HIGHLIGHT_PRESETS {
                                if ui.button(*lbl).clicked() {
                                    self.insert_or_wrap_in_editor(
                                        &ctx,
                                        "==",
                                        "==",
                                        "중요 강조",
                                    );
                                    ui.close_menu();
                                }
                            }
                        },
                    );

                    if ui::slim_tool_btn(
                        ui,
                        pal,
                        "Tx",
                        "서식 · 외부 HTML 속성 코드 일괄 정리 (이미지는 보존)",
                        false,
                    )
                    .clicked()
                    {
                        self.push_undo_snapshot();
                        let cleaned =
                            win_clipboard::strip_html_wrappers_preserving_images(&self.draft_body);
                        if cleaned != self.draft_body {
                            self.draft_body = cleaned;
                            self.commit_all_block_edits();
                            self.mark_dirty();
                            self.show_toast("외부 HTML 속성 코드를 정리했습니다.", false);
                        } else {
                            self.show_toast("정리할 외부 HTML 태그가 없습니다.", false);
                        }
                    }

                    ui::toolbar_divider(ui, pal);

                    // Group 4: Align / List / Task / Indent
                    if ui::slim_tool_btn(ui, pal, "•", "글머리 기호 목록", false).clicked() {
                        self.insert_at_body_cursor(&ctx, "\n- 항목 내용");
                    }
                    if ui::slim_tool_btn(ui, pal, "1.", "번호 매기기 목록", false).clicked() {
                        self.insert_at_body_cursor(&ctx, "\n1. 첫 번째 항목");
                    }
                    if ui::slim_tool_btn(ui, pal, "☑", "할 일 체크박스", false).clicked() {
                        self.insert_at_body_cursor(&ctx, "\n- [ ] 할 일 항목");
                    }
                    if ui::slim_tool_btn(ui, pal, "⇤", "내어쓰기", false).clicked() {
                        self.insert_at_body_cursor(&ctx, "\n- ");
                    }
                    if ui::slim_tool_btn(ui, pal, "⇥", "들여쓰기", false).clicked() {
                        self.insert_at_body_cursor(&ctx, "  - ");
                    }

                    ui::toolbar_divider(ui, pal);

                    // Group 5: Callout / Toggle / Columns / Quote / HR / PageBreak / TOC
                    let co_btn =
                        ui::slim_tool_btn(ui, pal, "💡콜아웃▾", "12종 콜아웃 블록", false);
                    let co_pop_id = Id::new("tb_callout_pop");
                    if co_btn.clicked() {
                        ui.memory_mut(|m| m.toggle_popup(co_pop_id));
                    }
                    egui::popup_below_widget(
                        ui,
                        co_pop_id,
                        &co_btn,
                        egui::PopupCloseBehavior::CloseOnClickOutside,
                        |ui| {
                            for (kind, lbl) in CALLOUT_TYPES {
                                if ui.button(*lbl).clicked() {
                                    self.insert_at_body_cursor(
                                        &ctx,
                                        &format!("\n> [!{kind}] 메모 제목\n> 핵심 내용을 입력하세요.\n"),
                                    );
                                    ui.close_menu();
                                }
                            }
                        },
                    );

                    if ui::slim_tool_btn(ui, pal, "▸토글", "접기/펼치기 토글 블록", false)
                        .clicked()
                    {
                        self.insert_at_body_cursor(
                            &ctx,
                            "\n<details>\n<summary>클릭하여 세부 내용 펼치기</summary>\n\n여기에 상세 내용을 입력하세요.\n</details>\n",
                        );
                    }

                    let col_btn = ui::slim_tool_btn(ui, pal, "▥다단▾", "2단/3단 다단 컬럼", false);
                    let col_pop_id = Id::new("tb_cols_pop");
                    if col_btn.clicked() {
                        ui.memory_mut(|m| m.toggle_popup(col_pop_id));
                    }
                    egui::popup_below_widget(
                        ui,
                        col_pop_id,
                        &col_btn,
                        egui::PopupCloseBehavior::CloseOnClickOutside,
                        |ui| {
                            if ui.button("▥ 2단 나란히 레이아웃").clicked() {
                                self.insert_at_body_cursor(
                                    &ctx,
                                    "\n<div class=\"doc-cols doc-cols-2\">\n<div class=\"doc-col\">\n\n### 왼쪽 영역\n- 내용\n\n</div>\n<div class=\"doc-col\">\n\n### 오른쪽 영역\n- 내용\n\n</div>\n</div>\n",
                                );
                                ui.close_menu();
                            }
                            if ui.button("▥ 3단 비교 카드 레이아웃").clicked() {
                                self.insert_at_body_cursor(
                                    &ctx,
                                    "\n<div class=\"doc-cols doc-cols-3\">\n<div class=\"doc-col\">\n\n### 열 1\n- \n\n</div>\n<div class=\"doc-col\">\n\n### 열 2\n- \n\n</div>\n<div class=\"doc-col\">\n\n### 열 3\n- \n\n</div>\n</div>\n",
                                );
                                ui.close_menu();
                            }
                        },
                    );

                    if ui::slim_tool_btn(ui, pal, "❝", "인용구", false).clicked() {
                        self.insert_at_body_cursor(&ctx, "\n> 인용할 내용을 입력하세요.\n");
                    }
                    if ui::slim_tool_btn(ui, pal, "―", "구분선", false).clicked() {
                        self.insert_at_body_cursor(&ctx, "\n---\n");
                    }
                    if ui::slim_tool_btn(ui, pal, "✂", "인쇄/PDF 페이지 나누기", false).clicked()
                    {
                        self.insert_at_body_cursor(&ctx, "\n<!-- pagebreak -->\n");
                    }
                    if ui::slim_tool_btn(ui, pal, "📑", "현재 문서 자동 목차 삽입", false)
                        .clicked()
                    {
                        let headings = vault::extract_headings(&self.draft_body);
                        if headings.is_empty() {
                            self.show_toast("제목(H1~H6)이 없습니다.", true);
                        } else {
                            let toc = headings
                                .iter()
                                .map(|h| {
                                    format!(
                                        "{}- [[#{}]]",
                                        "  ".repeat(h.level.saturating_sub(1) as usize),
                                        h.text
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join("\n");
                            self.insert_at_body_cursor(&ctx, &format!("\n### 목차\n{toc}\n"));
                        }
                    }

                    ui::toolbar_divider(ui, pal);

                    // Group 6: Table / Link / WikiLink / Footnote / Image / File Attach
                    let tbl_btn = ui::slim_tool_btn(ui, pal, "📊표▾", "마크다운 표 삽입", false);
                    let tbl_pop_id = Id::new("tb_table_pop");
                    if tbl_btn.clicked() {
                        ui.memory_mut(|m| m.toggle_popup(tbl_pop_id));
                    }
                    egui::popup_below_widget(
                        ui,
                        tbl_pop_id,
                        &tbl_btn,
                        egui::PopupCloseBehavior::CloseOnClickOutside,
                        |ui| {
                            for (lbl, cols, rows) in [
                                ("2 × 2 기본 표", 2usize, 2usize),
                                ("3 × 3 표준 표", 3usize, 3usize),
                                ("4 × 4 상세 표", 4usize, 4usize),
                            ] {
                                if ui.button(lbl).clicked() {
                                    let hdr = (1..=cols)
                                        .map(|c| format!("항목 {c}"))
                                        .collect::<Vec<_>>()
                                        .join(" | ");
                                    let sep =
                                        (0..cols).map(|_| "---").collect::<Vec<_>>().join(" | ");
                                    let body_row =
                                        (0..cols).map(|_| "내용").collect::<Vec<_>>().join(" | ");
                                    let mut lines =
                                        vec![format!("| {hdr} |"), format!("| {sep} |")];
                                    for _ in 0..rows.saturating_sub(1) {
                                        lines.push(format!("| {body_row} |"));
                                    }
                                    self.insert_at_body_cursor(
                                        &ctx,
                                        &format!("\n{}\n", lines.join("\n")),
                                    );
                                    ui.close_menu();
                                }
                            }
                        },
                    );

                    if ui::slim_tool_btn(ui, pal, "🔗", "웹 링크 삽입", false).clicked() {
                        self.link_modal = Some(LinkInsertModalState {
                            label: "링크 이름".into(),
                            url: "https://".into(),
                        });
                    }
                    if ui::slim_tool_btn(ui, pal, "[[위키]]", "노트 위키링크 연결", false).clicked()
                    {
                        self.wikilink_query.clear();
                        self.show_wikilink_modal = true;
                    }
                    if ui::slim_tool_btn(ui, pal, "[^1]", "각주 삽입", false).clicked() {
                        self.insert_at_body_cursor(&ctx, "[^1]\n\n[^1]: 각주 설명을 입력하세요.\n");
                    }
                    if ui::slim_tool_btn(ui, pal, "🖼️", "PC 이미지 첨부", false).clicked() {
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
                    if ui::slim_tool_btn(ui, pal, "📎", "일반 파일 첨부 (PDF 등)", false).clicked()
                    {
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

                    ui::toolbar_divider(ui, pal);

                    // Group 7: Code / Math / Flowchart
                    if ui::slim_tool_btn(ui, pal, "</>", "인라인 코드 (`)", false).clicked() {
                        self.insert_or_wrap_in_editor(&ctx, "`", "`", "code");
                    }
                    if ui::slim_tool_btn(ui, pal, "💻코드", "코드 블록 (```)", false).clicked() {
                        self.insert_at_body_cursor(&ctx, "\n```rust\n// 코드 입력\n```\n");
                    }

                    let math_btn =
                        ui::slim_tool_btn(ui, pal, "∑수식▾", "수식 및 수학 기호", false);
                    let math_pop_id = Id::new("tb_math_pop");
                    if math_btn.clicked() {
                        ui.memory_mut(|m| m.toggle_popup(math_pop_id));
                    }
                    egui::popup_below_widget(
                        ui,
                        math_pop_id,
                        &math_btn,
                        egui::PopupCloseBehavior::CloseOnClickOutside,
                        |ui| {
                            ui.set_min_width(240.0);
                            if ui.button("∑ 블록 수식 ($$ ... $$)").clicked() {
                                self.insert_at_body_cursor(&ctx, "\n$$\nE = mc^2\n$$\n");
                                ui.close_menu();
                            }
                            ui.separator();
                            ui.horizontal_wrapped(|ui| {
                                for s in MATH_SYMBOLS {
                                    if ui.small_button(*s).clicked() {
                                        self.insert_at_body_cursor(&ctx, s);
                                        ui.close_menu();
                                    }
                                }
                            });
                        },
                    );

                    let flow_btn = ui::slim_tool_btn(
                        ui,
                        pal,
                        "◇플로우▾",
                        "고급 플로우차트 빌더 및 다이어그램 삽입",
                        false,
                    );
                    let flow_pop_id = Id::new("tb_flow_pop");
                    if flow_btn.clicked() {
                        ui.memory_mut(|m| m.toggle_popup(flow_pop_id));
                    }
                    egui::popup_below_widget(
                        ui,
                        flow_pop_id,
                        &flow_btn,
                        egui::PopupCloseBehavior::CloseOnClickOutside,
                        |ui| {
                            if ui.button("🛠️ 비주얼 플로우차트 빌더 열기…").clicked() {
                                self.flow_builder =
                                    Some(FlowBuilderState::from_code(None, ""));
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button("⬇ 세로 흐름도 (flowchart TB)").clicked() {
                                self.insert_at_body_cursor(
                                    &ctx,
                                    "\n```flowchart\nflowchart TB\n    A([시작]) --> B{조건 확인}\n    B -->|성공| C[처리 완료]\n    B -->|재시도| D[(대기열)]\n```\n",
                                );
                                ui.close_menu();
                            }
                            if ui.button("➡ 가로 프로세스 (flowchart LR)").clicked() {
                                self.insert_at_body_cursor(
                                    &ctx,
                                    "\n```flowchart\nflowchart LR\n    U([작성]) --> S[암호화] --> N[(NAS 동기화)] --> M[모바일 확인]\n```\n",
                                );
                                ui.close_menu();
                            }
                            if ui.button("⇄ 시퀀스 다이어그램 (sequenceDiagram)").clicked() {
                                self.insert_at_body_cursor(
                                    &ctx,
                                    "\n```mermaid\nsequenceDiagram\n    participant PC as 데스크톱\n    participant NAS as Synology NAS\n    PC->>NAS: 변경 노트 업로드\n    NAS-->>PC: 동기화 완료\n```\n",
                                );
                                ui.close_menu();
                            }
                            if ui.button("🧠 마인드맵 (mindmap)").clicked() {
                                self.insert_at_body_cursor(
                                    &ctx,
                                    "\n```mermaid\nmindmap\n  root((핵심 주제))\n    계획\n    구현\n    검증\n```\n",
                                );
                                ui.close_menu();
                            }
                        },
                    );

                    ui::toolbar_divider(ui, pal);

                    // Group 8: Date / Badges / Symbols / Transforms / Snippets / Copy / Zoom
                    let dt_btn = ui::slim_tool_btn(ui, pal, "📅날짜▾", "날짜/시간 삽입", false);
                    let dt_pop_id = Id::new("tb_date_pop");
                    if dt_btn.clicked() {
                        ui.memory_mut(|m| m.toggle_popup(dt_pop_id));
                    }
                    egui::popup_below_widget(
                        ui,
                        dt_pop_id,
                        &dt_btn,
                        egui::PopupCloseBehavior::CloseOnClickOutside,
                        |ui| {
                            let now = chrono::Local::now();
                            for (lbl, fmt) in [
                                ("📅 YYYY-MM-DD", now.format("%Y-%m-%d").to_string()),
                                (
                                    "🗓️ YYYY년 M월 D일",
                                    format!("{}년 {}월 {}일", now.year(), now.month(), now.day()),
                                ),
                                ("⏰ YYYY-MM-DD HH:MM", now.format("%Y-%m-%d %H:%M").to_string()),
                                ("🕒 현재 시각 HH:MM", now.format("%H:%M").to_string()),
                            ] {
                                if ui.button(lbl).clicked() {
                                    self.insert_at_body_cursor(&ctx, &fmt);
                                    ui.close_menu();
                                }
                            }
                        },
                    );

                    let bdg_btn = ui::slim_tool_btn(ui, pal, "🏷️배지▾", "상태 배지 삽입", false);
                    let bdg_pop_id = Id::new("tb_badge_pop");
                    if bdg_btn.clicked() {
                        ui.memory_mut(|m| m.toggle_popup(bdg_pop_id));
                    }
                    egui::popup_below_widget(
                        ui,
                        bdg_pop_id,
                        &bdg_btn,
                        egui::PopupCloseBehavior::CloseOnClickOutside,
                        |ui| {
                            for b in [
                                "`[🔵 진행중]` ",
                                "`[🟢 완료]` ",
                                "`[🟡 보류]` ",
                                "`[🔴 긴급]` ",
                                "`[████░░ 70%]` ",
                            ] {
                                if ui.button(b).clicked() {
                                    self.insert_at_body_cursor(&ctx, b);
                                    ui.close_menu();
                                }
                            }
                        },
                    );

                    let sym_btn = ui::slim_tool_btn(ui, pal, "※기호▾", "공문서·사무 기호", false);
                    let sym_pop_id = Id::new("tb_sym_pop");
                    if sym_btn.clicked() {
                        ui.memory_mut(|m| m.toggle_popup(sym_pop_id));
                    }
                    egui::popup_below_widget(
                        ui,
                        sym_pop_id,
                        &sym_btn,
                        egui::PopupCloseBehavior::CloseOnClickOutside,
                        |ui| {
                            ui.set_min_width(220.0);
                            ui.horizontal_wrapped(|ui| {
                                for s in SPECIAL_SYMBOLS {
                                    if ui.small_button(*s).clicked() {
                                        self.insert_at_body_cursor(&ctx, s);
                                        ui.close_menu();
                                    }
                                }
                            });
                        },
                    );

                    let tr_btn = ui::slim_tool_btn(ui, pal, "Aa변환▾", "텍스트 변환 도구", false);
                    let tr_pop_id = Id::new("tb_tr_pop");
                    if tr_btn.clicked() {
                        ui.memory_mut(|m| m.toggle_popup(tr_pop_id));
                    }
                    egui::popup_below_widget(
                        ui,
                        tr_pop_id,
                        &tr_btn,
                        egui::PopupCloseBehavior::CloseOnClickOutside,
                        |ui| {
                            if ui.button("✂ 빈줄 · 앞뒤 공백 정리").clicked() {
                                self.push_undo_snapshot();
                                self.draft_body = self
                                    .draft_body
                                    .lines()
                                    .map(|l| l.trim_end())
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                self.commit_all_block_edits();
                                self.mark_dirty();
                                ui.close_menu();
                            }
                            if ui.button("🧹 외부 HTML 속성 태그 일괄 제거").clicked() {
                                self.push_undo_snapshot();
                                self.draft_body =
                                    win_clipboard::strip_html_wrappers_preserving_images(
                                        &self.draft_body,
                                    );
                                self.commit_all_block_edits();
                                self.mark_dirty();
                                ui.close_menu();
                            }
                        },
                    );

                    let tpl_btn =
                        ui::slim_tool_btn(ui, pal, "📋양식▾", "회의록·업무보고 양식", false);
                    let tpl_pop_id = Id::new("tb_snippet_pop");
                    if tpl_btn.clicked() {
                        ui.memory_mut(|m| m.toggle_popup(tpl_pop_id));
                    }
                    egui::popup_below_widget(
                        ui,
                        tpl_pop_id,
                        &tpl_btn,
                        egui::PopupCloseBehavior::CloseOnClickOutside,
                        |ui| {
                            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
                            if ui.button("📝 회의록 양식").clicked() {
                                self.insert_at_body_cursor(
                                    &ctx,
                                    &format!("\n## 회의록 ({today})\n- **참석자**: \n- **안건**: \n\n### 논의 내용\n1. \n\n### 결정 및 실행 과제\n- [ ] 담당자 / 기한\n"),
                                );
                                ui.close_menu();
                            }
                            if ui.button("💼 일일 업무 보고").clicked() {
                                self.insert_at_body_cursor(
                                    &ctx,
                                    &format!("\n## 일일 업무 보고 ({today})\n### 1. 오늘 완료\n- \n\n### 2. 진행 중 / 이슈\n- \n\n### 3. 내일 계획\n- [ ] \n"),
                                );
                                ui.close_menu();
                            }
                            if ui.button("📅 주간 업무 계획 표").clicked() {
                                self.insert_at_body_cursor(
                                    &ctx,
                                    "\n## 주간 업무 계획\n| 요일 | 핵심 목표 | 세부 작업 | 상태 |\n| --- | --- | --- | --- |\n| 월 | | | |\n| 화 | | | |\n| 수 | | | |\n| 목 | | | |\n| 금 | | | |\n",
                                );
                                ui.close_menu();
                            }
                        },
                    );

                    if ui::slim_tool_btn(ui, pal, "📋복사", "마크다운 전체 복사", false).clicked()
                    {
                        ui.output_mut(|o| o.copied_text = self.draft_body.clone());
                        self.show_toast("마크다운 본문 전체를 복사했습니다.", false);
                    }
                    if ui::slim_tool_btn(ui, pal, "−", "화면 축소", false).clicked() {
                        self.workspace.editor_zoom =
                            self.workspace.editor_zoom.saturating_sub(10).max(50);
                        self.persist_workspace();
                    }
                    if ui::slim_tool_btn(ui, pal, "+", "화면 확대", false).clicked() {
                        self.workspace.editor_zoom = (self.workspace.editor_zoom + 10).min(150);
                        self.persist_workspace();
                    }
                });
            });
    }

    fn render_find_replace_bar(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        egui::Frame::none()
            .fill(pal.bg_secondary)
            .stroke(Stroke::new(1.0_f32, pal.border_subtle))
            .inner_margin(Margin::symmetric(14.0, 5.0))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("🔍 찾기").size(12.0).strong());
                    ui.add(
                        egui::TextEdit::singleline(&mut self.find_query)
                            .hint_text("검색할 단어")
                            .desired_width(160.0),
                    );
                    let cnt = if self.find_query.is_empty() {
                        0
                    } else {
                        self.draft_body
                            .to_lowercase()
                            .matches(&self.find_query.to_lowercase())
                            .count()
                    };
                    ui.label(
                        RichText::new(format!("{cnt}건 일치"))
                            .size(11.5)
                            .color(pal.text_muted),
                    );
                    ui.separator();
                    ui.label(RichText::new("바꾸기").size(12.0));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.replace_query)
                            .hint_text("바꿀 내용")
                            .desired_width(150.0),
                    );
                    if ui
                        .add_enabled(
                            !self.find_query.is_empty(),
                            egui::Button::new("모두 바꾸기"),
                        )
                        .clicked()
                    {
                        self.push_undo_snapshot();
                        let next = self
                            .draft_body
                            .replace(&self.find_query, &self.replace_query);
                        if next != self.draft_body {
                            self.draft_body = next;
                            self.commit_all_block_edits();
                            self.mark_dirty();
                            self.show_toast(format!("{cnt}건을 바꾸었습니다."), false);
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("✕").clicked() {
                            self.show_find_bar = false;
                        }
                    });
                });
            });
    }

    // ───────────────────────── Main Editor Workspace ─────────────────────────
    fn render_editor_workspace(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        if self.workspace.active_vault.is_none() {
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(80.0);
                    ui.label(RichText::new("📓").size(48.0));
                    ui.label(
                        RichText::new("Onyx Studio에 오신 것을 환영합니다")
                            .size(22.0)
                            .strong(),
                    );
                    ui.label(
                        RichText::new(
                            "노션처럼 직관적인 위지윅 편집기, 마크다운 실시간 분할 프리뷰, 위키링크 & 그래프 뷰,\n그리고 6종 클라우드/NAS 실시간 동기화를 한 곳에서 사용하세요.",
                        )
                        .size(13.5)
                        .color(pal.text_secondary),
                    );
                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        if ui::primary_button(ui, pal, "✨ 샘플 볼트 시작하기").clicked() {
                            self.handle_create_sample_vault();
                        }
                        if ui::secondary_button(ui, pal, "📂 내 PC 볼트 폴더 열기…").clicked() {
                            self.handle_open_vault_dialog();
                        }
                        if ui::secondary_button(ui, pal, "🔄 동기화 센터 설정").clicked() {
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
                        if ui::secondary_button(ui, pal, "📅 오늘의 일일 노트").clicked() {
                            self.open_today_daily_note();
                        }
                    });
                });
            });
            return;
        }

        if self.current_doc.as_ref().is_some_and(|d| d.locked) {
            self.render_locked_note_screen(ui, pal);
            return;
        }

        if self.split_doc.is_some() {
            let total_w = ui.available_width();
            let left_w = (total_w * 0.54).max(320.0);
            let avail_h = ui.available_height();
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(left_w, avail_h),
                    Layout::top_down(Align::Min),
                    |ui| {
                        self.render_main_note_pane(ui, pal);
                    },
                );
                ui.separator();
                let right_w = ui.available_width().max(220.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(right_w, avail_h),
                    Layout::top_down(Align::Min),
                    |ui| {
                        self.render_split_note_pane(ui, pal);
                    },
                );
            });
        } else {
            self.render_main_note_pane(ui, pal);
        }
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
                                    .color(pal.accent_blue),
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
        let is_a4 = self.workspace.page_layout != "full";
        let is_land = self.workspace.page_orientation == "landscape";
        let is_reader = self.note_interaction_mode == NoteInteractionMode::Read;
        let is_split_md = !is_reader && self.workspace.editor_mode == "markdown";

        let desk_bg = if is_a4 && !is_reader {
            pal.bg_desk
        } else {
            pal.bg_main
        };

        let avail_w = ui.available_width();
        let zoom = (self.workspace.editor_zoom.clamp(50, 150) as f32) / 100.0;
        let natural_a4_w = if is_land { 1123.0 } else { 794.0 };
        let mut sheet_w = if is_a4 && !is_reader {
            let max_fit = (avail_w - 36.0).max(360.0);
            let target = natural_a4_w * zoom;
            if self.workspace.auto_fit_a4 && target > max_fit {
                max_fit
            } else {
                target
            }
        } else if is_split_md {
            (avail_w - 24.0).max(480.0)
        } else {
            (avail_w - 32.0).min(1040.0 * zoom).max(360.0)
        };
        if is_reader {
            sheet_w = (avail_w - 32.0).min(900.0).max(340.0);
        }

        // 화면 좌우 여백: 실제 인쇄 cm 대비 화면에서는 50% 축소(~18.9px/cm) + 안전 범위 유지
        let scale = if is_a4 { sheet_w / natural_a4_w } else { 1.0 };
        let m_left_px =
            ((self.workspace.margin_left_cm as f32 * 18.9 * scale).max(20.0)).min(sheet_w * 0.25);
        let m_right_px =
            ((self.workspace.margin_right_cm as f32 * 18.9 * scale).max(20.0)).min(sheet_w * 0.25);
        let m_top_px = (self.workspace.margin_top_cm as f32 * 16.0 * scale).clamp(12.0, 72.0);
        let m_bot_px = (self.workspace.margin_bottom_cm as f32 * 16.0 * scale).clamp(16.0, 64.0);

        let mut clicked_wikilink: Option<String> = None;
        let mut toggled_task_line: Option<usize> = None;

        egui::Frame::none().fill(desk_bg).show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("main_note_editor_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.add_space(10.0);

                        // Top Centered Ruler Badge (WebView `.a4-ruler-badge` 1:1 일치)
                        ui.vertical_centered(|ui| {
                            egui::Frame::none()
                                .fill(pal.bg_inset)
                                .stroke(Stroke::new(1.0_f32, pal.border_subtle))
                                .rounding(Rounding::same(99.0))
                                .inner_margin(Margin::symmetric(12.0, 4.0))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                    let orient_txt = if is_land { "A4 가로" } else { "A4 세로" };
                                    let badge_text = if is_a4 {
                                        format!(
                                            "📄 {orient_txt} · 여백 상 {}/하 {}/좌 {}/우 {}cm · 배율 {}%",
                                            self.workspace.margin_top_cm,
                                            self.workspace.margin_bottom_cm,
                                            self.workspace.margin_left_cm,
                                            self.workspace.margin_right_cm,
                                            self.workspace.editor_zoom
                                        )
                                    } else {
                                        format!(
                                            "🖥️ 전체 화면 폭 모드 · 인쇄/PDF 여백 (상 {}/하 {}/좌 {}/우 {}cm)",
                                            self.workspace.margin_top_cm,
                                            self.workspace.margin_bottom_cm,
                                            self.workspace.margin_left_cm,
                                            self.workspace.margin_right_cm,
                                        )
                                    };
                                    ui.label(
                                        RichText::new(badge_text)
                                            .size(11.5)
                                            .color(pal.text_secondary),
                                    );
                                    if ui
                                        .small_button(if is_a4 { "📐 여백" } else { "📐 설정" })
                                        .clicked()
                                    {
                                        self.open_page_setup_modal();
                                    }
                                    if ui
                                        .small_button(if is_a4 {
                                            "🖥️ 전체폭"
                                        } else {
                                            "📄 A4 용지 보기"
                                        })
                                        .clicked()
                                    {
                                        self.workspace.page_layout = if is_a4 {
                                            "full".into()
                                        } else {
                                            "a4".into()
                                        };
                                        self.persist_workspace();
                                    }
                                });
                            });
                    });

                    ui.add_space(10.0);

                    // Centered A4 Paper Sheet / Full-Width Canvas
                    let mut sheet_outer_rect = Rect::NOTHING;
                    ui.vertical_centered(|ui| {
                        let sheet_frame = if is_a4 && !is_reader {
                            egui::Frame::none()
                                .fill(pal.bg_card)
                                .stroke(Stroke::new(1.0_f32, pal.border_strong))
                                .rounding(Rounding::same(4.0))
                                .inner_margin(Margin::ZERO)
                        } else {
                            egui::Frame::none()
                                .fill(pal.bg_main)
                                .stroke(Stroke::NONE)
                                .inner_margin(Margin::ZERO)
                        };

                        let resp_sheet = sheet_frame.show(ui, |ui| {
                            ui.set_width(sheet_w);
                            ui.set_max_width(sheet_w);
                            if is_a4 && !is_reader {
                                ui.set_min_height((1122.0 * scale).min(880.0));
                            }

                            // 1) 커버 배너
                            if let Some(cover_str) = self.draft_cover.clone() {
                                let cover_color = COVER_PRESETS
                                    .iter()
                                    .find(|(name, _)| cover_str.contains(name))
                                    .map(|(_, c)| *c)
                                    .unwrap_or(pal.accent_blue);
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width(), 140.0),
                                    Sense::hover(),
                                );
                                ui.painter().rect_filled(rect, Rounding::ZERO, cover_color);
                                ui.painter().text(
                                    rect.center(),
                                    Align2::CENTER_CENTER,
                                    &cover_str,
                                    egui::FontId::proportional(13.0),
                                    Color32::from_rgba_unmultiplied(255, 255, 255, 210),
                                );
                            }

                            // 2) 본문 여백 컬럼
                            let body_col_w = (sheet_w - m_left_px - m_right_px).max(200.0);
                            let content_resp = egui::Frame::none()
                                .inner_margin(Margin {
                                    left: m_left_px,
                                    right: m_right_px,
                                    top: m_top_px,
                                    bottom: m_bot_px,
                                })
                                .show(ui, |ui| {
                                    ui.set_width(body_col_w);
                                    ui.set_max_width(body_col_w);

                                    // 페이지 아이콘 (설정된 경우 상단에 크게 표시)
                                    if let Some(ic) = self.draft_icon.clone() {
                                        if ui
                                            .add(
                                                egui::Button::new(RichText::new(&ic).size(36.0))
                                                    .fill(Color32::TRANSPARENT)
                                                    .stroke(Stroke::NONE),
                                            )
                                            .on_hover_text("아이콘 변경")
                                            .clicked()
                                            && !is_reader
                                        {
                                            self.show_emoji_picker = true;
                                        }
                                    }

                                    // Notion 고스트 액션 바 (편집 모드에서만 표시)
                                    if !is_reader {
                                        ui.horizontal_wrapped(|ui| {
                                            ui.spacing_mut().item_spacing.x = 4.0;
                                            if ui::ghost_btn(ui, pal, "😀 아이콘 추가", "페이지 아이콘 선택")
                                                .clicked()
                                            {
                                                self.show_emoji_picker = true;
                                            }
                                            if ui::ghost_btn(ui, pal, "🖼️ 커버 추가", "커버 배너 추가/변경")
                                                .clicked()
                                            {
                                                self.show_cover_picker = true;
                                            }
                                            if self.draft_cover.is_some()
                                                && ui::ghost_btn(ui, pal, "커버 제거", "커버 배너 삭제")
                                                    .clicked()
                                            {
                                                self.draft_cover = None;
                                                self.mark_dirty();
                                            }
                                            if ui::ghost_btn(ui, pal, "✨ 템플릿 적용", "볼트 템플릿 불러오기")
                                                .clicked()
                                            {
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
                                                let is_fav =
                                                    self.workspace.favorites.contains(&cur_path);
                                                if ui::ghost_btn(
                                                    ui,
                                                    pal,
                                                    if is_fav {
                                                        "⭐ 즐겨찾기됨"
                                                    } else {
                                                        "☆ 즐겨찾기"
                                                    },
                                                    "즐겨찾기 고정/해제",
                                                )
                                                .clicked()
                                                {
                                                    self.toggle_favorite(&cur_path);
                                                }
                                            }
                                            if ui::ghost_btn(ui, pal, "🔒 암호화", "노트 암호화 설정")
                                                .clicked()
                                            {
                                                self.show_crypto_modal = true;
                                            }
                                            if self.current_doc.as_ref().is_some_and(|d| d.encrypted)
                                                && ui::ghost_btn(
                                                    ui,
                                                    pal,
                                                    "🔐 지금 잠그기",
                                                    "저장 후 즉시 잠금",
                                                )
                                                .clicked()
                                            {
                                                self.flush_save(false);
                                                if let (Some(vp), Some(cur)) = (
                                                    self.workspace.active_vault.clone(),
                                                    self.current_doc
                                                        .as_ref()
                                                        .map(|d| d.path.clone()),
                                                ) {
                                                    let res = {
                                                        let mut st = self.backend.lock().unwrap();
                                                        vault::lock_single_note(
                                                            Path::new(&vp),
                                                            &cur,
                                                            &mut st.crypto,
                                                        )
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
                                        ui.add_space(4.0);
                                    }

                                    // Borderless `28px` Bold Notion Title
                                    if is_reader {
                                        ui.label(
                                            RichText::new(&self.draft_title)
                                                .size(28.0)
                                                .strong()
                                                .color(pal.text_primary),
                                        );
                                    } else {
                                        let title_resp = ui.add(
                                            egui::TextEdit::singleline(&mut self.draft_title)
                                                .frame(false)
                                                .font(egui::FontId::proportional(28.0))
                                                .text_color(pal.text_primary)
                                                .hint_text("제목 없음")
                                                .desired_width(f32::INFINITY),
                                        );
                                        if title_resp.changed() {
                                            self.mark_dirty();
                                        }
                                    }

                                    ui.add_space(6.0);

                                    // Properties (with Tags Hidden by Default & Toggle Chip)
                                    self.render_properties_editor(ui, pal, is_reader);

                                    ui.add_space(4.0);
                                    ui.separator();
                                    ui.add_space(8.0);

                                    // Body Modes: Read / Unified Block WYSIWYG / Markdown Split
                                    if is_reader {
                                        let (cw, ttl) = self.render_reader_body(ui, pal);
                                        clicked_wikilink = cw;
                                        toggled_task_line = ttl;
                                    } else if is_split_md {
                                        let cw = self.render_split_markdown_body(ui, pal);
                                        clicked_wikilink = cw;
                                    } else {
                                        let (cw, ttl) = self.render_wysiwyg_blocks_body(ui, pal);
                                        clicked_wikilink = cw;
                                        toggled_task_line = ttl;
                                    }

                                    ui.add_space(16.0);
                                    if let Some(doc) = &self.current_doc {
                                        ui.label(
                                            RichText::new(format!(
                                                "단어 {} · 글자 {} · 수정 {}",
                                                doc.word_count, doc.char_count, doc.updated_at
                                            ))
                                            .size(11.5)
                                            .color(pal.text_muted),
                                        );
                                    }
                                });

                            // 희미한 좌우 여백 경계 가이드라인 (`workspace.show_page_guides`)
                            if self.workspace.show_page_guides {
                                let c_rect = content_resp.response.rect;
                                let left_x = c_rect.left() + m_left_px - 8.0;
                                let right_x = c_rect.right() - m_right_px + 8.0;
                                let y0 = c_rect.top() + 8.0;
                                let y1 = c_rect.bottom().max(y0 + 120.0);
                                ui.painter().line_segment(
                                    [Pos2::new(left_x, y0), Pos2::new(left_x, y1)],
                                    Stroke::new(1.0_f32, pal.guide_line),
                                );
                                ui.painter().line_segment(
                                    [Pos2::new(right_x, y0), Pos2::new(right_x, y1)],
                                    Stroke::new(1.0_f32, pal.guide_line),
                                );
                                if is_a4 {
                                    let page_h_px = 1122.0 * scale;
                                    let mut py = c_rect.top() + page_h_px;
                                    while py < c_rect.bottom() - 20.0 {
                                        ui.painter().hline(
                                            (left_x + 4.0)..=(right_x - 4.0),
                                            py,
                                            Stroke::new(1.0_f32, pal.guide_line),
                                        );
                                        py += page_h_px;
                                    }
                                }
                            }
                        });
                        sheet_outer_rect = resp_sheet.response.rect;
                    });
                    let _ = sheet_outer_rect;
                    ui.add_space(28.0);
                    });
                });
        });

        if let Some(line_idx) = toggled_task_line {
            self.toggle_task_at_line(line_idx);
        }
        if let Some(target) = clicked_wikilink {
            self.open_wikilink_by_title(&target);
        }
    }

    fn toggle_task_at_line(&mut self, line_idx: usize) {
        self.push_undo_snapshot();
        let mut lines: Vec<String> = self.draft_body.split('\n').map(str::to_string).collect();
        if let Some(l) = lines.get_mut(line_idx) {
            if l.contains("- [ ] ") {
                *l = l.replacen("- [ ] ", "- [x] ", 1);
            } else if l.contains("* [ ] ") {
                *l = l.replacen("* [ ] ", "* [x] ", 1);
            } else if l.contains("- [x] ") {
                *l = l.replacen("- [x] ", "- [ ] ", 1);
            } else if l.contains("- [X] ") {
                *l = l.replacen("- [X] ", "- [ ] ", 1);
            } else if l.contains("* [x] ") {
                *l = l.replacen("* [x] ", "* [ ] ", 1);
            }
            self.draft_body = lines.join("\n");
            self.mark_dirty();
        }
    }

    fn render_reader_body(
        &mut self,
        ui: &mut egui::Ui,
        pal: &ThemePalette,
    ) -> (Option<String>, Option<usize>) {
        let headings = vault::extract_headings(&self.draft_body);
        if headings.len() >= 2 {
            egui::CollapsingHeader::new(
                RichText::new(format!("📑 목차 ({})", headings.len()))
                    .strong()
                    .size(12.5),
            )
            .default_open(false)
            .show(ui, |ui| {
                for h in &headings {
                    ui.horizontal(|ui| {
                        ui.add_space((h.level.saturating_sub(1) as f32) * 12.0);
                        ui.label(RichText::new(&h.text).size(12.5).color(pal.text_secondary));
                    });
                }
            });
            ui.add_space(6.0);
        }

        let vp_opt = self.workspace.active_vault.clone();
        let outcome = render_markdown_preview_ex(
            ui,
            pal,
            &self.draft_body,
            vp_opt.as_deref().map(Path::new),
            &mut self.img_cache,
            true,
        );
        if let Some((start, end, act)) = outcome.diagram_action {
            self.handle_diagram_card_action(start, end, act);
        }

        ui.add_space(12.0);
        if ui::primary_button(ui, pal, "✏️ 이 노트 편집하기").clicked() {
            self.note_interaction_mode = NoteInteractionMode::Write;
        }
        (outcome.clicked_wikilink, outcome.toggled_task_line)
    }

    fn render_split_markdown_body(
        &mut self,
        ui: &mut egui::Ui,
        pal: &ThemePalette,
    ) -> Option<String> {
        let mut clicked = None;
        let vp_opt = self.workspace.active_vault.clone();
        let body_id = Id::new(BODY_EDITOR_ID_STR);

        ui.columns(2, |cols| {
            egui::Frame::none()
                .fill(pal.bg_secondary)
                .stroke(Stroke::new(1.0_f32, pal.border_subtle))
                .rounding(Rounding::same(6.0))
                .inner_margin(Margin::same(10.0))
                .show(&mut cols[0], |ui| {
                    let resp = ui.add(
                        egui::TextEdit::multiline(&mut self.draft_body)
                            .id(body_id)
                            .frame(false)
                            .font(egui::FontId::monospace(12.8))
                            .desired_rows(26)
                            .desired_width(f32::INFINITY)
                            .lock_focus(true)
                            .hint_text("마크다운 본문을 입력하세요…"),
                    );
                    if resp.changed() {
                        self.mark_dirty();
                    }
                    self.attach_body_context_menu(&resp);
                });

            egui::Frame::none()
                .fill(pal.bg_card)
                .stroke(Stroke::new(1.0_f32, pal.border_subtle))
                .rounding(Rounding::same(6.0))
                .inner_margin(Margin::same(12.0))
                .show(&mut cols[1], |ui| {
                    let outcome = render_markdown_preview_ex(
                        ui,
                        pal,
                        &self.draft_body,
                        vp_opt.as_deref().map(Path::new),
                        &mut self.img_cache,
                        true,
                    );
                    clicked = outcome.clicked_wikilink;
                    if let Some((s, e, act)) = outcome.diagram_action {
                        self.handle_diagram_card_action(s, e, act);
                    }
                });
        });
        clicked
    }

    /// **단일 문서 통합 Notion 블록 위지윅 편집기**:
    /// 더 이상 Raw Textarea 아래에 미리보기 박스를 이중 배치하지 않고,
    /// 문서 자체를 인플레이스로 클릭·편집하며 표·콜아웃·벡터 플로우차트·이미지를 직접 조작합니다.
    fn render_wysiwyg_blocks_body(
        &mut self,
        ui: &mut egui::Ui,
        pal: &ThemePalette,
    ) -> (Option<String>, Option<usize>) {
        let mut clicked_wiki: Option<String> = None;
        let mut toggled_task: Option<usize> = None;
        let vp_opt = self.workspace.active_vault.clone();
        let vp_ref = vp_opt.as_deref().map(Path::new);

        let blocks = parse_markdown_blocks(&self.draft_body);

        if blocks.is_empty() {
            let block_id = Id::new(BODY_EDITOR_ID_STR);
            let resp = ui.add(
                egui::TextEdit::multiline(&mut self.draft_body)
                    .id(block_id)
                    .frame(false)
                    .font(egui::FontId::proportional(13.33))
                    .desired_rows(10)
                    .desired_width(f32::INFINITY)
                    .hint_text(
                        "내용을 입력하세요… (상단 도구함 · Ctrl+V 일반 텍스트/이미지 붙여넣기)",
                    ),
            );
            if resp.changed() {
                self.mark_dirty();
            }
            self.attach_body_context_menu(&resp);
            return (None, None);
        }

        let num_blocks = blocks.len();
        for (b_idx, blk) in blocks.into_iter().enumerate() {
            // 현재 블록을 인플레이스 편집 중인 경우: 테두리 없는 TextEdit을 문서 해당 자리에 직접 표시
            if self.editing_block_idx == Some(b_idx) {
                let is_code_like = matches!(
                    blk.kind,
                    DocBlockKind::CodeBlock { .. } | DocBlockKind::Diagram { .. }
                );
                let font_size = match blk.kind {
                    DocBlockKind::Heading { level: 1 } => 22.0,
                    DocBlockKind::Heading { level: 2 } => 18.0,
                    DocBlockKind::Heading { level: 3 } => 15.5,
                    _ if is_code_like => 12.5,
                    _ => 13.33,
                };
                let font_id = if is_code_like {
                    egui::FontId::monospace(font_size)
                } else {
                    egui::FontId::proportional(font_size)
                };

                let frame_fill = if is_code_like {
                    pal.code_bg
                } else {
                    Color32::from_rgba_unmultiplied(
                        pal.accent_blue.r(),
                        pal.accent_blue.g(),
                        pal.accent_blue.b(),
                        10,
                    )
                };

                egui::Frame::none()
                    .fill(frame_fill)
                    .stroke(Stroke::new(1.0_f32, pal.border_subtle))
                    .rounding(Rounding::same(5.0))
                    .inner_margin(Margin::symmetric(6.0, 4.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if is_code_like {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("코드 / 다이어그램 편집 중")
                                        .size(11.0)
                                        .color(pal.text_muted),
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.small_button("✓ 렌더링 보기").clicked() {
                                        self.commit_all_block_edits();
                                    }
                                });
                            });
                        }
                        let ed_id = Id::new(BLOCK_EDITOR_ID_STR);
                        let rows = self.editing_block_text.split('\n').count().clamp(1, 18);
                        let ed_resp = ui.add(
                            egui::TextEdit::multiline(&mut self.editing_block_text)
                                .id(ed_id)
                                .frame(false)
                                .font(font_id)
                                .desired_rows(rows)
                                .desired_width(f32::INFINITY)
                                .lock_focus(true),
                        );
                        if self.request_focus_block {
                            ed_resp.request_focus();
                            self.request_focus_block = false;
                        }
                        if ed_resp.changed() {
                            self.sync_active_block_to_draft();
                        }
                        self.attach_body_context_menu(&ed_resp);
                        if ed_resp.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
                            self.commit_all_block_edits();
                        }
                    });
                ui.add_space(4.0);
                continue;
            }

            // 표(Table) 블록: 각 셀 직접 클릭 편집 + 행/열 추가·삭제 컨트롤
            if let DocBlockKind::Table { mut rows } = blk.kind.clone() {
                let is_editing_this_table = self
                    .editing_table_cell
                    .as_ref()
                    .is_some_and(|(bi, _, _, _)| *bi == b_idx);
                let num_cols = rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
                let col_w = ((ui.available_width() - 8.0) / num_cols as f32).max(76.0);
                let mut table_changed = false;

                if is_editing_this_table {
                    ui.horizontal(|ui| {
                        if ui.small_button("＋ 행").clicked() {
                            rows.push(vec![String::new(); num_cols]);
                            table_changed = true;
                        }
                        if ui.small_button("＋ 열").clicked() {
                            for (ri, r) in rows.iter_mut().enumerate() {
                                r.push(if ri == 0 {
                                    "새 열".into()
                                } else {
                                    String::new()
                                });
                            }
                            table_changed = true;
                        }
                        if rows.len() > 2 && ui.small_button("－ 행").clicked() {
                            rows.pop();
                            table_changed = true;
                        }
                        if num_cols > 1 && ui.small_button("－ 열").clicked() {
                            for r in &mut rows {
                                r.pop();
                            }
                            table_changed = true;
                        }
                        if ui.small_button("</> 소스").clicked() {
                            self.editing_table_cell = None;
                            self.begin_editing_block(b_idx, &blk);
                        }
                        if ui.small_button("✓ 완료").clicked() {
                            self.editing_table_cell = None;
                        }
                    });
                }

                egui::Frame::none()
                    .fill(pal.table_cell_bg)
                    .stroke(Stroke::new(1.0_f32, pal.table_outer))
                    .rounding(Rounding::same(3.0))
                    .show(ui, |ui| {
                        egui::Grid::new(("wysiwyg_tbl", b_idx))
                            .spacing([0.0, 0.0])
                            .show(ui, |ui| {
                                for r_idx in 0..rows.len() {
                                    let bg = if r_idx == 0 {
                                        pal.table_header_bg
                                    } else {
                                        pal.table_cell_bg
                                    };
                                    for c_idx in 0..num_cols {
                                        let is_cur_cell = self
                                            .editing_table_cell
                                            .as_ref()
                                            .is_some_and(|(bi, cr, cc, _)| {
                                                *bi == b_idx && *cr == r_idx && *cc == c_idx
                                            });
                                        let cell_frame = egui::Frame::none()
                                            .fill(bg)
                                            .stroke(Stroke::new(1.0_f32, pal.table_border))
                                            .inner_margin(Margin::symmetric(8.0, 5.0));
                                        let cresp = cell_frame
                                            .show(ui, |ui| {
                                                ui.set_min_width(col_w);
                                                if is_cur_cell {
                                                    if let Some((_, _, _, ref mut buf)) =
                                                        self.editing_table_cell
                                                    {
                                                        let er = ui.add(
                                                            egui::TextEdit::singleline(buf)
                                                                .frame(false)
                                                                .font(egui::FontId::proportional(
                                                                    13.0,
                                                                ))
                                                                .desired_width(col_w - 16.0),
                                                        );
                                                        if er.changed() {
                                                            while rows[r_idx].len() <= c_idx {
                                                                rows[r_idx].push(String::new());
                                                            }
                                                            rows[r_idx][c_idx] = buf.clone();
                                                            table_changed = true;
                                                        }
                                                    }
                                                } else {
                                                    let cell_val = rows[r_idx]
                                                        .get(c_idx)
                                                        .map(String::as_str)
                                                        .unwrap_or("");
                                                    if let Some(w) = graph_view::render_inline_line(
                                                        ui,
                                                        pal,
                                                        cell_val,
                                                        13.0,
                                                        r_idx == 0,
                                                        vp_ref,
                                                        &mut self.img_cache,
                                                    ) {
                                                        clicked_wiki = Some(w);
                                                    }
                                                }
                                            })
                                            .response
                                            .interact(Sense::click());
                                        if cresp.clicked() && !is_cur_cell {
                                            let init_val =
                                                rows[r_idx].get(c_idx).cloned().unwrap_or_default();
                                            self.editing_block_idx = None;
                                            self.editing_table_cell =
                                                Some((b_idx, r_idx, c_idx, init_val));
                                        }
                                    }
                                    ui.end_row();
                                }
                            });
                    });

                if table_changed {
                    let md_tbl = serialize_table_rows(&rows);
                    self.replace_line_range_in_draft(blk.start_line, blk.end_line, &md_tbl);
                    break;
                }
                ui.add_space(5.0);
                continue;
            }

            // 다이어그램 블록: 카드 버튼(방향/테마/곡선/빌더/</> 코드) 직접 조작
            if let DocBlockKind::Diagram { ref code } = blk.kind {
                let d_act = flowchart::render_diagram_card(ui, pal, code);
                match d_act {
                    DiagramCardAction::None => {}
                    DiagramCardAction::UpdateCode(next_code) => {
                        let fenced = format!("```flowchart\n{next_code}\n```");
                        self.replace_line_range_in_draft(blk.start_line, blk.end_line, &fenced);
                        break;
                    }
                    DiagramCardAction::RequestEditCode => {
                        self.begin_editing_block(b_idx, &blk);
                        break;
                    }
                    DiagramCardAction::OpenBuilder(cur_code) => {
                        self.flow_builder =
                            Some(FlowBuilderState::from_code(Some(b_idx), &cur_code));
                    }
                }
                ui.add_space(5.0);
                continue;
            }

            // 일반 리치 블록 렌더링 + 클릭 시 인플레이스 편집 활성화
            let blk_resp = ui
                .vertical(|ui| {
                    let out = render_markdown_preview_ex(
                        ui,
                        pal,
                        &blk.raw,
                        vp_ref,
                        &mut self.img_cache,
                        true,
                    );
                    if out.clicked_wikilink.is_some() {
                        clicked_wiki = out.clicked_wikilink;
                    }
                    if out.toggled_task_line.is_some() {
                        toggled_task = out.toggled_task_line.map(|rel| blk.start_line + rel);
                    }
                })
                .response
                .interact(Sense::click());

            if blk_resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
            }
            if blk_resp.clicked() && clicked_wiki.is_none() && toggled_task.is_none() {
                self.begin_editing_block(b_idx, &blk);
            }
            blk_resp.context_menu(|ui| {
                if ui.button("✏️ 이 블록 편집하기").clicked() {
                    self.begin_editing_block(b_idx, &blk);
                    ui.close_menu();
                }
            });
            ui.add_space(2.0);
        }

        // 하단 여백 클릭 시 새 문단 이어쓰기
        let _ = num_blocks;
        ui.add_space(10.0);
        let ctx_tail = ui.ctx().clone();
        let (tail_rect, tail_resp) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 96.0), Sense::click());
        if tail_resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
        }
        if self.editing_block_idx.is_none() {
            ui.painter().text(
                Pos2::new(tail_rect.left() + 2.0, tail_rect.top() + 8.0),
                Align2::LEFT_TOP,
                "클릭하여 아래에 새 문단 이어서 작성…",
                egui::FontId::proportional(12.5),
                pal.text_muted,
            );
        }
        if tail_resp.clicked() {
            self.push_undo_snapshot();
            if !self.draft_body.is_empty() && !self.draft_body.ends_with('\n') {
                self.draft_body.push_str("\n\n");
            } else {
                self.draft_body.push('\n');
            }
            self.draft_body.push_str(" ");
            let total_lines = self.draft_body.split('\n').count();
            let last_idx = total_lines.saturating_sub(1);
            self.editing_block_idx = Some(
                parse_markdown_blocks(&self.draft_body)
                    .len()
                    .saturating_sub(1),
            );
            self.editing_block_range = (last_idx, total_lines);
            self.editing_block_text = String::new();
            self.request_focus_block = true;
            self.mark_dirty();
            ctx_tail.request_repaint();
        }

        (clicked_wiki, toggled_task)
    }

    fn begin_editing_block(&mut self, b_idx: usize, blk: &DocBlock) {
        self.push_undo_snapshot();
        self.editing_table_cell = None;
        self.editing_block_idx = Some(b_idx);
        self.editing_block_range = (blk.start_line, blk.end_line);
        self.editing_block_text = blk.raw.clone();
        self.request_focus_block = true;
    }

    fn handle_diagram_card_action(
        &mut self,
        start_line: usize,
        end_line: usize,
        act: DiagramCardAction,
    ) {
        match act {
            DiagramCardAction::None => {}
            DiagramCardAction::UpdateCode(next_code) => {
                let fenced = format!("```flowchart\n{next_code}\n```");
                self.replace_line_range_in_draft(start_line, end_line, &fenced);
            }
            DiagramCardAction::RequestEditCode => {
                self.workspace.editor_mode = "markdown".into();
                self.persist_workspace();
            }
            DiagramCardAction::OpenBuilder(cur_code) => {
                self.flow_builder = Some(FlowBuilderState::from_code(None, &cur_code));
            }
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
            if ui.button("B 굵게 강조 (**)").clicked() {
                self.insert_or_wrap_in_editor(&ctx, "**", "**", "굵은 텍스트");
                ui.close_menu();
            }
            if ui.button("🟡 형광펜 (==텍스트==)").clicked() {
                self.insert_or_wrap_in_editor(&ctx, "==", "==", "강조 내용");
                ui.close_menu();
            }
            if ui.button("H2 제목 (##)").clicked() {
                self.insert_at_body_cursor(&ctx, "\n## ");
                ui.close_menu();
            }
            if ui.button("💡 콜아웃 박스").clicked() {
                self.insert_at_body_cursor(&ctx, "\n> [!note] 메모\n> 내용을 입력하세요.\n");
                ui.close_menu();
            }
            if ui.button("📊 3×3 표 삽입").clicked() {
                self.insert_at_body_cursor(
                    &ctx,
                    "\n| 항목 1 | 항목 2 | 항목 3 |\n| --- | --- | --- |\n| 값 1 | 값 2 | 값 3 |\n",
                );
                ui.close_menu();
            }
            if ui.button("◇ 플로우차트 빌더…").clicked() {
                self.flow_builder = Some(FlowBuilderState::from_code(None, ""));
                ui.close_menu();
            }
            ui.separator();
            if ui
                .button("🧹 본문 내 외부 HTML 속성 코드 일괄 정리")
                .clicked()
            {
                self.push_undo_snapshot();
                let cleaned =
                    win_clipboard::strip_html_wrappers_preserving_images(&self.draft_body);
                if cleaned != self.draft_body {
                    self.draft_body = cleaned;
                    self.commit_all_block_edits();
                    self.mark_dirty();
                    self.show_toast("본문의 불필요한 HTML 태그·속성 코드를 정리했습니다.", false);
                } else {
                    self.show_toast("정리할 외부 HTML 태그가 없습니다.", false);
                }
                ui.close_menu();
            }
        });
    }

    // ───────────────────────── Properties Editor (Tags Hidden by Default) ─────────────────────────
    fn render_properties_editor(&mut self, ui: &mut egui::Ui, pal: &ThemePalette, is_reader: bool) {
        let mut remove_idx: Option<usize> = None;
        let mut changed = false;
        let show_tags = self.workspace.show_tags;

        let mut hidden_tag_count = 0usize;
        for prop in &self.draft_properties {
            let is_tag_prop =
                prop.prop_type == "tags" || prop.key.trim().eq_ignore_ascii_case("tags");
            if is_tag_prop && !show_tags {
                hidden_tag_count += prop
                    .value
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .count();
            }
        }

        if is_reader {
            ui.horizontal_wrapped(|ui| {
                for prop in &self.draft_properties {
                    let val = prop.value.trim();
                    let is_tag_prop =
                        prop.prop_type == "tags" || prop.key.trim().eq_ignore_ascii_case("tags");
                    if is_tag_prop {
                        if !show_tags {
                            continue;
                        }
                        for t in val.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
                            ui::pill_badge(
                                ui,
                                &format!("#{}", t.trim_start_matches('#')),
                                pal.accent_blue,
                                pal.accent_blue_soft,
                            );
                        }
                    } else if !val.is_empty() {
                        ui::pill_badge(
                            ui,
                            &format!("{}: {val}", prop.key),
                            pal.text_secondary,
                            pal.bg_inset,
                        );
                    }
                }
                if hidden_tag_count > 0 {
                    if ui
                        .small_button(format!("🏷️ 태그 {hidden_tag_count}개 보기"))
                        .clicked()
                    {
                        self.toggle_show_tags();
                    }
                }
            });
            return;
        }

        for (idx, prop) in self.draft_properties.iter_mut().enumerate() {
            let is_tag_prop =
                prop.prop_type == "tags" || prop.key.trim().eq_ignore_ascii_case("tags");
            if is_tag_prop && !show_tags {
                continue;
            }
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
                    .width(82.0)
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
                            .frame(false)
                            .hint_text("속성 이름")
                            .desired_width(105.0),
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
                            .frame(false)
                            .hint_text(if prop.prop_type == "tags" {
                                "쉼표로 구분 (예: 업무, 회의)"
                            } else {
                                "값 입력"
                            })
                            .desired_width((ui.available_width() - 36.0).max(120.0)),
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
            if ui::ghost_btn(ui, pal, "＋ 속성 추가", "노션 스타일 속성 추가").clicked()
            {
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
                if !has_tags {
                    self.workspace.show_tags = true;
                }
                changed = true;
            }

            let tag_pill_lbl = if self.workspace.show_tags {
                "🏷️ 태그 숨기기".to_string()
            } else if hidden_tag_count > 0 {
                format!("🏷️ 태그 보기 ({hidden_tag_count})")
            } else {
                "🏷️ 태그 보기".to_string()
            };
            if ui
                .add(
                    egui::Button::new(
                        RichText::new(tag_pill_lbl)
                            .size(11.5)
                            .color(pal.text_secondary),
                    )
                    .fill(if self.workspace.show_tags {
                        pal.accent_blue_soft
                    } else {
                        pal.bg_inset
                    })
                    .stroke(Stroke::new(1.0_f32, pal.border_subtle))
                    .rounding(Rounding::same(99.0)),
                )
                .clicked()
            {
                self.toggle_show_tags();
            }

            if ui::ghost_btn(
                ui,
                pal,
                if self.show_raw_yaml {
                    "▲ 원본 YAML 접기"
                } else {
                    "▼ 원본 YAML 보기"
                },
                "Frontmatter YAML 원본 보기",
            )
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
                        } else {
                            let out = render_markdown_preview_ex(
                                ui,
                                pal,
                                &split.body,
                                vault_root_ref,
                                &mut self.img_cache,
                                false,
                            );
                            clicked_wiki = out.clicked_wikilink;
                        }
                    });
            });

        if let Some(target) = clicked_wiki {
            self.open_wikilink_by_title(&target);
        }
    }

    // ───────────────────────── Right Inspector Panel ─────────────────────────
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

    // ───────────────────────── Global Graph View (3 Layouts + Filters + Card) ─────────────────────────
    fn render_global_graph_view(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        if self.global_graph_data.is_none() {
            self.refresh_global_graph();
        }

        egui::Frame::none()
            .inner_margin(Margin::same(16.0))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("🕸️ 지식 그래프").strong().size(15.0));

                    for mode in [
                        GraphLayoutMode::Cluster,
                        GraphLayoutMode::Radial,
                        GraphLayoutMode::Hierarchy,
                    ] {
                        if ui
                            .selectable_label(
                                self.global_graph_widget.layout_mode == mode,
                                mode.label(),
                            )
                            .clicked()
                        {
                            self.global_graph_widget.set_layout_mode(mode);
                        }
                    }

                    ui.separator();

                    let folders = self.global_graph_widget.available_folders();
                    let cur_ff = if self.global_graph_widget.folder_filter.is_empty() {
                        "모든 폴더".to_string()
                    } else {
                        self.global_graph_widget.folder_filter.clone()
                    };
                    egui::ComboBox::from_id_salt("graph_folder_filter_combo")
                        .selected_text(cur_ff)
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(
                                    self.global_graph_widget.folder_filter.is_empty(),
                                    "모든 폴더",
                                )
                                .clicked()
                            {
                                self.global_graph_widget.set_folder_filter(String::new());
                            }
                            for f in folders {
                                let sel = self.global_graph_widget.folder_filter == f;
                                if ui.selectable_label(sel, &f).clicked() {
                                    self.global_graph_widget.set_folder_filter(f);
                                }
                            }
                        });

                    let mut hide_orph = self.global_graph_widget.hide_orphans;
                    if ui
                        .checkbox(&mut hide_orph, "연결 없는 노드 숨기기")
                        .changed()
                    {
                        self.global_graph_widget.set_hide_orphans(hide_orph);
                    }

                    if ui
                        .checkbox(&mut self.global_graph_include_tags, "#태그 노드")
                        .changed()
                    {
                        self.refresh_global_graph();
                    }

                    if ui
                        .checkbox(&mut self.global_graph_local_only, "현재 노트(2단계)만")
                        .changed()
                    {
                        self.refresh_global_graph();
                    }

                    ui.add(
                        egui::TextEdit::singleline(&mut self.global_graph_widget.search_query)
                            .hint_text("노드 이름 검색…")
                            .desired_width(150.0),
                    );

                    if ui.button("카메라 맞춤").clicked() {
                        self.global_graph_widget.reset_camera();
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui::primary_button(ui, pal, "← 노트 편집기로").clicked() {
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

    // ───────────────────────── All Modals ─────────────────────────
    fn render_modals(&mut self, ctx: &egui::Context, pal: &ThemePalette) {
        // 1. 빠른 검색 / 명령어 팔레트 (Ctrl+K / Ctrl+Shift+P)
        if self.show_command_palette {
            let mut close = false;
            let mut open_path: Option<String> = None;

            egui::Window::new("🔍 빠른 검색 및 명령어 팔레트")
                .collapsible(false)
                .resizable(false)
                .fixed_size([540.0, 410.0])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.palette_query)
                            .hint_text("노트 제목·본문·#태그 검색 또는 아래 빠른 명령 실행…")
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
                        .max_height(310.0)
                        .show(ui, |ui| {
                            if self.palette_query.trim().is_empty() {
                                ui.horizontal_wrapped(|ui| {
                                    if ui.small_button("📐 A4 용지/여백 설정").clicked() {
                                        self.open_page_setup_modal();
                                        close = true;
                                    }
                                    if ui.small_button("🖨️ 인쇄 (Ctrl+P)").clicked() {
                                        self.execute_print_note();
                                        close = true;
                                    }
                                    if ui.small_button("📄 PDF 저장").clicked() {
                                        self.execute_export_pdf();
                                        close = true;
                                    }
                                    if ui.small_button("📅 일일 폴더 설정").clicked() {
                                        self.daily_folder_modal = Some(DailyFolderModalState {
                                            selected_folder: self.get_active_daily_folder(),
                                            new_subfolder_name: String::new(),
                                        });
                                        close = true;
                                    }
                                    let auto_lbl = if self.sync_snap.autostart {
                                        "🔄 자동 실행 끄기"
                                    } else {
                                        "🔄 PC 재시작 시 자동 실행 켜기"
                                    };
                                    if ui.small_button(auto_lbl).clicked() {
                                        let next = !self.sync_snap.autostart;
                                        let mut st = self.backend.lock().unwrap();
                                        st.sync.set_autostart(next);
                                        self.sync_snap = st.sync.snapshot();
                                        close = true;
                                    }
                                    if ui.small_button("🚀 GitHub 업데이트 확인").clicked()
                                    {
                                        self.backend.lock().unwrap().updater.check_async(true);
                                        self.show_toast("GitHub 최신 버전을 확인 중입니다…", false);
                                        close = true;
                                    }
                                });
                                ui.separator();
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

        // 3. 커스텀 폴더 아이콘·색상 변경 모달 (13종 팔레트 + 자유 RGB 색상 + 36종 이모지 + 직접 입력 + 라이브 미리보기)
        if let Some(mut fsm) = self.folder_style_modal.take() {
            let mut keep_open = true;
            let mut apply_style = false;
            let mut reset_style = false;

            egui::Window::new(format!("🎨 폴더 아이콘 · 색상 ({})", fsm.folder_name))
                .collapsible(false)
                .resizable(false)
                .fixed_size([430.0, 380.0])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    // 라이브 미리보기
                    ui::sub_card(ui, pal, |ui| {
                        ui.horizontal(|ui| {
                            let preview_col = if fsm.use_custom_color {
                                Some(Color32::from_rgb(fsm.rgb[0], fsm.rgb[1], fsm.rgb[2]))
                            } else {
                                None
                            };
                            let (irec, _) =
                                ui.allocate_exact_size(Vec2::new(20.0, 18.0), Sense::hover());
                            if fsm.icon.trim().is_empty() {
                                ui::paint_folder_icon(ui.painter(), irec, preview_col, false, pal);
                            } else {
                                ui.painter().text(
                                    irec.center(),
                                    Align2::CENTER_CENTER,
                                    fsm.icon.trim(),
                                    egui::FontId::proportional(15.0),
                                    preview_col.unwrap_or(pal.text_primary),
                                );
                            }
                            ui.label(
                                RichText::new(&fsm.folder_name)
                                    .strong()
                                    .size(14.5)
                                    .color(preview_col.unwrap_or(pal.text_primary)),
                            );
                        });
                    });

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("폴더 색상").strong().size(12.5));
                        if fsm.use_custom_color {
                            if ui.color_edit_button_srgb(&mut fsm.rgb).changed() {
                                fsm.color = format!(
                                    "#{:02x}{:02x}{:02x}",
                                    fsm.rgb[0], fsm.rgb[1], fsm.rgb[2]
                                );
                            }
                            ui.label(RichText::new(&fsm.color).monospace().size(11.5));
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        for (lbl, hex) in FOLDER_COLOR_PRESETS {
                            let selected = if hex.is_empty() {
                                !fsm.use_custom_color
                            } else {
                                fsm.use_custom_color && fsm.color.eq_ignore_ascii_case(hex)
                            };
                            let col = ui::parse_hex_color(hex).unwrap_or(pal.text_primary);
                            if ui
                                .selectable_label(selected, RichText::new(*lbl).strong().color(col))
                                .clicked()
                            {
                                if hex.is_empty() {
                                    fsm.use_custom_color = false;
                                    fsm.color.clear();
                                } else {
                                    fsm.use_custom_color = true;
                                    fsm.color = (*hex).to_string();
                                    fsm.rgb = [col.r(), col.g(), col.b()];
                                }
                            }
                        }
                    });

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("폴더 아이콘").strong().size(12.5));
                        if ui.small_button("기본 폴더 모양 사용").clicked() {
                            fsm.icon.clear();
                        }
                        ui.add(
                            egui::TextEdit::singleline(&mut fsm.icon)
                                .hint_text("직접 입력 (예: 💼)")
                                .desired_width(110.0),
                        );
                    });
                    ui.horizontal_wrapped(|ui| {
                        for ic in EMOJI_PRESETS {
                            let selected = fsm.icon == *ic;
                            if ui
                                .selectable_label(selected, RichText::new(*ic).size(16.0))
                                .clicked()
                            {
                                fsm.icon = (*ic).to_string();
                            }
                        }
                    });

                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui::primary_button(ui, pal, "적용").clicked() {
                            apply_style = true;
                            keep_open = false;
                        }
                        if ui::secondary_button(ui, pal, "초기화").clicked() {
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
                        Some(fsm.icon.trim())
                    };
                    let col_opt =
                        if reset_style || !fsm.use_custom_color || fsm.color.trim().is_empty() {
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

        // 4. 폴더/노트 다른 폴더로 이동 모달
        if let Some(mut mv) = self.move_modal.take() {
            let mut keep_open = true;
            let mut confirm_move = false;
            let mut folders = Vec::new();
            if let Some(ref scan) = self.scan {
                collect_folder_paths_vec(&scan.tree, &mut folders);
            }

            egui::Window::new(format!("📦 이동: {}", mv.display_name))
                .collapsible(false)
                .resizable(false)
                .fixed_size([380.0, 320.0])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(
                        RichText::new("이동할 대상 폴더를 선택하세요:")
                            .size(12.5)
                            .color(pal.text_secondary),
                    );
                    egui::ScrollArea::vertical()
                        .max_height(210.0)
                        .show(ui, |ui| {
                            if ui
                                .selectable_label(
                                    mv.target_folder.is_empty(),
                                    "🏠 (볼트 최상위 루트)",
                                )
                                .clicked()
                            {
                                mv.target_folder.clear();
                            }
                            for f in &folders {
                                if mv.is_folder
                                    && (f == &mv.source_path
                                        || f.starts_with(&format!("{}/", mv.source_path)))
                                {
                                    continue;
                                }
                                if ui
                                    .selectable_label(&mv.target_folder == f, format!("📁 {f}"))
                                    .clicked()
                                {
                                    mv.target_folder = f.clone();
                                }
                            }
                        });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui::primary_button(ui, pal, "이 위치로 이동").clicked() {
                            confirm_move = true;
                            keep_open = false;
                        }
                        if ui.button("취소").clicked() {
                            keep_open = false;
                        }
                    });
                });

            if confirm_move {
                self.execute_move_entry(&mv.source_path, &mv.target_folder);
            } else if keep_open {
                self.move_modal = Some(mv);
            }
        }

        // 5. 일일 문서(Daily) 저장 폴더 설정 모달 (볼트별 저장)
        if let Some(mut dfm) = self.daily_folder_modal.take() {
            let mut keep_open = true;
            let mut save_daily = false;
            let mut folders = Vec::new();
            if let Some(ref scan) = self.scan {
                collect_folder_paths_vec(&scan.tree, &mut folders);
            }

            egui::Window::new("📅 일일 문서(Daily) 저장 폴더 설정")
                .collapsible(false)
                .resizable(false)
                .fixed_size([400.0, 340.0])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(
                        RichText::new("Ctrl+D 또는 '오늘의 일일 노트' 클릭 시 노트가 생성될 폴더를 지정합니다:")
                            .size(12.5)
                            .color(pal.text_secondary),
                    );
                    egui::ScrollArea::vertical()
                        .max_height(180.0)
                        .show(ui, |ui| {
                            if ui
                                .selectable_label(dfm.selected_folder.is_empty(), "🏠 (볼트 최상위 루트)")
                                .clicked()
                            {
                                dfm.selected_folder.clear();
                            }
                            if !folders.iter().any(|f| f == "Daily")
                                && ui
                                    .selectable_label(dfm.selected_folder == "Daily", "📁 Daily (기본)")
                                    .clicked()
                            {
                                dfm.selected_folder = "Daily".into();
                            }
                            for f in &folders {
                                if ui
                                    .selectable_label(&dfm.selected_folder == f, format!("📁 {f}"))
                                    .clicked()
                                {
                                    dfm.selected_folder = f.clone();
                                }
                            }
                        });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut dfm.new_subfolder_name)
                                .hint_text("새 일일 폴더 이름 (예: 일일업무)")
                                .desired_width(220.0),
                        );
                        if ui.button("📁 폴더 생성").clicked() && !dfm.new_subfolder_name.trim().is_empty()
                        {
                            if let Some(ref vp) = self.workspace.active_vault.clone() {
                                if let Ok(created) = vault::create_folder(
                                    Path::new(vp),
                                    "",
                                    dfm.new_subfolder_name.trim(),
                                ) {
                                    dfm.selected_folder = created;
                                    dfm.new_subfolder_name.clear();
                                    self.rescan_active_vault();
                                }
                            }
                        }
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui::primary_button(ui, pal, "설정 저장").clicked() {
                            save_daily = true;
                            keep_open = false;
                        }
                        if ui.button("취소").clicked() {
                            keep_open = false;
                        }
                    });
                });

            if save_daily {
                if let Some(vp) = self.workspace.active_vault.clone() {
                    self.workspace
                        .daily_folders_by_vault
                        .insert(vp, dfm.selected_folder.clone());
                }
                if !dfm.selected_folder.trim().is_empty() {
                    self.workspace.daily_folder = dfm.selected_folder.clone();
                }
                self.persist_workspace();
                self.show_toast(
                    if dfm.selected_folder.is_empty() {
                        "일일 문서 폴더를 볼트 루트로 설정했습니다.".to_string()
                    } else {
                        format!(
                            "일일 문서 폴더가 '{}'로 설정되었습니다.",
                            dfm.selected_folder
                        )
                    },
                    false,
                );
            } else if keep_open {
                self.daily_folder_modal = Some(dfm);
            }
        }

        // 6. A4 용지 및 인쇄/PDF 여백 설정 모달 (`📐`)
        if let Some(mut ps) = self.page_setup_modal.take() {
            let mut keep_open = true;
            let mut apply_only = false;
            let mut apply_and_print = false;
            let mut apply_and_pdf = false;

            egui::Window::new("📐 A4 용지 및 인쇄/PDF 여백 설정")
                .collapsible(false)
                .resizable(false)
                .fixed_size([440.0, 390.0])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("편집 화면 레이아웃").strong());
                        ui.radio_value(&mut ps.page_layout, "a4".into(), "A4 용지");
                        ui.radio_value(&mut ps.page_layout, "full".into(), "전체 화면 폭");
                    });
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("용지 방향").strong());
                        ui.radio_value(
                            &mut ps.page_orientation,
                            "portrait".into(),
                            "세로 (210×297mm)",
                        );
                        ui.radio_value(
                            &mut ps.page_orientation,
                            "landscape".into(),
                            "가로 (297×210mm)",
                        );
                    });

                    ui.add_space(6.0);
                    ui::sub_card(ui, pal, |ui| {
                        ui.label(RichText::new("인쇄 / PDF 출력 여백 (cm)").strong());
                        egui::Grid::new("margin_setup_grid")
                            .num_columns(4)
                            .spacing([10.0, 6.0])
                            .show(ui, |ui| {
                                ui.label("상단 (Top)");
                                ui.add(
                                    egui::DragValue::new(&mut ps.margin_top_cm)
                                        .range(0.0..=6.0)
                                        .speed(0.1)
                                        .suffix("cm"),
                                );
                                ui.label("하단 (Bottom)");
                                ui.add(
                                    egui::DragValue::new(&mut ps.margin_bottom_cm)
                                        .range(0.0..=6.0)
                                        .speed(0.1)
                                        .suffix("cm"),
                                );
                                ui.end_row();

                                ui.label("좌측 (Left)");
                                ui.add(
                                    egui::DragValue::new(&mut ps.margin_left_cm)
                                        .range(0.0..=6.0)
                                        .speed(0.1)
                                        .suffix("cm"),
                                );
                                ui.label("우측 (Right)");
                                ui.add(
                                    egui::DragValue::new(&mut ps.margin_right_cm)
                                        .range(0.0..=6.0)
                                        .speed(0.1)
                                        .suffix("cm"),
                                );
                                ui.end_row();
                            });
                    });

                    ui.add_space(6.0);
                    ui.checkbox(
                        &mut ps.show_page_guides,
                        "편집기 좌우 여백 희미한 경계선 및 페이지 구분 표시",
                    );
                    ui.checkbox(&mut ps.auto_fit_a4, "작은 창에서 A4 용지 폭 자동 맞춤");
                    ui.checkbox(
                        &mut ps.print_include_title,
                        "인쇄/PDF 상단에 노트 제목 포함",
                    );
                    ui.checkbox(
                        &mut ps.print_include_properties,
                        "인쇄/PDF 상단에 속성 표 포함",
                    );
                    ui.horizontal(|ui| {
                        ui.label("화면 배율 (%)");
                        ui.add(egui::Slider::new(&mut ps.editor_zoom, 50..=150).suffix("%"));
                    });

                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui::primary_button(ui, pal, "설정 적용").clicked() {
                            apply_only = true;
                            keep_open = false;
                        }
                        if ui::secondary_button(ui, pal, "🖨️ 지금 인쇄").clicked() {
                            apply_and_print = true;
                            keep_open = false;
                        }
                        if ui::secondary_button(ui, pal, "📄 PDF 저장").clicked() {
                            apply_and_pdf = true;
                            keep_open = false;
                        }
                        if ui.button("취소").clicked() {
                            keep_open = false;
                        }
                    });
                });

            if apply_only || apply_and_print || apply_and_pdf {
                self.workspace.page_layout = ps.page_layout;
                self.workspace.page_orientation = ps.page_orientation;
                self.workspace.margin_top_cm = ps.margin_top_cm;
                self.workspace.margin_bottom_cm = ps.margin_bottom_cm;
                self.workspace.margin_left_cm = ps.margin_left_cm;
                self.workspace.margin_right_cm = ps.margin_right_cm;
                self.workspace.auto_fit_a4 = ps.auto_fit_a4;
                self.workspace.show_page_guides = ps.show_page_guides;
                self.workspace.editor_zoom = ps.editor_zoom;
                self.workspace.print_include_title = ps.print_include_title;
                self.workspace.print_include_properties = ps.print_include_properties;
                self.persist_workspace();
                if apply_and_print {
                    self.execute_print_note();
                } else if apply_and_pdf {
                    self.execute_export_pdf();
                } else {
                    self.show_toast("용지 및 여백 설정을 적용했습니다.", false);
                }
            } else if keep_open {
                self.page_setup_modal = Some(ps);
            }
        }

        // 7. 웹 링크 삽입 모달 (`🔗`)
        if let Some(mut lm) = self.link_modal.take() {
            let mut keep_open = true;
            let mut insert_it = false;
            egui::Window::new("🔗 웹 링크 삽입")
                .collapsible(false)
                .resizable(false)
                .fixed_size([380.0, 160.0])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("표시 텍스트");
                        ui.text_edit_singleline(&mut lm.label);
                    });
                    ui.horizontal(|ui| {
                        ui.label("링크 주소  ");
                        ui.text_edit_singleline(&mut lm.url);
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui::primary_button(ui, pal, "삽입").clicked() {
                            insert_it = true;
                            keep_open = false;
                        }
                        if ui.button("취소").clicked() {
                            keep_open = false;
                        }
                    });
                });
            if insert_it && !lm.url.trim().is_empty() {
                let txt = if lm.label.trim().is_empty() {
                    lm.url.trim()
                } else {
                    lm.label.trim()
                };
                self.insert_at_body_cursor(ctx, &format!("[{txt}]({})", lm.url.trim()));
            } else if keep_open {
                self.link_modal = Some(lm);
            }
        }

        // 8. 비주얼 플로우차트 빌더 모달
        if let Some(mut fb) = self.flow_builder.take() {
            match flowchart::render_diagram_builder_modal(ctx, pal, &mut fb) {
                BuilderModalAction::None => {
                    self.flow_builder = Some(fb);
                }
                BuilderModalAction::Close => {}
                BuilderModalAction::Apply { block_idx, code } => {
                    let fenced = format!("```flowchart\n{code}\n```");
                    if let Some(bi) = block_idx {
                        let blocks = parse_markdown_blocks(&self.draft_body);
                        if let Some(target_blk) = blocks.get(bi) {
                            self.replace_line_range_in_draft(
                                target_blk.start_line,
                                target_blk.end_line,
                                &fenced,
                            );
                        } else {
                            self.insert_at_body_cursor(ctx, &format!("\n{fenced}\n"));
                        }
                    } else {
                        self.insert_at_body_cursor(ctx, &format!("\n{fenced}\n"));
                    }
                    self.show_toast("플로우차트를 노트에 적용했습니다.", false);
                }
            }
        }

        // 9. 노트 · 볼트 AES-256-GCM 암호화 관리 모달
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
                                } else if ui.button("🔓 암호화 영구 해제(평문 전환)").clicked()
                                {
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

        // 10. 이모지 아이콘 선택 모달
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

        // 11. 커버 선택 모달
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

        // 12. 노트 템플릿 모달
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
                self.push_undo_snapshot();
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
                self.commit_all_block_edits();
                self.mark_dirty();
                self.show_toast("템플릿을 적용했습니다.", false);
            }
            if close {
                self.show_templates_modal = false;
            }
        }

        // 13. 위키링크 삽입 모달
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

        // 14. 볼트 휴지통 (.trash) 모달
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
                        (Color32::WHITE, pal.accent_blue)
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
