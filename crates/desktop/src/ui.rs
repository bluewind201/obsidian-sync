//! Notion 스타일 테마 팔레트(라이트 `#FFFFFF`/`#F7F7F5` · 다크 `#202020`/`#191919`, 노션 블루 `#2383E2`),
//! `10pt` 한국어 타이포그래피(`malgun.ttf` + `consola.ttf` + `seguiemj.ttf`), 플랫 버튼 및 벡터 폴더 아이콘.

use eframe::egui::{
    self, Align2, Color32, FontData, FontDefinitions, FontFamily, FontId, Margin, Pos2, Rect,
    RichText, Rounding, Sense, Stroke, Vec2,
};

#[derive(Clone, Copy, Debug)]
pub struct ThemePalette {
    pub is_dark: bool,
    pub bg_main: Color32,
    pub bg_sidebar: Color32,
    pub bg_secondary: Color32,
    pub bg_card: Color32,
    pub bg_inset: Color32,
    pub bg_desk: Color32,
    pub bg_hover: Color32,
    pub bg_active: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,
    pub border_subtle: Color32,
    pub border_strong: Color32,
    pub accent_blue: Color32,
    pub accent_blue_soft: Color32,
    pub accent_purple: Color32,
    pub accent_purple_soft: Color32,
    pub ok: Color32,
    pub ok_bg: Color32,
    pub warn: Color32,
    pub warn_bg: Color32,
    pub danger: Color32,
    pub danger_bg: Color32,
    pub code_bg: Color32,
    pub code_fg: Color32,
    pub mark_bg: Color32,
    pub table_border: Color32,
    pub table_outer: Color32,
    pub table_header_bg: Color32,
    pub table_cell_bg: Color32,
    pub table_text: Color32,
    pub guide_line: Color32,
}

impl ThemePalette {
    pub fn light() -> Self {
        Self {
            is_dark: false,
            bg_main: Color32::WHITE,                        // #FFFFFF
            bg_sidebar: Color32::from_rgb(247, 247, 245),   // #F7F7F5
            bg_secondary: Color32::from_rgb(251, 251, 250), // #FBFBFA
            bg_card: Color32::WHITE,                        // #FFFFFF
            bg_inset: Color32::from_rgb(243, 243, 240),     // #F3F3F0
            bg_desk: Color32::from_rgb(241, 241, 238),      // #F1F1EE
            bg_hover: Color32::from_rgba_unmultiplied(55, 53, 47, 15),
            bg_active: Color32::from_rgba_unmultiplied(55, 53, 47, 26),
            text_primary: Color32::from_rgb(55, 53, 47), // #37352F
            text_secondary: Color32::from_rgb(111, 110, 105), // #6F6E69
            text_muted: Color32::from_rgb(155, 154, 151), // #9B9A97
            border_subtle: Color32::from_rgba_unmultiplied(55, 53, 47, 23),
            border_strong: Color32::from_rgba_unmultiplied(55, 53, 47, 41),
            accent_blue: Color32::from_rgb(35, 131, 226), // #2383E2
            accent_blue_soft: Color32::from_rgba_unmultiplied(35, 131, 226, 31),
            accent_purple: Color32::from_rgb(124, 58, 237), // #7C3AED
            accent_purple_soft: Color32::from_rgba_unmultiplied(124, 58, 237, 28),
            ok: Color32::from_rgb(15, 123, 108), // #0F7B6C
            ok_bg: Color32::from_rgba_unmultiplied(15, 123, 108, 31),
            warn: Color32::from_rgb(203, 123, 25), // #CB7B19
            warn_bg: Color32::from_rgba_unmultiplied(203, 123, 25, 33),
            danger: Color32::from_rgb(224, 62, 62), // #E03E3E
            danger_bg: Color32::from_rgba_unmultiplied(224, 62, 62, 31),
            code_bg: Color32::from_rgb(246, 245, 243), // #F6F5F3
            code_fg: Color32::from_rgb(235, 87, 87),   // #EB5757
            mark_bg: Color32::from_rgba_unmultiplied(250, 204, 21, 108),
            table_border: Color32::from_rgb(203, 213, 225), // #CBD5E1
            table_outer: Color32::from_rgb(148, 163, 184),  // #94A3B8
            table_header_bg: Color32::from_rgb(241, 245, 249), // #F1F5F9
            table_cell_bg: Color32::WHITE,                  // #FFFFFF
            table_text: Color32::from_rgb(30, 41, 59),      // #1E293B
            guide_line: Color32::from_rgba_unmultiplied(55, 53, 47, 22),
        }
    }

