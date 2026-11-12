// the token stream as chips; hover one to see where it came from

use crate::{theme, widgets};
use eframe::egui::{self, text::LayoutJob, Color32, CornerRadius, RichText, Sense, Stroke, StrokeKind, TextFormat};
use toolchain::pipeline::Build;

#[derive(Default)]
pub struct TokensView {
    hovered: Option<usize>,
}

impl TokensView {
    pub fn show(&mut self, ui: &mut egui::Ui, b: &Build) {
        let c = &b.compiled;
        let (Some(toks), Some(pre)) = (&c.tokens, &c.preprocessed) else {
            widgets::empty(ui, "no tokens: this is an assembly file, or an earlier stage failed");
            return;
        };
        widgets::heading(ui, "lexer", theme::TOKENS);
        widgets::caption(ui, "the source becomes a flat list of tokens: keywords, identifiers, literals, punctuation and operators (longest match first).");

        // counts per kind
        ui.horizontal_wrapped(|ui| {
            for (kind, label) in [("kw", "keyword"), ("id", "identifier"), ("intl", "int"), ("chrl", "char"), ("strl", "string"), ("p", "punct"), ("o", "operator")] {
                let n = toks.iter().filter(|t| t.tok.kind() == kind).count();
                widgets::pill(ui, &format!("{} {}", label, n), theme::token_color(kind));
            }
        });
        ui.add_space(6.0);

        let hovered_now = std::cell::Cell::new(None);
        ui.columns(2, |cols| {
            widgets::card(&mut cols[0], |ui| {
                ui.label(RichText::new(format!("{} tokens", toks.len())).color(theme::DIM));
                egui::ScrollArea::vertical().id_salt("toks").auto_shrink([false, false]).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                        for (i, t) in toks.iter().enumerate() {
                            let color = theme::token_color(t.tok.kind());
                            let text = t.tok.text();
                            let galley = ui.painter().layout_no_wrap(text, theme::mono(12.0), color);
                            let size = galley.size() + egui::vec2(10.0, 6.0);
                            let (r, resp) = ui.allocate_exact_size(size, Sense::hover());
                            let on = self.hovered == Some(i);
                            ui.painter().rect(
                                r,
                                CornerRadius::same(4),
                                if on { color.gamma_multiply(0.3) } else { color.gamma_multiply(0.09) },
                                Stroke::new(1.0, color.gamma_multiply(if on { 0.9 } else { 0.3 })),
                                StrokeKind::Inside,
                            );
                            ui.painter().galley(r.center() - galley.size() / 2.0, galley, color);
                            if resp.hovered() {
                                hovered_now.set(Some(i));
                            }
                            resp.on_hover_text(format!("{}  [{}..{}]", t.tok.repr(), t.start, t.end));
                        }
                    });
                });
            });
            widgets::card(&mut cols[1], |ui| {
                ui.label(RichText::new("preprocessed source").color(theme::DIM));
                egui::ScrollArea::both().id_salt("toksrc").auto_shrink([false, false]).show(ui, |ui| {
                    let span = self.hovered.and_then(|i| toks.get(i)).map(|t| (t.start, t.end));
                    ui.label(source_with_span(pre, span));
                });
            });
        });
        // sticky: the last hovered chip stays lit while you read the source
        if let Some(h) = hovered_now.get() {
            self.hovered = Some(h);
        }
    }
}

fn source_with_span(src: &str, span: Option<(usize, usize)>) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;
    let fmt = |bg: Color32, color: Color32| TextFormat { font_id: theme::mono(12.5), color, background: bg, ..Default::default() };
    let chars: Vec<char> = src.chars().collect();
    let (s, e) = span.unwrap_or((0, 0));
    let (s, e) = (s.min(chars.len()), e.min(chars.len()));
    let part = |a: usize, b: usize| chars[a..b].iter().collect::<String>();
    job.append(&part(0, s), 0.0, fmt(Color32::TRANSPARENT, theme::DIM));
    job.append(&part(s, e), 0.0, fmt(theme::TOKENS.gamma_multiply(0.35), theme::TEXT));
    job.append(&part(e, chars.len()), 0.0, fmt(Color32::TRANSPARENT, theme::DIM));
    job
}
