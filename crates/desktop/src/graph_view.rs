//! 네이티브 Rust(`egui`) 기반 마크다운 미리보기 렌더러(인라인 이미지 포함) & 인터랙티브 지식 그래프 위젯.
//! - 마크다운 이미지(`![설명](경로)`) 및 Obsidian 임베드(`![[파일.png]]`, `![[파일.png|400]]`) 실제 이미지 렌더링 지원
//! - 콜아웃(`> [!note]`), 하이라이트(`==텍스트==`), 위키링크(`[[노트]]`), 체크박스, 표, 코드 블록 지원
//! - Force-directed 물리 시뮬레이션 기반 로컬/전체 지식 그래프(확대/축소/드래그/클릭 이동)

use crate::ui::ThemePalette;
use crate::vault::{self, GraphDto, GraphNodeDto};
use crate::win_clipboard;
use eframe::egui::{
    self, Align2, Color32, ColorImage, FontId, Margin, Pos2, RichText, Rounding, Sense, Stroke,
    TextureHandle, TextureOptions, Vec2,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

enum CachedImageState {
    Loaded {
        texture: TextureHandle,
        size: [usize; 2],
        abs_path: PathBuf,
    },
    Failed(String),
}

#[derive(Default)]
pub struct ImageTextureCache {
    entries: HashMap<String, CachedImageState>,
}

impl ImageTextureCache {
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    fn get_or_load(
        &mut self,
        ctx: &egui::Context,
        vault_root: &Path,
        raw_target: &str,
    ) -> &CachedImageState {
        let clean_target = normalize_image_target(raw_target);
        let key = format!("{}::{}", vault_root.display(), clean_target);

        if self.entries.len() > 64 && !self.entries.contains_key(&key) {
            self.entries.clear();
        }

        self.entries.entry(key.clone()).or_insert_with(|| {
            let Some(abs_path) = vault::resolve_vault_media_file(vault_root, &clean_target) else {
                return CachedImageState::Failed(format!("이미지를 찾을 수 없음: {clean_target}"));
            };
            let Ok(bytes) = std::fs::read(&abs_path) else {
                return CachedImageState::Failed(format!(
                    "이미지 읽기 실패: {}",
                    abs_path.display()
                ));
            };
            match image::load_from_memory(&bytes) {
                Ok(dyn_img) => {
                    let max_side = 1600u32;
                    let resized = if dyn_img.width() > max_side || dyn_img.height() > max_side {
                        dyn_img.resize(max_side, max_side, image::imageops::FilterType::Triangle)
                    } else {
                        dyn_img
                    };
                    let rgba = resized.to_rgba8();
                    let size = [rgba.width() as usize, rgba.height() as usize];
                    let color_img =
                        ColorImage::from_rgba_unmultiplied(size, rgba.as_flat_samples().as_slice());
                    let tex = ctx.load_texture(key, color_img, TextureOptions::LINEAR);
                    CachedImageState::Loaded {
                        texture: tex,
                        size,
                        abs_path,
                    }
                }
                Err(e) => {
                    CachedImageState::Failed(format!("이미지 디코딩 실패 ({clean_target}): {e}"))
                }
            }
        })
    }
}

fn normalize_image_target(raw: &str) -> String {
    let trimmed = raw
        .trim()
        .trim_matches(|c| c == '<' || c == '>' || c == '"');
    let without_title = match trimmed.split_once(" \"") {
        Some((p, _)) => p.trim(),
        None => trimmed,
    };
    percent_encoding::percent_decode_str(without_title)
        .decode_utf8_lossy()
        .replace('\\', "/")
}

fn is_embed_image_path(target: &str) -> bool {
    let lower = target.trim().to_ascii_lowercase();
    [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".ico"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

/// 한 줄이 단독 이미지(`![alt](path)` 또는 `![[image.png]]` / `![[image.png|420]]`)인지 파싱합니다.
fn parse_standalone_image_line(trimmed: &str) -> Option<(String, String, Option<f32>)> {
    if let Some(inner) = trimmed
        .strip_prefix("![[")
        .and_then(|s| s.strip_suffix("]]"))
    {
        let (target_raw, opt_pipe) = match inner.split_once('|') {
            Some((t, p)) => (t.trim(), Some(p.trim())),
            None => (inner.trim(), None),
        };
        if is_embed_image_path(target_raw) {
            let custom_w = opt_pipe.and_then(|p| {
                p.split('x')
                    .next()
                    .and_then(|w| w.trim().parse::<f32>().ok())
                    .filter(|&w| w >= 24.0 && w <= 2400.0)
            });
            let alt = opt_pipe
                .filter(|_| custom_w.is_none())
                .unwrap_or(target_raw)
                .to_string();
            return Some((target_raw.to_string(), alt, custom_w));
        }
    }

    if let Some(after_bang) = trimmed.strip_prefix("![") {
        if let Some(mid) = after_bang.find("](") {
            if let Some(inner_rest) = after_bang[mid + 2..].strip_suffix(')') {
                if !inner_rest.contains(')') {
                    let alt_raw = after_bang[..mid].trim();
                    let (alt, custom_w) = match alt_raw.rsplit_once('|') {
                        Some((a, w_str)) => match w_str.trim().parse::<f32>() {
                            Ok(w) if (24.0..=2400.0).contains(&w) => {
                                (a.trim().to_string(), Some(w))
                            }
                            _ => (alt_raw.to_string(), None),
                        },
                        None => (alt_raw.to_string(), None),
                    };
                    return Some((inner_rest.trim().to_string(), alt, custom_w));
                }
            }
        }
    }
    None
}

fn render_vault_image_block(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    vault_root: Option<&Path>,
    target: &str,
    alt: &str,
    custom_width: Option<f32>,
    img_cache: &mut ImageTextureCache,
) {
    if target.starts_with("http://") || target.starts_with("https://") {
        egui::Frame::none()
            .fill(pal.bg_inset)
            .stroke(Stroke::new(1.0_f32, pal.border_subtle))
            .rounding(Rounding::same(7.0))
            .inner_margin(Margin::symmetric(10.0, 8.0))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("🖼️ 외부 이미지:").strong());
                    if ui
                        .hyperlink_to(if alt.is_empty() { target } else { alt }, target)
                        .clicked()
                    {
                        let _ = open::that(target);
                    }
                });
            });
        ui.add_space(4.0);
        return;
    }

    let Some(vr) = vault_root else {
        ui.label(
            RichText::new(format!("🖼️ [이미지: {target}]"))
                .size(12.5)
                .color(pal.text_muted),
        );
        return;
    };

    let ctx = ui.ctx().clone();
    match img_cache.get_or_load(&ctx, vr, target) {
        CachedImageState::Loaded {
            texture,
            size,
            abs_path,
        } => {
            let nat_w = (size[0].max(1)) as f32;
            let nat_h = (size[1].max(1)) as f32;
            let max_avail = ui.available_width().max(120.0).min(680.0);
            let desired_w = custom_width.unwrap_or(nat_w).min(max_avail).max(48.0);
            let desired_h = desired_w * (nat_h / nat_w);
            let abs_clone = abs_path.clone();

            egui::Frame::none()
                .fill(pal.bg_card)
                .stroke(Stroke::new(1.0_f32, pal.border_subtle))
                .rounding(Rounding::same(8.0))
                .inner_margin(Margin::same(6.0))
                .show(ui, |ui| {
                    let img_widget =
                        egui::Image::from_texture((texture.id(), egui::vec2(nat_w, nat_h)))
                            .fit_to_exact_size(egui::vec2(desired_w, desired_h))
                            .rounding(Rounding::same(5.0))
                            .sense(Sense::click());

                    let resp = ui
                        .add(img_widget)
                        .on_hover_text(format!("클릭하여 원본 열기: {}", abs_clone.display()));
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked() {
                        let _ = open::that(&abs_clone);
                    }
                    if !alt.is_empty() && alt != "이미지" && alt != target {
                        ui.label(RichText::new(alt).size(11.5).color(pal.text_muted));
                    }
                });
            ui.add_space(4.0);
        }
        CachedImageState::Failed(msg) => {
            egui::Frame::none()
                .fill(pal.bg_inset)
                .stroke(Stroke::new(1.0_f32, pal.border_subtle))
                .rounding(Rounding::same(6.0))
                .inner_margin(Margin::symmetric(10.0, 6.0))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("🖼️ {msg}"))
                            .size(12.0)
                            .color(pal.text_muted),
                    );
                });
            ui.add_space(4.0);
        }
    }
}

