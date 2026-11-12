// colors and style: neutral dark grays, one blue accent for selection and
// focus, and a muted color per pipeline stage so a stage keeps the same color
// in the sidebar and its views.

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle};
use std::sync::Arc;

pub const BG: Color32 = Color32::from_rgb(0x11, 0x13, 0x17);
pub const PANEL: Color32 = Color32::from_rgb(0x18, 0x1b, 0x20);
pub const RAISED: Color32 = Color32::from_rgb(0x1f, 0x23, 0x2a);
pub const BORDER: Color32 = Color32::from_rgb(0x2c, 0x31, 0x39);
pub const TEXT: Color32 = Color32::from_rgb(0xe3, 0xe6, 0xeb);
pub const DIM: Color32 = Color32::from_rgb(0x9a, 0xa1, 0xac);
pub const FAINT: Color32 = Color32::from_rgb(0x5f, 0x66, 0x72);
pub const ACCENT: Color32 = Color32::from_rgb(0x4c, 0x8d, 0xff);

pub const OK: Color32 = Color32::from_rgb(0x5f, 0xb8, 0x7a);
pub const ERR: Color32 = Color32::from_rgb(0xe5, 0x5c, 0x5c);
pub const WARN: Color32 = Color32::from_rgb(0xd9, 0xa2, 0x45);

// stage accents
pub const SOURCE: Color32 = Color32::from_rgb(0xa3, 0xac, 0xb9);
pub const TOKENS: Color32 = Color32::from_rgb(0x6f, 0x9f, 0xe0);
pub const AST: Color32 = Color32::from_rgb(0xa4, 0x8a, 0xd9);
pub const ASM: Color32 = Color32::from_rgb(0xd4, 0xa6, 0x56);
pub const LINK: Color32 = Color32::from_rgb(0x7f, 0xbf, 0x86);
pub const ELF: Color32 = Color32::from_rgb(0x58, 0xb3, 0xbf);
pub const CPU: Color32 = Color32::from_rgb(0xd9, 0x80, 0x9a);

// token kinds, same mapping as toolchain_videos/viz.py
pub fn token_color(kind: &str) -> Color32 {
    match kind {
        "kw" => Color32::from_rgb(0x4f, 0xc1, 0xb6),
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
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("jbm".into(), Arc::new(FontData::from_static(include_bytes!("../assets/fonts/JetBrainsMonoNL-Regular.ttf"))));
    fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "jbm".into());
    ctx.set_fonts(fonts);

    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = BG;
    visuals.faint_bg_color = RAISED;
    visuals.override_text_color = Some(TEXT);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.inactive.bg_fill = RAISED;
    visuals.widgets.inactive.weak_bg_fill = RAISED;
    visuals.widgets.inactive.corner_radius = CornerRadius::same(3);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(3);
    visuals.widgets.active.corner_radius = CornerRadius::same(3);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x27, 0x2c, 0x34);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(0x27, 0x2c, 0x34);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.selection.bg_fill = Color32::from_rgb(0x23, 0x3a, 0x63);
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
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
