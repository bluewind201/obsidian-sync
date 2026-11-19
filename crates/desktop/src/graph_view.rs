//! 네이티브 Rust(`egui`) 기반 Notion 마크다운 렌더러(bordered table · 벡터 플로우차트 · 다단 · 콜아웃 · 이미지)
//! 및 의미론적 3종 레이아웃(클러스터 / 방사형 / 계층) 지식 그래프 위젯.

use crate::flowchart::{self, DiagramCardAction};
use crate::ui::ThemePalette;
use crate::vault::{self, GraphDto, GraphNodeDto};
use crate::win_clipboard;
use eframe::egui::{
    self, Align2, Color32, ColorImage, FontId, Margin, Pos2, Rect, RichText, Rounding, Sense,
    Shape, Stroke, TextureHandle, TextureOptions, Vec2,
};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
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

pub fn normalize_image_target(raw: &str) -> String {
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
pub fn parse_standalone_image_line(trimmed: &str) -> Option<(String, String, Option<f32>)> {
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

pub fn render_vault_image_block(
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
            .rounding(Rounding::same(6.0))
            .inner_margin(Margin::symmetric(10.0, 6.0))
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
                .size(12.0)
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
                .inner_margin(Margin::same(5.0))
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
                        ui.label(RichText::new(alt).size(11.0).color(pal.text_muted));
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
                            .size(11.5)
                            .color(pal.text_muted),
                    );
                });
            ui.add_space(4.0);
        }
    }
}

// ───────────────────────── 의미론적 3종 레이아웃 지식 그래프 ─────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GraphLayoutMode {
    Cluster,
    Radial,
    Hierarchy,
}

impl GraphLayoutMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cluster => "🧩 폴더 클러스터",
            Self::Radial => "🎯 방사형 중심",
            Self::Hierarchy => "🪜 계층 링크",
        }
    }
}

const CLUSTER_PALETTE: &[Color32] = &[
    Color32::from_rgb(37, 99, 235),
    Color32::from_rgb(124, 58, 237),
    Color32::from_rgb(8, 145, 178),
    Color32::from_rgb(217, 119, 6),
    Color32::from_rgb(5, 150, 105),
    Color32::from_rgb(219, 39, 119),
    Color32::from_rgb(79, 70, 229),
    Color32::from_rgb(13, 148, 136),
];

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct SimNode {
    pub dto: GraphNodeDto,
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub radius: f32,
    pub pinned: bool,
    pub hop: usize,
    pub rank: usize,
}

#[derive(Clone, Debug)]
struct SimEdge {
    pub src_idx: usize,
    pub dst_idx: usize,
}

pub struct ForceGraphWidget {
    all_nodes: Vec<GraphNodeDto>,
    all_edges: Vec<(String, String)>,
    nodes: Vec<SimNode>,
    edges: Vec<SimEdge>,
    adj: HashMap<String, HashSet<String>>,
    folder_colors: HashMap<String, Color32>,
    folder_centroids: Vec<(String, f32, f32)>,
    pub center_path: Option<String>,
    pub compact: bool,
    pub layout_mode: GraphLayoutMode,
    pub hide_orphans: bool,
    pub folder_filter: String,
    pub search_query: String,
    pub selected_id: Option<String>,
    pub pan: Vec2,
    pub scale: f32,
    pub alpha: f32,
    dragged_idx: Option<usize>,
    drag_moved: bool,
    last_signature: String,
    last_canvas_size: Vec2,
}

impl ForceGraphWidget {
    pub fn new(compact: bool) -> Self {
        Self {
            all_nodes: Vec::new(),
            all_edges: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            adj: HashMap::new(),
            folder_colors: HashMap::new(),
            folder_centroids: Vec::new(),
            center_path: None,
            compact,
            layout_mode: GraphLayoutMode::Cluster,
            hide_orphans: false,
            folder_filter: String::new(),
            search_query: String::new(),
            selected_id: None,
            pan: Vec2::ZERO,
            scale: 1.0,
            alpha: 1.0,
            dragged_idx: None,
            drag_moved: false,
            last_signature: String::new(),
            last_canvas_size: Vec2::new(640.0, 420.0),
        }
    }

    pub fn set_layout_mode(&mut self, mode: GraphLayoutMode) {
        if self.layout_mode != mode {
            self.layout_mode = mode;
            self.rebuild_visible_and_positions(false);
            self.fit_to_screen();
        }
    }

    pub fn set_hide_orphans(&mut self, hide: bool) {
        if self.hide_orphans != hide {
            self.hide_orphans = hide;
            self.rebuild_visible_and_positions(false);
            self.fit_to_screen();
        }
    }

    pub fn set_folder_filter(&mut self, folder: String) {
        if self.folder_filter != folder {
            self.folder_filter = folder;
            self.rebuild_visible_and_positions(false);
            self.fit_to_screen();
        }
    }

    pub fn available_folders(&self) -> Vec<String> {
        let mut set: Vec<String> = self
            .all_nodes
            .iter()
            .filter(|n| n.kind == "note")
            .map(|n| {
                if n.folder.is_empty() {
                    "(루트)".to_string()
                } else {
                    n.folder.clone()
                }
            })
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        set.sort();
        set
    }

    pub fn reset_camera(&mut self) {
        self.fit_to_screen();
        self.alpha = self.alpha.max(0.25);
    }