    pub fn dark() -> Self {
        Self {
            is_dark: true,
            bg_main: Color32::from_rgb(32, 32, 32), // #202020
            bg_sidebar: Color32::from_rgb(25, 25, 25), // #191919
            bg_secondary: Color32::from_rgb(28, 28, 28), // #1C1C1C
            bg_card: Color32::from_rgb(38, 38, 38), // #262626
            bg_inset: Color32::from_rgb(42, 42, 42), // #2A2A2A
            bg_desk: Color32::from_rgb(22, 22, 22), // #161616
            bg_hover: Color32::from_rgba_unmultiplied(255, 255, 255, 15),
            bg_active: Color32::from_rgba_unmultiplied(255, 255, 255, 28),
            text_primary: Color32::from_rgb(230, 230, 230), // #E6E6E6
            text_secondary: Color32::from_rgb(158, 158, 158), // #9E9E9E
            text_muted: Color32::from_rgb(104, 104, 104),   // #686868
            border_subtle: Color32::from_rgba_unmultiplied(255, 255, 255, 21),
            border_strong: Color32::from_rgba_unmultiplied(255, 255, 255, 38),
            accent_blue: Color32::from_rgb(82, 156, 202), // #529CCA
            accent_blue_soft: Color32::from_rgba_unmultiplied(82, 156, 202, 42),
            accent_purple: Color32::from_rgb(155, 102, 245), // #9B66F5
            accent_purple_soft: Color32::from_rgba_unmultiplied(155, 102, 245, 42),
            ok: Color32::from_rgb(77, 171, 154), // #4DAB9A
            ok_bg: Color32::from_rgba_unmultiplied(77, 171, 154, 41),
            warn: Color32::from_rgb(255, 163, 68), // #FFA344
            warn_bg: Color32::from_rgba_unmultiplied(255, 163, 68, 41),
            danger: Color32::from_rgb(255, 115, 105), // #FF7369
            danger_bg: Color32::from_rgba_unmultiplied(255, 115, 105, 41),
            code_bg: Color32::from_rgb(42, 42, 42), // #2A2A2A
            code_fg: Color32::from_rgb(255, 133, 122), // #FF857A
            mark_bg: Color32::from_rgba_unmultiplied(234, 179, 8, 88),
            table_border: Color32::from_rgb(71, 85, 105), // #475569
            table_outer: Color32::from_rgb(100, 116, 139), // #64748B
            table_header_bg: Color32::from_rgb(40, 48, 64), // #283040
            table_cell_bg: Color32::from_rgb(30, 34, 42), // #1E222A
            table_text: Color32::from_rgb(226, 232, 240), // #E2E8F0
            guide_line: Color32::from_rgba_unmultiplied(255, 255, 255, 18),
        }
    }

    pub fn for_theme(theme: &str) -> Self {
        if theme.eq_ignore_ascii_case("dark") {
            Self::dark()
        } else {
            Self::light()
        }
    }
}

/// `#RRGGBB` 및 `#RGB` 헥스 문자열을 `Color32`로 파싱합니다.
pub fn parse_hex_color(raw: &str) -> Option<Color32> {
    let hex = raw.trim().trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some(Color32::from_rgb(r, g, b))
    } else if hex.len() == 3 {
        let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
        let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
        let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
        Some(Color32::from_rgb(r * 17, g * 17, b * 17))
    } else {
        None
    }
}

pub fn setup_fonts(ctx: &egui::Context) {
    let korean_candidates = [
        r"C:\Windows\Fonts\malgun.ttf",
        r"C:\Windows\Fonts\malgunbd.ttf",
        r"C:\Windows\Fonts\gulim.ttc",
        "/usr/share/fonts/truetype/nanum/NanumGothic.ttf",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/System/Library/Fonts/AppleSDGothicNeo.ttc",
    ];
    let mono_candidates = [
        r"C:\Windows\Fonts\consola.ttf",
        r"C:\Windows\Fonts\lucon.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    ];
    let symbol_candidates = [
        ("win_emoji", r"C:\Windows\Fonts\seguiemj.ttf"),
        ("win_symbol", r"C:\Windows\Fonts\seguisym.ttf"),
    ];

    let mut fonts = FontDefinitions::default();

    let mut loaded_korean = false;
    for path in korean_candidates {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert("korean".into(), FontData::from_owned(bytes));
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .insert(0, "korean".into());
            loaded_korean = true;
            break;
        }
    }

    let mut mono_idx = 0usize;
    for path in mono_candidates {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert("mono_latin".into(), FontData::from_owned(bytes));
            fonts
                .families
                .entry(FontFamily::Monospace)
                .or_default()
                .insert(0, "mono_latin".into());
            mono_idx = 1;
            break;
        }
    }

    if loaded_korean {
        fonts
            .families
            .entry(FontFamily::Monospace)
            .or_default()
            .insert(mono_idx, "korean".into());
    }

    for (name, path) in symbol_candidates {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert(name.into(), FontData::from_owned(bytes));
            for fam in [FontFamily::Proportional, FontFamily::Monospace] {
                fonts.families.entry(fam).or_default().push(name.into());
            }
        }
    }

    ctx.set_fonts(fonts);
}

