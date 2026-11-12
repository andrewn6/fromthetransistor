// what the compiler actually reads after #include and #define ran. the
// gutter says which file and line each expanded line came from.

use crate::snapshot::Snapshot;
use crate::srcmap::{Loc, SourceMap};
use crate::{highlight, theme, widgets};
use eframe::egui::{self, Align2, RichText, Sense};

#[derive(Default)]
pub struct PreprocessView {
    // the followed line this view last scrolled to
    seen: Option<Loc>,
}

impl PreprocessView {
    pub fn show(&mut self, ui: &mut egui::Ui, b: &Snapshot, sm: &SourceMap, focus: &mut Option<Loc>) {
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
                egui::ScrollArea::both().id_salt("pre").auto_shrink([false, false]).show(ui, |ui| self.text(ui, pre, sm, focus));
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

    fn text(&mut self, ui: &mut egui::Ui, pre: &str, sm: &SourceMap, focus: &mut Option<Loc>) {
        let gutter = 96.0;
        let mut job = highlight::c(pre, 12.5);
        job.wrap.max_width = f32::INFINITY;
        let galley = ui.painter().layout_job(job);
        let (r, resp) = ui.allocate_exact_size(galley.size() + egui::vec2(gutter, 0.0), Sense::click());
        let text_pos = r.left_top() + egui::vec2(gutter, 0.0);
        let p = ui.painter();
        let mut scroll = None;
        let jumped = *focus != self.seen;
        let mut picked = None;
        for (i, row) in galley.rows.iter().enumerate() {
            let rr = row.rect().translate(text_pos.to_vec2());
            let line = egui::Rect::from_x_y_ranges(r.x_range(), rr.y_range());
            let from = sm.pre_loc(i);
            if from.is_some() && from == focus.as_ref() {
                p.rect_filled(line, 0.0, theme::ACCENT.gamma_multiply(0.14));
                // scroll by the gutter cell so the view never shifts sideways
                if jumped && scroll.is_none() {
                    scroll = Some(egui::Rect::from_x_y_ranges(r.left()..=r.left() + gutter, line.y_range()));
                }
            }
            if let Some(l) = from {
                // the first line pasted in from another file is labeled with its name
                let prev = if i > 0 { sm.pre_loc(i - 1) } else { None };
                let text = if prev.map_or(true, |x| x.file != l.file) { l.label() } else { (l.line + 1).to_string() };
                p.text(egui::pos2(r.left() + gutter - 10.0, line.center().y), Align2::RIGHT_CENTER, text, theme::mono(10.5), theme::FAINT);
            }
            if resp.clicked() && resp.interact_pointer_pos().map_or(false, |pp| line.contains(pp)) {
                picked = from.cloned();
            }
        }
        p.galley(text_pos, galley, theme::TEXT);
        if let Some(l) = picked {
            *focus = Some(l);
        }
        if let Some(s) = scroll {
            ui.scroll_to_rect(s, Some(egui::Align::Center));
        }
        self.seen = focus.clone();
    }
}