    pub fn fit_to_screen(&mut self) {
        if self.nodes.is_empty() {
            self.pan = Vec2::ZERO;
            self.scale = 1.0;
            return;
        }
        let w = self.last_canvas_size.x.max(240.0);
        let h = self.last_canvas_size.y.max(200.0);
        let right_reserve = if !self.compact && self.selected_id.is_some() && w > 720.0 {
            270.0
        } else {
            0.0
        };
        let usable_w = (w - right_reserve - 72.0).max(160.0);
        let usable_h = (h - 72.0).max(140.0);

        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for n in &self.nodes {
            min_x = min_x.min(n.x - 42.0);
            max_x = max_x.max(n.x + 42.0);
            min_y = min_y.min(n.y - 28.0);
            max_y = max_y.max(n.y + 28.0);
        }
        let span_w = (max_x - min_x).max(140.0);
        let span_h = (max_y - min_y).max(120.0);
        let next_scale = (usable_w / span_w).min(usable_h / span_h).clamp(0.32, 1.45);
        let mid_x = (min_x + max_x) * 0.5;
        let mid_y = (min_y + max_y) * 0.5;

        self.scale = next_scale;
        self.pan = Vec2::new(
            -mid_x * next_scale - right_reserve * 0.35,
            -mid_y * next_scale,
        );
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
        self.all_nodes = data.nodes.clone();
        self.all_edges = data
            .edges
            .iter()
            .map(|e| (e.source.clone(), e.target.clone()))
            .collect();

        self.folder_colors.clear();
        let mut groups: Vec<String> = self
            .all_nodes
            .iter()
            .filter(|n| n.kind == "note")
            .map(|n| {
                if !n.folder.is_empty() {
                    n.folder.clone()
                } else if !n.group.is_empty() {
                    n.group.clone()
                } else {
                    "root".to_string()
                }
            })
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        groups.sort();
        for (idx, g) in groups.into_iter().enumerate() {
            self.folder_colors
                .insert(g, CLUSTER_PALETTE[idx % CLUSTER_PALETTE.length_or_len()]);
        }

        self.rebuild_visible_and_positions(true);
        self.fit_to_screen();
    }