#[derive(Clone, Debug)]
struct SimNode {
    pub dto: GraphNodeDto,
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub radius: f32,
    pub pinned: bool,
}

#[derive(Clone, Debug)]
struct SimEdge {
    pub src_idx: usize,
    pub dst_idx: usize,
}

pub struct ForceGraphWidget {
    nodes: Vec<SimNode>,
    edges: Vec<SimEdge>,
    adj: HashMap<String, HashSet<String>>,
    pub center_path: Option<String>,
    pub compact: bool,
    pub search_query: String,
    pub pan: Vec2,
    pub scale: f32,
    pub alpha: f32,
    dragged_idx: Option<usize>,
    drag_moved: bool,
    last_signature: String,
}

impl ForceGraphWidget {
    pub fn new(compact: bool) -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            adj: HashMap::new(),
            center_path: None,
            compact,
            search_query: String::new(),
            pan: Vec2::ZERO,
            scale: 1.0,
            alpha: 1.0,
            dragged_idx: None,
            drag_moved: false,
            last_signature: String::new(),
        }
    }

    pub fn reset_camera(&mut self) {
        self.pan = Vec2::ZERO;
        self.scale = 1.0;
        self.alpha = self.alpha.max(0.35);
    }

    pub fn set_data(&mut self, data: &GraphDto, center_path: Option<&str>) {
        let sig = format!(
            "{}:{}:{}:{}",
            data.nodes.len(),
            data.edges.len(),
            center_path.unwrap_or(""),
            data.nodes.first().map(|n| n.id.as_str()).unwrap_or("")
        );
        if sig == self.last_signature && !self.nodes.is_empty() {
            return;
        }
        self.last_signature = sig;
        self.center_path = center_path.map(str::to_string);

        let mut prev_pos: HashMap<String, (f32, f32)> = HashMap::new();
        for n in &self.nodes {
            prev_pos.insert(n.dto.id.clone(), (n.x, n.y));
        }

        let count = data.nodes.len().max(1) as f32;
        let spread = (48.0 * count.sqrt()).min(260.0);

        self.nodes = data
            .nodes
            .iter()
            .enumerate()
            .map(|(idx, n)| {
                let (x, y) = if let Some(&(px, py)) = prev_pos.get(&n.id) {
                    (px, py)
                } else {
                    let angle = (idx as f32 / count) * std::f32::consts::TAU;
                    let is_center = center_path.is_some_and(|cp| n.path.as_deref() == Some(cp));
                    let dist = if is_center {
                        0.0
                    } else {
                        spread * (0.35 + 0.65 * (((idx * 7) % 11) as f32 / 11.0))
                    };
                    (angle.cos() * dist, angle.sin() * dist)
                };
                let radius = if n.kind == "tag" {
                    5.5
                } else {
                    (5.5 + ((1 + n.degree) as f32).log2() * 2.4).min(15.0)
                };
                SimNode {
                    dto: n.clone(),
                    x,
                    y,
                    vx: 0.0,
                    vy: 0.0,
                    radius,
                    pinned: false,
                }
            })
            .collect();

        let mut id_to_idx: HashMap<String, usize> = HashMap::new();
        self.adj.clear();
        for (idx, n) in self.nodes.iter().enumerate() {
            id_to_idx.insert(n.dto.id.clone(), idx);
            self.adj.insert(n.dto.id.clone(), HashSet::new());
        }

        self.edges.clear();
        for e in &data.edges {
            if let (Some(&s), Some(&t)) = (id_to_idx.get(&e.source), id_to_idx.get(&e.target)) {
                if s != t {
                    self.edges.push(SimEdge {
                        src_idx: s,
                        dst_idx: t,
                    });
                    if let Some(set) = self.adj.get_mut(&e.source) {
                        set.insert(e.target.clone());
                    }
                    if let Some(set) = self.adj.get_mut(&e.target) {
                        set.insert(e.source.clone());
                    }
                }
            }
        }

        self.alpha = 1.0;
    }

    fn step_physics(&mut self) {
        if self.alpha < 0.004 || self.nodes.is_empty() {
            return;
        }
        let repulsion = if self.compact { 1100.0 } else { 2100.0 };
        let spring_len = if self.compact { 68.0 } else { 105.0 };
        let spring_k = 0.038;
        let gravity = 0.014;

        let len = self.nodes.len();
        for i in 0..len {
            for j in (i + 1)..len {
                let mut dx = self.nodes[j].x - self.nodes[i].x;
                let mut dy = self.nodes[j].y - self.nodes[i].y;
                let mut dist_sq = dx * dx + dy * dy;
                if dist_sq < 1.0 {
                    dx = 1.3;
                    dy = -0.9;
                    dist_sq = 2.5;
                }
                if dist_sq > 160_000.0 {
                    continue;
                }
                let dist = dist_sq.sqrt();
                let force = (repulsion / dist_sq) * self.alpha;
                let fx = (dx / dist) * force;
                let fy = (dy / dist) * force;
                if !self.nodes[i].pinned {
                    self.nodes[i].vx -= fx;
                    self.nodes[i].vy -= fy;
                }
                if !self.nodes[j].pinned {
                    self.nodes[j].vx += fx;
                    self.nodes[j].vy += fy;
                }
            }
        }

        for e in &self.edges {
            let s = e.src_idx;
            let t = e.dst_idx;
            let dx = self.nodes[t].x - self.nodes[s].x;
            let dy = self.nodes[t].y - self.nodes[s].y;
            let dist = dx.hypot(dy).max(1.0);
            let displacement = dist - spring_len;
            let force = displacement * spring_k * self.alpha;
            let fx = (dx / dist) * force;
            let fy = (dy / dist) * force;
            if !self.nodes[s].pinned {
                self.nodes[s].vx += fx;
                self.nodes[s].vy += fy;
            }
            if !self.nodes[t].pinned {
                self.nodes[t].vx -= fx;
                self.nodes[t].vy -= fy;
            }
        }

        for n in &mut self.nodes {
            if n.pinned {
                continue;
            }
            let is_center = self
                .center_path
                .as_deref()
                .is_some_and(|cp| n.dto.path.as_deref() == Some(cp));
            if is_center {
                n.vx += -n.x * 0.08 * self.alpha;
                n.vy += -n.y * 0.08 * self.alpha;
            } else {
                n.vx += -n.x * gravity * self.alpha;
                n.vy += -n.y * gravity * self.alpha;
            }
            n.vx *= 0.82;
            n.vy *= 0.82;
            n.x += n.vx;
            n.y += n.vy;
        }

        self.alpha *= 0.985;
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        pal: &ThemePalette,
        desired_size: Vec2,
    ) -> Option<GraphNodeDto> {
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());
        let painter = ui.painter_at(rect);

        painter.rect_filled(
            rect,
            Rounding::same(8.0),
            if pal.is_dark {
                Color32::from_rgb(24, 24, 28)
            } else {
                Color32::from_rgb(250, 250, 252)
            },
        );
        painter.rect_stroke(
            rect,
            Rounding::same(8.0),
            Stroke::new(1.0_f32, pal.border_subtle),
        );

        if self.nodes.is_empty() {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "표시할 연결 노트가 없습니다",
                FontId::proportional(13.0),
                pal.text_muted,
            );
            return None;
        }

        if self.alpha >= 0.004 {
            self.step_physics();
            ui.ctx().request_repaint();
        }

        if response.hovered() {
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll.abs() > 0.1 {
                let zoom = if scroll > 0.0 { 1.12 } else { 0.89 };
                self.scale = (self.scale * zoom).clamp(0.25, 3.8);
            }
        }

        let center = rect.center() + self.pan;
        let cur_scale = self.scale;
        let to_screen = |wx: f32, wy: f32| -> Pos2 {
            Pos2::new(center.x + wx * cur_scale, center.y + wy * cur_scale)
        };
        let to_world = |sp: Pos2| -> (f32, f32) {
            ((sp.x - center.x) / cur_scale, (sp.y - center.y) / cur_scale)
        };

        let pointer_pos = response
            .hover_pos()
            .or_else(|| ui.input(|i| i.pointer.interact_pos()));
        let mut hovered_idx: Option<usize> = None;
        if let Some(sp) = pointer_pos {
            if rect.contains(sp) {
                let (wx, wy) = to_world(sp);
                for (idx, n) in self.nodes.iter().enumerate().rev() {
                    let hit_r = n.radius + 5.0 / self.scale;
                    let dx = wx - n.x;
                    let dy = wy - n.y;
                    if dx * dx + dy * dy <= hit_r * hit_r {
                        hovered_idx = Some(idx);
                        break;
                    }
                }
            }
        }

        if hovered_idx.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        let mut clicked_node: Option<GraphNodeDto> = None;

        if response.drag_started() {
            self.drag_moved = false;
            if let Some(idx) = hovered_idx {
                self.dragged_idx = Some(idx);
                self.nodes[idx].pinned = true;
                self.alpha = self.alpha.max(0.45);
            }
        }

        if response.dragged() {
            let delta = response.drag_delta();
            if delta.length() > 1.5 {
                self.drag_moved = true;
            }
            if let Some(idx) = self.dragged_idx {
                if let Some(sp) = pointer_pos {
                    let (wx, wy) = to_world(sp);
                    self.nodes[idx].x = wx;
                    self.nodes[idx].y = wy;
                    self.nodes[idx].vx = 0.0;
                    self.nodes[idx].vy = 0.0;
                    self.alpha = self.alpha.max(0.35);
                }
            } else {
                self.pan += delta;
            }
        }

        if response.drag_stopped() {
            if let Some(idx) = self.dragged_idx.take() {
                self.nodes[idx].pinned = false;
                if !self.drag_moved {
                    clicked_node = Some(self.nodes[idx].dto.clone());
                }
            }
        } else if response.clicked() {
            if let Some(idx) = hovered_idx {
                clicked_node = Some(self.nodes[idx].dto.clone());
            }
        }

        let hovered_id = hovered_idx.map(|i| self.nodes[i].dto.id.clone());
        let hovered_neighbors = hovered_id.as_ref().and_then(|id| self.adj.get(id));
        let query_lower = self.search_query.trim().to_lowercase();

        for e in &self.edges {
            let s = &self.nodes[e.src_idx];
            let t = &self.nodes[e.dst_idx];
            let p1 = to_screen(s.x, s.y);
            let p2 = to_screen(t.x, t.y);
            let spotlit = hovered_id
                .as_ref()
                .is_some_and(|hid| &s.dto.id == hid || &t.dto.id == hid);
            let color = if spotlit {
                pal.accent_blue
            } else if hovered_id.is_some() {
                Color32::from_rgba_unmultiplied(128, 128, 140, 22)
            } else {
                Color32::from_rgba_unmultiplied(128, 128, 140, 58)
            };
            let width = if spotlit { 1.8_f32 } else { 1.1_f32 };
            painter.line_segment([p1, p2], Stroke::new(width, color));
        }

        for (idx, n) in self.nodes.iter().enumerate() {
            let pos = to_screen(n.x, n.y);
            let r = (n.radius * self.scale).clamp(3.5, 24.0);
            let is_center = self
                .center_path
                .as_deref()
                .is_some_and(|cp| n.dto.path.as_deref() == Some(cp));
            let is_hovered = hovered_idx == Some(idx);
            let is_neighbor = hovered_neighbors.is_some_and(|set| set.contains(&n.dto.id));
            let matches_query =
                query_lower.is_empty() || n.dto.label.to_lowercase().contains(&query_lower);

            let dimmed = (hovered_idx.is_some() && !is_hovered && !is_neighbor)
                || (!query_lower.is_empty() && !matches_query);

            let mut fill = if n.dto.kind == "tag" {
                pal.ok
            } else if n.dto.kind == "ghost" {
                pal.text_muted
            } else if is_center {
                pal.danger
            } else {
                pal.accent_blue
            };

            if dimmed {
                fill = Color32::from_rgba_unmultiplied(fill.r(), fill.g(), fill.b(), 55);
            }

            if is_center || is_hovered || (!query_lower.is_empty() && matches_query) {
                let halo_color = if is_center {
                    Color32::from_rgba_unmultiplied(224, 62, 62, 55)
                } else {
                    Color32::from_rgba_unmultiplied(35, 131, 226, 60)
                };
                painter.circle_filled(pos, r + 4.0, halo_color);
            }

            painter.circle_filled(pos, r, fill);

            let show_label = is_hovered
                || is_center
                || is_neighbor
                || self.scale > 0.72
                || self.nodes.len() <= 45;
            if show_label && !dimmed {
                let text_color = if is_hovered || is_center {
                    pal.text_primary
                } else {
                    pal.text_secondary
                };
                painter.text(
                    Pos2::new(pos.x, pos.y + r + 9.0),
                    Align2::CENTER_CENTER,
                    &n.dto.label,
                    FontId::proportional(11.5),
                    text_color,
                );
            }
        }

        clicked_node
    }
}

