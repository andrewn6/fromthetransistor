// top-level state and layout: example picker, the pipeline strip (which is
// also the navigation), the source editor and the selected stage's view.

use crate::examples::{Kind, EXAMPLES};
use crate::snapshot::Snapshot;
use crate::{highlight, python, theme, views, widgets};
use eframe::egui::{self, Align2, Color32, CornerRadius, RichText, Sense, Stroke};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Preprocess,
    Tokens,
    Ast,
    Asm,
    Link,
    Elf,
    Cpu,
    Board,
}

impl Tab {
    const ALL: [Tab; 8] = [Tab::Preprocess, Tab::Tokens, Tab::Ast, Tab::Asm, Tab::Link, Tab::Elf, Tab::Cpu, Tab::Board];

    fn slug(self) -> &'static str {
        match self {
            Tab::Preprocess => "preprocess",
            Tab::Tokens => "tokens",
            Tab::Ast => "ast",
            Tab::Asm => "asm",
            Tab::Link => "link",
            Tab::Elf => "elf",
            Tab::Cpu => "cpu",
            Tab::Board => "board",
        }
    }

    fn from_slug(s: &str) -> Option<Tab> {
        Tab::ALL.into_iter().find(|t| t.slug() == s)
    }
}

#[cfg(target_arch = "wasm32")]
fn location_hash() -> String {
    web_sys::window().and_then(|w| w.location().hash().ok()).unwrap_or_default()
}

#[cfg(not(target_arch = "wasm32"))]
fn location_hash() -> String {
    String::new()
}

