// top-level state and layout: example picker, the pipeline strip (which is
// also the navigation), the source editor and the selected stage's view.

use crate::examples::{Kind, EXAMPLES};
use crate::snapshot::Snapshot;
use crate::{highlight, python, theme, views, widgets};
use eframe::egui::{self, Align2, Color32, CornerRadius, RichText, Sense, Stroke, StrokeKind};
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
    Roadmap,
}

impl Tab {
    const ALL: [Tab; 8] = [Tab::Preprocess, Tab::Tokens, Tab::Ast, Tab::Asm, Tab::Link, Tab::Elf, Tab::Cpu, Tab::Roadmap];

    fn slug(self) -> &'static str {
        match self {
            Tab::Preprocess => "preprocess",
            Tab::Tokens => "tokens",
            Tab::Ast => "ast",
            Tab::Asm => "asm",
            Tab::Link => "link",
            Tab::Elf => "elf",
            Tab::Cpu => "cpu",
            Tab::Roadmap => "roadmap",
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

    // ----- top bar -----

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("from the transistor").strong().size(17.0));
            ui.label(RichText::new("toolchain playground").color(theme::DIM).size(15.0));
            ui.add_space(16.0);
            let mut pick = None;
            for (i, f) in self.files.iter().enumerate() {
                let edited = f.text != f.original;
                let label = if edited { format!("{} *", f.name) } else { f.name.clone() };
                let r = ui.selectable_label(i == self.current, RichText::new(label).monospace()).on_hover_text(f.about);
                if r.clicked() {
                    pick = Some(i);
                }
            }
            if let Some(i) = pick {
                self.select(i);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_enabled(self.py.ready, egui::Button::new("reload .py")).on_hover_text("re-read compiler.py, assembler.py, linker.py and build.py from the server").clicked() {
                    python::reload();
                }
                let col = if self.py.ready { theme::OK } else if self.py.status.starts_with("failed") || self.py.status.starts_with("pyodide failed") { theme::ERR } else { theme::WARN };
                let files = if self.py.files.is_empty() { "nothing loaded yet".to_string() } else { self.py.files.join("\n") };
                widgets::pill(ui, &self.py.status, col).on_hover_text(format!("the toolchain running in this page:\n{}", files));
            });
        });
    }

    // ----- pipeline strip -----

    fn pipeline(&mut self, ui: &mut egui::Ui) {
        let b = &self.build;
        let is_c = self.file().kind == Kind::C;
        let err_stage = b.error.as_ref().map(|e| e.stage.as_str());
        let failed = |stages: &[&str]| err_stage.map_or(false, |s| stages.contains(&s));

        struct Node {
            tab: Tab,
            title: &'static str,
            metric: String,
            color: Color32,
            state: NodeState,
        }
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

        let insns = b.insn_count();
        let assembled = b.text.is_some();
        let cpu_metric = self.cpu.metric();
        let nodes = vec![
            Node {
                tab: Tab::Preprocess,
                title: "preprocess",
                metric: b.preprocessed.as_ref().map_or("-".into(), |p| format!("{} lines", p.lines().count())),
                color: theme::SOURCE,
                state: state(b.preprocessed.is_some(), failed(&["preprocess", "import"]), is_c),
            },
            Node {
                tab: Tab::Tokens,
                title: "lex",
                metric: b.tokens.as_ref().map_or("-".into(), |t| format!("{} tokens", t.len())),
                color: theme::TOKENS,
                state: state(b.tokens.is_some(), failed(&["lex"]), is_c),
            },
            Node {
                tab: Tab::Ast,
                title: "parse",
                metric: b.ast.as_ref().map_or("-".into(), |a| format!("{} decls", a.len())),
                color: theme::AST,
                state: state(b.ast.is_some(), failed(&["parse"]), is_c),
            },
            Node {
                tab: Tab::Asm,
                title: "codegen + assemble",
                metric: if assembled { format!("{} instrs", insns) } else { "-".into() },
                color: theme::ASM,
                state: state(assembled && !failed(&["assemble"]), failed(&["codegen", "assemble"]), true),
            },
            Node {
                tab: Tab::Link,
                title: "link",
                metric: b.link.as_ref().map_or("-".into(), |l| format!("{} syms, {} relocs", l.symbols.len(), l.patches.len())),
                color: theme::LINK,
                state: state(b.elf.is_some(), failed(&["link"]), true),
            },
            Node {
                tab: Tab::Elf,
                title: "elf",
                metric: b.elf.as_ref().map_or("-".into(), |e| widgets::bytes(e.len())),
                color: theme::ELF,
                state: state(b.elf.is_some(), failed(&["build.py", "bridge"]), true),
            },
            Node {
                tab: Tab::Cpu,
                title: "arm7 cpu",
                metric: cpu_metric,
                color: theme::CPU,
                state: state(b.flat.is_some(), false, true),
            },
        ];

        let height = 64.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::hover());
        let painter = ui.painter_at(rect);
        let ghost_w = 210.0;
        let gap = 18.0;
        let n = nodes.len() as f32;
        let w = ((rect.width() - ghost_w - gap * n) / n).max(80.0);
        let mut x = rect.left();
        for (i, node) in nodes.iter().enumerate() {
            let r = egui::Rect::from_min_size(egui::pos2(x, rect.top() + 4.0), egui::vec2(w, height - 8.0));
            let resp = ui.interact(r, ui.scope_id().with(("stage", i)), Sense::click());
            let selected = self.tab == node.tab;
            let (fg, border) = match node.state {
                NodeState::Done => (theme::TEXT, node.color.gamma_multiply(0.6)),
                NodeState::Failed => (theme::ERR, theme::ERR),
                NodeState::Skipped => (theme::FAINT, theme::BORDER),
                NodeState::NotApplicable => (theme::FAINT, theme::BORDER),
            };
            let fill = if selected {
                node.color.gamma_multiply(0.16)
            } else if resp.hovered() {
                theme::RAISED
            } else {
                theme::PANEL
            };
            painter.rect(r, CornerRadius::same(6), fill, Stroke::new(if selected { 1.5 } else { 1.0 }, if selected { node.color } else { border }), StrokeKind::Inside);
            painter.rect_filled(
                egui::Rect::from_min_size(r.left_top() + egui::vec2(0.0, 6.0), egui::vec2(3.0, r.height() - 12.0)),
                CornerRadius::same(1),
                if node.state == NodeState::Done || node.state == NodeState::Failed { if node.state == NodeState::Failed { theme::ERR } else { node.color } } else { theme::FAINT },
            );
            painter.text(r.left_top() + egui::vec2(12.0, 9.0), Align2::LEFT_TOP, node.title, egui::FontId::proportional(13.5), fg);
            let metric = match node.state {
                NodeState::Failed => "failed".to_string(),
                NodeState::NotApplicable => "n/a for .s".to_string(),
                _ => node.metric.clone(),
            };
            painter.text(r.left_bottom() + egui::vec2(12.0, -9.0), Align2::LEFT_BOTTOM, metric, theme::mono(11.5), if node.state == NodeState::Done { theme::DIM } else { fg });
            if resp.clicked() {
                self.tab = node.tab;
            }
            x += w;
            // arrow to the next node
            let y = r.center().y;
            let a = egui::pos2(x + 3.0, y);
            painter.arrow(a, egui::vec2(gap - 6.0, 0.0), Stroke::new(1.2, theme::FAINT));
            x += gap;
        }
        // what comes after the cpu, per the outline
        let r = egui::Rect::from_min_size(egui::pos2(x, rect.top() + 4.0), egui::vec2(ghost_w - 4.0, height - 8.0));
        let resp = ui.interact(r, ui.scope_id().with("roadmap"), Sense::click());
        let selected = self.tab == Tab::Roadmap;
        let stroke = Stroke::new(1.0, if selected || resp.hovered() { theme::DIM } else { theme::BORDER });
        dashed_rect(&painter, r, stroke);
        painter.text(r.left_top() + egui::vec2(12.0, 9.0), Align2::LEFT_TOP, "next in the course", egui::FontId::proportional(13.5), theme::DIM);
        painter.text(r.left_bottom() + egui::vec2(12.0, -9.0), Align2::LEFT_BOTTOM, "bootloader > os > browser", theme::mono(11.5), theme::FAINT);
        if resp.clicked() {
            self.tab = Tab::Roadmap;
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum NodeState {
    Done,
    Failed,
    Skipped,
    NotApplicable,
}

fn banner(ui: &mut egui::Ui, color: Color32, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE
        .fill(color.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.6)))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
    ui.add_space(6.0);
}

fn dashed_rect(p: &egui::Painter, r: egui::Rect, stroke: Stroke) {
    let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    for w in pts.windows(2) {
        p.add(egui::Shape::dashed_line(&[w[0], w[1]], stroke, 5.0, 4.0));
    }
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
        set_location_hash(&self.link());

        egui::Panel::top("top")
            .frame(egui::Frame::NONE.fill(theme::BG).inner_margin(egui::Margin::symmetric(14, 10)))
            .show(ui, |ui| {
                self.top_bar(ui);
                ui.add_space(8.0);
                self.pipeline(ui);
            });

        egui::Panel::left("editor")
            .resizable(true)
            .default_size(470.0)
            .frame(egui::Frame::NONE.fill(theme::PANEL).inner_margin(egui::Margin::same(12)))
            .show(ui, |ui| self.editor(ui));

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme::BG).inner_margin(egui::Margin::same(12)))
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
                    Tab::Roadmap => views::roadmap::show(ui, &self.cpu),
                }
            });
    }
}
