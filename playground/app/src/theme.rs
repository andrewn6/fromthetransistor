// colors and style. one accent per pipeline stage so a stage keeps the
// same color in the pipeline strip, its tab and its views.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};

pub const BG: Color32 = Color32::from_rgb(0x0d, 0x10, 0x14);
pub const PANEL: Color32 = Color32::from_rgb(0x13, 0x17, 0x1d);
pub const RAISED: Color32 = Color32::from_rgb(0x1a, 0x20, 0x29);
pub const BORDER: Color32 = Color32::from_rgb(0x2a, 0x32, 0x3e);
pub const TEXT: Color32 = Color32::from_rgb(0xd5, 0xdb, 0xe4);
pub const DIM: Color32 = Color32::from_rgb(0x7c, 0x87, 0x97);
pub const FAINT: Color32 = Color32::from_rgb(0x4a, 0x53, 0x60);

pub const OK: Color32 = Color32::from_rgb(0x8f, 0xd1, 0x8a);
pub const ERR: Color32 = Color32::from_rgb(0xf2, 0x6d, 0x6d);
pub const WARN: Color32 = Color32::from_rgb(0xe8, 0xb3, 0x5a);

// stage accents
pub const SOURCE: Color32 = Color32::from_rgb(0xa8, 0xb3, 0xc7);
pub const TOKENS: Color32 = Color32::from_rgb(0x6f, 0xa8, 0xf5);
pub const AST: Color32 = Color32::from_rgb(0xb7, 0x93, 0xf2);
pub const ASM: Color32 = Color32::from_rgb(0xe8, 0xb3, 0x5a);
pub const LINK: Color32 = Color32::from_rgb(0x8f, 0xd1, 0x8a);
pub const ELF: Color32 = Color32::from_rgb(0x4c, 0xc9, 0xd8);
pub const CPU: Color32 = Color32::from_rgb(0xf2, 0x7f, 0x9b);

// token kinds, same mapping as toolchain_videos/viz.py
pub fn token_color(kind: &str) -> Color32 {
    match kind {
        "kw" => Color32::from_rgb(0x4f, 0xd1, 0xc5),
        "id" => Color32::from_rgb(0x6f, 0xa8, 0xf5),
        "intl" | "strl" | "chrl" => Color32::from_rgb(0xe8, 0xd0, 0x6a),
        "o" => Color32::from_rgb(0xf0, 0x96, 0x4f),
        _ => DIM,
    }
}

// a stable color per name, for functions in the timeline and call graph
pub fn hashed(name: &str) -> Color32 {
    let mut h: u32 = 2166136261;
    for b in name.bytes() {
        h = (h ^ b as u32).wrapping_mul(16777619);
    }
    let hue = (h % 360) as f32 / 360.0;
    egui::ecolor::Hsva::new(hue, 0.55, 0.85, 1.0).into()
}

pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

pub fn install(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = BG;
    visuals.faint_bg_color = RAISED;
    visuals.override_text_color = Some(TEXT);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.inactive.bg_fill = RAISED;
    visuals.widgets.inactive.weak_bg_fill = RAISED;
    visuals.widgets.inactive.corner_radius = CornerRadius::same(4);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(4);
    visuals.widgets.active.corner_radius = CornerRadius::same(4);
    visuals.selection.bg_fill = Color32::from_rgb(0x2b, 0x3d, 0x5c);
    visuals.selection.stroke = Stroke::new(1.0, TOKENS);
    ctx.set_visuals(visuals);

    ctx.all_styles_mut(|style| {
        style.text_styles.insert(TextStyle::Body, FontId::new(14.0, FontFamily::Proportional));
        style.text_styles.insert(TextStyle::Button, FontId::new(14.0, FontFamily::Proportional));
        style.text_styles.insert(TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace));
        style.text_styles.insert(TextStyle::Small, FontId::new(11.5, FontFamily::Proportional));
        style.text_styles.insert(TextStyle::Heading, FontId::new(18.0, FontFamily::Proportional));
        style.spacing.item_spacing = egui::vec2(8.0, 5.0);
        style.spacing.button_padding = egui::vec2(8.0, 3.0);
    });
}