// keep the address bar in sync so any view can be shared as a link
#[cfg(target_arch = "wasm32")]
fn set_location_hash(h: &str) {
    if let Some(w) = web_sys::window() {
        if w.location().hash().ok().as_deref() != Some(h) {
            if let Ok(history) = w.history() {
                let _ = history.replace_state_with_url(&eframe::wasm_bindgen::JsValue::NULL, "", Some(h));
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn set_location_hash(_h: &str) {}

pub struct SourceFile {
    pub name: String,
    pub kind: Kind,
    pub text: String,
    pub original: &'static str,
    pub about: &'static str,
    pub semihost: bool,
    pub uart: bool,
}

pub struct App {
    pub files: Vec<SourceFile>,
    pub current: usize,
    pub build: Snapshot,
    built_text: String,
    // python toolchain state, and the .py version the current build used
    py: python::State,
    built_version: u32,
    show_traceback: bool,
    last_edit: f64,
    pub tab: Tab,
    pub cpu: views::cpu::CpuView,
    pub asm: views::asm::AsmView,
    pub ast: views::ast::AstView,
    pub tokens: views::tokens::TokensView,
    pub link: views::link::LinkView,
    pub elf: views::elf::ElfView,
    pub board: views::board::BoardView,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        let files = EXAMPLES
            .iter()
            .map(|e| SourceFile {
                name: e.name.to_string(),
                kind: e.kind,
                text: e.text.to_string(),
                original: e.text,
                about: e.about,
                semihost: e.semihost,
                uart: e.uart,
            })
            .collect();
        let mut app = App {
            files,
            current: 0,
            build: Snapshot::default(),
            built_text: String::new(),
            py: python::State::default(),
            built_version: 0,
            show_traceback: false,
            last_edit: 0.0,
            tab: Tab::Cpu,
            cpu: views::cpu::CpuView::default(),
            asm: views::asm::AsmView::default(),
            ast: views::ast::AstView::default(),
            tokens: views::tokens::TokensView::default(),
            link: views::link::LinkView::default(),
            elf: views::elf::ElfView::default(),
            board: views::board::BoardView::default(),
        };
        let f = &app.files[0];
        app.cpu.configure(f.semihost, f.uart);
        app.apply_link(&location_hash());
        app
    }

    // deep links: #<tab>[/<file>], e.g. #asm/test.c
    fn apply_link(&mut self, hash: &str) {
        let mut parts = hash.trim_start_matches('#').splitn(2, '/');
        if let Some(t) = parts.next().and_then(Tab::from_slug) {
            self.tab = t;
        }
        if let Some(i) = parts.next().and_then(|f| self.files.iter().position(|x| x.name == f)) {
            self.select(i);
        }
    }

    fn link(&self) -> String {
        format!("#{}/{}", self.tab.slug(), self.file().name)
    }

    pub fn file(&self) -> &SourceFile {
        &self.files[self.current]
    }

    pub fn rebuild(&mut self) {
        let f = &self.files[self.current];
        self.built_text = f.text.clone();
        if !self.py.ready {
            // the first build happens once python has loaded
            return;
        }
        // every c file goes along so #include "vatlib.c" resolves
        let files: serde_json::Map<String, serde_json::Value> = self
            .files
            .iter()
            .filter(|x| x.kind == Kind::C || x.name == f.name)
            .map(|x| (x.name.clone(), serde_json::Value::String(x.text.clone())))
            .collect();
        let req = serde_json::json!({
            "name": f.name,
            "kind": if f.kind == Kind::C { "c" } else { "asm" },
            "files": files,
        });
        self.build = python::build(&req.to_string());
        self.built_version = self.py.version;
        self.cpu.load(&self.build);
        self.asm.on_rebuild();
        self.ast.on_rebuild();
        self.link.on_rebuild();
    }

    fn select(&mut self, i: usize) {
        if i != self.current {
            self.current = i;
            // each example picks its own run mode; edits keep whatever the user set
            let f = &self.files[i];
            self.cpu.configure(f.semihost, f.uart);
            self.rebuild();
        }
    }

    // ----- header -----

    fn header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.label(RichText::new("from the transistor").strong().size(15.0));
            ui.label(RichText::new("/").color(theme::FAINT).size(15.0));
            ui.label(RichText::new("toolchain playground").color(theme::DIM).size(15.0));
            ui.add_space(24.0);
            ui.label(RichText::new("program").color(theme::DIM));
            let mut pick = None;
            let label = |f: &SourceFile| if f.text != f.original { format!("{}  (edited)", f.name) } else { f.name.clone() };
            egui::ComboBox::from_id_salt("program")
                .width(200.0)
                .selected_text(RichText::new(label(self.file())).monospace())
                .show_ui(ui, |ui| {
                    for (i, f) in self.files.iter().enumerate() {
                        if ui.selectable_label(i == self.current, RichText::new(label(f)).monospace()).on_hover_text(f.about).clicked() {
                            pick = Some(i);
                        }
                    }
                });
            if let Some(i) = pick {
                self.select(i);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_enabled(self.py.ready, egui::Button::new("reload .py")).on_hover_text("re-read compiler.py, assembler.py, linker.py and build.py from the server").clicked() {
                    python::reload();
                }
                let col = if self.py.ready { theme::OK } else if self.py.status.starts_with("failed") || self.py.status.starts_with("pyodide failed") { theme::ERR } else { theme::WARN };
                let files = if self.py.files.is_empty() { "nothing loaded yet".to_string() } else { self.py.files.join("\n") };
                let resp = ui.label(RichText::new(&self.py.status).color(theme::DIM).size(12.5)).on_hover_text(format!("the toolchain running in this page:\n{}", files));
                let dot = egui::pos2(resp.rect.left() - 8.0, resp.rect.center().y);
                ui.painter().circle_filled(dot, 3.5, col);
                ui.add_space(10.0);
            });
        });
    }

    // ----- stage sidebar -----

    fn stages(&self) -> Vec<Stage> {
        let b = &self.build;
        let is_c = self.file().kind == Kind::C;
        let err_stage = b.error.as_ref().map(|e| e.stage.as_str());
        let failed = |stages: &[&str]| err_stage.map_or(false, |s| stages.contains(&s));
        let state = |done: bool, fail: bool, applicable: bool| {
            if !applicable {
                NodeState::NotApplicable
            } else if fail {
                NodeState::Failed
            } else if done {
                NodeState::Done
            } else {
                NodeState::Skipped
            }
        };
        let assembled = b.text.is_some();
        vec![
            Stage {
                tab: Tab::Preprocess,
                title: "preprocess",
                metric: b.preprocessed.as_ref().map_or("-".into(), |p| format!("{} lines", p.lines().count())),
                color: theme::SOURCE,
                state: state(b.preprocessed.is_some(), failed(&["preprocess", "import"]), is_c),
            },
            Stage {
                tab: Tab::Tokens,
                title: "lex",
                metric: b.tokens.as_ref().map_or("-".into(), |t| format!("{} tokens", t.len())),
                color: theme::TOKENS,
                state: state(b.tokens.is_some(), failed(&["lex"]), is_c),
            },
            Stage {
                tab: Tab::Ast,
                title: "parse",
                metric: b.ast.as_ref().map_or("-".into(), |a| format!("{} decls", a.len())),
                color: theme::AST,
                state: state(b.ast.is_some(), failed(&["parse"]), is_c),
            },
            Stage {
                tab: Tab::Asm,
                title: "codegen + assemble",
                metric: if assembled { format!("{} instrs", b.insn_count()) } else { "-".into() },
                color: theme::ASM,
                state: state(assembled && !failed(&["assemble"]), failed(&["codegen", "assemble"]), true),
            },
            Stage {
                tab: Tab::Link,
                title: "link",
                metric: b.link.as_ref().map_or("-".into(), |l| format!("{} syms, {} relocs", l.symbols.len(), l.patches.len())),
                color: theme::LINK,
                state: state(b.elf.is_some(), failed(&["link"]), true),
            },
            Stage {
                tab: Tab::Elf,
                title: "elf",
                metric: b.elf.as_ref().map_or("-".into(), |e| widgets::bytes(e.len())),
                color: theme::ELF,
                state: state(b.elf.is_some(), failed(&["build.py", "bridge"]), true),
            },
            Stage {
                tab: Tab::Cpu,
                title: "arm7 cpu",
                metric: self.cpu.metric(),
                color: theme::CPU,
                state: state(b.flat.is_some(), false, true),
            },
        ]
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let stages = self.stages();
        section_label(ui, "pipeline");
        let row_h = 46.0;
        let mut centers = Vec::new();
        for (i, st) in stages.iter().enumerate() {
            let metric = match st.state {
                NodeState::Failed => "failed".to_string(),
                NodeState::NotApplicable => "n/a for .s".to_string(),
                _ => st.metric.clone(),
            };
            let (resp, c) = nav_row(ui, row_h, self.tab == st.tab, st.title, &metric, |p, c| {
                // step marker: number in a ring, filled once the stage has run
                let (ring, fill, num) = match st.state {
                    NodeState::Done => (st.color, st.color.gamma_multiply(0.18), theme::TEXT),
                    NodeState::Failed => (theme::ERR, theme::ERR.gamma_multiply(0.18), theme::ERR),
                    _ => (theme::BORDER, Color32::TRANSPARENT, theme::FAINT),
                };
                p.circle(c, 10.0, fill, Stroke::new(1.5, ring));
                p.text(c, Align2::CENTER_CENTER, (i + 1).to_string(), egui::FontId::proportional(11.5), num);
            }, st.state == NodeState::Failed);
            centers.push(c);
            if resp.clicked() {
                self.tab = st.tab;
            }
        }
        // connector between the step markers
        for w in centers.windows(2) {
            ui.painter().line_segment([w[0] + egui::vec2(0.0, 11.0), w[1] - egui::vec2(0.0, 11.0)], Stroke::new(1.0, theme::FAINT.gamma_multiply(0.6)));
        }

        ui.add_space(14.0);
        section_label(ui, "hardware");
        let on = self.board.board.powered();
        let detail = self.board.metric();
        let resp = nav_row(ui, row_h, self.tab == Tab::Board, "fpga board", &detail, |p, c| {
            // a small chip, lit while the board has power
            let chip = egui::Rect::from_center_size(c, egui::vec2(14.0, 14.0));
            p.rect_stroke(chip, CornerRadius::same(2), Stroke::new(1.5, if on { theme::OK } else { theme::BORDER }), egui::StrokeKind::Inside);
            if on {
                p.rect_filled(chip.shrink(4.0), CornerRadius::same(1), theme::OK);
            }
        }, false)
        .0;
        if resp.clicked() {
            self.tab = Tab::Board;
        }
    }

    fn error_banner(&mut self, ui: &mut egui::Ui) {
        if !self.py.ready && self.build.error.is_none() && self.build.kind.is_empty() {
            banner(ui, theme::WARN, |ui| {
                ui.label(RichText::new(&self.py.status).color(theme::WARN));
                widgets::caption(ui, "the page runs the repo's own compiler.py / assembler.py / linker.py / build.py in pyodide.");
            });
        }
        if let Some(e) = &self.build.error {
            let show = &mut self.show_traceback;
            banner(ui, theme::ERR, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("{} failed", e.stage)).color(theme::ERR).strong());
                    ui.label(RichText::new(format!("{}: {}", e.kind, e.msg)).monospace().color(theme::TEXT));
                    if !e.traceback.is_empty() {
                        ui.toggle_value(show, "python traceback");
                    }
                });
                if *show {
                    ui.label(RichText::new(&e.traceback).monospace().size(11.5).color(theme::DIM));
                }
            });
        }
        for w in &self.build.warnings {
            banner(ui, theme::WARN, |ui| {
                ui.label(RichText::new(w).color(theme::WARN));
            });
        }
    }

    // ----- editor -----

    fn editor(&mut self, ui: &mut egui::Ui) {
        let kind = self.file().kind;
        ui.horizontal(|ui| {
            widgets::heading(ui, &self.files[self.current].name, theme::SOURCE);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let f = &mut self.files[self.current];
                if ui.add_enabled(f.text != f.original, egui::Button::new("revert")).clicked() {
                    f.text = f.original.to_string();
                }
            });
        });
        widgets::caption(ui, self.file().about);
        ui.add_space(4.0);
        let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap_width: f32| -> Arc<egui::Galley> {
            let mut job = match kind {
                Kind::C => highlight::c(text.as_str(), 13.0),
                Kind::Asm => highlight::asm(text.as_str(), 13.0),
            };
            job.wrap.max_width = wrap_width;
            ui.painter().layout_job(job)
        };
        egui::ScrollArea::both().id_salt("editor").auto_shrink([false, false]).show(ui, |ui| {
            let f = &mut self.files[self.current];
            let resp = ui.add(
                egui::TextEdit::multiline(&mut f.text)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .desired_rows(40)
                    .lock_focus(true)
                    .layouter(&mut layouter),
            );
            if resp.changed() {
                self.last_edit = ui.input(|i| i.time);
            }
        });
    }
}

