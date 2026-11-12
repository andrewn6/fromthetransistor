// the parse tree as a graph, one top-level declaration at a time, plus the
// c layout of every struct (offsets, padding, size)

use crate::{theme, widgets};
use eframe::egui::{self, epaint::CubicBezierShape, Align2, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, StrokeKind};
use toolchain::compiler::ast::*;
use toolchain::compiler::codegen::Layout;
use toolchain::pipeline::Build;

const LEVEL: f32 = 58.0;
const NODE_H: f32 = 24.0;
const GAP: f32 = 10.0;

pub struct AstView {
    selected: Option<String>,
    scene: Rect,
}

impl Default for AstView {
    fn default() -> Self {
        AstView { selected: None, scene: Rect::ZERO }
    }
}

struct TNode {
    label: String,
    role: &'static str,
    color: Color32,
    children: Vec<TNode>,
}

fn node(label: impl Into<String>, color: Color32, children: Vec<TNode>) -> TNode {
    TNode { label: label.into(), role: "", color, children }
}

fn with_role(mut n: TNode, role: &'static str) -> TNode {
    n.role = role;
    n
}

const DECL: Color32 = theme::AST;
const STMT: Color32 = Color32::from_rgb(0x6f, 0xa8, 0xf5);
const OP: Color32 = Color32::from_rgb(0xf0, 0x96, 0x4f);
const LIT: Color32 = Color32::from_rgb(0xe8, 0xd0, 0x6a);
const NAME: Color32 = Color32::from_rgb(0x9c, 0xd7, 0xff);

fn short(s: &str) -> String {
    let s = format!("{:?}", s);
    if s.chars().count() > 22 { format!("{}...\"", s.chars().take(19).collect::<String>()) } else { s }
}

fn expr(e: &Expr) -> TNode {
    match e {
        Expr::IntLit(v) => node(v.to_string(), LIT, vec![]),
        Expr::CharLit(c) => node(format!("{:?}", c), LIT, vec![]),
        Expr::StrLit(s) => node(short(s), LIT, vec![]),
        Expr::Var(n) => node(n.clone(), NAME, vec![]),
        Expr::Assign(t, v) => node("=", OP, vec![expr(t), expr(v)]),
        Expr::CompAssign(op, t, v) => node(format!("{}=", op.symbol()), OP, vec![expr(t), expr(v)]),
        Expr::PostIncDec(t, d) => node(if *d > 0 { "x++" } else { "x--" }, OP, vec![expr(t)]),
        Expr::Sizeof(SizeofArg::Type(t)) => node(format!("sizeof({})", t.c_name()), OP, vec![]),
        Expr::Sizeof(SizeofArg::Expr(x)) => node("sizeof", OP, vec![expr(x)]),
        Expr::VaArg(t) => node(format!("va_arg({})", t.c_name()), OP, vec![]),
        Expr::BinOp(op, l, r) => node(op.symbol(), OP, vec![expr(l), expr(r)]),
        Expr::UnOp(op, o) => node(op.symbol(), OP, vec![expr(o)]),
        Expr::Call(n, args) => node(format!("call {}", n), NAME, args.iter().map(expr).collect()),
        Expr::Index(b, i) => node("[ ]", OP, vec![with_role(expr(b), "base"), with_role(expr(i), "index")]),
        Expr::Member(b, n) => node(format!(".{}", n), OP, vec![expr(b)]),
        Expr::Deref(o) => node("*", OP, vec![expr(o)]),
        Expr::AddrOf(o) => node("&", OP, vec![expr(o)]),
    }
}

fn stmt(s: &Stmt) -> TNode {
    match s {
        Stmt::Expr(e) => expr(e),
        Stmt::Block(xs) => node("block", STMT, xs.iter().map(stmt).collect()),
        Stmt::If(c, t, e) => {
            let mut kids = vec![with_role(expr(c), "cond"), with_role(stmt(t), "then")];
            if let Some(e) = e {
                kids.push(with_role(stmt(e), "else"));
            }
            node("if", STMT, kids)
        }
        Stmt::While(c, b) => node("while", STMT, vec![with_role(expr(c), "cond"), with_role(stmt(b), "body")]),
        Stmt::For(i, c, st, b) => {
            let mut kids = vec![];
            if let Some(i) = i {
                kids.push(with_role(stmt(i), "init"));
            }
            if let Some(c) = c {
                kids.push(with_role(expr(c), "cond"));
            }
            if let Some(st) = st {
                kids.push(with_role(expr(st), "step"));
            }
            kids.push(with_role(stmt(b), "body"));
            node("for", STMT, kids)
        }
        Stmt::Return(e) => node("return", STMT, e.iter().map(expr).collect()),
        Stmt::Decl(t, n, init) => node(format!("{} {}", t.c_name(), n), DECL, init.iter().map(|e| with_role(expr(e), "init")).collect()),
        Stmt::Break => node("break", STMT, vec![]),
        Stmt::Continue => node("continue", STMT, vec![]),
    }
}

