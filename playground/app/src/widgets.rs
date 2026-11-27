// small shared ui pieces

use crate::theme;
use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke, StrokeKind};

pub fn heading(ui: &mut egui::Ui, text: &str, accent: Color32) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(3.0, 16.0), egui::Sense::hover());
        ui.painter().rect_filled(r, CornerRadius::same(1), accent);
        ui.label(RichText::new(text).strong().size(14.0));
    });
}

pub fn caption(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(theme::DIM).size(12.5));
}

pub fn pill(ui: &mut egui::Ui, text: &str, color: Color32) -> egui::Response {
    let font = egui::FontId::proportional(11.5);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, color);
    let size = galley.size() + egui::vec2(12.0, 4.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect(
        rect,
        CornerRadius::same(3),
        color.gamma_multiply(0.14),
        Stroke::new(1.0, color.gamma_multiply(0.5)),
        StrokeKind::Inside,
    );
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    resp
}

pub fn empty(ui: &mut egui::Ui, text: &str) {
    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(text).color(theme::DIM));
    });
}

pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::NONE
        .fill(theme::PANEL)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(egui::Margin::same(10))
        .show(ui, add)
        .inner
}

pub fn hex32(v: u32) -> String {
    format!("{:08x}", v)
}

pub fn bytes(n: usize) -> String {
    if n >= 1024 {
        format!("{:.1} KB", n as f32 / 1024.0)
    } else {
        format!("{} B", n)
    }
}

// a fixed-size, top-down column inside a horizontal row
pub fn column<R>(ui: &mut egui::Ui, w: f32, h: f32, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.allocate_ui_with_layout(egui::vec2(w, h), egui::Layout::top_down(egui::Align::LEFT), |ui| {
        ui.set_width(w);
        add(ui)
    })
    .inner
}

// one stage of the lineage strip: a small title over a monospace detail
pub fn step_chip(ui: &mut egui::Ui, color: Color32, title: &str, detail: &str, selected: bool) -> egui::Response {
    let t = ui.painter().layout_no_wrap(title.to_string(), egui::FontId::proportional(10.5), color);
    let d = ui.painter().layout_no_wrap(detail.to_string(), theme::mono(11.0), theme::TEXT);
    let size = egui::vec2(t.size().x.max(d.size().x) + 14.0, t.size().y + d.size().y + 8.0);
    let (r, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let fill = if selected { color.gamma_multiply(0.16) } else if resp.hovered() { theme::RAISED } else { theme::BG };
    let stroke = Stroke::new(1.0, if selected { color.gamma_multiply(0.8) } else { theme::BORDER });
    ui.painter().rect(r, CornerRadius::same(3), fill, stroke, StrokeKind::Inside);
    let top = r.left_top() + egui::vec2(7.0, 4.0);
    let dy = t.size().y;
    ui.painter().galley(top, t, color);
    ui.painter().galley(top + egui::vec2(0.0, dy), d, theme::TEXT);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}