struct Stage {
    tab: Tab,
    title: &'static str,
    metric: String,
    color: Color32,
    state: NodeState,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NodeState {
    Done,
    Failed,
    Skipped,
    NotApplicable,
}

fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(text.to_uppercase()).size(11.0).color(theme::FAINT).extra_letter_spacing(0.8));
    ui.add_space(2.0);
}

// one sidebar entry: a marker, a title and a one-line detail. the selected
// entry gets a raised fill and an accent bar. returns the marker center.
fn nav_row(ui: &mut egui::Ui, h: f32, selected: bool, title: &str, detail: &str, marker: impl FnOnce(&egui::Painter, egui::Pos2), failed: bool) -> (egui::Response, egui::Pos2) {
    let (r, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), h), Sense::click());
    let p = ui.painter();
    if selected || resp.hovered() {
        p.rect_filled(r, CornerRadius::same(4), if selected { theme::RAISED } else { theme::RAISED.gamma_multiply(0.6) });
    }
    if selected {
        p.rect_filled(egui::Rect::from_min_size(r.left_top() + egui::vec2(0.0, 8.0), egui::vec2(2.0, h - 16.0)), CornerRadius::same(1), theme::ACCENT);
    }
    let c = egui::pos2(r.left() + 22.0, r.center().y);
    marker(p, c);
    let fg = if failed { theme::ERR } else if selected { theme::TEXT } else { theme::TEXT.gamma_multiply(0.85) };
    p.text(egui::pos2(r.left() + 42.0, r.center().y - 1.0), Align2::LEFT_BOTTOM, title, egui::FontId::proportional(13.5), fg);
    p.text(egui::pos2(r.left() + 42.0, r.center().y + 2.0), Align2::LEFT_TOP, detail, theme::mono(11.0), theme::DIM);
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    (resp, c)
}

