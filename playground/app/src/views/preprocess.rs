// what the compiler actually reads after #include and #define ran

use crate::{highlight, theme, widgets};
use eframe::egui::{self, RichText};
use crate::snapshot::Snapshot;

pub fn show(ui: &mut egui::Ui, b: &Snapshot) {
    let c = b;
    let Some(pre) = &c.preprocessed else {
        widgets::empty(ui, "nothing preprocessed: this is an assembly file, or preprocessing failed");
        return;
    };
    widgets::heading(ui, "preprocessor", theme::SOURCE);
    widgets::caption(
        ui,
        "#include pastes the file in place, #define does whole-word text replacement in definition order. \
         the lexer only ever sees the text below.",
    );
    ui.add_space(6.0);
    ui.columns(2, |cols| {
        let ui = &mut cols[0];
        widgets::card(ui, |ui| {
            ui.label(RichText::new(format!("expanded source  ({} lines)", pre.lines().count())).color(theme::DIM));
            egui::ScrollArea::both().id_salt("pre").auto_shrink([false, false]).show(ui, |ui| {
                let mut job = highlight::c(pre, 12.5);
                job.wrap.max_width = f32::INFINITY;
                ui.label(job);
            });
        });
        let ui = &mut cols[1];
        widgets::card(ui, |ui| {
            ui.label(RichText::new("defines").color(theme::DIM));
            if c.defines.is_empty() {
                widgets::caption(ui, "none");
            }
            egui::Grid::new("defines").striped(true).show(ui, |ui| {
                for (k, v) in &c.defines {
                    ui.monospace(RichText::new(k).color(theme::token_color("id")));
                    ui.monospace(if v.is_empty() { "(empty)" } else { v.as_str() });
                    ui.end_row();
                }
            });
            ui.add_space(10.0);
            ui.label(RichText::new("supported directives").color(theme::DIM));
            for d in ["#include \"file\"", "#define NAME text"] {
                ui.monospace(d);
            }
            widgets::caption(ui, "anything else (#ifndef, #if, function-like macros) is a SyntaxError, same as compiler.py.");
        });
    });
}