    fn rebuild_visible_and_positions(&mut self, preserve_prev: bool) {
        let prev_pos: HashMap<String, (f32, f32)> = if preserve_prev {
            self.nodes
                .iter()
                .map(|n| (n.dto.id.clone(), (n.x, n.y)))
                .collect()
        } else {
            HashMap::new()
        };

        let mut visible: Vec<GraphNodeDto> = self.all_nodes.clone();
        if !self.folder_filter.is_empty() {
            let ff = self.folder_filter.as_str();
            let mut matched_ids: HashSet<String> = HashSet::new();
            for n in &visible {
                if n.kind != "note" {
                    continue;
                }
                let nf = n.folder.as_str();
                let ok = if ff == "(루트)" || ff == "__root__" {
                    nf.is_empty()
                } else {
                    nf == ff || nf.starts_with(&format!("{ff}/"))
                };
                if ok {
                    matched_ids.insert(n.id.clone());
                }
            }
            visible.retain(|n| matched_ids.contains(&n.id));
        }

        if self.hide_orphans {
            let mut connected: HashSet<String> = HashSet::new();
            let vis_ids: HashSet<&str> = visible.iter().map(|n| n.id.as_str()).collect();
            for (s, t) in &self.all_edges {
                if vis_ids.contains(s.as_str()) && vis_ids.contains(t.as_str()) {
                    connected.insert(s.clone());
                    connected.insert(t.clone());
                }
            }
            visible.retain(|n| connected.contains(&n.id));
        }

        let vis_ids: HashSet<&str> = visible.iter().map(|n| n.id.as_str()).collect();
        let vis_edges: Vec<(String, String)> = self
            .all_edges
            .iter()
            .filter(|(s, t)| vis_ids.contains(s.as_str()) && vis_ids.contains(t.as_str()))
            .cloned()
            .collect();

        // Cluster centroids
        let mut clusters: Vec<String> = visible
            .iter()
            .map(|n| Self::cluster_key_of(n))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        clusters.sort();
        let cluster_ring_r = if self.compact {
            110.0
        } else {
            (150.0 + clusters.len() as f32 * 18.0).min(300.0)
        };
        let mut centroid_map: HashMap<String, (f32, f32)> = HashMap::new();
        self.folder_centroids.clear();
        let c_len = clusters.len().max(1) as f32;
        for (idx, c) in clusters.iter().enumerate() {
            let (cx, cy) = if clusters.len() == 1 {
                (0.0, 0.0)
            } else {
                let ang =
                    (idx as f32 / c_len) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                (ang.cos() * cluster_ring_r, ang.sin() * cluster_ring_r)
            };
            centroid_map.insert(c.clone(), (cx, cy));
            self.folder_centroids.push((c.clone(), cx, cy));
        }

        // BFS hops from center or max-degree hub
        let center_id = self
            .center_path
            .as_deref()
            .and_then(|cp| visible.iter().find(|n| n.path.as_deref() == Some(cp)))
            .or_else(|| visible.iter().max_by_key(|n| n.degree))
            .map(|n| n.id.clone());
        let mut adj_tmp: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut out_adj: HashMap<&str, Vec<&str>> = HashMap::new();
        for (s, t) in &vis_edges {
            adj_tmp.entry(s.as_str()).or_default().push(t.as_str());
            adj_tmp.entry(t.as_str()).or_default().push(s.as_str());
            out_adj.entry(s.as_str()).or_default().push(t.as_str());
        }
        let mut hops: HashMap<String, usize> = HashMap::new();
        if let Some(cid) = &center_id {
            let mut q = VecDeque::new();
            hops.insert(cid.clone(), 0);
            q.push_back(cid.as_str());
            while let Some(u) = q.pop_front() {
                let d = hops[u];
                if let Some(nbs) = adj_tmp.get(u) {
                    for &v in nbs {
                        if !hops.contains_key(v) {
                            hops.insert(v.to_string(), d + 1);
                            q.push_back(v);
                        }
                    }
                }
            }
        }

        // Hierarchy ranks
        let mut in_deg: HashMap<&str, usize> = visible.iter().map(|n| (n.id.as_str(), 0)).collect();
        for (_, t) in &vis_edges {
            if let Some(v) = in_deg.get_mut(t.as_str()) {
                *v += 1;
            }
        }
        let mut ranks: HashMap<String, usize> = visible.iter().map(|n| (n.id.clone(), 0)).collect();
        for _ in 0..visible.len().min(12) {
            for (s, t) in &vis_edges {
                let rs = ranks.get(s.as_str()).copied().unwrap_or(0);
                let rt = ranks.get(t.as_str()).copied().unwrap_or(0);
                if rt < rs + 1 && rs < 8 {
                    ranks.insert(t.clone(), rs + 1);
                }
            }
        }

        let mut hop_groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        let mut rank_groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();

        self.nodes = visible
            .into_iter()
            .enumerate()
            .map(|(idx, n)| {
                let hop = hops.get(&n.id).copied().unwrap_or(3);
                let rank = ranks.get(&n.id).copied().unwrap_or(0);
                hop_groups.entry(hop).or_default().push(idx);
                rank_groups.entry(rank).or_default().push(idx);

                let (x, y) = if let Some(&(px, py)) = prev_pos.get(&n.id) {
                    (px, py)
                } else {
                    let ckey = Self::cluster_key_of(&n);
                    let (cx, cy) = centroid_map.get(&ckey).copied().unwrap_or((0.0, 0.0));
                    let a = (idx as f32 * 2.39996) % std::f32::consts::TAU;
                    let jr = 24.0 + (idx % 5) as f32 * 14.0;
                    (cx + a.cos() * jr, cy + a.sin() * jr)
                };

                let radius = if n.kind == "tag" {
                    5.5
                } else if n.kind == "ghost" {
                    6.0
                } else {
                    (6.5 + ((1 + n.degree) as f32).sqrt() * 2.2).min(16.0)
                };

                SimNode {
                    dto: n,
                    x,
                    y,
                    vx: 0.0,
                    vy: 0.0,
                    radius,
                    pinned: false,
                    hop,
                    rank,
                }
            })
            .collect();

        match self.layout_mode {
            GraphLayoutMode::Radial => {
                for (hop, list) in hop_groups {
                    let ring_r = if hop == 0 {
                        0.0
                    } else {
                        (hop as f32) * if self.compact { 68.0 } else { 105.0 }
                    };
                    let cnt = list.len().max(1) as f32;
                    for (k, &ni) in list.iter().enumerate() {
                        if hop == 0 && list.len() == 1 {
                            self.nodes[ni].x = 0.0;
                            self.nodes[ni].y = 0.0;
                        } else {
                            let ang = (k as f32 / cnt) * std::f32::consts::TAU;
                            self.nodes[ni].x = ang.cos() * ring_r;
                            self.nodes[ni].y = ang.sin() * ring_r;
                        }
                    }
                }
            }
            GraphLayoutMode::Hierarchy => {
                let row_h = if self.compact { 62.0 } else { 96.0 };
                let col_w = if self.compact { 78.0 } else { 112.0 };
                let total_ranks = rank_groups.len().max(1) as f32;
                let y_off = -(total_ranks - 1.0) * row_h * 0.5;
                for (r_idx, (_, list)) in rank_groups.into_iter().enumerate() {
                    let y = y_off + r_idx as f32 * row_h;
                    let span = (list.len().saturating_sub(1) as f32) * col_w;
                    for (k, &ni) in list.iter().enumerate() {
                        self.nodes[ni].x = -span * 0.5 + k as f32 * col_w;
                        self.nodes[ni].y = y;
                    }
                }
            }
            GraphLayoutMode::Cluster => {}
        }

        let id_to_idx: HashMap<String, usize> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.dto.id.clone(), i))
            .collect();
        self.adj = self
            .nodes
            .iter()
            .map(|n| (n.dto.id.clone(), HashSet::new()))
            .collect();
        self.edges.clear();
        for (s, t) in vis_edges {
            if let (Some(&si), Some(&ti)) = (id_to_idx.get(&s), id_to_idx.get(&t)) {
                if si != ti {
                    self.edges.push(SimEdge {
                        src_idx: si,
                        dst_idx: ti,
                    });
                    if let Some(set) = self.adj.get_mut(&s) {
                        set.insert(t.clone());
                    }
                    if let Some(set) = self.adj.get_mut(&t) {
                        set.insert(s.clone());
                    }
                }
            }
        }

        self.alpha = match self.layout_mode {
            GraphLayoutMode::Cluster => 1.0,
            GraphLayoutMode::Radial | GraphLayoutMode::Hierarchy => 0.16,
        };
    }

    fn cluster_key_of(n: &GraphNodeDto) -> String {
        if n.kind == "tag" {
            "태그".into()
        } else if n.kind == "ghost" {
            "미생성 링크".into()
        } else if !n.folder.is_empty() {
            n.folder.split('/').next().unwrap_or(&n.folder).to_string()
        } else if !n.group.is_empty() {
            n.group.clone()
        } else {
            "root".into()
        }
    }

    fn color_for_node(&self, n: &GraphNodeDto, pal: &ThemePalette) -> Color32 {
        if n.kind == "tag" {
            return pal.ok;
        }
        if n.kind == "ghost" {
            return pal.text_muted;
        }
        if let Some(c) = self.folder_colors.get(&n.folder) {
            return *c;
        }
        let key = Self::cluster_key_of(n);
        if let Some(c) = self.folder_colors.get(&key) {
            return *c;
        }
        pal.accent_blue
    }

    fn step_physics(&mut self) {
        if self.alpha < 0.004 || self.nodes.is_empty() {
            return;
        }
        let repulsion = if self.compact { 1050.0 } else { 1950.0 };
        let spring_len = if self.compact { 68.0 } else { 104.0 };
        let spring_k = 0.036;

        let len = self.nodes.len();
        for i in 0..len {
            for j in (i + 1)..len {
                let mut dx = self.nodes[j].x - self.nodes[i].x;
                let mut dy = self.nodes[j].y - self.nodes[i].y;
                let mut dist_sq = dx * dx + dy * dy;
                if dist_sq < 1.0 {
                    dx = 1.4;
                    dy = -0.9;
                    dist_sq = 2.8;
                }
                if dist_sq > 140_000.0 {
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

        if self.layout_mode == GraphLayoutMode::Cluster {
            let centroid_lookup: HashMap<&str, (f32, f32)> = self
                .folder_centroids
                .iter()
                .map(|(k, x, y)| (k.as_str(), (*x, *y)))
                .collect();

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
                let ckey = Self::cluster_key_of(&n.dto);
                let (cx, cy) = centroid_lookup
                    .get(ckey.as_str())
                    .copied()
                    .unwrap_or((0.0, 0.0));
                n.vx += (cx - n.x) * 0.045 * self.alpha;
                n.vy += (cy - n.y) * 0.045 * self.alpha;
                n.vx *= 0.80;
                n.vy *= 0.80;
                n.x += n.vx;
                n.y += n.vy;
            }
        } else {
            for n in &mut self.nodes {
                if !n.pinned {
                    n.vx *= 0.72;
                    n.vy *= 0.72;
                    n.x += n.vx * 0.35;
                    n.y += n.vy * 0.35;
                }
            }
        }

        self.alpha *= 0.982;
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        pal: &ThemePalette,
        desired_size: Vec2,
    ) -> Option<GraphNodeDto> {
        self.last_canvas_size = desired_size;
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());
        let painter = ui.painter_at(rect);

        painter.rect_filled(
            rect,
            Rounding::same(8.0),
            if pal.is_dark {
                Color32::from_rgb(24, 24, 24)
            } else {
                Color32::from_rgb(251, 251, 250)
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

        let mut open_node: Option<GraphNodeDto> = None;

        if response.drag_started() {
            self.drag_moved = false;
            if let Some(idx) = hovered_idx {
                self.dragged_idx = Some(idx);
                self.nodes[idx].pinned = true;
                self.alpha = self.alpha.max(0.42);
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
                    self.alpha = self.alpha.max(0.3);
                }
            } else {
                self.pan += delta;
            }
        }

        if response.drag_stopped() {
            if let Some(idx) = self.dragged_idx.take() {
                self.nodes[idx].pinned = false;
                if !self.drag_moved {
                    let dto = self.nodes[idx].dto.clone();
                    if self.compact || self.selected_id.as_deref() == Some(dto.id.as_str()) {
                        open_node = Some(dto);
                    } else {
                        self.selected_id = Some(dto.id);
                    }
                }
            }
        } else if response.double_clicked() {
            if let Some(idx) = hovered_idx {
                open_node = Some(self.nodes[idx].dto.clone());
            }
        } else if response.clicked() {
            if let Some(idx) = hovered_idx {
                let dto = self.nodes[idx].dto.clone();
                if self.compact || self.selected_id.as_deref() == Some(dto.id.as_str()) {
                    open_node = Some(dto);
                } else {
                    self.selected_id = Some(dto.id);
                }
            } else if !self.compact {
                self.selected_id = None;
            }
        }

        // Background Cluster Halos or Radial Rings
        if !self.compact && self.layout_mode == GraphLayoutMode::Cluster {
            let mut by_cluster: HashMap<String, Vec<usize>> = HashMap::new();
            for (i, n) in self.nodes.iter().enumerate() {
                if n.dto.kind == "note" {
                    by_cluster
                        .entry(Self::cluster_key_of(&n.dto))
                        .or_default()
                        .push(i);
                }
            }
            for (cname, idxs) in by_cluster {
                if idxs.len() < 2 {
                    continue;
                }
                let avg_x = idxs.iter().map(|&i| self.nodes[i].x).sum::<f32>() / idxs.len() as f32;
                let avg_y = idxs.iter().map(|&i| self.nodes[i].y).sum::<f32>() / idxs.len() as f32;
                let max_d = idxs
                    .iter()
                    .map(|&i| (self.nodes[i].x - avg_x).hypot(self.nodes[i].y - avg_y))
                    .fold(26.0f32, f32::max);
                let sc = to_screen(avg_x, avg_y);
                let sr = (max_d + 28.0) * cur_scale;
                let col = self
                    .folder_colors
                    .get(&cname)
                    .copied()
                    .unwrap_or(pal.accent_blue);
                painter.circle_filled(
                    sc,
                    sr,
                    Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 16),
                );
                painter.circle_stroke(
                    sc,
                    sr,
                    Stroke::new(
                        1.0_f32,
                        Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 45),
                    ),
                );
                if cname != "root" {
                    painter.text(
                        Pos2::new(sc.x, sc.y - sr + 10.0),
                        Align2::CENTER_CENTER,
                        format!("📁 {cname}"),
                        FontId::proportional(10.5),
                        Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 190),
                    );
                }
            }
        } else if !self.compact && self.layout_mode == GraphLayoutMode::Radial {
            for hop in 1..=3 {
                let rad = (hop as f32 * 105.0) * cur_scale;
                painter.circle_stroke(
                    to_screen(0.0, 0.0),
                    rad,
                    Stroke::new(1.0_f32, pal.border_subtle),
                );
            }
        }

        let spotlight_id = hovered_idx
            .map(|i| self.nodes[i].dto.id.clone())
            .or_else(|| self.selected_id.clone());
        let spotlight_neighbors = spotlight_id.as_ref().and_then(|id| self.adj.get(id));
        let query_lower = self.search_query.trim().to_lowercase();

        for e in &self.edges {
            let s = &self.nodes[e.src_idx];
            let t = &self.nodes[e.dst_idx];
            let p1 = to_screen(s.x, s.y);
            let p2 = to_screen(t.x, t.y);
            let spotlit = spotlight_id
                .as_ref()
                .is_some_and(|hid| &s.dto.id == hid || &t.dto.id == hid);
            let color = if spotlit {
                pal.accent_blue
            } else if spotlight_id.is_some() {
                Color32::from_rgba_unmultiplied(128, 128, 140, 22)
            } else {
                Color32::from_rgba_unmultiplied(128, 128, 140, 62)
            };
            let width = if spotlit { 1.8_f32 } else { 1.0_f32 };
            painter.line_segment([p1, p2], Stroke::new(width, color));

            if self.layout_mode == GraphLayoutMode::Hierarchy || spotlit {
                let dir = (p2 - p1).normalized();
                if dir.length_sq() > 0.1 {
                    let tr = t.radius * cur_scale;
                    let tip = p2 - dir * (tr + 2.0);
                    let perp = Vec2::new(-dir.y, dir.x);
                    let base = tip - dir * 7.0;
                    painter.add(Shape::convex_polygon(
                        vec![tip, base + perp * 3.4, base - perp * 3.4],
                        color,
                        Stroke::NONE,
                    ));
                }
            }
        }

        for (idx, n) in self.nodes.iter().enumerate() {
            let pos = to_screen(n.x, n.y);
            let r = (n.radius * cur_scale).clamp(3.5, 24.0);
            let is_center = self
                .center_path
                .as_deref()
                .is_some_and(|cp| n.dto.path.as_deref() == Some(cp));
            let is_hovered = hovered_idx == Some(idx);
            let is_selected = self.selected_id.as_deref() == Some(n.dto.id.as_str());
            let is_neighbor = spotlight_neighbors.is_some_and(|set| set.contains(&n.dto.id));
            let matches_query =
                query_lower.is_empty() || n.dto.label.to_lowercase().contains(&query_lower);

            let dimmed = (spotlight_id.is_some() && !is_hovered && !is_selected && !is_neighbor)
                || (!query_lower.is_empty() && !matches_query);

            let mut fill = if is_center {
                pal.danger
            } else {
                self.color_for_node(&n.dto, pal)
            };

            if dimmed {
                fill = Color32::from_rgba_unmultiplied(fill.r(), fill.g(), fill.b(), 52);
            }

            if is_center || is_hovered || is_selected || (!query_lower.is_empty() && matches_query)
            {
                let halo = Color32::from_rgba_unmultiplied(fill.r(), fill.g(), fill.b(), 65);
                painter.circle_filled(pos, r + 4.5, halo);
            }

            painter.circle_filled(pos, r, fill);
            if is_selected {
                painter.circle_stroke(pos, r + 2.0, Stroke::new(2.0_f32, Color32::WHITE));
            }

            let show_label = is_hovered
                || is_selected
                || is_center
                || is_neighbor
                || cur_scale > 0.72
                || self.nodes.len() <= 45;
            if show_label && !dimmed {
                let text_color = if is_hovered || is_selected || is_center {
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

        // Global Graph: 오른쪽 선택 노드 인스펙터 오버레이 카드
        if !self.compact {
            if let Some(sel_id) = self.selected_id.clone() {
                if let Some(sel_node) = self
                    .nodes
                    .iter()
                    .find(|n| n.dto.id == sel_id)
                    .map(|n| n.dto.clone())
                {
                    let card_w = 250.0;
                    let card_rect = Rect::from_min_size(
                        Pos2::new(rect.right() - card_w - 14.0, rect.top() + 14.0),
                        Vec2::new(card_w, (rect.height() - 28.0).min(360.0)),
                    );
                    let mut ui_ov = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(card_rect)
                            .layout(egui::Layout::top_down(egui::Align::LEFT)),
                    );
                    egui::Frame::none()
                        .fill(pal.bg_card)
                        .stroke(Stroke::new(1.0_f32, pal.border_strong))
                        .rounding(Rounding::same(10.0))
                        .inner_margin(Margin::same(12.0))
                        .show(&mut ui_ov, |ui| {
                            ui.set_width(card_w - 24.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "{} {}",
                                        sel_node.icon.as_deref().unwrap_or("📄"),
                                        sel_node.label
                                    ))
                                    .strong()
                                    .size(14.0),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.small_button("✕").clicked() {
                                            self.selected_id = None;
                                        }
                                    },
                                );
                            });
                            if !sel_node.folder.is_empty() {
                                ui.label(
                                    RichText::new(format!("📁 {}", sel_node.folder))
                                        .size(11.5)
                                        .color(pal.text_muted),
                                );
                            }
                            if !sel_node.excerpt.is_empty() {
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(&sel_node.excerpt)
                                        .size(11.5)
                                        .color(pal.text_secondary),
                                );
                            }
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new(format!(
                                    "들어오는 링크 {} · 나가는 링크 {}",
                                    sel_node.inbound_count, sel_node.outbound_count
                                ))
                                .size(11.0)
                                .color(pal.text_muted),
                            );
                            if !sel_node.tags.is_empty() {
                                ui.horizontal_wrapped(|ui| {
                                    for t in sel_node.tags.iter().take(6) {
                                        crate::ui::pill_badge(
                                            ui,
                                            &format!("#{t}"),
                                            pal.accent_blue,
                                            pal.accent_blue_soft,
                                        );
                                    }
                                });
                            }
                            ui.add_space(8.0);
                            if sel_node.kind != "tag"
                                && crate::ui::primary_button(ui, pal, "📄 노트 열기").clicked()
                            {
                                open_node = Some(sel_node.clone());
                            }
                        });
                }
            }
        }

        open_node
    }
}