pub fn apply_theme(ctx: &egui::Context, theme: &str) -> ThemePalette {
    let pal = ThemePalette::for_theme(theme);
    let mut v = if pal.is_dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    v.panel_fill = pal.bg_main;
    v.window_fill = pal.bg_card;
    v.faint_bg_color = pal.bg_inset;
    v.extreme_bg_color = if pal.is_dark {
        Color32::from_rgb(28, 28, 28)
    } else {
        Color32::WHITE
    };
    v.code_bg_color = pal.code_bg;
    v.hyperlink_color = pal.accent_blue;
    v.selection.bg_fill = pal.accent_blue_soft;
    v.selection.stroke = Stroke::new(1.0_f32, pal.accent_blue);

    v.widgets.noninteractive.fg_stroke.color = pal.text_primary;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, pal.border_subtle);
    v.widgets.noninteractive.rounding = Rounding::same(6.0);

    // 기본 weak 버튼은 배경/테두리 없는 플랫 스타일로 유지
    v.widgets.inactive.fg_stroke.color = pal.text_primary;
    v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    v.widgets.inactive.bg_fill = pal.bg_card;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, pal.border_subtle);
    v.widgets.inactive.rounding = Rounding::same(6.0);

    v.widgets.hovered.fg_stroke.color = pal.text_primary;
    v.widgets.hovered.weak_bg_fill = pal.bg_hover;
    v.widgets.hovered.bg_fill = pal.bg_hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, pal.border_strong);
    v.widgets.hovered.rounding = Rounding::same(6.0);

    v.widgets.active.fg_stroke.color = pal.text_primary;
    v.widgets.active.weak_bg_fill = pal.bg_active;
    v.widgets.active.bg_fill = pal.bg_active;
    v.widgets.active.bg_stroke = Stroke::new(1.2_f32, pal.accent_blue);
    v.widgets.active.rounding = Rounding::same(6.0);

    v.window_rounding = Rounding::same(10.0);
    v.window_stroke = Stroke::new(1.0_f32, pal.border_strong);

    ctx.set_visuals(v);

    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 5.0);
        s.spacing.button_padding = egui::vec2(8.0, 4.0);
        s.spacing.interact_size.y = 24.0;
        // 10pt 기준 타이포그래피 (~13.33px)
        s.text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(13.33));
        s.text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(12.5));
        s.text_styles
            .insert(egui::TextStyle::Small, FontId::proportional(11.5));
        s.text_styles
            .insert(egui::TextStyle::Heading, FontId::proportional(20.0));
        s.text_styles
            .insert(egui::TextStyle::Monospace, FontId::monospace(12.8));
    });

    pal
}

pub fn card<R>(ui: &mut egui::Ui, pal: &ThemePalette, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::none()
        .fill(pal.bg_card)
        .stroke(Stroke::new(1.0_f32, pal.border_subtle))
        .rounding(Rounding::same(10.0))
        .inner_margin(Margin::same(16.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

pub fn sub_card<R>(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    egui::Frame::none()
        .fill(pal.bg_secondary)
        .stroke(Stroke::new(1.0_f32, pal.border_subtle))
        .rounding(Rounding::same(8.0))
        .inner_margin(Margin::same(12.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

pub fn status_dot(ui: &mut egui::Ui, color: Color32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), size / 2.0, color);
}

pub fn pill_badge(ui: &mut egui::Ui, text: &str, fg: Color32, bg: Color32) {
    egui::Frame::none()
        .fill(bg)
        .rounding(Rounding::same(99.0))
        .inner_margin(Margin::symmetric(8.0, 2.0))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.5).strong().color(fg));
        });
}

