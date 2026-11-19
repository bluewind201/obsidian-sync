//! Onyx Studio 벡터 플로우차트 · 다이어그램 레이아웃 및 듀얼 렌더러 (`egui::Painter` + SVG 문자열).
//! - Mermaid `flowchart TB/TD/BT/LR/RL`, `graph`, `sequenceDiagram`, `mindmap` 지원
//! - Sugiyama 계층 랭크/무게중심 정렬, 서브그래프 박스, 부드러운 베지어/직각 경로, 5종 테마
//! - 인터랙티브 비주얼 플로우차트 빌더 모달 및 인쇄/PDF용 독립 `<svg>` 출력 지원

use crate::ui::ThemePalette;
use eframe::egui::{
    self, Align2, Color32, FontId, Margin, Pos2, Rect, RichText, Rounding, Sense, Shape, Stroke,
    Vec2,
};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FlowDirection {
    TB,
    BT,
    LR,
    RL,
}

impl FlowDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TB => "TB",
            Self::BT => "BT",
            Self::LR => "LR",
            Self::RL => "RL",
        }
    }

    pub fn from_token(raw: &str) -> Self {
        match raw.trim().to_ascii_uppercase().as_str() {
            "BT" => Self::BT,
            "LR" => Self::LR,
            "RL" => Self::RL,
            _ => Self::TB,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::TB => Self::LR,
            Self::LR => Self::BT,
            Self::BT => Self::RL,
            Self::RL => Self::TB,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FlowThemeName {
    Modern,
    Ocean,
    Emerald,
    Sunset,
    Mono,
}

impl FlowThemeName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Modern => "modern",
            Self::Ocean => "ocean",
            Self::Emerald => "emerald",
            Self::Sunset => "sunset",
            Self::Mono => "mono",
        }
    }

    pub fn label_ko(self) -> &'static str {
        match self {
            Self::Modern => "모던",
            Self::Ocean => "오션",
            Self::Emerald => "민트",
            Self::Sunset => "선셋",
            Self::Mono => "모노",
        }
    }

    pub fn from_token(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "ocean" => Self::Ocean,
            "emerald" => Self::Emerald,
            "sunset" => Self::Sunset,
            "mono" => Self::Mono,
            _ => Self::Modern,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Modern => Self::Ocean,
            Self::Ocean => Self::Emerald,
            Self::Emerald => Self::Sunset,
            Self::Sunset => Self::Mono,
            Self::Mono => Self::Modern,
        }
    }

    pub fn colors(self, is_dark: bool) -> ThemeColors {
        match (self, is_dark) {
            (Self::Ocean, false) => ThemeColors {
                node_fill: Color32::from_rgb(239, 246, 255),
                node_stroke: Color32::from_rgb(37, 99, 235),
                node_text: Color32::from_rgb(30, 58, 138),
                edge: Color32::from_rgb(59, 130, 246),
                sub_fill: Color32::from_rgba_unmultiplied(219, 234, 254, 115),
                sub_stroke: Color32::from_rgb(96, 165, 250),
                accent: Color32::from_rgb(37, 99, 235),
            },
            (Self::Ocean, true) => ThemeColors {
                node_fill: Color32::from_rgb(23, 37, 68),
                node_stroke: Color32::from_rgb(96, 165, 250),
                node_text: Color32::from_rgb(219, 234, 254),
                edge: Color32::from_rgb(147, 197, 253),
                sub_fill: Color32::from_rgba_unmultiplied(30, 58, 138, 60),
                sub_stroke: Color32::from_rgb(59, 130, 246),
                accent: Color32::from_rgb(96, 165, 250),
            },
            (Self::Emerald, false) => ThemeColors {
                node_fill: Color32::from_rgb(236, 253, 245),
                node_stroke: Color32::from_rgb(5, 150, 105),
                node_text: Color32::from_rgb(6, 78, 59),
                edge: Color32::from_rgb(16, 185, 129),
                sub_fill: Color32::from_rgba_unmultiplied(209, 250, 229, 115),
                sub_stroke: Color32::from_rgb(52, 211, 153),
                accent: Color32::from_rgb(5, 150, 105),
            },
            (Self::Emerald, true) => ThemeColors {
                node_fill: Color32::from_rgb(16, 48, 39),
                node_stroke: Color32::from_rgb(52, 211, 153),
                node_text: Color32::from_rgb(209, 250, 229),
                edge: Color32::from_rgb(110, 231, 183),
                sub_fill: Color32::from_rgba_unmultiplied(6, 95, 70, 60),
                sub_stroke: Color32::from_rgb(16, 185, 129),
                accent: Color32::from_rgb(52, 211, 153),
            },
            (Self::Sunset, false) => ThemeColors {
                node_fill: Color32::from_rgb(255, 247, 237),
                node_stroke: Color32::from_rgb(217, 119, 6),
                node_text: Color32::from_rgb(124, 45, 18),
                edge: Color32::from_rgb(234, 88, 12),
                sub_fill: Color32::from_rgba_unmultiplied(254, 243, 199, 120),
                sub_stroke: Color32::from_rgb(245, 158, 11),
                accent: Color32::from_rgb(217, 119, 6),
            },
            (Self::Sunset, true) => ThemeColors {
                node_fill: Color32::from_rgb(66, 32, 16),
                node_stroke: Color32::from_rgb(251, 146, 60),
                node_text: Color32::from_rgb(255, 237, 213),
                edge: Color32::from_rgb(253, 186, 116),
                sub_fill: Color32::from_rgba_unmultiplied(124, 45, 18, 60),
                sub_stroke: Color32::from_rgb(245, 158, 11),
                accent: Color32::from_rgb(251, 146, 60),
            },
            (Self::Mono, false) => ThemeColors {
                node_fill: Color32::from_rgb(249, 250, 251),
                node_stroke: Color32::from_rgb(55, 65, 81),
                node_text: Color32::from_rgb(17, 24, 39),
                edge: Color32::from_rgb(75, 85, 99),
                sub_fill: Color32::from_rgba_unmultiplied(243, 244, 246, 140),
                sub_stroke: Color32::from_rgb(156, 163, 175),
                accent: Color32::from_rgb(55, 65, 81),
            },
            (Self::Mono, true) => ThemeColors {
                node_fill: Color32::from_rgb(38, 40, 46),
                node_stroke: Color32::from_rgb(156, 163, 175),
                node_text: Color32::from_rgb(243, 244, 246),
                edge: Color32::from_rgb(156, 163, 175),
                sub_fill: Color32::from_rgba_unmultiplied(55, 65, 81, 60),
                sub_stroke: Color32::from_rgb(107, 114, 128),
                accent: Color32::from_rgb(209, 213, 219),
            },
            // Modern (default)
            (Self::Modern, false) => ThemeColors {
                node_fill: Color32::WHITE,
                node_stroke: Color32::from_rgb(35, 131, 226),
                node_text: Color32::from_rgb(30, 41, 59),
                edge: Color32::from_rgb(100, 116, 139),
                sub_fill: Color32::from_rgba_unmultiplied(241, 245, 249, 175),
                sub_stroke: Color32::from_rgb(148, 163, 184),
                accent: Color32::from_rgb(35, 131, 226),
            },
            (Self::Modern, true) => ThemeColors {
                node_fill: Color32::from_rgb(38, 42, 52),
                node_stroke: Color32::from_rgb(82, 156, 202),
                node_text: Color32::from_rgb(226, 232, 240),
                edge: Color32::from_rgb(148, 163, 184),
                sub_fill: Color32::from_rgba_unmultiplied(30, 41, 59, 95),
                sub_stroke: Color32::from_rgb(71, 85, 105),
                accent: Color32::from_rgb(82, 156, 202),
            },
        }
    }
}