fn top(d: &TopDecl, structs: &Structs) -> TNode {
    match d {
        TopDecl::Func(f) => {
            let params: Vec<String> = f.params.iter().map(|(t, n)| format!("{} {}", t.c_name(), n)).collect();
            let va = if f.varargs { ", ..." } else { "" };
            let body = match &f.body {
                Stmt::Block(xs) => xs.iter().map(stmt).collect(),
                other => vec![stmt(other)],
            };
            node(format!("{} {}({}{})", f.ret.c_name(), f.name, params.join(", "), va), DECL, body)
        }
        TopDecl::Global(g) => node(format!("global {} {}", g.ty.c_name(), g.name), DECL, g.init.iter().map(expr).collect()),
        TopDecl::StructDef(tag) => {
            let members = structs.get(tag).map(|m| m.iter().map(|(n, t)| node(format!("{} {}", t.c_name(), n), NAME, vec![])).collect()).unwrap_or_default();
            node(format!("struct {}", tag), DECL, members)
        }
        TopDecl::TypedefDef(n) => node(format!("typedef {}", n), DECL, vec![]),
    }
}

fn decl_name(d: &TopDecl) -> String {
    match d {
        TopDecl::Func(f) => format!("{}()", f.name),
        TopDecl::Global(g) => g.name.clone(),
        TopDecl::StructDef(t) => format!("struct {}", t),
        TopDecl::TypedefDef(n) => format!("typedef {}", n),
    }
}

struct Placed {
    rect: Rect,
    label: String,
    role: &'static str,
    color: Color32,
    parent: Option<usize>,
}

fn width(n: &TNode) -> f32 {
    (n.label.chars().count() as f32 * 7.4 + 18.0).max(28.0)
}

// tidy layout: leaves left to right, parents centered over their children
fn place(n: &TNode, depth: usize, cursor: &mut f32, out: &mut Vec<Placed>, parent: Option<usize>) -> f32 {
    let w = width(n);
    let idx = out.len();
    out.push(Placed { rect: Rect::NOTHING, label: n.label.clone(), role: n.role, color: n.color, parent });
    let start = *cursor;
    let cx = if n.children.is_empty() {
        *cursor += w + GAP;
        start + w / 2.0
    } else {
        let centers: Vec<f32> = n.children.iter().map(|c| place(c, depth + 1, cursor, out, Some(idx))).collect();
        let cx = ((centers[0] + centers[centers.len() - 1]) / 2.0).max(start + w / 2.0);
        *cursor = cursor.max(cx + w / 2.0 + GAP);
        cx
    };
    out[idx].rect = Rect::from_center_size(egui::pos2(cx, depth as f32 * LEVEL + NODE_H / 2.0), egui::vec2(w, NODE_H));
    cx
}

impl AstView {
    pub fn on_rebuild(&mut self) {
        self.scene = Rect::ZERO;
    }