trait SliceLenHelper {
    fn length_or_len(&self) -> usize;
}
impl<T> SliceLenHelper for [T] {
    fn length_or_len(&self) -> usize {
        self.len().max(1)
    }
}

// ───────────────────────── 리치 Notion 마크다운 렌더러 ─────────────────────────

pub struct MarkdownPreviewOutcome {
    pub clicked_wikilink: Option<String>,
    pub toggled_task_line: Option<usize>,
    pub diagram_action: Option<(usize, usize, DiagramCardAction)>, // (start_line, end_line, action)
}

pub fn render_markdown_preview(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    markdown: &str,
    vault_root: Option<&Path>,
    img_cache: &mut ImageTextureCache,
) -> Option<String> {
    render_markdown_preview_ex(ui, pal, markdown, vault_root, img_cache, false).clicked_wikilink
}

/// 표(실선 테두리 + 헤더 음영), 벡터 플로우차트, 콜아웃, 다단, 체크박스, 인라인 `code`(`#EB5757`)를 모두 렌더링합니다.
pub fn render_markdown_preview_ex(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    markdown: &str,
    vault_root: Option<&Path>,
    img_cache: &mut ImageTextureCache,
    allow_task_toggle: bool,
) -> MarkdownPreviewOutcome {
    let mut out = MarkdownPreviewOutcome {
        clicked_wikilink: None,
        toggled_task_line: None,
        diagram_action: None,
    };
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

        // 페이지 나누기
        if trimmed.contains("page-break") || trimmed == "<!-- pagebreak -->" {
            ui.add_space(4.0);
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 16.0), Sense::hover());
            let painter = ui.painter();
            painter.hline(
                rect.left()..=rect.right(),
                rect.center().y,
                Stroke::new(1.0_f32, pal.border_strong),
            );
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                " ✂ 인쇄 페이지 나누기 ",
                FontId::proportional(10.5),
                pal.text_muted,
            );
            ui.add_space(4.0);
            i += 1;
            continue;
        }

        // 다단 컬럼 (<div class="doc-cols ..."> ... </div>)
        if trimmed.starts_with("<div class=\"doc-cols") {
            let num_cols = if trimmed.contains("doc-cols-3") { 3 } else { 2 };
            let mut col_blocks: Vec<String> = Vec::new();
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
            if !col_blocks.is_empty() {
                ui.columns(num_cols.max(col_blocks.len()).min(3), |cols| {
                    for (ci, block_md) in col_blocks.iter().enumerate() {
                        if ci < cols.len() {
                            egui::Frame::none()
                                .fill(pal.bg_secondary)
                                .stroke(Stroke::new(1.0_f32, pal.border_subtle))
                                .rounding(Rounding::same(6.0))
                                .inner_margin(Margin::same(10.0))
                                .show(&mut cols[ci], |ui| {
                                    let sub = render_markdown_preview_ex(
                                        ui, pal, block_md, vault_root, img_cache, false,
                                    );
                                    if out.clicked_wikilink.is_none() {
                                        out.clicked_wikilink = sub.clicked_wikilink;
                                    }
                                });
                        }
                    }
                });
                ui.add_space(4.0);
            }
            continue;
        }

        // 접기/펼치기 토글 (<details><summary>...</summary>...</details>)
        if trimmed.starts_with("<details") {
            let mut summary = "세부 내용".to_string();
            let mut inner_md = String::new();
            let start_line = i;
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
                    inner_md.push_str(lines[i]);
                    inner_md.push('\n');
                }
                i += 1;
            }
            let hdr_id = ui.make_persistent_id(format!("details_toggle_{start_line}"));
            egui::CollapsingHeader::new(RichText::new(summary).strong().size(13.33))
                .id_salt(hdr_id)
                .default_open(false)
                .show(ui, |ui| {
                    let sub = render_markdown_preview_ex(
                        ui, pal, &inner_md, vault_root, img_cache, false,
                    );
                    if out.clicked_wikilink.is_none() {
                        out.clicked_wikilink = sub.clicked_wikilink;
                    }
                });
            continue;
        }

        // 코드 블록 (``` 또는 ~~~) + 벡터 플로우차트 자동 감지
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let block_start = i;
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
            let block_end = i;
            let code_text = code_lines.join("\n");
            if flowchart::is_diagram_code_block(lang, &code_text) {
                let d_act = flowchart::render_diagram_card(ui, pal, &code_text);
                if !matches!(d_act, DiagramCardAction::None) && out.diagram_action.is_none() {
                    out.diagram_action = Some((block_start, block_end, d_act));
                }
                ui.add_space(4.0);
                continue;
            }
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
                            .size(12.2)
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

        // 구분선
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
                        ui.label(RichText::new(header_txt).strong().size(13.33).color(accent));
                        for ql in quote_lines.iter().skip(1) {
                            if let Some(w) =
                                render_inline_line(ui, pal, ql, 13.33, false, vault_root, img_cache)
                            {
                                out.clicked_wikilink = Some(w);
                            }
                        }
                    });
            } else {
                egui::Frame::none()
                    .fill(pal.bg_inset)
                    .stroke(Stroke::NONE)
                    .rounding(Rounding::same(4.0))
                    .inner_margin(Margin {
                        left: 14.0,
                        right: 12.0,
                        top: 8.0,
                        bottom: 8.0,
                    })
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let bar_rect = ui.max_rect();
                        ui.painter().line_segment(
                            [
                                Pos2::new(bar_rect.left() - 12.0, bar_rect.top() - 4.0),
                                Pos2::new(bar_rect.left() - 12.0, bar_rect.bottom() + 4.0),
                            ],
                            Stroke::new(3.5_f32, pal.accent_blue),
                        );
                        for ql in &quote_lines {
                            if let Some(w) =
                                render_inline_line(ui, pal, ql, 13.33, false, vault_root, img_cache)
                            {
                                out.clicked_wikilink = Some(w);
                            }
                        }
                    });
            }
            ui.add_space(4.0);
            continue;
        }

        // 마크다운 표 (`| col1 | col2 |`) — 명확한 셀 경계 테두리 + 헤더 음영 (WebView 1:1 일치)
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
                let num_cols = table_rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
                let col_w = ((ui.available_width() - 4.0) / num_cols as f32).max(80.0);
                egui::Frame::none()
                    .fill(pal.table_cell_bg)
                    .stroke(Stroke::new(1.0_f32, pal.table_outer))
                    .rounding(Rounding::same(3.0))
                    .inner_margin(Margin::same(0.0))
                    .show(ui, |ui| {
                        egui::Grid::new(ui.next_auto_id().with("md_bordered_table"))
                            .spacing([0.0, 0.0])
                            .show(ui, |ui| {
                                for (r_idx, row) in table_rows.iter().enumerate() {
                                    let bg = if r_idx == 0 {
                                        pal.table_header_bg
                                    } else {
                                        pal.table_cell_bg
                                    };
                                    for c_idx in 0..num_cols {
                                        let cell = row.get(c_idx).map(String::as_str).unwrap_or("");
                                        egui::Frame::none()
                                            .fill(bg)
                                            .stroke(Stroke::new(1.0_f32, pal.table_border))
                                            .inner_margin(Margin::symmetric(9.0, 6.0))
                                            .show(ui, |ui| {
                                                ui.set_min_width(col_w);
                                                if let Some(w) = render_inline_line(
                                                    ui,
                                                    pal,
                                                    cell,
                                                    13.0,
                                                    r_idx == 0,
                                                    vault_root,
                                                    img_cache,
                                                ) {
                                                    out.clicked_wikilink = Some(w);
                                                }
                                            });
                                    }
                                    ui.end_row();
                                }
                            });
                    });
                ui.add_space(5.0);
            }
            continue;
        }

        // 제목 (H1~H6)
        let hashes = trimmed.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
            let heading_text = trimmed[hashes..].trim();
            let size = match hashes {
                1 => 22.0,
                2 => 18.0,
                3 => 15.5,
                _ => 14.0,
            };
            ui.add_space(6.0);
            if let Some(w) =
                render_inline_line(ui, pal, heading_text, size, true, vault_root, img_cache)
            {
                out.clicked_wikilink = Some(w);
            }
            ui.add_space(2.0);
            i += 1;
            continue;
        }

        // 체크리스트 (`- [ ]` / `- [x]`)
        if let Some(rest) = trimmed
            .strip_prefix("- [ ] ")
            .or_else(|| trimmed.strip_prefix("* [ ] "))
        {
            let line_idx = i;
            ui.horizontal_wrapped(|ui| {
                let mut checked = false;
                if allow_task_toggle {
                    if ui.checkbox(&mut checked, "").changed() && out.toggled_task_line.is_none() {
                        out.toggled_task_line = Some(line_idx);
                    }
                } else {
                    ui.add_enabled(false, egui::Checkbox::new(&mut checked, ""));
                }
                if let Some(w) =
                    render_inline_line(ui, pal, rest, 13.33, false, vault_root, img_cache)
                {
                    out.clicked_wikilink = Some(w);
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
            let line_idx = i;
            ui.horizontal_wrapped(|ui| {
                let mut checked = true;
                if allow_task_toggle {
                    if ui.checkbox(&mut checked, "").changed() && out.toggled_task_line.is_none() {
                        out.toggled_task_line = Some(line_idx);
                    }
                } else {
                    ui.add_enabled(false, egui::Checkbox::new(&mut checked, ""));
                }
                if let Some(w) =
                    render_inline_line(ui, pal, rest, 13.33, false, vault_root, img_cache)
                {
                    out.clicked_wikilink = Some(w);
                }
            });
            i += 1;
            continue;
        }

        // 번호 목록 (`1. `, `2. `)
        if let Some(dot_pos) = trimmed.find(". ") {
            if dot_pos > 0 && dot_pos <= 4 && trimmed[..dot_pos].chars().all(|c| c.is_ascii_digit())
            {
                let num_prefix = &trimmed[..dot_pos + 1];
                let rest = &trimmed[dot_pos + 2..];
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(num_prefix)
                            .size(13.33)
                            .strong()
                            .color(pal.text_secondary),
                    );
                    if let Some(w) =
                        render_inline_line(ui, pal, rest, 13.33, false, vault_root, img_cache)
                    {
                        out.clicked_wikilink = Some(w);
                    }
                });
                i += 1;
                continue;
            }
        }

        // 불릿 리스트 (`- ` / `* `)
        if let Some(rest) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("•")
                        .size(13.33)
                        .color(pal.text_primary)
                        .strong(),
                );
                if let Some(w) =
                    render_inline_line(ui, pal, rest, 13.33, false, vault_root, img_cache)
                {
                    out.clicked_wikilink = Some(w);
                }
            });
            i += 1;
            continue;
        }

        // 일반 문단 (`10pt` = `13.33px`)
        if let Some(w) = render_inline_line(ui, pal, trimmed, 13.33, false, vault_root, img_cache) {
            out.clicked_wikilink = Some(w);
        }
        i += 1;
    }

    out
}