#[derive(Clone, Copy)]
pub struct ThemeColors {
    pub node_fill: Color32,
    pub node_stroke: Color32,
    pub node_text: Color32,
    pub edge: Color32,
    pub sub_fill: Color32,
    pub sub_stroke: Color32,
    pub accent: Color32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FlowCurveMode {
    Smooth,
    Ortho,
}

impl FlowCurveMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Smooth => "smooth",
            Self::Ortho => "ortho",
        }
    }

    pub fn from_token(raw: &str) -> Self {
        if raw.trim().eq_ignore_ascii_case("ortho") {
            Self::Ortho
        } else {
            Self::Smooth
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FlowShape {
    Rect,
    Round,
    Stadium,
    Subroutine,
    Cylinder,
    Circle,
    Diamond,
    Hexagon,
}

impl FlowShape {
    pub fn all() -> &'static [(FlowShape, &'static str)] {
        &[
            (FlowShape::Rect, "사각형 [A]"),
            (FlowShape::Round, "둥근 사각형 (A)"),
            (FlowShape::Stadium, "시작/종료 ([A])"),
            (FlowShape::Diamond, "분기/조건 {A}"),
            (FlowShape::Cylinder, "데이터베이스 [(A)]"),
            (FlowShape::Hexagon, "준비 {{A}}"),
            (FlowShape::Subroutine, "서브루틴 [[A]]"),
            (FlowShape::Circle, "원형 ((A))"),
        ]
    }

    pub fn wrap_label(self, id: &str, label: &str) -> String {
        let safe = label.replace('"', "'");
        match self {
            Self::Rect => format!("{id}[{safe}]"),
            Self::Round => format!("{id}({safe})"),
            Self::Stadium => format!("{id}([{safe}])"),
            Self::Diamond => format!("{id}{{{safe}}}"),
            Self::Cylinder => format!("{id}[({safe})]"),
            Self::Hexagon => format!("{id}{{{{{safe}}}}}"),
            Self::Subroutine => format!("{id}[[{safe}]]"),
            Self::Circle => format!("{id}(({safe}))"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EdgeLineStyle {
    Solid,
    Dotted,
    Thick,
}

#[derive(Clone, Debug)]
pub struct FlowNode {
    pub id: String,
    pub label: String,
    pub shape: FlowShape,
    pub subgraph_id: Option<String>,
    pub rank: usize,
    pub order: usize,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub lines: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct FlowEdge {
    pub from: String,
    pub to: String,
    pub label: String,
    pub style: EdgeLineStyle,
    pub has_arrow: bool,
}

#[derive(Clone, Debug)]
pub struct FlowSubgraph {
    pub id: String,
    pub title: String,
    pub node_ids: Vec<String>,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Debug)]
pub struct ParsedFlowchart {
    pub kind_label: String,
    pub direction: FlowDirection,
    pub theme: FlowThemeName,
    pub curve: FlowCurveMode,
    pub nodes: Vec<FlowNode>,
    pub edges: Vec<FlowEdge>,
    pub subgraphs: Vec<FlowSubgraph>,
    pub width: f32,
    pub height: f32,
}

pub enum DiagramCardAction {
    None,
    UpdateCode(String),
    RequestEditCode,
    OpenBuilder(String),
}

pub fn first_significant_line(code: &str) -> &str {
    for line in code.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("%%") {
            continue;
        }
        return t;
    }
    ""
}

pub fn is_diagram_code_block(lang: &str, code: &str) -> bool {
    let l = lang.trim().to_ascii_lowercase();
    if matches!(
        l.as_str(),
        "mermaid" | "flowchart" | "flow" | "diagram" | "graph"
    ) {
        return true;
    }
    if !l.is_empty() && !matches!(l.as_str(), "text" | "plain" | "txt") {
        return false;
    }
    let first = first_significant_line(code).to_ascii_lowercase();
    first.starts_with("flowchart")
        || first.starts_with("graph ")
        || first == "graph"
        || first.starts_with("sequencediagram")
        || first.starts_with("mindmap")
}

pub fn parse_diagram_meta(code: &str) -> (FlowThemeName, FlowCurveMode, Vec<&str>) {
    let mut theme = FlowThemeName::Modern;
    let mut curve = FlowCurveMode::Smooth;
    let mut lines = Vec::new();
    for raw in code.lines() {
        let t = raw.trim();
        if let Some(rest) = t.strip_prefix("%%") {
            let r = rest.trim();
            if let Some((k, v)) = r.split_once(':') {
                if k.trim().eq_ignore_ascii_case("onyx-theme") {
                    theme = FlowThemeName::from_token(v);
                    continue;
                }
                if k.trim().eq_ignore_ascii_case("onyx-curve") {
                    curve = FlowCurveMode::from_token(v);
                    continue;
                }
            }
            continue;
        }
        lines.push(raw);
    }
    (theme, curve, lines)
}

fn upsert_meta_comment(code: &str, key: &str, value: &str) -> String {
    let target_line = format!("%% {key}: {value}");
    let mut lines: Vec<String> = code.lines().map(|l| l.to_string()).collect();
    let prefix = format!("{key}:");
    let mut replaced = false;
    for l in &mut lines {
        let t = l.trim();
        if let Some(after) = t.strip_prefix("%%") {
            if after.trim().to_ascii_lowercase().starts_with(&prefix) {
                *l = target_line.clone();
                replaced = true;
                break;
            }
        }
    }
    if !replaced {
        lines.insert(0, target_line);
    }
    lines.join("\n")
}

pub fn toggle_flowchart_direction(code: &str) -> String {
    let mut lines: Vec<String> = code.lines().map(|s| s.to_string()).collect();
    for l in &mut lines {
        let t = l.trim();
        if t.is_empty() || t.starts_with("%%") {
            continue;
        }
        let parts: Vec<&str> = t.split_whitespace().collect();
        if parts.is_empty() {
            break;
        }
        let kw = parts[0];
        if kw.eq_ignore_ascii_case("flowchart") || kw.eq_ignore_ascii_case("graph") {
            let cur = parts
                .get(1)
                .map(|d| FlowDirection::from_token(d))
                .unwrap_or(FlowDirection::TB);
            let next = cur.next();
            *l = format!("{} {}", kw.to_ascii_lowercase(), next.as_str());
        }
        break;
    }
    lines.join("\n")
}

pub fn cycle_flowchart_theme(code: &str) -> String {
    let (theme, _, _) = parse_diagram_meta(code);
    upsert_meta_comment(code, "onyx-theme", theme.next().as_str())
}

pub fn toggle_flowchart_curve(code: &str) -> String {
    let (_, curve, _) = parse_diagram_meta(code);
    let next = match curve {
        FlowCurveMode::Smooth => FlowCurveMode::Ortho,
        FlowCurveMode::Ortho => FlowCurveMode::Smooth,
    };
    upsert_meta_comment(code, "onyx-curve", next.as_str())
}

fn clean_label(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    if ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
        && s.len() >= 2
    {
        s = s[1..s.len() - 1].to_string();
    }
    s.replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<br>", "\n")
        .replace("\\n", "\n")
        .trim()
        .to_string()
}

fn char_visual_width(ch: char) -> f32 {
    if ch == ' ' || ch == '\t' {
        return 4.2;
    }
    let u = ch as u32;
    if (0xAC00..=0xD7AF).contains(&u) || (0x4E00..=0x9FFF).contains(&u) {
        return 12.6;
    }
    if u >= 0x1F300 {
        return 14.5;
    }
    if ch.is_ascii_uppercase() {
        return 8.0;
    }
    7.0
}

fn measure_line_px(line: &str) -> f32 {
    line.chars().map(char_visual_width).sum::<f32>().max(8.0)
}

fn wrap_node_label(raw: &str) -> (Vec<String>, f32, f32) {
    let max_px = 188.0;
    let normalized = raw.trim();
    if normalized.is_empty() {
        return (vec![" ".into()], 76.0, 36.0);
    }
    let mut lines = Vec::new();
    for para in normalized.lines() {
        let p = para.trim();
        if p.is_empty() {
            continue;
        }
        if measure_line_px(p) <= max_px {
            lines.push(p.to_string());
        } else {
            let mut cur = String::new();
            for ch in p.chars() {
                cur.push(ch);
                if measure_line_px(&cur) > max_px {
                    let tail = cur.pop();
                    lines.push(cur.trim().to_string());
                    cur = tail.map(|c| c.to_string()).unwrap_or_default();
                }
            }
            if !cur.trim().is_empty() {
                lines.push(cur.trim().to_string());
            }
        }
    }
    if lines.is_empty() {
        lines.push(normalized.to_string());
    }
    let max_w = lines
        .iter()
        .map(|l| measure_line_px(l))
        .fold(40.0f32, f32::max);
    let w = (max_w + 30.0).clamp(78.0, 236.0);
    let h = (lines.len() as f32 * 17.0 + 18.0).max(36.0);
    (lines, w, h)
}

fn parse_node_token(raw: &str) -> Option<(String, Option<String>, Option<FlowShape>)> {
    let s = raw.trim().trim_end_matches(';').trim();
    if s.is_empty() {
        return None;
    }
    // Strip optional trailing :::class
    let token = s.split(":::").next().unwrap_or(s).trim();

    let patterns: &[(&str, &str, FlowShape)] = &[
        ("([", "])", FlowShape::Stadium),
        ("[[", "]]", FlowShape::Subroutine),
        ("[(", ")]", FlowShape::Cylinder),
        ("((", "))", FlowShape::Circle),
        ("{{", "}}", FlowShape::Hexagon),
        ("{", "}", FlowShape::Diamond),
        ("(", ")", FlowShape::Round),
        ("[", "]", FlowShape::Rect),
    ];

    // Pick earliest opening delimiter inside `token`
    let mut best: Option<(usize, &str, &str, FlowShape)> = None;
    for &(open, close, shape) in patterns {
        if let Some(pos) = token.find(open) {
            if pos == 0 {
                continue;
            }
            if let Some(end_rel) = token[pos + open.len()..].rfind(close) {
                if token[pos + open.len() + end_rel + close.len()..]
                    .trim()
                    .is_empty()
                {
                    let better = match best {
                        Some((bpos, bopen, _, _)) => {
                            pos < bpos || (pos == bpos && open.len() > bopen.len())
                        }
                        None => true,
                    };
                    if better {
                        best = Some((pos, open, close, shape));
                    }
                }
            }
        }
    }

    if let Some((pos, open, close, shape)) = best {
        let id = token[..pos].trim();
        let inner = &token[pos + open.len()..token.len() - close.len()];
        if id.is_empty() {
            return None;
        }
        return Some((id.to_string(), Some(clean_label(inner)), Some(shape)));
    }

    // Bare ID (`A`, `Step_1`)
    if token
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Some((token.to_string(), None, None));
    }
    None
}

fn find_next_edge_operator(s: &str) -> Option<(usize, usize, String, EdgeLineStyle, bool)> {
    // Supported arrow operators outside quotes/brackets
    let ops: &[(&str, EdgeLineStyle, bool)] = &[
        ("-.->", EdgeLineStyle::Dotted, true),
        ("-.->", EdgeLineStyle::Dotted, true),
        ("==>", EdgeLineStyle::Thick, true),
        ("<-->", EdgeLineStyle::Solid, true),
        ("--->", EdgeLineStyle::Solid, true),
        ("-->", EdgeLineStyle::Solid, true),
        ("-.-", EdgeLineStyle::Dotted, false),
        ("===", EdgeLineStyle::Thick, false),
        ("---", EdgeLineStyle::Solid, false),
    ];

    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut in_quote: Option<u8> = None;
    let mut i = 0usize;

    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = in_quote {
            if b == q {
                in_quote = None;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' | b'\'' => in_quote = Some(b),
            b'[' | b'(' | b'{' => depth += 1,
            b']' | b')' | b'}' => depth = (depth - 1).max(0),
            _ if depth == 0 => {
                // Check `-- label -->` pattern
                if s[i..].starts_with("-- ") {
                    if let Some(end_arrow) = s[i + 3..].find("-->") {
                        let lbl = s[i + 3..i + 3 + end_arrow].trim().to_string();
                        let end = i + 3 + end_arrow + 3;
                        return Some((i, end, clean_label(&lbl), EdgeLineStyle::Solid, true));
                    }
                }
                for &(sym, style, arrow) in ops {
                    if s[i..].starts_with(sym) {
                        let mut end = i + sym.len();
                        let mut lbl = String::new();
                        let after = s[end..].trim_start();
                        if after.starts_with('|') {
                            let ws = s[end..].len() - after.len();
                            if let Some(pipe_end) = after[1..].find('|') {
                                lbl = clean_label(&after[1..1 + pipe_end]);
                                end += ws + 1 + pipe_end + 1;
                            }
                        }
                        return Some((i, end, lbl, style, arrow));
                    }
                }
            }
            _ => {}
        }
        i += s[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    }
    None
}

fn upsert_node(
    nodes_map: &mut BTreeMap<String, FlowNode>,
    node_order: &mut Vec<String>,
    subgraph_id: Option<&str>,
    id: &str,
    label_opt: Option<String>,
    shape_opt: Option<FlowShape>,
    subgraphs: &mut Vec<FlowSubgraph>,
) {
    let entry = nodes_map.entry(id.to_string()).or_insert_with(|| {
        node_order.push(id.to_string());
        let (lines, w, h) = wrap_node_label(id);
        FlowNode {
            id: id.to_string(),
            label: id.to_string(),
            shape: FlowShape::Round,
            subgraph_id: subgraph_id.map(str::to_string),
            rank: 0,
            order: 0,
            x: 0.0,
            y: 0.0,
            w,
            h,
            lines,
        }
    });

    if let Some(sg_id) = subgraph_id {
        if entry.subgraph_id.is_none() {
            entry.subgraph_id = Some(sg_id.to_string());
        }
        if let Some(sg) = subgraphs.iter_mut().find(|s| s.id == sg_id) {
            if !sg.node_ids.iter().any(|x| x == id) {
                sg.node_ids.push(id.to_string());
            }
        }
    }

    if let Some(lbl) = label_opt {
        if !lbl.is_empty() {
            entry.label = lbl.clone();
            let (lines, mut w, mut h) = wrap_node_label(&lbl);
            let shape = shape_opt.unwrap_or(entry.shape);
            match shape {
                FlowShape::Diamond => {
                    w = (w * 1.24).max(96.0);
                    h = (h * 1.38).max(52.0);
                }
                FlowShape::Hexagon => w += 22.0,
                FlowShape::Cylinder => h += 12.0,
                FlowShape::Circle => {
                    let d = w.max(h).max(54.0).min(130.0);
                    w = d;
                    h = d;
                }
                _ => {}
            }
            entry.lines = lines;
            entry.w = w;
            entry.h = h;
        }
    }

    if let Some(sh) = shape_opt {
        entry.shape = sh;
    }
}

pub fn parse_diagram(code: &str) -> Option<ParsedFlowchart> {
    let (theme, curve, lines) = parse_diagram_meta(code);
    let first = lines
        .iter()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())?
        .to_string();
    let lower = first.to_ascii_lowercase();

    if lower.starts_with("sequencediagram") {
        return parse_sequence_as_flow(&lines, theme, curve);
    }
    if lower.starts_with("mindmap") {
        return parse_mindmap_as_flow(&lines, theme, curve);
    }

    let mut direction = FlowDirection::TB;
    if lower.starts_with("flowchart") || lower.starts_with("graph") {
        if let Some(dir_tok) = first.split_whitespace().nth(1) {
            direction = FlowDirection::from_token(dir_tok);
        }
    } else if !is_diagram_code_block("", code) {
        return None;
    }

    let mut nodes_map: BTreeMap<String, FlowNode> = BTreeMap::new();
    let mut node_order: Vec<String> = Vec::new();
    let mut edges: Vec<FlowEdge> = Vec::new();
    let mut subgraphs: Vec<FlowSubgraph> = Vec::new();
    let mut sg_stack: Vec<String> = Vec::new();

    for raw in &lines {
        let t = raw.trim();
        if t.is_empty() {
            continue;
        }
        let tl = t.to_ascii_lowercase();
        if tl.starts_with("flowchart")
            || tl.starts_with("graph ")
            || tl == "graph"
            || tl.starts_with("classdef ")
            || tl.starts_with("class ")
            || tl.starts_with("style ")
            || tl.starts_with("linkstyle ")
            || tl.starts_with("direction ")
        {
            continue;
        }

        if let Some(rest) = t
            .strip_prefix("subgraph")
            .or_else(|| t.strip_prefix("SUBGRAPH"))
        {
            let r = rest.trim();
            let (sg_id, sg_title) = if let Some((id_part, title_part)) = r.split_once('[') {
                (
                    id_part.trim().to_string(),
                    clean_label(title_part.trim_end_matches(']')),
                )
            } else {
                let id_clean = r.replace(' ', "_");
                (id_clean, clean_label(r))
            };
            subgraphs.push(FlowSubgraph {
                id: sg_id.clone(),
                title: if sg_title.is_empty() {
                    sg_id.clone()
                } else {
                    sg_title
                },
                node_ids: Vec::new(),
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            });
            sg_stack.push(sg_id);
            continue;
        }

        if tl == "end" {
            sg_stack.pop();
            continue;
        }

        let cur_sg = sg_stack.last().map(String::as_str);

        // Split chained edges like `A[Start] --> B{Check} -->|Yes| C[Done]`
        let mut cursor = t;
        let mut prev_ids: Vec<String> = Vec::new();
        let mut had_edge = false;

        while let Some((op_start, op_end, lbl, style, arrow)) = find_next_edge_operator(cursor) {
            had_edge = true;
            let left_str = cursor[..op_start].trim();
            let mut cur_left_ids = Vec::new();
            if !left_str.is_empty() {
                for seg in left_str.split('&') {
                    if let Some((id, lopt, sopt)) = parse_node_token(seg) {
                        upsert_node(
                            &mut nodes_map,
                            &mut node_order,
                            cur_sg,
                            &id,
                            lopt,
                            sopt,
                            &mut subgraphs,
                        );
                        cur_left_ids.push(id);
                    }
                }
            } else {
                cur_left_ids = prev_ids.clone();
            }

            let after_op = cursor[op_end..].trim();
            let next_op = find_next_edge_operator(after_op);
            let right_str = match next_op {
                Some((ns, _, _, _, _)) => after_op[..ns].trim(),
                None => after_op,
            };

            let mut cur_right_ids = Vec::new();
            for seg in right_str.split('&') {
                if let Some((id, lopt, sopt)) = parse_node_token(seg) {
                    upsert_node(
                        &mut nodes_map,
                        &mut node_order,
                        cur_sg,
                        &id,
                        lopt,
                        sopt,
                        &mut subgraphs,
                    );
                    cur_right_ids.push(id);
                }
            }

            for f in &cur_left_ids {
                for to in &cur_right_ids {
                    edges.push(FlowEdge {
                        from: f.clone(),
                        to: to.clone(),
                        label: lbl.clone(),
                        style,
                        has_arrow: arrow,
                    });
                }
            }

            prev_ids = cur_right_ids;
            cursor = if let Some((ns, _, _, _, _)) = next_op {
                &after_op[ns..]
            } else {
                ""
            };
            if cursor.is_empty() {
                break;
            }
        }

        if !had_edge {
            if let Some((id, lopt, sopt)) = parse_node_token(t) {
                upsert_node(
                    &mut nodes_map,
                    &mut node_order,
                    cur_sg,
                    &id,
                    lopt,
                    sopt,
                    &mut subgraphs,
                );
            }
        }
    }

    if node_order.is_empty() {
        return None;
    }

    let mut nodes: Vec<FlowNode> = node_order
        .into_iter()
        .filter_map(|id| nodes_map.remove(&id))
        .collect();

    layout_flowchart(&mut nodes, &edges, &mut subgraphs, direction);

    let max_x = nodes
        .iter()
        .map(|n| n.x + n.w * 0.5)
        .chain(subgraphs.iter().map(|s| s.x + s.w))
        .fold(200.0f32, f32::max);
    let max_y = nodes
        .iter()
        .map(|n| n.y + n.h * 0.5)
        .chain(subgraphs.iter().map(|s| s.y + s.h))
        .fold(140.0f32, f32::max);

    Some(ParsedFlowchart {
        kind_label: format!("FLOWCHART ({})", direction.as_str()),
        direction,
        theme,
        curve,
        nodes,
        edges,
        subgraphs,
        width: max_x + 28.0,
        height: max_y + 28.0,
    })
}

fn parse_sequence_as_flow(
    lines: &[&str],
    theme: FlowThemeName,
    curve: FlowCurveMode,
) -> Option<ParsedFlowchart> {
    let mut nodes_map: BTreeMap<String, FlowNode> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut edges: Vec<FlowEdge> = Vec::new();
    let mut subgraphs: Vec<FlowSubgraph> = Vec::new();

    for raw in lines.iter().skip(1) {
        let t = raw.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(rest) = t.strip_prefix("participant ") {
            let (id, lbl) = if let Some((a, b)) = rest.split_once(" as ") {
                (a.trim(), b.trim())
            } else {
                (rest.trim(), rest.trim())
            };
            upsert_node(
                &mut nodes_map,
                &mut order,
                None,
                id,
                Some(clean_label(lbl)),
                Some(FlowShape::Round),
                &mut subgraphs,
            );
            continue;
        }
        for sym in ["-->>", "->>", "-->", "->"] {
            if let Some((lhs, rhs_all)) = t.split_once(sym) {
                let (to_id, lbl) = rhs_all.split_once(':').unwrap_or((rhs_all, ""));
                let from_id = lhs.trim();
                let to_clean = to_id.trim();
                if !from_id.is_empty() && !to_clean.is_empty() {
                    upsert_node(
                        &mut nodes_map,
                        &mut order,
                        None,
                        from_id,
                        None,
                        Some(FlowShape::Round),
                        &mut subgraphs,
                    );
                    upsert_node(
                        &mut nodes_map,
                        &mut order,
                        None,
                        to_clean,
                        None,
                        Some(FlowShape::Round),
                        &mut subgraphs,
                    );
                    edges.push(FlowEdge {
                        from: from_id.to_string(),
                        to: to_clean.to_string(),
                        label: clean_label(lbl),
                        style: if sym.starts_with("--") {
                            EdgeLineStyle::Dotted
                        } else {
                            EdgeLineStyle::Solid
                        },
                        has_arrow: true,
                    });
                }
                break;
            }
        }
    }

    if order.is_empty() {
        return None;
    }
    let mut nodes: Vec<FlowNode> = order
        .into_iter()
        .filter_map(|id| nodes_map.remove(&id))
        .collect();
    layout_flowchart(&mut nodes, &edges, &mut subgraphs, FlowDirection::LR);
    let max_x = nodes
        .iter()
        .map(|n| n.x + n.w * 0.5)
        .fold(200.0f32, f32::max);
    let max_y = nodes
        .iter()
        .map(|n| n.y + n.h * 0.5)
        .fold(140.0f32, f32::max);
    Some(ParsedFlowchart {
        kind_label: "SEQUENCE".into(),
        direction: FlowDirection::LR,
        theme,
        curve,
        nodes,
        edges,
        subgraphs,
        width: max_x + 28.0,
        height: max_y + 28.0,
    })
}

fn parse_mindmap_as_flow(
    lines: &[&str],
    theme: FlowThemeName,
    curve: FlowCurveMode,
) -> Option<ParsedFlowchart> {
    let mut nodes: Vec<FlowNode> = Vec::new();
    let mut edges: Vec<FlowEdge> = Vec::new();
    let mut stack: Vec<(usize, String)> = Vec::new();
    let mut seq = 0usize;

    for raw in lines.iter().skip(1) {
        if raw.trim().is_empty() {
            continue;
        }
        let indent = raw.chars().take_while(|c| *c == ' ' || *c == '\t').count();
        let trimmed = raw
            .trim()
            .trim_start_matches("root")
            .trim()
            .trim_matches(|c| matches!(c, '(' | ')' | '[' | ']' | '{' | '}'));
        let lbl = clean_label(trimmed);
        if lbl.is_empty() {
            continue;
        }
        seq += 1;
        let id = format!("mm_{seq}");
        let (wrapped, w, h) = wrap_node_label(&lbl);
        nodes.push(FlowNode {
            id: id.clone(),
            label: lbl,
            shape: if seq == 1 {
                FlowShape::Stadium
            } else {
                FlowShape::Round
            },
            subgraph_id: None,
            rank: 0,
            order: 0,
            x: 0.0,
            y: 0.0,
            w,
            h,
            lines: wrapped,
        });
        while stack.last().is_some_and(|(ind, _)| *ind >= indent) {
            stack.pop();
        }
        if let Some((_, pid)) = stack.last() {
            edges.push(FlowEdge {
                from: pid.clone(),
                to: id.clone(),
                label: String::new(),
                style: EdgeLineStyle::Solid,
                has_arrow: false,
            });
        }
        stack.push((indent, id));
    }

    if nodes.is_empty() {
        return None;
    }
    let mut subgraphs = Vec::new();
    layout_flowchart(&mut nodes, &edges, &mut subgraphs, FlowDirection::LR);
    let max_x = nodes
        .iter()
        .map(|n| n.x + n.w * 0.5)
        .fold(200.0f32, f32::max);
    let max_y = nodes
        .iter()
        .map(|n| n.y + n.h * 0.5)
        .fold(140.0f32, f32::max);
    Some(ParsedFlowchart {
        kind_label: "MINDMAP".into(),
        direction: FlowDirection::LR,
        theme,
        curve,
        nodes,
        edges,
        subgraphs,
        width: max_x + 28.0,
        height: max_y + 28.0,
    })
}

fn layout_flowchart(
    nodes: &mut [FlowNode],
    edges: &[FlowEdge],
    subgraphs: &mut [FlowSubgraph],
    direction: FlowDirection,
) {
    let n = nodes.len();
    let idx_map: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, nd)| (nd.id.as_str(), i))
        .collect();

    // Remove cyclic back-edges for rank assignment via DFS state (0=unvisited, 1=visiting, 2=done)
    let mut state = vec![0u8; n];
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for e in edges {
        if let (Some(&s), Some(&t)) = (idx_map.get(e.from.as_str()), idx_map.get(e.to.as_str())) {
            if s != t {
                adj[s].push(t);
            }
        }
    }

    let mut dag_edges: Vec<(usize, usize)> = Vec::new();
    fn dfs(u: usize, adj: &[Vec<usize>], state: &mut [u8], dag: &mut Vec<(usize, usize)>) {
        state[u] = 1;
        for &v in &adj[u] {
            if state[v] == 0 {
                dag.push((u, v));
                dfs(v, adj, state, dag);
            } else if state[v] == 2 {
                dag.push((u, v));
            }
        }
        state[u] = 2;
    }
    for i in 0..n {
        if state[i] == 0 {
            dfs(i, &adj, &mut state, &mut dag_edges);
        }
    }

    // Bellman-Ford longest path on DAG edges for clean rank assignment
    let mut ranks = vec![0usize; n];
    for _ in 0..n {
        let mut changed = false;
        for &(u, v) in &dag_edges {
            if ranks[v] < ranks[u] + 1 {
                ranks[v] = ranks[u] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let max_rank = ranks.iter().copied().max().unwrap_or(0);
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); max_rank + 1];
    for (i, &r) in ranks.iter().enumerate() {
        nodes[i].rank = r;
        layers[r].push(i);
    }

    // 2-pass barycenter sweep to minimize crossings
    let mut pos_in_layer = vec![0usize; n];
    for layer in &layers {
        for (ord, &ni) in layer.iter().enumerate() {
            pos_in_layer[ni] = ord;
        }
    }

    let mut rev_adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(u, v) in &dag_edges {
        rev_adj[v].push(u);
    }

    for _ in 0..2 {
        for r in 1..=max_rank {
            layers[r].sort_by(|&a, &b| {
                let bary = |u: usize| -> f32 {
                    if rev_adj[u].is_empty() {
                        pos_in_layer[u] as f32
                    } else {
                        rev_adj[u]
                            .iter()
                            .map(|&p| pos_in_layer[p] as f32)
                            .sum::<f32>()
                            / rev_adj[u].len() as f32
                    }
                };
                bary(a)
                    .partial_cmp(&bary(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            for (ord, &ni) in layers[r].iter().enumerate() {
                pos_in_layer[ni] = ord;
            }
        }
    }

    let horizontal = matches!(direction, FlowDirection::LR | FlowDirection::RL);
    let reversed = matches!(direction, FlowDirection::BT | FlowDirection::RL);
    let rank_gap = if horizontal { 64.0 } else { 54.0 };
    let node_gap = if horizontal { 32.0 } else { 36.0 };

    // Measure each layer's span along the cross axis and depth along main axis
    let mut layer_depths = Vec::with_capacity(max_rank + 1);
    let mut layer_cross_spans = Vec::with_capacity(max_rank + 1);
    let mut max_cross_span = 0.0f32;

    for layer in &layers {
        let mut depth = 40.0f32;
        let mut span = 0.0f32;
        for (k, &ni) in layer.iter().enumerate() {
            let main_dim = if horizontal { nodes[ni].w } else { nodes[ni].h };
            let cross_dim = if horizontal { nodes[ni].h } else { nodes[ni].w };
            depth = depth.max(main_dim);
            if k > 0 {
                span += node_gap;
            }
            span += cross_dim;
        }
        max_cross_span = max_cross_span.max(span);
        layer_depths.push(depth);
        layer_cross_spans.push(span);
    }

    let pad = 34.0;
    let total_main: f32 =
        layer_depths.iter().sum::<f32>() + (layers.len().saturating_sub(1) as f32) * rank_gap;

    let mut cur_main = pad;
    for r in 0..=max_rank {
        let r_idx = if reversed { max_rank - r } else { r };
        let layer = &layers[r_idx];
        let depth = layer_depths[r_idx];
        let span = layer_cross_spans[r_idx];
        let mut cur_cross = pad + (max_cross_span - span) * 0.5;

        for &ni in layer {
            let main_center = cur_main + depth * 0.5;
            let cross_dim = if horizontal { nodes[ni].h } else { nodes[ni].w };
            let cross_center = cur_cross + cross_dim * 0.5;
            if horizontal {
                nodes[ni].x = main_center;
                nodes[ni].y = cross_center;
            } else {
                nodes[ni].x = cross_center;
                nodes[ni].y = main_center;
            }
            cur_cross += cross_dim + node_gap;
        }
        cur_main += depth + rank_gap;
    }
    let _ = total_main;

    // Subgraph bounding rectangles
    let map_after: HashMap<&str, &FlowNode> = nodes.iter().map(|nd| (nd.id.as_str(), nd)).collect();
    for sg in subgraphs.iter_mut() {
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for nid in &sg.node_ids {
            if let Some(nd) = map_after.get(nid.as_str()) {
                min_x = min_x.min(nd.x - nd.w * 0.5);
                max_x = max_x.max(nd.x + nd.w * 0.5);
                min_y = min_y.min(nd.y - nd.h * 0.5);
                max_y = max_y.max(nd.y + nd.h * 0.5);
            }
        }
        if min_x.is_finite() {
            sg.x = min_x - 16.0;
            sg.y = min_y - 26.0;
            sg.w = (max_x - min_x) + 32.0;
            sg.h = (max_y - min_y) + 42.0;
        }
    }
}

fn edge_endpoints(src: &FlowNode, dst: &FlowNode, dir: FlowDirection) -> (Pos2, Pos2, bool) {
    let horizontal = matches!(dir, FlowDirection::LR | FlowDirection::RL);
    if horizontal {
        let forward = dst.x >= src.x;
        if forward {
            (
                Pos2::new(src.x + src.w * 0.5, src.y),
                Pos2::new(dst.x - dst.w * 0.5, dst.y),
                true,
            )
        } else {
            (
                Pos2::new(src.x - src.w * 0.5, src.y),
                Pos2::new(dst.x + dst.w * 0.5, dst.y),
                true,
            )
        }
    } else {
        let down = dst.y >= src.y;
        if down {
            (
                Pos2::new(src.x, src.y + src.h * 0.5),
                Pos2::new(dst.x, dst.y - dst.h * 0.5),
                false,
            )
        } else {
            (
                Pos2::new(src.x, src.y - src.h * 0.5),
                Pos2::new(dst.x, dst.y + dst.h * 0.5),
                false,
            )
        }
    }
}

fn sample_cubic_bezier(p0: Pos2, c1: Pos2, c2: Pos2, p3: Pos2, steps: usize) -> Vec<Pos2> {
    let mut pts = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let mt = 1.0 - t;
        let x = mt * mt * mt * p0.x
            + 3.0 * mt * mt * t * c1.x
            + 3.0 * mt * t * t * c2.x
            + t * t * t * p3.x;
        let y = mt * mt * mt * p0.y
            + 3.0 * mt * mt * t * c1.y
            + 3.0 * mt * t * t * c2.y
            + t * t * t * p3.y;
        pts.push(Pos2::new(x, y));
    }
    pts
}

fn edge_polyline(
    p0: Pos2,
    p1: Pos2,
    horizontal: bool,
    curve: FlowCurveMode,
) -> (Vec<Pos2>, Pos2, Pos2) {
    match curve {
        FlowCurveMode::Ortho => {
            if horizontal {
                let mid_x = (p0.x + p1.x) * 0.5;
                let pts = vec![p0, Pos2::new(mid_x, p0.y), Pos2::new(mid_x, p1.y), p1];
                let mid = Pos2::new(mid_x, (p0.y + p1.y) * 0.5);
                let prev = Pos2::new(mid_x, p1.y);
                (pts, mid, prev)
            } else {
                let mid_y = (p0.y + p1.y) * 0.5;
                let pts = vec![p0, Pos2::new(p0.x, mid_y), Pos2::new(p1.x, mid_y), p1];
                let mid = Pos2::new((p0.x + p1.x) * 0.5, mid_y);
                let prev = Pos2::new(p1.x, mid_y);
                (pts, mid, prev)
            }
        }
        FlowCurveMode::Smooth => {
            let (c1, c2) = if horizontal {
                let dx = (p1.x - p0.x) * 0.48;
                (Pos2::new(p0.x + dx, p0.y), Pos2::new(p1.x - dx, p1.y))
            } else {
                let dy = (p1.y - p0.y) * 0.48;
                (Pos2::new(p0.x, p0.y + dy), Pos2::new(p1.x, p1.y - dy))
            };
            let pts = sample_cubic_bezier(p0, c1, c2, p1, 16);
            let mid = pts[pts.len() / 2];
            let prev = pts[pts.len().saturating_sub(2)];
            (pts, mid, prev)
        }
    }
}

fn ellipse_poly(cx: f32, cy: f32, rx: f32, ry: f32, steps: usize) -> Vec<Pos2> {
    (0..steps)
        .map(|i| {
            let ang = (i as f32 / steps as f32) * std::f32::consts::TAU;
            Pos2::new(cx + ang.cos() * rx, cy + ang.sin() * ry)
        })
        .collect()
}

pub fn render_diagram_card(ui: &mut egui::Ui, pal: &ThemePalette, code: &str) -> DiagramCardAction {
    let Some(parsed) = parse_diagram(code) else {
        return DiagramCardAction::None;
    };

    let mut action = DiagramCardAction::None;
    let colors = parsed.theme.colors(pal.is_dark);

    egui::Frame::none()
        .fill(pal.bg_card)
        .stroke(Stroke::new(1.0_f32, pal.border_subtle))
        .rounding(Rounding::same(8.0))
        .inner_margin(Margin::same(10.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            // 카드 헤더 바 (WebView `.onyx-flow-toolbar` 대응)
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("◇ {}", parsed.kind_label))
                        .size(11.0)
                        .strong()
                        .color(colors.accent),
                );
                ui.label(
                    RichText::new(format!(
                        "{}노드 · {}연결",
                        parsed.nodes.len(),
                        parsed.edges.len()
                    ))
                    .size(10.5)
                    .color(pal.text_muted),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("</> 코드")
                        .on_hover_text("마크다운 코드 블록 직접 편집")
                        .clicked()
                    {
                        action = DiagramCardAction::RequestEditCode;
                    }
                    if ui
                        .small_button("🛠️ 빌더")
                        .on_hover_text("비주얼 플로우차트 빌더 열기")
                        .clicked()
                    {
                        action = DiagramCardAction::OpenBuilder(code.to_string());
                    }
                    if ui
                        .small_button(match parsed.curve {
                            FlowCurveMode::Smooth => "〰 곡선",
                            FlowCurveMode::Ortho => "📐 직각",
                        })
                        .on_hover_text("연결선 스타일 전환 (곡선/직각)")
                        .clicked()
                    {
                        action = DiagramCardAction::UpdateCode(toggle_flowchart_curve(code));
                    }
                    if ui
                        .small_button(format!("🎨 {}", parsed.theme.label_ko()))
                        .on_hover_text("다이어그램 색상 테마 변경")
                        .clicked()
                    {
                        action = DiagramCardAction::UpdateCode(cycle_flowchart_theme(code));
                    }
                    if ui
                        .small_button("⇄ 방향")
                        .on_hover_text("흐름 방향 전환 (TB → LR → BT → RL)")
                        .clicked()
                    {
                        action = DiagramCardAction::UpdateCode(toggle_flowchart_direction(code));
                    }
                });
            });

            ui.add_space(4.0);

            let avail_w = ui.available_width().max(200.0);
            let scale = (avail_w / parsed.width.max(1.0)).min(1.15).max(0.45);
            let canvas_w = (parsed.width * scale).max(avail_w.min(parsed.width));
            let canvas_h = (parsed.height * scale).max(110.0);

            let (rect, _) = ui.allocate_exact_size(Vec2::new(canvas_w, canvas_h), Sense::hover());
            let painter = ui.painter_at(rect);

            let ox = rect.left() + (rect.width() - parsed.width * scale) * 0.5;
            let oy = rect.top();
            let to_pt = |x: f32, y: f32| Pos2::new(ox + x * scale, oy + y * scale);

            // 1. 서브그래프 그룹 박스
            for sg in &parsed.subgraphs {
                if sg.w <= 0.0 {
                    continue;
                }
                let r =
                    Rect::from_min_size(to_pt(sg.x, sg.y), Vec2::new(sg.w * scale, sg.h * scale));
                painter.rect_filled(r, Rounding::same(8.0 * scale), colors.sub_fill);
                painter.rect_stroke(
                    r,
                    Rounding::same(8.0 * scale),
                    Stroke::new(1.2_f32, colors.sub_stroke),
                );
                painter.text(
                    Pos2::new(r.left() + 10.0, r.top() + 10.0),
                    Align2::LEFT_CENTER,
                    &sg.title,
                    FontId::proportional((11.5 * scale).max(9.5)),
                    colors.accent,
                );
            }

            // 2. 연결선(Edges) 및 라벨
            let node_map: HashMap<&str, &FlowNode> =
                parsed.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

            for e in &parsed.edges {
                let (Some(src), Some(dst)) =
                    (node_map.get(e.from.as_str()), node_map.get(e.to.as_str()))
                else {
                    continue;
                };
                let (p0w, p1w, horiz) = edge_endpoints(src, dst, parsed.direction);
                let p0 = to_pt(p0w.x, p0w.y);
                let p1 = to_pt(p1w.x, p1w.y);
                let (pts, mid, prev) = edge_polyline(p0, p1, horiz, parsed.curve);
                let width = match e.style {
                    EdgeLineStyle::Thick => 2.6_f32,
                    _ => 1.6_f32,
                };
                let stroke = Stroke::new(width, colors.edge);
                if e.style == EdgeLineStyle::Dotted {
                    painter.add(Shape::dashed_line(&pts, stroke, 5.0, 4.0));
                } else {
                    painter.add(Shape::line(pts, stroke));
                }

                if e.has_arrow {
                    let dir_v = (p1 - prev).normalized();
                    let perp = Vec2::new(-dir_v.y, dir_v.x);
                    let arrow_len = 8.0 * scale.min(1.0).max(0.75);
                    let arrow_w = 4.2 * scale.min(1.0).max(0.75);
                    let base = p1 - dir_v * arrow_len;
                    painter.add(Shape::convex_polygon(
                        vec![p1, base + perp * arrow_w, base - perp * arrow_w],
                        colors.edge,
                        Stroke::NONE,
                    ));
                }

                if !e.label.is_empty() {
                    let lbl_w = (measure_line_px(&e.label) * scale + 14.0).max(26.0);
                    let lbl_rect = Rect::from_center_size(mid, Vec2::new(lbl_w, 18.0));
                    painter.rect_filled(lbl_rect, Rounding::same(9.0), pal.bg_card);
                    painter.rect_stroke(
                        lbl_rect,
                        Rounding::same(9.0),
                        Stroke::new(1.0_f32, colors.edge),
                    );
                    painter.text(
                        mid,
                        Align2::CENTER_CENTER,
                        &e.label,
                        FontId::proportional((10.5 * scale).max(9.0)),
                        colors.node_text,
                    );
                }
            }

            // 3. 노드(Nodes) 도형 렌더링
            for nd in &parsed.nodes {
                let c = to_pt(nd.x, nd.y);
                let size = Vec2::new(nd.w * scale, nd.h * scale);
                let nr = Rect::from_center_size(c, size);
                let stroke = Stroke::new(1.6_f32, colors.node_stroke);

                match nd.shape {
                    FlowShape::Rect => {
                        painter.rect_filled(nr, Rounding::same(4.0 * scale), colors.node_fill);
                        painter.rect_stroke(nr, Rounding::same(4.0 * scale), stroke);
                    }
                    FlowShape::Round => {
                        painter.rect_filled(nr, Rounding::same(9.0 * scale), colors.node_fill);
                        painter.rect_stroke(nr, Rounding::same(9.0 * scale), stroke);
                    }
                    FlowShape::Stadium => {
                        let rr = (size.y * 0.5).max(8.0);
                        painter.rect_filled(nr, Rounding::same(rr), colors.node_fill);
                        painter.rect_stroke(nr, Rounding::same(rr), stroke);
                    }
                    FlowShape::Circle => {
                        let rad = size.x.min(size.y) * 0.5;
                        painter.circle_filled(c, rad, colors.node_fill);
                        painter.circle_stroke(c, rad, stroke);
                    }
                    FlowShape::Diamond => {
                        let pts = vec![
                            Pos2::new(c.x, nr.top()),
                            Pos2::new(nr.right(), c.y),
                            Pos2::new(c.x, nr.bottom()),
                            Pos2::new(nr.left(), c.y),
                        ];
                        painter.add(Shape::convex_polygon(pts, colors.node_fill, stroke));
                    }
                    FlowShape::Hexagon => {
                        let inset = (size.x * 0.14).min(14.0);
                        let pts = vec![
                            Pos2::new(nr.left() + inset, nr.top()),
                            Pos2::new(nr.right() - inset, nr.top()),
                            Pos2::new(nr.right(), c.y),
                            Pos2::new(nr.right() - inset, nr.bottom()),
                            Pos2::new(nr.left() + inset, nr.bottom()),
                            Pos2::new(nr.left(), c.y),
                        ];
                        painter.add(Shape::convex_polygon(pts, colors.node_fill, stroke));
                    }
                    FlowShape::Subroutine => {
                        painter.rect_filled(nr, Rounding::same(4.0 * scale), colors.node_fill);
                        painter.rect_stroke(nr, Rounding::same(4.0 * scale), stroke);
                        let off = 7.0 * scale;
                        painter.line_segment(
                            [
                                Pos2::new(nr.left() + off, nr.top()),
                                Pos2::new(nr.left() + off, nr.bottom()),
                            ],
                            stroke,
                        );
                        painter.line_segment(
                            [
                                Pos2::new(nr.right() - off, nr.top()),
                                Pos2::new(nr.right() - off, nr.bottom()),
                            ],
                            stroke,
                        );
                    }
                    FlowShape::Cylinder => {
                        let cap_ry = (size.y * 0.14).max(4.5);
                        let body = Rect::from_min_max(
                            Pos2::new(nr.left(), nr.top() + cap_ry),
                            Pos2::new(nr.right(), nr.bottom() - cap_ry),
                        );
                        painter.rect_filled(body, Rounding::ZERO, colors.node_fill);
                        let bot_cap =
                            ellipse_poly(c.x, nr.bottom() - cap_ry, size.x * 0.5, cap_ry, 24);
                        painter.add(Shape::convex_polygon(
                            bot_cap.clone(),
                            colors.node_fill,
                            stroke,
                        ));
                        painter.rect_filled(body, Rounding::ZERO, colors.node_fill);
                        painter.line_segment(
                            [
                                Pos2::new(nr.left(), body.top()),
                                Pos2::new(nr.left(), body.bottom()),
                            ],
                            stroke,
                        );
                        painter.line_segment(
                            [
                                Pos2::new(nr.right(), body.top()),
                                Pos2::new(nr.right(), body.bottom()),
                            ],
                            stroke,
                        );
                        let top_cap =
                            ellipse_poly(c.x, nr.top() + cap_ry, size.x * 0.5, cap_ry, 24);
                        painter.add(Shape::convex_polygon(top_cap, colors.node_fill, stroke));
                    }
                }

                let text_font = (12.0 * scale).max(9.5);
                let line_h = text_font * 1.25;
                let total_th = nd.lines.len() as f32 * line_h;
                let mut ty = c.y - total_th * 0.5 + line_h * 0.5;
                for line in &nd.lines {
                    painter.text(
                        Pos2::new(c.x, ty),
                        Align2::CENTER_CENTER,
                        line,
                        FontId::proportional(text_font),
                        colors.node_text,
                    );
                    ty += line_h;
                }
            }
        });

    action
}

