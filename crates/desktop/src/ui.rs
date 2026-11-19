//! 화면 테마(라이트 `#F6F7FA` / 다크 모드, 오닉스 퍼플 `#7C3AED` · 노션 블루 `#2383E2` 액센트) 및 공통 컴포넌트.
//! 한국어 폰트(`C:\Windows\Fonts\malgun.ttf`)와 이모지/심볼 폴백 폰트(`seguiemj.ttf`, `seguisym.ttf`)를 로드하여
//! 한글·이모지 깨짐 없이 100% 순수 네이티브 Rust GUI(`eframe`/`egui`)로 렌더링합니다.

use eframe::egui::{
    self, Color32, FontData, FontDefinitions, FontFamily, FontId, Margin, RichText, Rounding,
    Stroke, Vec2,
};

#[derive(Clone, Copy, Debug)]
pub struct ThemePalette {
    pub is_dark: bool,
    pub bg_main: Color32,
    pub bg_sidebar: Color32,
    pub bg_secondary: Color32,
    pub bg_card: Color32,
    pub bg_inset: Color32,
    pub bg_hover: Color32,
    pub bg_active: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,
    pub border_subtle: Color32,
    pub border_strong: Color32,
    pub accent_purple: Color32,
    pub accent_purple_soft: Color32,
    pub accent_blue: Color32,
    pub accent_blue_soft: Color32,
    pub ok: Color32,
    pub ok_bg: Color32,
    pub warn: Color32,
    pub warn_bg: Color32,
    pub danger: Color32,
    pub danger_bg: Color32,
    pub code_bg: Color32,
}

impl ThemePalette {
    pub fn light() -> Self {
        Self {
            is_dark: false,
            bg_main: Color32::from_rgb(246, 247, 250), // #F6F7FA
            bg_sidebar: Color32::from_rgb(238, 239, 245), // #EEEFF5
            bg_secondary: Color32::from_rgb(251, 251, 250), // #FBFBFA
            bg_card: Color32::WHITE,                   // #FFFFFF
            bg_inset: Color32::from_rgb(240, 241, 246), // #F0F1F6
            bg_hover: Color32::from_rgb(232, 234, 242),
            bg_active: Color32::from_rgb(224, 226, 237),
            text_primary: Color32::from_rgb(30, 32, 40), // #1E2028
            text_secondary: Color32::from_rgb(95, 99, 110), // #5F636E
            text_muted: Color32::from_rgb(142, 146, 158), // #8E929E
            border_subtle: Color32::from_rgb(225, 227, 234), // #E1E3EA
            border_strong: Color32::from_rgb(200, 204, 216),
            accent_purple: Color32::from_rgb(124, 58, 237), // #7C3AED
            accent_purple_soft: Color32::from_rgb(243, 238, 255),
            accent_blue: Color32::from_rgb(35, 131, 226), // #2383E2
            accent_blue_soft: Color32::from_rgb(232, 243, 254),
            ok: Color32::from_rgb(15, 123, 108), // #0F7B6C
            ok_bg: Color32::from_rgb(230, 247, 242),
            warn: Color32::from_rgb(203, 123, 25), // #CB7B19
            warn_bg: Color32::from_rgb(254, 245, 231),
            danger: Color32::from_rgb(224, 62, 62), // #E03E3E
            danger_bg: Color32::from_rgb(253, 236, 236),
            code_bg: Color32::from_rgb(243, 242, 239),
        }
    }