pub fn primary_button(ui: &mut egui::Ui, pal: &ThemePalette, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(Color32::WHITE).strong())
            .fill(pal.accent_blue)
            .stroke(Stroke::NONE)
            .rounding(Rounding::same(6.0))
            .min_size(egui::vec2(0.0, 30.0)),
    )
}

pub fn primary_blue_button(ui: &mut egui::Ui, pal: &ThemePalette, text: &str) -> egui::Response {
    primary_button(ui, pal, text)
}

pub fn secondary_button(ui: &mut egui::Ui, pal: &ThemePalette, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(pal.text_primary).size(12.5))
            .fill(pal.bg_card)
            .stroke(Stroke::new(1.0_f32, pal.border_strong))
            .rounding(Rounding::same(6.0))
            .min_size(egui::vec2(0.0, 28.0)),
    )
}

pub fn danger_button(ui: &mut egui::Ui, pal: &ThemePalette, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(pal.danger).strong())
            .fill(pal.danger_bg)
            .stroke(Stroke::new(1.0_f32, pal.danger))
            .rounding(Rounding::same(6.0))
            .min_size(egui::vec2(0.0, 28.0)),
    )
}

/// 상단바 / 사이드바 헤더용 `26×26` 플랫 아이콘 버튼
pub fn icon_btn(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    text: &str,
    tooltip: &str,
    active: bool,
) -> egui::Response {
    let fill = if active {
        pal.accent_blue_soft
    } else {
        Color32::TRANSPARENT
    };
    let fg = if active {
        pal.accent_blue
    } else {
        pal.text_secondary
    };
    let resp = ui
        .add(
            egui::Button::new(RichText::new(text).size(13.0).color(fg))
                .fill(fill)
                .stroke(Stroke::NONE)
                .rounding(Rounding::same(5.0))
                .min_size(egui::vec2(26.0, 26.0)),
        )
        .on_hover_text(tooltip);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Bar 3 50% 슬림 도구함 버튼 (`.tui-custom-btn` 대응)
pub fn slim_tool_btn(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    text: &str,
    tooltip: &str,
    active: bool,
) -> egui::Response {
    slim_tool_btn_colored(ui, pal, text, tooltip, active, None)
}

pub fn slim_tool_btn_colored(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    text: &str,
    tooltip: &str,
    active: bool,
    bg_override: Option<Color32>,
) -> egui::Response {
    let fill = bg_override.unwrap_or_else(|| {
        if active {
            pal.accent_blue_soft
        } else {
            Color32::TRANSPARENT
        }
    });
    let fg = if active {
        pal.accent_blue
    } else {
        pal.text_primary
    };
    let resp = ui
        .add(
            egui::Button::new(RichText::new(text).size(11.0).strong().color(fg))
                .fill(fill)
                .stroke(Stroke::NONE)
                .rounding(Rounding::same(4.0))
                .min_size(egui::vec2(20.0, 20.0)),
        )
        .on_hover_text(tooltip);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Notion 상단 페이지 컨트롤용 테두리 없는 고스트 버튼 (`😀 아이콘 추가`, `🖼️ 커버 추가` 등)
pub fn ghost_btn(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    text: &str,
    tooltip: &str,
) -> egui::Response {
    let resp = ui
        .add(
            egui::Button::new(RichText::new(text).size(12.0).color(pal.text_muted))
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::NONE)
                .rounding(Rounding::same(5.0))
                .min_size(egui::vec2(0.0, 22.0)),
        )
        .on_hover_text(tooltip);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// 사이드바 단축키 배지 (`Ctrl+K`)
pub fn kbd_pill(ui: &mut egui::Ui, pal: &ThemePalette, shortcut: &str) {
    egui::Frame::none()
        .fill(pal.bg_inset)
        .stroke(Stroke::new(1.0_f32, pal.border_subtle))
        .rounding(Rounding::same(4.0))
        .inner_margin(Margin::symmetric(5.0, 1.0))
        .show(ui, |ui| {
            ui.label(RichText::new(shortcut).size(10.5).color(pal.text_muted));
        });
}

/// 세로 구분선 (50% 슬림 도구함 그룹 구분용)
pub fn toolbar_divider(ui: &mut egui::Ui, pal: &ThemePalette) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(5.0, 16.0), egui::Sense::hover());
    let x = rect.center().x;
    ui.painter().line_segment(
        [
            Pos2::new(x, rect.top() + 2.0),
            Pos2::new(x, rect.bottom() - 2.0),
        ],
        Stroke::new(1.0_f32, pal.border_strong),
    );
}