fn c32_hex(c: Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

fn escape_xml(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// 인쇄 및 PDF 내보내기 HTML 안에 그대로 삽입할 독립 `<svg>` 벡터 다이어그램을 생성합니다.
pub fn render_diagram_to_svg(code: &str) -> Option<String> {
    let parsed = parse_diagram(code)?;
    let colors = parsed.theme.colors(false);
    let fill_hex = c32_hex(colors.node_fill);
    let stroke_hex = c32_hex(colors.node_stroke);
    let text_hex = c32_hex(colors.node_text);
    let edge_hex = c32_hex(colors.edge);
    let sub_stroke_hex = c32_hex(colors.sub_stroke);
    let accent_hex = c32_hex(colors.accent);

    let mut out = String::new();
    out.push_str(&format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {:.0} {:.0}" style="max-width:100%;height:auto;display:block;margin:10px auto;">"#,
        parsed.width, parsed.height
    ));
    out.push_str(&format!(
        r#"<defs><marker id="onyx-arr" markerWidth="10" markerHeight="7" refX="9" refY="3.5" orient="auto"><polygon points="0 0, 10 3.5, 0 7" fill="{edge_hex}"/></marker></defs>"#
    ));

    for sg in &parsed.subgraphs {
        if sg.w <= 0.0 {
            continue;
        }
        out.push_str(&format!(
            r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="8" fill="#F1F5F9" fill-opacity="0.7" stroke="{sub_stroke_hex}" stroke-width="1.2"/>"##,
            sg.x, sg.y, sg.w, sg.h
        ));
        out.push_str(&format!(
            r#"<text x="{:.1}" y="{:.1}" font-size="11.5" font-weight="700" fill="{accent_hex}">{}</text>"#,
            sg.x + 10.0,
            sg.y + 15.0,
            escape_xml(&sg.title)
        ));
    }

    let node_map: HashMap<&str, &FlowNode> =
        parsed.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    for e in &parsed.edges {
        let (Some(src), Some(dst)) = (node_map.get(e.from.as_str()), node_map.get(e.to.as_str()))
        else {
            continue;
        };
        let (p0, p1, horiz) = edge_endpoints(src, dst, parsed.direction);
        let d_attr = match parsed.curve {
            FlowCurveMode::Ortho => {
                if horiz {
                    let mx = (p0.x + p1.x) * 0.5;
                    format!(
                        "M {:.1} {:.1} L {:.1} {:.1} L {:.1} {:.1} L {:.1} {:.1}",
                        p0.x, p0.y, mx, p0.y, mx, p1.y, p1.x, p1.y
                    )
                } else {
                    let my = (p0.y + p1.y) * 0.5;
                    format!(
                        "M {:.1} {:.1} L {:.1} {:.1} L {:.1} {:.1} L {:.1} {:.1}",
                        p0.x, p0.y, p0.x, my, p1.x, my, p1.x, p1.y
                    )
                }
            }
            FlowCurveMode::Smooth => {
                if horiz {
                    let dx = (p1.x - p0.x) * 0.48;
                    format!(
                        "M {:.1} {:.1} C {:.1} {:.1}, {:.1} {:.1}, {:.1} {:.1}",
                        p0.x,
                        p0.y,
                        p0.x + dx,
                        p0.y,
                        p1.x - dx,
                        p1.y,
                        p1.x,
                        p1.y
                    )
                } else {
                    let dy = (p1.y - p0.y) * 0.48;
                    format!(
                        "M {:.1} {:.1} C {:.1} {:.1}, {:.1} {:.1}, {:.1} {:.1}",
                        p0.x,
                        p0.y,
                        p0.x,
                        p0.y + dy,
                        p1.x,
                        p1.y - dy,
                        p1.x,
                        p1.y
                    )
                }
            }
        };
        let sw = match e.style {
            EdgeLineStyle::Thick => "2.5",
            _ => "1.5",
        };
        let dash = if e.style == EdgeLineStyle::Dotted {
            r#" stroke-dasharray="5,4""#
        } else {
            ""
        };
        let marker = if e.has_arrow {
            r#" marker-end="url(#onyx-arr)""#
        } else {
            ""
        };
        out.push_str(&format!(
            r#"<path d="{d_attr}" fill="none" stroke="{edge_hex}" stroke-width="{sw}"{dash}{marker}/>"#
        ));

        if !e.label.is_empty() {
            let mx = (p0.x + p1.x) * 0.5;
            let my = (p0.y + p1.y) * 0.5;
            let lw = measure_line_px(&e.label) + 12.0;
            out.push_str(&format!(
                r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="18" rx="9" fill="#FFFFFF" stroke="{edge_hex}" stroke-width="1"/>"##,
                mx - lw * 0.5,
                my - 9.0,
                lw
            ));
            out.push_str(&format!(
                r#"<text x="{:.1}" y="{:.1}" text-anchor="middle" dominant-baseline="central" font-size="10.5" fill="{text_hex}">{}</text>"#,
                mx,
                my,
                escape_xml(&e.label)
            ));
        }
    }

    for nd in &parsed.nodes {
        let x0 = nd.x - nd.w * 0.5;
        let y0 = nd.y - nd.h * 0.5;
        match nd.shape {
            FlowShape::Diamond => {
                out.push_str(&format!(
                    r#"<polygon points="{:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}" fill="{fill_hex}" stroke="{stroke_hex}" stroke-width="1.6"/>"#,
                    nd.x,
                    y0,
                    x0 + nd.w,
                    nd.y,
                    nd.x,
                    y0 + nd.h,
                    x0,
                    nd.y
                ));
            }
            FlowShape::Circle => {
                let rad = nd.w.min(nd.h) * 0.5;
                out.push_str(&format!(
                    r#"<circle cx="{:.1}" cy="{:.1}" r="{:.1}" fill="{fill_hex}" stroke="{stroke_hex}" stroke-width="1.6"/>"#,
                    nd.x, nd.y, rad
                ));
            }
            FlowShape::Stadium => {
                let rx = nd.h * 0.5;
                out.push_str(&format!(
                    r#"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="{:.1}" fill="{fill_hex}" stroke="{stroke_hex}" stroke-width="1.6"/>"#,
                    x0, y0, nd.w, nd.h, rx
                ));
            }
            _ => {
                out.push_str(&format!(
                    r#"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="7" fill="{fill_hex}" stroke="{stroke_hex}" stroke-width="1.6"/>"#,
                    x0, y0, nd.w, nd.h
                ));
            }
        }
        let line_h = 15.0;
        let mut ty = nd.y - ((nd.lines.len() as f32 - 1.0) * line_h) * 0.5;
        for l in &nd.lines {
            out.push_str(&format!(
                r#"<text x="{:.1}" y="{:.1}" text-anchor="middle" dominant-baseline="central" font-size="12" fill="{text_hex}">{}</text>"#,
                nd.x,
                ty,
                escape_xml(l)
            ));
            ty += line_h;
        }
    }

    out.push_str("</svg>");
    Some(out)
}

#[derive(Clone, Debug)]
pub struct BuilderNodeItem {
    pub id: String,
    pub label: String,
    pub shape: FlowShape,
}

#[derive(Clone, Debug)]
pub struct BuilderEdgeItem {
    pub from: String,
    pub to: String,
    pub label: String,
    pub dotted: bool,
}

#[derive(Clone, Debug)]
pub struct FlowBuilderState {
    pub block_idx: Option<usize>,
    pub direction: FlowDirection,
    pub theme: FlowThemeName,
    pub curve: FlowCurveMode,
    pub nodes: Vec<BuilderNodeItem>,
    pub edges: Vec<BuilderEdgeItem>,
    pub raw_code: String,
}

impl FlowBuilderState {
    pub fn from_code(block_idx: Option<usize>, code: &str) -> Self {
        let mut st = if let Some(parsed) = parse_diagram(code) {
            let nodes: Vec<BuilderNodeItem> = parsed
                .nodes
                .into_iter()
                .map(|n| BuilderNodeItem {
                    id: n.id,
                    label: n.label,
                    shape: n.shape,
                })
                .collect();
            let edges: Vec<BuilderEdgeItem> = parsed
                .edges
                .into_iter()
                .map(|e| BuilderEdgeItem {
                    from: e.from,
                    to: e.to,
                    label: e.label,
                    dotted: e.style == EdgeLineStyle::Dotted,
                })
                .collect();
            Self {
                block_idx,
                direction: parsed.direction,
                theme: parsed.theme,
                curve: parsed.curve,
                nodes,
                edges,
                raw_code: code.to_string(),
            }
        } else {
            Self {
                block_idx,
                direction: FlowDirection::TB,
                theme: FlowThemeName::Modern,
                curve: FlowCurveMode::Smooth,
                nodes: vec![
                    BuilderNodeItem {
                        id: "A".into(),
                        label: "시작".into(),
                        shape: FlowShape::Stadium,
                    },
                    BuilderNodeItem {
                        id: "B".into(),
                        label: "검토 조건".into(),
                        shape: FlowShape::Diamond,
                    },
                    BuilderNodeItem {
                        id: "C".into(),
                        label: "완료".into(),
                        shape: FlowShape::Rect,
                    },
                ],
                edges: vec![
                    BuilderEdgeItem {
                        from: "A".into(),
                        to: "B".into(),
                        label: String::new(),
                        dotted: false,
                    },
                    BuilderEdgeItem {
                        from: "B".into(),
                        to: "C".into(),
                        label: "승인".into(),
                        dotted: false,
                    },
                ],
                raw_code: String::new(),
            }
        };
        st.rebuild_code();
        st
    }

    pub fn rebuild_code(&mut self) {
        let mut out = Vec::new();
        out.push(format!("%% onyx-theme: {}", self.theme.as_str()));
        out.push(format!("%% onyx-curve: {}", self.curve.as_str()));
        out.push(format!("flowchart {}", self.direction.as_str()));
        for nd in &self.nodes {
            let clean_id = if nd.id.trim().is_empty() {
                "N"
            } else {
                nd.id.trim()
            };
            let clean_lbl = if nd.label.trim().is_empty() {
                clean_id
            } else {
                nd.label.trim()
            };
            out.push(format!("    {}", nd.shape.wrap_label(clean_id, clean_lbl)));
        }
        for e in &self.edges {
            if e.from.trim().is_empty() || e.to.trim().is_empty() {
                continue;
            }
            let arrow = if e.dotted { "-.->" } else { "-->" };
            if e.label.trim().is_empty() {
                out.push(format!("    {} {} {}", e.from.trim(), arrow, e.to.trim()));
            } else {
                out.push(format!(
                    "    {} {}|{}| {}",
                    e.from.trim(),
                    arrow,
                    e.label.trim(),
                    e.to.trim()
                ));
            }
        }
        self.raw_code = out.join("\n");
    }
}

pub enum BuilderModalAction {
    None,
    Close,
    Apply {
        block_idx: Option<usize>,
        code: String,
    },
}

pub fn render_diagram_builder_modal(
    ctx: &egui::Context,
    pal: &ThemePalette,
    st: &mut FlowBuilderState,
) -> BuilderModalAction {
    let mut act = BuilderModalAction::None;
    let mut open = true;

    egui::Window::new("◇ 비주얼 플로우차트 빌더")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size([780.0, 560.0])
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            let mut changed = false;

            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("방향").strong().size(12.0));
                for d in [
                    FlowDirection::TB,
                    FlowDirection::LR,
                    FlowDirection::BT,
                    FlowDirection::RL,
                ] {
                    if ui.selectable_label(st.direction == d, d.as_str()).clicked() {
                        st.direction = d;
                        changed = true;
                    }
                }
                ui.separator();
                ui.label(RichText::new("테마").strong().size(12.0));
                for th in [
                    FlowThemeName::Modern,
                    FlowThemeName::Ocean,
                    FlowThemeName::Emerald,
                    FlowThemeName::Sunset,
                    FlowThemeName::Mono,
                ] {
                    if ui.selectable_label(st.theme == th, th.label_ko()).clicked() {
                        st.theme = th;
                        changed = true;
                    }
                }
                ui.separator();
                if ui
                    .selectable_label(st.curve == FlowCurveMode::Smooth, "곡선")
                    .clicked()
                {
                    st.curve = FlowCurveMode::Smooth;
                    changed = true;
                }
                if ui
                    .selectable_label(st.curve == FlowCurveMode::Ortho, "직각")
                    .clicked()
                {
                    st.curve = FlowCurveMode::Ortho;
                    changed = true;
                }
            });

            ui.separator();

            ui.columns(2, |cols| {
                egui::ScrollArea::vertical()
                    .id_salt("flow_builder_left")
                    .max_height(390.0)
                    .show(&mut cols[0], |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("노드 목록").strong());
                            if ui.small_button("＋ 노드 추가").clicked() {
                                let next_id = format!("N{}", st.nodes.len() + 1);
                                st.nodes.push(BuilderNodeItem {
                                    id: next_id.clone(),
                                    label: format!("단계 {}", st.nodes.len() + 1),
                                    shape: FlowShape::Round,
                                });
                                changed = true;
                            }
                        });
                        let mut del_node: Option<usize> = None;
                        let can_del_node = st.nodes.len() > 1;
                        for (idx, nd) in st.nodes.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                if ui
                                    .add(egui::TextEdit::singleline(&mut nd.id).desired_width(42.0))
                                    .changed()
                                {
                                    changed = true;
                                }
                                if ui
                                    .add(
                                        egui::TextEdit::singleline(&mut nd.label)
                                            .desired_width(120.0),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                                egui::ComboBox::from_id_salt(format!("fb_sh_{idx}"))
                                    .selected_text(format!("{:?}", nd.shape))
                                    .width(86.0)
                                    .show_ui(ui, |ui| {
                                        for &(sh, slbl) in FlowShape::all() {
                                            if ui
                                                .selectable_value(&mut nd.shape, sh, slbl)
                                                .changed()
                                            {
                                                changed = true;
                                            }
                                        }
                                    });
                                if can_del_node && ui.small_button("✕").clicked() {
                                    del_node = Some(idx);
                                }
                            });
                        }
                        if let Some(rm) = del_node {
                            st.nodes.remove(rm);
                            changed = true;
                        }

                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("연결선 목록").strong());
                            if ui.small_button("＋ 연결 추가").clicked() {
                                let from = st
                                    .nodes
                                    .first()
                                    .map(|n| n.id.clone())
                                    .unwrap_or_else(|| "A".into());
                                let to = st
                                    .nodes
                                    .get(1)
                                    .or_else(|| st.nodes.first())
                                    .map(|n| n.id.clone())
                                    .unwrap_or_else(|| "B".into());
                                st.edges.push(BuilderEdgeItem {
                                    from,
                                    to,
                                    label: String::new(),
                                    dotted: false,
                                });
                                changed = true;
                            }
                        });
                        let mut del_edge: Option<usize> = None;
                        for (idx, ed) in st.edges.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                if ui
                                    .add(
                                        egui::TextEdit::singleline(&mut ed.from)
                                            .desired_width(42.0),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                                ui.label("→");
                                if ui
                                    .add(egui::TextEdit::singleline(&mut ed.to).desired_width(42.0))
                                    .changed()
                                {
                                    changed = true;
                                }
                                if ui
                                    .add(
                                        egui::TextEdit::singleline(&mut ed.label)
                                            .hint_text("라벨")
                                            .desired_width(92.0),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                                if ui.checkbox(&mut ed.dotted, "점선").changed() {
                                    changed = true;
                                }
                                if ui.small_button("✕").clicked() {
                                    del_edge = Some(idx);
                                }
                            });
                        }
                        if let Some(rm) = del_edge {
                            st.edges.remove(rm);
                            changed = true;
                        }
                    });

                cols[1].vertical(|ui| {
                    ui.label(RichText::new("실시간 미리보기").strong());
                    egui::ScrollArea::vertical()
                        .id_salt("flow_builder_preview")
                        .max_height(390.0)
                        .show(ui, |ui| {
                            let _ = render_diagram_card(ui, pal, &st.raw_code);
                        });
                });
            });

            if changed {
                st.rebuild_code();
            }

            ui.separator();
            ui.horizontal(|ui| {
                if crate::ui::primary_button(ui, pal, "노트에 삽입 / 업데이트").clicked() {
                    act = BuilderModalAction::Apply {
                        block_idx: st.block_idx,
                        code: st.raw_code.clone(),
                    };
                }
                if crate::ui::secondary_button(ui, pal, "취소").clicked() {
                    act = BuilderModalAction::Close;
                }
            });
        });

    if !open && matches!(act, BuilderModalAction::None) {
        return BuilderModalAction::Close;
    }
    act
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_flowchart_tb_lr_korean_and_subgraph() {
        let code = r#"
%% onyx-theme: ocean
%% onyx-curve: ortho
flowchart TB
    subgraph S1 [인증 파이프라인]
        A([시작 요청]) --> B{토큰 유효?}
    end
    B -->|예| C[(DB 저장)]
    B -->|아니오| D{{오류 반환}}
"#;
        let parsed = parse_diagram(code).expect("flowchart should parse");
        assert_eq!(parsed.direction, FlowDirection::TB);
        assert_eq!(parsed.theme, FlowThemeName::Ocean);
        assert_eq!(parsed.curve, FlowCurveMode::Ortho);
        assert_eq!(parsed.nodes.len(), 4);
        assert_eq!(parsed.edges.len(), 3);
        assert_eq!(parsed.subgraphs.len(), 1);
        assert_eq!(parsed.nodes[0].shape, FlowShape::Stadium);
        assert_eq!(parsed.nodes[1].shape, FlowShape::Diamond);
        assert_eq!(parsed.nodes[2].shape, FlowShape::Cylinder);
        assert_eq!(parsed.nodes[3].shape, FlowShape::Hexagon);

        let toggled = toggle_flowchart_direction(code);
        let parsed2 = parse_diagram(&toggled).unwrap();
        assert_eq!(parsed2.direction, FlowDirection::LR);

        let svg = render_diagram_to_svg(code).expect("svg should generate");
        assert!(svg.starts_with("<svg "));
        assert!(svg.contains("인증 파이프라인"));
        assert!(svg.contains("DB 저장"));
    }

    #[test]
    fn sequence_and_mindmap_parse_and_render_svg() {
        let seq = "sequenceDiagram\nparticipant U as 사용자\nparticipant S as 서버\nU->>S: 로그인";
        let svg_seq = render_diagram_to_svg(seq).unwrap();
        assert!(svg_seq.contains("사용자"));
        assert!(svg_seq.contains("로그인"));

        let mm = "mindmap\n  root((Onyx Studio))\n    에디터\n    동기화";
        let svg_mm = render_diagram_to_svg(mm).unwrap();
        assert!(svg_mm.contains("Onyx Studio"));
        assert!(svg_mm.contains("동기화"));
    }
}