    pub fn dark() -> Self {
        Self {
            is_dark: true,
            bg_main: Color32::from_rgb(32, 32, 35), // #202023
            bg_sidebar: Color32::from_rgb(25, 25, 28), // #19191C
            bg_secondary: Color32::from_rgb(28, 28, 31),
            bg_card: Color32::from_rgb(38, 38, 43), // #26262B
            bg_inset: Color32::from_rgb(45, 45, 52),
            bg_hover: Color32::from_rgb(50, 50, 58),
            bg_active: Color32::from_rgb(60, 60, 70),
            text_primary: Color32::from_rgb(238, 239, 244),
            text_secondary: Color32::from_rgb(168, 172, 184),
            text_muted: Color32::from_rgb(115, 119, 132),
            border_subtle: Color32::from_rgb(56, 57, 66),
            border_strong: Color32::from_rgb(76, 78, 90),
            accent_purple: Color32::from_rgb(155, 102, 245), // #9B66F5
            accent_purple_soft: Color32::from_rgb(56, 40, 88),
            accent_blue: Color32::from_rgb(82, 156, 202), // #529CCA
            accent_blue_soft: Color32::from_rgb(36, 58, 78),
            ok: Color32::from_rgb(77, 171, 154), // #4DAB9A
            ok_bg: Color32::from_rgb(30, 62, 56),
            warn: Color32::from_rgb(255, 163, 68), // #FFA344
            warn_bg: Color32::from_rgb(72, 50, 26),
            danger: Color32::from_rgb(255, 115, 105), // #FF7369
            danger_bg: Color32::from_rgb(74, 34, 34),
            code_bg: Color32::from_rgb(42, 42, 48),
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

pub fn parse_hex_color(raw: &str) -> Option<Color32> {
    let hex = raw.trim().trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some(Color32::from_rgb(r, g, b))
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
    let symbol_candidates = [
        ("win_emoji", r"C:\Windows\Fonts\seguiemj.ttf"),
        ("win_symbol", r"C:\Windows\Fonts\seguisym.ttf"),
    ];

    let mut fonts = FontDefinitions::default();
    for path in korean_candidates {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert("korean".into(), FontData::from_owned(bytes));
            for fam in [FontFamily::Proportional, FontFamily::Monospace] {
                fonts
                    .families
                    .entry(fam)
                    .or_default()
                    .insert(0, "korean".into());
            }
            break;
        }
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
        Color32::from_rgb(26, 26, 30)
    } else {
        Color32::WHITE
    };
    v.code_bg_color = pal.code_bg;
    v.hyperlink_color = pal.accent_blue;
    v.selection.bg_fill = pal.accent_purple_soft;
    v.selection.stroke = Stroke::new(1.0_f32, pal.accent_purple);

    v.widgets.noninteractive.fg_stroke.color = pal.text_primary;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, pal.border_subtle);
    v.widgets.noninteractive.rounding = Rounding::same(6.0);

    v.widgets.inactive.fg_stroke.color = pal.text_primary;
    v.widgets.inactive.weak_bg_fill = pal.bg_inset;
    v.widgets.inactive.bg_fill = pal.bg_inset;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, pal.border_subtle);
    v.widgets.inactive.rounding = Rounding::same(6.0);

    v.widgets.hovered.fg_stroke.color = pal.text_primary;
    v.widgets.hovered.weak_bg_fill = pal.bg_hover;
    v.widgets.hovered.bg_fill = pal.bg_hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, pal.accent_purple);
    v.widgets.hovered.rounding = Rounding::same(6.0);

    v.widgets.active.fg_stroke.color = pal.text_primary;
    v.widgets.active.weak_bg_fill = pal.bg_active;
    v.widgets.active.bg_fill = pal.bg_active;
    v.widgets.active.bg_stroke = Stroke::new(1.5_f32, pal.accent_purple);
    v.widgets.active.rounding = Rounding::same(6.0);

    v.window_rounding = Rounding::same(10.0);
    v.window_stroke = Stroke::new(1.0_f32, pal.border_subtle);

    ctx.set_visuals(v);

    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
        s.spacing.interact_size.y = 28.0;
        s.text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(14.0));
        s.text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(13.5));
        s.text_styles
            .insert(egui::TextStyle::Small, FontId::proportional(12.0));
        s.text_styles
            .insert(egui::TextStyle::Heading, FontId::proportional(20.0));
        s.text_styles
            .insert(egui::TextStyle::Monospace, FontId::monospace(13.0));
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
            .fill(pal.accent_purple)
            .rounding(Rounding::same(7.0))
            .min_size(egui::vec2(0.0, 32.0)),
    )
}

pub fn primary_blue_button(ui: &mut egui::Ui, pal: &ThemePalette, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(Color32::WHITE).strong())
            .fill(pal.accent_blue)
            .rounding(Rounding::same(7.0))
            .min_size(egui::vec2(0.0, 32.0)),
    )
}

pub fn danger_button(ui: &mut egui::Ui, pal: &ThemePalette, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(pal.danger).strong())
            .fill(pal.danger_bg)
            .stroke(Stroke::new(1.0_f32, pal.danger))
            .rounding(Rounding::same(7.0))
            .min_size(egui::vec2(0.0, 30.0)),
    )
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
                (pal.accent_purple, n.to_string())
            };
            ui.painter().circle_filled(rect.center(), 12.0, fill);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                txt,
                FontId::proportional(13.0),
                Color32::WHITE,
            );
            ui.label(
                RichText::new(title)
                    .size(15.5)
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
            pal.accent_purple_soft
        } else {
            pal.bg_card
        })
        .stroke(Stroke::new(
            if selected { 2.0_f32 } else { 1.0_f32 },
            if selected {
                pal.accent_purple
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
                    egui::Align2::CENTER_CENTER,
                    badge,
                    FontId::proportional(12.0),
                    Color32::WHITE,
                );
                ui.vertical(|ui| {
                    ui.label(RichText::new(name).strong().size(13.5).color(if selected {
                        pal.accent_purple
                    } else {
                        pal.text_primary
                    }));
                    ui.label(RichText::new(desc).size(11.5).color(pal.text_secondary));
                });
            });
        })
        .response;

    let resp = resp.interact(egui::Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}
