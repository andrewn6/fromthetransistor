// the token stream as chips; hover one to see where it came from

use crate::{theme, widgets};
use eframe::egui::{self, text::LayoutJob, Color32, CornerRadius, RichText, Sense, Stroke, StrokeKind, TextFormat};
use crate::snapshot::Snapshot;
use crate::srcmap::{Loc, SourceMap};

#[derive(Default)]
pub struct TokensView {
    hovered: Option<usize>,
    // the followed line this view last jumped to
    seen: Option<Loc>,
}

impl TokensView {
    pub fn show(&mut self, ui: &mut egui::Ui, b: &Snapshot, sm: &SourceMap, focus: &mut Option<Loc>) {
        let c = b;
        let (Some(toks), Some(pre)) = (&c.tokens, &c.preprocessed) else {
            widgets::empty(ui, "no tokens: this is an assembly file, or an earlier stage failed");
            return;
        };
        widgets::heading(ui, "lexer", theme::TOKENS);
        widgets::caption(ui, "the source becomes a flat list of tokens: keywords, identifiers, literals, punctuation and operators (longest match first).");

        // counts per kind
        ui.horizontal_wrapped(|ui| {
            for (kind, label) in [("kw", "keyword"), ("id", "identifier"), ("intl", "int"), ("chrl", "char"), ("strl", "string"), ("p", "punct"), ("o", "operator")] {
                let n = toks.iter().filter(|t| t.kind == kind).count();
                widgets::pill(ui, &format!("{} {}", label, n), theme::token_color(kind));
            }
        });
        ui.add_space(6.0);

        let hovered_now = std::cell::Cell::new(None);
        let picked = std::cell::Cell::new(None);
        // tokens of the followed line get a ring; a new line lights its first token
        let tok_loc = |t: &crate::snapshot::Token| t.start.and_then(|s| sm.pre_loc(sm.pre_line_of(s)));
        let jumped = *focus != self.seen;
        if jumped {
            if let Some(i) = toks.iter().position(|t| focus.is_some() && tok_loc(t) == focus.as_ref()) {
                self.hovered = Some(i);
            }
        }
        let mut scrolled = !jumped;
        ui.columns(2, |cols| {
            widgets::card(&mut cols[0], |ui| {
                ui.label(RichText::new(format!("{} tokens", toks.len())).color(theme::DIM));
                egui::ScrollArea::vertical().id_salt("toks").auto_shrink([false, false]).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                        for (i, t) in toks.iter().enumerate() {
                            let color = theme::token_color(&t.kind);
                            let text = if t.kind == "strl" || t.kind == "chrl" { format!("{:?}", t.text) } else { t.text.clone() };
                            let galley = ui.painter().layout_no_wrap(text, theme::mono(12.0), color);
                            let size = galley.size() + egui::vec2(10.0, 6.0);
                            let (r, resp) = ui.allocate_exact_size(size, Sense::click());
                            let on = self.hovered == Some(i);
                            let followed = focus.is_some() && tok_loc(t) == focus.as_ref();
                            if followed {
                                ui.painter().rect_stroke(r.expand(1.5), CornerRadius::same(5), Stroke::new(1.0, theme::ACCENT), StrokeKind::Outside);
                                if !scrolled {
                                    ui.scroll_to_rect(r, Some(egui::Align::Center));
                                    scrolled = true;
                                }
                            }
                            if resp.clicked() {
                                picked.set(Some(i));
                            }
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
                            resp.on_hover_text(match (t.start, t.end) {
                                (Some(s), Some(e)) => format!("{}  [{}..{}]", t.repr, s, e),
                                _ => t.repr.clone(),
                            });
                        }
                    });
                });
            });
            widgets::card(&mut cols[1], |ui| {
                ui.label(RichText::new("preprocessed source").color(theme::DIM));
                egui::ScrollArea::both().id_salt("toksrc").auto_shrink([false, false]).show(ui, |ui| {
                    let span = self.hovered.and_then(|i| toks.get(i)).and_then(|t| Some((t.start?, t.end?)));
                    ui.label(source_with_span(pre, span));
                });
            });
        });
        // sticky: the last hovered chip stays lit while you read the source
        if let Some(h) = hovered_now.get() {
            self.hovered = Some(h);
        }
        if let Some(l) = picked.get().and_then(|i| tok_loc(&toks[i])) {
            *focus = Some(l.clone());
        }
        self.seen = focus.clone();
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