fn banner(ui: &mut egui::Ui, color: Color32, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE
        .fill(color.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.6)))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
    ui.add_space(6.0);
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // python loads asynchronously; build as soon as it is ready and again
        // whenever the .py files are reloaded
        self.py = python::state();
        if !self.py.ready {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        } else if self.py.version != self.built_version {
            self.rebuild();
        }
        // rebuild a moment after the last keystroke
        let now = ui.input(|i| i.time);
        if self.file().text != self.built_text {
            if now - self.last_edit > 0.25 {
                self.rebuild();
            } else {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
        self.cpu.tick(ui.ctx());
        // the board runs whichever page is open, like real hardware
        self.board.refresh_roms(&self.py);
        self.board.tick(ui.ctx());
        set_location_hash(&self.link());

        let edge = Stroke::new(1.0, theme::BORDER);
        egui::Panel::top("header")
            .exact_size(48.0)
            .frame(egui::Frame::NONE.fill(theme::PANEL).stroke(edge).inner_margin(egui::Margin::symmetric(16, 0)))
            .show(ui, |ui| self.header(ui));

        egui::Panel::left("stages")
            .resizable(false)
            .exact_size(220.0)
            .frame(egui::Frame::NONE.fill(theme::PANEL).stroke(edge).inner_margin(egui::Margin::symmetric(10, 12)))
            .show(ui, |ui| self.sidebar(ui));

        // the board page is the whole machine; the editor stays on the pipeline pages
        if self.tab != Tab::Board {
            egui::Panel::left("editor")
                .resizable(true)
                .default_size(400.0)
                .frame(egui::Frame::NONE.fill(theme::BG).stroke(edge).inner_margin(egui::Margin::same(12)))
                .show(ui, |ui| self.editor(ui));
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme::BG).inner_margin(egui::Margin::same(16)))
            .show(ui, |ui| {
                self.error_banner(ui);
                match self.tab {
                    Tab::Preprocess => views::preprocess::show(ui, &self.build),
                    Tab::Tokens => self.tokens.show(ui, &self.build),
                    Tab::Ast => self.ast.show(ui, &self.build),
                    Tab::Asm => self.asm.show(ui, &self.build),
                    Tab::Link => self.link.show(ui, &self.build),
                    Tab::Elf => self.elf.show(ui, &self.build),
                    Tab::Cpu => self.cpu.show(ui, &self.build),
                    Tab::Board => {
                        let name = self.files[self.current].name.clone();
                        self.board.show(ui, &self.build, &name)
                    }
                }
            });
    }
}