/// 마크다운 문서를 Notion 스타일 블록/인라인 + 실제 볼트 이미지로 렌더링하고 클릭된 `[[위키링크]]`를 반환합니다.
pub fn render_markdown_preview(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    markdown: &str,
    vault_root: Option<&Path>,
    img_cache: &mut ImageTextureCache,
) -> Option<String> {
    let mut clicked_wikilink: Option<String> = None;
    let lines: Vec<&str> = markdown.lines().collect();
    let mut i = 0usize;

    while i < lines.len() {
        let raw = lines[i];
        let trimmed = raw.trim();

        if trimmed.is_empty() {
            ui.add_space(4.0);
            i += 1;
            continue;
        }

        // 코드 블록 (```)
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let lang = trimmed.trim_start_matches(['`', '~']).trim();
            let mut code_lines = Vec::new();
            i += 1;
            while i < lines.len() {
                let l = lines[i];
                if l.trim().starts_with("```") || l.trim().starts_with("~~~") {
                    i += 1;
                    break;
                }
                code_lines.push(l);
                i += 1;
            }
            let code_text = code_lines.join("\n");
            egui::Frame::none()
                .fill(pal.code_bg)
                .stroke(Stroke::new(1.0_f32, pal.border_subtle))
                .rounding(Rounding::same(7.0))
                .inner_margin(Margin::same(10.0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if !lang.is_empty() {
                        ui.label(
                            RichText::new(lang.to_uppercase())
                                .size(10.5)
                                .color(pal.text_muted)
                                .strong(),
                        );
                    }
                    ui.label(
                        RichText::new(code_text)
                            .monospace()
                            .size(12.5)
                            .color(pal.text_primary),
                    );
                });
            ui.add_space(4.0);
            continue;
        }

        // 단독 이미지 블록 (`![alt](path)` 또는 `![[image.png]]`)
        if let Some((target, alt, custom_w)) = parse_standalone_image_line(trimmed) {
            render_vault_image_block(ui, pal, vault_root, &target, &alt, custom_w, img_cache);
            i += 1;
            continue;
        }

        // 구분선 (--- 또는 ***)
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            ui.separator();
            i += 1;
            continue;
        }

        // 콜아웃 또는 인용문 (`> ...`)
        if let Some(first_quote) = trimmed.strip_prefix('>') {
            let mut quote_lines = vec![first_quote.trim_start()];
            i += 1;
            while i < lines.len() {
                if let Some(q) = lines[i].trim().strip_prefix('>') {
                    quote_lines.push(q.trim_start());
                    i += 1;
                } else {
                    break;
                }
            }
            let first = quote_lines[0];
            let callout_header = if first.starts_with("[!") {
                first.find(']').map(|end| {
                    let kind = first[2..end].to_lowercase();
                    let title = first[end + 1..].trim();
                    (kind, title)
                })
            } else {
                None
            };

            if let Some((kind, title)) = callout_header {
                let (icon, accent, bg) = match kind.as_str() {
                    "tip" | "success" | "check" => ("💡", pal.ok, pal.ok_bg),
                    "warning" | "caution" | "important" => ("⚠️", pal.warn, pal.warn_bg),
                    "danger" | "error" => ("🔥", pal.danger, pal.danger_bg),
                    _ => ("📝", pal.accent_blue, pal.accent_blue_soft),
                };
                egui::Frame::none()
                    .fill(bg)
                    .stroke(Stroke::new(1.0_f32, accent))
                    .rounding(Rounding::same(8.0))
                    .inner_margin(Margin::same(11.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let header_txt = if title.is_empty() {
                            format!("{icon} {}", kind.to_uppercase())
                        } else {
                            format!("{icon} {title}")
                        };
                        ui.label(RichText::new(header_txt).strong().color(accent));
                        for ql in quote_lines.iter().skip(1) {
                            if let Some(w) =
                                render_inline_line(ui, pal, ql, 13.5, false, vault_root, img_cache)
                            {
                                clicked_wikilink = Some(w);
                            }
                        }
                    });
            } else {
                egui::Frame::none()
                    .fill(pal.bg_inset)
                    .stroke(Stroke::new(1.5_f32, pal.accent_purple))
                    .rounding(Rounding::same(6.0))
                    .inner_margin(Margin::symmetric(12.0, 8.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        for ql in &quote_lines {
                            if let Some(w) =
                                render_inline_line(ui, pal, ql, 13.5, false, vault_root, img_cache)
                            {
                                clicked_wikilink = Some(w);
                            }
                        }
                    });
            }
            ui.add_space(4.0);
            continue;
        }

        // 마크다운 표 (`| col1 | col2 |`)
        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            let mut table_rows: Vec<Vec<String>> = Vec::new();
            while i < lines.len() {
                let tl = lines[i].trim();
                if tl.starts_with('|') && tl.ends_with('|') {
                    let cells: Vec<String> = tl[1..tl.len() - 1]
                        .split('|')
                        .map(|c| c.trim().to_string())
                        .collect();
                    let is_divider = cells.iter().all(|c| {
                        !c.is_empty() && c.chars().all(|ch| matches!(ch, '-' | ':' | ' '))
                    });
                    if !is_divider {
                        table_rows.push(cells);
                    }
                    i += 1;
                } else {
                    break;
                }
            }
            if !table_rows.is_empty() {
                egui::Frame::none()
                    .fill(pal.bg_card)
                    .stroke(Stroke::new(1.0_f32, pal.border_subtle))
                    .rounding(Rounding::same(6.0))
                    .inner_margin(Margin::same(8.0))
                    .show(ui, |ui| {
                        egui::Grid::new(ui.next_auto_id().with("md_table"))
                            .striped(true)
                            .spacing([14.0, 6.0])
                            .show(ui, |ui| {
                                for (r_idx, row) in table_rows.iter().enumerate() {
                                    for cell in row {
                                        if let Some(w) = render_inline_line(
                                            ui,
                                            pal,
                                            cell,
                                            13.0,
                                            r_idx == 0,
                                            vault_root,
                                            img_cache,
                                        ) {
                                            clicked_wikilink = Some(w);
                                        }
                                    }
                                    ui.end_row();
                                }
                            });
                    });
                ui.add_space(4.0);
            }
            continue;
        }

        // 제목 (H1~H6)
        let hashes = trimmed.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
            let heading_text = trimmed[hashes..].trim();
            let size = match hashes {
                1 => 23.0,
                2 => 19.0,
                3 => 16.5,
                _ => 15.0,
            };
            ui.add_space(4.0);
            if let Some(w) =
                render_inline_line(ui, pal, heading_text, size, true, vault_root, img_cache)
            {
                clicked_wikilink = Some(w);
            }
            if hashes <= 2 {
                ui.separator();
            }
            i += 1;
            continue;
        }

        // 체크리스트 (`- [ ]` / `- [x]`)
        if let Some(rest) = trimmed
            .strip_prefix("- [ ] ")
            .or_else(|| trimmed.strip_prefix("* [ ] "))
        {
            ui.horizontal_wrapped(|ui| {
                let mut dummy = false;
                ui.disable();
                ui.checkbox(&mut dummy, "");
                if let Some(w) =
                    render_inline_line(ui, pal, rest, 14.0, false, vault_root, img_cache)
                {
                    clicked_wikilink = Some(w);
                }
            });
            i += 1;
            continue;
        }
        if let Some(rest) = trimmed
            .strip_prefix("- [x] ")
            .or_else(|| trimmed.strip_prefix("- [X] "))
            .or_else(|| trimmed.strip_prefix("* [x] "))
        {
            ui.horizontal_wrapped(|ui| {
                let mut dummy = true;
                ui.disable();
                ui.checkbox(&mut dummy, "");
                if let Some(w) =
                    render_inline_line(ui, pal, rest, 14.0, false, vault_root, img_cache)
                {
                    clicked_wikilink = Some(w);
                }
            });
            i += 1;
            continue;
        }

        // 불릿 리스트 (`- ` / `* `)
        if let Some(rest) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("•").color(pal.accent_purple).strong());
                if let Some(w) =
                    render_inline_line(ui, pal, rest, 14.0, false, vault_root, img_cache)
                {
                    clicked_wikilink = Some(w);
                }
            });
            i += 1;
            continue;
        }

        // 일반 문단
        if let Some(w) = render_inline_line(ui, pal, trimmed, 14.0, false, vault_root, img_cache) {
            clicked_wikilink = Some(w);
        }
        i += 1;
    }

    clicked_wikilink
}