/// 폴더 펼침/접힘 삼각형 벡터 아이콘
pub fn paint_chevron(painter: &egui::Painter, rect: Rect, expanded: bool, color: Color32) {
    let c = rect.center();
    let pts = if expanded {
        vec![
            Pos2::new(c.x - 3.5, c.y - 2.0),
            Pos2::new(c.x + 3.5, c.y - 2.0),
            Pos2::new(c.x, c.y + 3.0),
        ]
    } else {
        vec![
            Pos2::new(c.x - 2.0, c.y - 3.5),
            Pos2::new(c.x + 3.0, c.y),
            Pos2::new(c.x - 2.0, c.y + 3.5),
        ]
    };
    painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
}

/// 색상 틴트가 즉시 반영되는 SVG 스타일 벡터 폴더 아이콘
pub fn paint_folder_icon(
    painter: &egui::Painter,
    rect: Rect,
    custom_color: Option<Color32>,
    expanded: bool,
    pal: &ThemePalette,
) {
    let base = custom_color.unwrap_or_else(|| {
        if pal.is_dark {
            Color32::from_rgb(203, 168, 92)
        } else {
            Color32::from_rgb(217, 155, 38)
        }
    });
    let tab_col = Color32::from_rgb(
        base.r().saturating_sub(24),
        base.g().saturating_sub(24),
        base.b().saturating_sub(24),
    );

    let w = rect.width().min(16.0);
    let h = rect.height().min(13.0);
    let left = rect.center().x - w * 0.5;
    let top = rect.center().y - h * 0.5;

    // 상단 탭
    let tab_rect = Rect::from_min_size(Pos2::new(left + 1.0, top), Vec2::new(w * 0.44, h * 0.34));
    painter.rect_filled(tab_rect, Rounding::same(2.0), tab_col);

    // 폴더 본체
    let body_rect = Rect::from_min_size(Pos2::new(left, top + h * 0.22), Vec2::new(w, h * 0.78));
    painter.rect_filled(body_rect, Rounding::same(2.5), base);

    // 열림 상태 하이라이트 립
    if expanded {
        let flap = Rect::from_min_size(
            Pos2::new(left + 1.5, top + h * 0.44),
            Vec2::new(w - 3.0, h * 0.52),
        );
        let lighter = Color32::from_rgb(
            base.r().saturating_add(22),
            base.g().saturating_add(22),
            base.b().saturating_add(22),
        );
        painter.rect_filled(flap, Rounding::same(2.0), lighter);
    }
}

pub fn step_card(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    n: u8,
    title: &str,
    done: bool,
    add: impl FnOnce(&mut egui::Ui),
) {
    card(ui, pal, |ui| {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
            let (fill, txt) = if done {
                (pal.ok, "✓".to_string())
            } else {
                (pal.accent_blue, n.to_string())
            };
            ui.painter().circle_filled(rect.center(), 12.0, fill);
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                txt,
                FontId::proportional(12.5),
                Color32::WHITE,
            );
            ui.label(
                RichText::new(title)
                    .size(15.0)
                    .strong()
                    .color(pal.text_primary),
            );
        });
        ui.add_space(6.0);
        add(ui);
    });
}

pub fn backend_choice_card(
    ui: &mut egui::Ui,
    pal: &ThemePalette,
    width: f32,
    badge: &str,
    badge_color: Color32,
    name: &str,
    desc: &str,
    selected: bool,
) -> egui::Response {
    let frame = egui::Frame::none()
        .fill(if selected {
            pal.accent_blue_soft
        } else {
            pal.bg_card
        })
        .stroke(Stroke::new(
            if selected { 2.0_f32 } else { 1.0_f32 },
            if selected {
                pal.accent_blue
            } else {
                pal.border_subtle
            },
        ))
        .rounding(Rounding::same(8.0))
        .inner_margin(Margin::same(10.0));

    let resp = frame
        .show(ui, |ui| {
            ui.set_width((width - 24.0).max(120.0));
            ui.set_min_height(46.0);
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(36.0, 36.0), egui::Sense::hover());
                ui.painter()
                    .rect_filled(rect, Rounding::same(8.0), badge_color);
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    badge,
                    FontId::proportional(12.0),
                    Color32::WHITE,
                );
                ui.vertical(|ui| {
                    ui.label(RichText::new(name).strong().size(13.5).color(if selected {
                        pal.accent_blue
                    } else {
                        pal.text_primary
                    }));
                    ui.label(RichText::new(desc).size(11.5).color(pal.text_secondary));
                });
            });
        })
        .response;

    let resp = resp.interact(Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}
