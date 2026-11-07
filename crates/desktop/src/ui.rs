//! 화면 테마와 공통 위젯
use eframe::egui::{self, Color32, RichText, Rounding, Stroke};

pub const ACCENT: Color32 = Color32::from_rgb(124, 58, 237);
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(243, 238, 255);
pub const OK: Color32 = Color32::from_rgb(22, 150, 85);
pub const WARN: Color32 = Color32::from_rgb(214, 120, 0);
pub const DANGER: Color32 = Color32::from_rgb(208, 56, 56);
pub const MUTED: Color32 = Color32::from_rgb(110, 114, 128);
pub const TEXT: Color32 = Color32::from_rgb(30, 32, 40);
pub const BG: Color32 = Color32::from_rgb(246, 247, 250);
pub const SIDEBAR: Color32 = Color32::from_rgb(238, 239, 245);
pub const CARD: Color32 = Color32::WHITE;
pub const BORDER: Color32 = Color32::from_rgb(225, 227, 234);
pub const OK_BG: Color32 = Color32::from_rgb(230, 247, 237);
pub const DANGER_BG: Color32 = Color32::from_rgb(253, 236, 236);

pub fn setup_style(ctx: &egui::Context) {
    let candidates = [
        r"C:\Windows\Fonts\malgun.ttf",
        r"C:\Windows\Fonts\gulim.ttc",
        "/usr/share/fonts/truetype/nanum/NanumGothic.ttf",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/System/Library/Fonts/AppleSDGothicNeo.ttc",
    ];
    let mut fonts = egui::FontDefinitions::default();
    for c in candidates {
        if let Ok(bytes) = std::fs::read(c) {
            fonts
                .font_data
                .insert("korean".into(), egui::FontData::from_owned(bytes));
            for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                // 영문은 기본 글꼴, 한글은 맑은 고딕으로 표시
                fonts.families.entry(fam).or_default().push("korean".into());
            }
            break;
        }
    }
    ctx.set_fonts(fonts);

    let mut v = egui::Visuals::light();
    v.panel_fill = BG;
    v.window_fill = CARD;
    v.widgets.noninteractive.fg_stroke.color = TEXT;
    v.widgets.inactive.fg_stroke.color = TEXT;
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = ACCENT_SOFT;
    v.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    v.widgets.inactive.rounding = Rounding::same(6.0);
    v.widgets.hovered.rounding = Rounding::same(6.0);
    v.widgets.active.rounding = Rounding::same(6.0);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(240, 241, 246);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(232, 233, 241);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    v.extreme_bg_color = Color32::WHITE; // 입력칸 배경
    ctx.set_visuals(v);

    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 7.0);
        s.spacing.button_padding = egui::vec2(12.0, 6.0);
        s.spacing.interact_size.y = 28.0;
        s.text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(14.5));
        s.text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.5));
    });
}

pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::none()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .rounding(Rounding::same(10.0))
        .inner_margin(egui::Margin::same(16.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

pub fn inset<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::none()
        .fill(ACCENT_SOFT)
        .rounding(Rounding::same(8.0))
        .inner_margin(egui::Margin::same(12.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

pub fn status_dot(ui: &mut egui::Ui, color: Color32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), size / 2.0, color);
}

pub fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(Color32::WHITE).strong())
            .fill(ACCENT)
            .rounding(Rounding::same(8.0))
            .min_size(egui::vec2(0.0, 36.0)),
    )
}

pub fn primary_button_widget(text: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(text).color(Color32::WHITE).strong())
        .fill(ACCENT)
        .rounding(Rounding::same(8.0))
        .min_size(egui::vec2(0.0, 36.0))
}

pub fn big_button(text: &str) -> egui::Button<'_> {
    egui::Button::new(text)
        .rounding(Rounding::same(8.0))
        .min_size(egui::vec2(0.0, 36.0))
}

pub fn nav_button(ui: &mut egui::Ui, text: &str, selected: bool) -> egui::Response {
    let btn = egui::Button::new(RichText::new(text).size(15.0).color(if selected {
        ACCENT
    } else {
        TEXT
    }))
    .fill(if selected { CARD } else { Color32::TRANSPARENT })
    .stroke(if selected {
        Stroke::new(1.0_f32, BORDER)
    } else {
        Stroke::NONE
    })
    .rounding(Rounding::same(8.0))
    .min_size(egui::vec2(ui.available_width(), 34.0));
    ui.add(btn)
}

/// 번호가 붙은 설정 단계 카드
pub fn step_card(
    ui: &mut egui::Ui,
    n: u8,
    title: &str,
    done: bool,
    add: impl FnOnce(&mut egui::Ui),
) {
    card(ui, |ui| {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
            let (fill, txt) = if done {
                (OK, "✔".to_string())
            } else {
                (ACCENT, n.to_string())
            };
            ui.painter().circle_filled(rect.center(), 12.0, fill);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                txt,
                egui::FontId::proportional(13.0),
                Color32::WHITE,
            );
            ui.label(RichText::new(title).size(16.0).strong());
        });
        ui.add_space(6.0);
        add(ui);
    });
}

/// 선택형 카드 (저장소 종류 선택)
pub fn choice_card(
    ui: &mut egui::Ui,
    width: f32,
    badge: &str,
    color: Color32,
    name: &str,
    desc: &str,
    selected: bool,
) -> egui::Response {
    let frame = egui::Frame::none()
        .fill(if selected { ACCENT_SOFT } else { CARD })
        .stroke(Stroke::new(
            if selected { 2.0_f32 } else { 1.0 },
            if selected { ACCENT } else { BORDER },
        ))
        .rounding(Rounding::same(8.0))
        .inner_margin(egui::Margin::same(10.0));
    let resp = frame
        .show(ui, |ui| {
            ui.set_width(width - 26.0);
            ui.set_min_height(46.0);
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(36.0, 36.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, Rounding::same(8.0), color);
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    badge,
                    egui::FontId::proportional(12.0),
                    Color32::WHITE,
                );
                ui.vertical(|ui| {
                    ui.label(RichText::new(name).strong().color(if selected {
                        ACCENT
                    } else {
                        TEXT
                    }));
                    ui.label(RichText::new(desc).small().color(MUTED));
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

/// 라벨 + 입력칸 한 줄 (Grid 안에서 사용)
pub fn field(ui: &mut egui::Ui, label: &str, value: &mut String, secret: bool, hint: &str) {
    ui.label(RichText::new(label).color(MUTED));
    ui.add(
        egui::TextEdit::singleline(value)
            .password(secret)
            .hint_text(hint)
            .desired_width(ui.available_width().min(420.0)),
    );
    ui.end_row();
}