fn render_inline_line(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    raw_line: &str,
    font_size: f32,
    bold_default: bool,
    vault_root: Option<&Path>,
    img_cache: &mut ImageTextureCache,
) -> Option<String> {
    let cleaned_cow;
    let line = if raw_line.contains('<') && raw_line.contains('>') {
        cleaned_cow = win_clipboard::strip_html_wrappers_preserving_images(raw_line);
        cleaned_cow.as_str()
    } else {
        raw_line
    };

    let mut clicked: Option<String> = None;
    let mut deferred_images: Vec<(String, String, Option<f32>)> = Vec::new();

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        let mut rest = line;

        while !rest.is_empty() {
            let next_embed = rest.find("![[");
            let next_md_img = rest.find("![");
            let next_wiki = rest.find("[[");
            let next_mark = rest.find("==");
            let next_code = rest.find('`');
            let next_bold = rest.find("**");

            let earliest = [
                next_embed.map(|p| (p, "embed_img")),
                next_md_img
                    .filter(|&p| next_embed != Some(p))
                    .map(|p| (p, "md_img")),
                next_wiki
                    .filter(|&p| next_embed.map(|e| e + 1) != Some(p))
                    .map(|p| (p, "wiki")),
                next_mark.map(|p| (p, "mark")),
                next_code.map(|p| (p, "code")),
                next_bold.map(|p| (p, "bold")),
            ]
            .into_iter()
            .flatten()
            .min_by_key(|(pos, _)| *pos);

            let Some((pos, kind)) = earliest else {
                let mut rt = RichText::new(rest).size(font_size).color(pal.text_primary);
                if bold_default {
                    rt = rt.strong();
                }
                ui.label(rt);
                break;
            };

            if pos > 0 {
                let prefix = &rest[..pos];
                let mut rt = RichText::new(prefix)
                    .size(font_size)
                    .color(pal.text_primary);
                if bold_default {
                    rt = rt.strong();
                }
                ui.label(rt);
                rest = &rest[pos..];
            }

            match kind {
                "embed_img" => {
                    if let Some(end) = rest[3..].find("]]") {
                        let inner = &rest[3..3 + end];
                        let full = format!("![[{inner}]]");
                        if let Some((target, alt, cw)) = parse_standalone_image_line(&full) {
                            deferred_images.push((target, alt, cw));
                        } else {
                            let target = inner.split('|').next().unwrap_or(inner).trim();
                            if ui
                                .button(
                                    RichText::new(format!("📎 {target}"))
                                        .size(font_size - 0.5)
                                        .color(pal.accent_blue),
                                )
                                .clicked()
                            {
                                clicked = Some(target.to_string());
                            }
                        }
                        rest = &rest[3 + end + 2..];
                    } else {
                        ui.label(RichText::new("![[").size(font_size));
                        rest = &rest[3..];
                    }
                }
                "md_img" => {
                    if let Some(mid) = rest[2..].find("](") {
                        let after_mid = &rest[2 + mid + 2..];
                        if let Some(end_paren) = after_mid.find(')') {
                            let alt = rest[2..2 + mid].trim().to_string();
                            let target = after_mid[..end_paren].trim().to_string();
                            deferred_images.push((target, alt, None));
                            rest = &after_mid[end_paren + 1..];
                        } else {
                            ui.label(RichText::new("![").size(font_size));
                            rest = &rest[2..];
                        }
                    } else {
                        ui.label(RichText::new("![").size(font_size));
                        rest = &rest[2..];
                    }
                }
                "wiki" => {
                    if let Some(end) = rest[2..].find("]]") {
                        let inner = &rest[2..2 + end];
                        let (target, display) = match inner.split_once('|') {
                            Some((t, a)) => (t.trim(), a.trim()),
                            None => (inner.trim(), inner.trim()),
                        };
                        let btn = egui::Button::new(
                            RichText::new(format!("📄 {display}"))
                                .size(font_size - 0.5)
                                .color(pal.accent_blue)
                                .strong(),
                        )
                        .fill(pal.accent_blue_soft)
                        .stroke(Stroke::NONE)
                        .rounding(Rounding::same(5.0));
                        if ui
                            .add(btn)
                            .on_hover_text(format!("노트 열기: {target}"))
                            .clicked()
                        {
                            clicked = Some(target.to_string());
                        }
                        rest = &rest[2 + end + 2..];
                    } else {
                        ui.label(RichText::new("[[").size(font_size));
                        rest = &rest[2..];
                    }
                }
                "mark" => {
                    if let Some(end) = rest[2..].find("==") {
                        let inner = &rest[2..2 + end];
                        egui::Frame::none()
                            .fill(Color32::from_rgba_unmultiplied(250, 204, 21, 95))
                            .rounding(Rounding::same(4.0))
                            .inner_margin(Margin::symmetric(4.0, 1.0))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(inner)
                                        .size(font_size)
                                        .color(pal.text_primary)
                                        .strong(),
                                );
                            });
                        rest = &rest[2 + end + 2..];
                    } else {
                        ui.label(RichText::new("==").size(font_size));
                        rest = &rest[2..];
                    }
                }
                "code" => {
                    if let Some(end) = rest[1..].find('`') {
                        let inner = &rest[1..1 + end];
                        egui::Frame::none()
                            .fill(pal.code_bg)
                            .rounding(Rounding::same(4.0))
                            .inner_margin(Margin::symmetric(5.0, 1.0))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(inner)
                                        .monospace()
                                        .size(font_size - 1.0)
                                        .color(pal.accent_purple),
                                );
                            });
                        rest = &rest[1 + end + 1..];
                    } else {
                        ui.label(RichText::new("`").size(font_size));
                        rest = &rest[1..];
                    }
                }
                "bold" => {
                    if let Some(end) = rest[2..].find("**") {
                        let inner = &rest[2..2 + end];
                        ui.label(
                            RichText::new(inner)
                                .size(font_size)
                                .strong()
                                .color(pal.text_primary),
                        );
                        rest = &rest[2 + end + 2..];
                    } else {
                        ui.label(RichText::new("**").size(font_size));
                        rest = &rest[2..];
                    }
                }
                _ => break,
            }
        }
    });

    for (target, alt, cw) in deferred_images {
        render_vault_image_block(ui, pal, vault_root, &target, &alt, cw, img_cache);
    }

    clicked
}