    pub fn show(&mut self, ui: &mut egui::Ui, b: &Build) {
        let c = &b.compiled;
        let Some(decls) = &c.ast else {
            widgets::empty(ui, "no syntax tree: this is an assembly file, or an earlier stage failed");
            return;
        };
        widgets::heading(ui, "parser", theme::AST);
        widgets::caption(ui, "recursive descent, one function per precedence level. drag to pan, scroll to zoom.");

        let names: Vec<String> = decls.iter().map(decl_name).collect();
        if self.selected.as_ref().map_or(true, |s| !names.contains(s)) {
            self.selected = names.iter().find(|n| *n == "main()").or(names.first()).cloned();
        }
        ui.horizontal_wrapped(|ui| {
            for n in &names {
                if ui.selectable_label(self.selected.as_ref() == Some(n), RichText::new(n).monospace()).clicked() {
                    self.selected = Some(n.clone());
                    self.scene = Rect::ZERO;
                }
            }
        });
        let Some(i) = names.iter().position(|n| Some(n) == self.selected.as_ref()) else { return };
        let decl = &decls[i];

        if let TopDecl::StructDef(tag) = decl {
            struct_layout(ui, c, tag);
        } else if !c.structs.is_empty() {
            egui::CollapsingHeader::new(RichText::new("struct layouts").color(theme::DIM)).show(ui, |ui| {
                for (tag, _) in c.structs.iter() {
                    struct_layout(ui, c, tag);
                }
            });
        }

        let tree = top(decl, &c.structs);
        let mut placed = Vec::new();
        let mut cursor = 0.0;
        place(&tree, 0, &mut cursor, &mut placed, None);
        let bounds = placed.iter().fold(Rect::NOTHING, |a, p| a.union(p.rect)).expand(20.0);

        egui::Frame::NONE.fill(theme::PANEL).stroke(Stroke::new(1.0, theme::BORDER)).corner_radius(CornerRadius::same(6)).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            egui::Scene::new().zoom_range(0.1..=2.5).show(ui, &mut self.scene, |ui| {
                let (resp, painter) = ui.allocate_painter(bounds.size(), Sense::hover());
                let off = resp.rect.min - bounds.min;
                for p in &placed {
                    if let Some(par) = p.parent {
                        let a = placed[par].rect.center_bottom() + off;
                        let z = p.rect.center_top() + off;
                        let mid = (a.y + z.y) / 2.0;
                        painter.add(CubicBezierShape::from_points_stroke(
                            [a, Pos2::new(a.x, mid), Pos2::new(z.x, mid), z],
                            false,
                            Color32::TRANSPARENT,
                            Stroke::new(1.0, theme::FAINT),
                        ));
                    }
                }
                for p in &placed {
                    let r = p.rect.translate(off);
                    painter.rect(r, CornerRadius::same(5), p.color.gamma_multiply(0.12), Stroke::new(1.0, p.color.gamma_multiply(0.7)), StrokeKind::Inside);
                    painter.text(r.center(), Align2::CENTER_CENTER, &p.label, theme::mono(12.0), p.color);
                    if !p.role.is_empty() {
                        painter.text(r.center_top() - egui::vec2(0.0, 3.0), Align2::CENTER_BOTTOM, p.role, egui::FontId::proportional(10.0), theme::DIM);
                    }
                }
            });
        });
    }
}

fn struct_layout(ui: &mut egui::Ui, c: &toolchain::compiler::Compiled, tag: &str) {
    let layout = Layout { structs: &c.structs };
    let Ok((members, size)) = layout.struct_layout(tag) else { return };
    widgets::card(ui, |ui| {
        ui.label(RichText::new(format!("struct {}  {} bytes", tag, size)).monospace());
        let total = size.max(1) as f32;
        let w = ui.available_width().min(900.0);
        let (r, _) = ui.allocate_exact_size(egui::vec2(w, 34.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(r, CornerRadius::same(3), theme::BG);
        // padding shows as the gaps between member boxes
        for (name, t, off) in &members {
            let sz = layout.type_size(t).unwrap_or(0) as f32;
            let x0 = r.left() + *off as f32 / total * w;
            let x1 = r.left() + (*off as f32 + sz) / total * w;
            let mr = Rect::from_x_y_ranges(x0..=x1.max(x0 + 2.0), r.y_range()).shrink2(egui::vec2(0.5, 0.0));
            let col = theme::hashed(name);
            p.rect(mr, CornerRadius::same(3), col.gamma_multiply(0.22), Stroke::new(1.0, col), StrokeKind::Inside);
            if mr.width() > 30.0 {
                p.text(mr.center(), Align2::CENTER_CENTER, format!("{} @{}", name, off), theme::mono(11.0), theme::TEXT);
            }
        }
        ui.horizontal_wrapped(|ui| {
            for (name, t, off) in &members {
                let sz = layout.type_size(t).unwrap_or(0);
                ui.label(RichText::new(format!("+{:<3} {} {} ({}B, align {})", off, t.c_name(), name, sz, layout.align_of(t).unwrap_or(1))).monospace().size(11.5).color(theme::hashed(name)));
            }
        });
    });
}