pub fn render_inline_line(
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
        ui.spacing_mut().item_spacing.x = 2.0;
        let mut rest = line;

        while !rest.is_empty() {
            let next_embed = rest.find("![[");
            let next_md_img = rest.find("![");
            let next_wiki = rest.find("[[");
            let next_md_link = rest.find('[');
            let next_mark = rest.find("==");
            let next_code = rest.find('`');
            let next_bold = rest.find("**");
            let next_strike = rest.find("~~");

            let earliest = [
                next_embed.map(|p| (p, "embed_img")),
                next_md_img
                    .filter(|&p| next_embed != Some(p))
                    .map(|p| (p, "md_img")),
                next_wiki
                    .filter(|&p| next_embed.map(|e| e + 1) != Some(p))
                    .map(|p| (p, "wiki")),
                next_md_link
                    .filter(|&p| {
                        next_wiki != Some(p)
                            && next_embed.map(|e| e + 1) != Some(p)
                            && next_md_img.map(|e| e + 1) != Some(p)
                    })
                    .map(|p| (p, "md_link")),
                next_mark.map(|p| (p, "mark")),
                next_code.map(|p| (p, "code")),
                next_bold.map(|p| (p, "bold")),
                next_strike.map(|p| (p, "strike")),
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
                            RichText::new(display)
                                .size(font_size)
                                .color(pal.accent_blue)
                                .strong(),
                        )
                        .fill(pal.accent_blue_soft)
                        .stroke(Stroke::NONE)
                        .rounding(Rounding::same(4.0));
                        if ui
                            .add(btn)
                            .on_hover_text(format!("노트 열기: [[{target}]]"))
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
                "md_link" => {
                    if let Some(mid) = rest[1..].find("](") {
                        let after = &rest[1 + mid + 2..];
                        if let Some(end_p) = after.find(')') {
                            let lbl = &rest[1..1 + mid];
                            let href = after[..end_p].trim();
                            if href.starts_with("http://") || href.starts_with("https://") {
                                if ui.hyperlink_to(lbl, href).clicked() {
                                    let _ = open::that(href);
                                }
                            } else {
                                ui.label(
                                    RichText::new(lbl)
                                        .size(font_size)
                                        .color(pal.accent_blue)
                                        .underline(),
                                );
                            }
                            rest = &after[end_p + 1..];
                        } else {
                            ui.label(RichText::new("[").size(font_size));
                            rest = &rest[1..];
                        }
                    } else {
                        ui.label(RichText::new("[").size(font_size));
                        rest = &rest[1..];
                    }
                }
                "mark" => {
                    if let Some(end) = rest[2..].find("==") {
                        let inner = &rest[2..2 + end];
                        egui::Frame::none()
                            .fill(pal.mark_bg)
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
                                        .size((font_size - 1.0).max(11.0))
                                        .color(pal.code_fg),
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
                "strike" => {
                    if let Some(end) = rest[2..].find("~~") {
                        let inner = &rest[2..2 + end];
                        ui.label(
                            RichText::new(inner)
                                .size(font_size)
                                .strikethrough()
                                .color(pal.text_muted),
                        );
                        rest = &rest[2 + end + 2..];
                    } else {
                        ui.label(RichText::new("~~").size(font_size));
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
