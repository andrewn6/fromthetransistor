// the parse tree as a graph, one top-level declaration at a time, plus the
// c layout of every struct (offsets, padding, size).
//
// the tree is whatever dataclasses compiler.py builds, sent over as
// {"_": "ClassName", field: value}. known classes get friendlier labels;
// anything new still shows up, labeled by its class and fields.

use crate::snapshot::{Snapshot, StructLayout};
use crate::{theme, widgets};
use eframe::egui::{self, epaint::CubicBezierShape, Align2, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, StrokeKind};
use serde_json::Value;

const LEVEL: f32 = 58.0;
const NODE_H: f32 = 24.0;
const GAP: f32 = 10.0;

pub struct AstView {
    selected: Option<String>,
    scene: Rect,
    // a fresh tree first auto-fits; if that is too small to read, the next
    // frame re-centers on the root at 100% zoom
    refit: bool,
}

impl Default for AstView {
    fn default() -> Self {
        AstView { selected: None, scene: Rect::ZERO, refit: true }
    }
}

struct TNode {
    label: String,
    role: String,
    color: Color32,
    children: Vec<TNode>,
}

const DECL: Color32 = theme::AST;
const STMT: Color32 = Color32::from_rgb(0x6f, 0xa8, 0xf5);
const OP: Color32 = Color32::from_rgb(0xf0, 0x96, 0x4f);
const LIT: Color32 = Color32::from_rgb(0xe8, 0xd0, 0x6a);
const NAME: Color32 = Color32::from_rgb(0x9c, 0xd7, 0xff);

fn class(v: &Value) -> &str {
    v.get("_").and_then(|c| c.as_str()).unwrap_or("")
}

fn s(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(x)) => x.clone(),
        Some(Value::Null) | None => String::new(),
        Some(o) => o.to_string(),
    }
}

fn is_type(v: &Value) -> bool {
    v.get("_type").and_then(|t| t.as_bool()).unwrap_or(false)
}

// TInt -> int, TPtr(TChar) -> char*, ...
pub fn ctype(v: &Value) -> String {
    match class(v) {
        "TInt" => "int".into(),
        "TUInt" => "unsigned int".into(),
        "TChar" => "char".into(),
        "TVoid" => "void".into(),
        "TPtr" => format!("{}*", ctype(&v["inner"])),
        "TArr" => format!("{}[{}]", ctype(&v["elem"]), s(v, "count")),
        "TStruct" => format!("struct {}", s(v, "tag")),
        "" => v.to_string(),
        other => other.to_string(),
    }
}

fn short(text: &str) -> String {
    let q = format!("{:?}", text);
    if q.chars().count() > 22 { format!("{}...\"", q.chars().take(19).collect::<String>()) } else { q }
}

fn op_symbol(op: &str) -> &str {
    match op {
        "add" => "+", "sub" => "-", "mul" => "*", "div" => "/", "mod" => "%",
        "eq" => "==", "ne" => "!=", "lt" => "<", "le" => "<=", "gt" => ">", "ge" => ">=",
        "and" => "&&", "or" => "||", "band" => "&", "bor" => "|", "bxor" => "^",
        "shl" => "<<", "shr" => ">>", "neg" => "-", "not" => "!", "bnot" => "~",
        other => other,
    }
}

fn leaf(label: String, color: Color32) -> TNode {
    TNode { label, role: String::new(), color, children: vec![] }
}

// generic: every dataclass/list field becomes a child (named by its role),
// scalar fields and types go into the label
fn generic(v: &Value, label: Option<String>, color: Color32) -> TNode {
    let mut scalars = vec![];
    let mut children = vec![];
    if let Some(o) = v.as_object() {
        for (k, f) in o {
            if k == "_" || k == "_type" {
                continue;
            }
            match f {
                Value::Object(_) if is_type(f) => scalars.push(ctype(f)),
                Value::Object(_) => children.push(with_role(node(f), k)),
                Value::Array(xs) => {
                    for x in xs {
                        // (Type, name) pairs, e.g. params
                        if let Value::Array(pair) = x {
                            let parts: Vec<String> = pair.iter().map(|p| if is_type(p) { ctype(p) } else { p.as_str().map(String::from).unwrap_or(p.to_string()) }).collect();
                            children.push(with_role(leaf(parts.join(" "), NAME), k));
                        } else if x.is_object() {
                            children.push(with_role(node(x), k));
                        } else {
                            children.push(with_role(leaf(x.to_string(), LIT), k));
                        }
                    }
                }
                Value::Null => {}
                Value::String(t) => scalars.push(t.clone()),
                other => scalars.push(other.to_string()),
            }
        }
    }
    let label = label.unwrap_or_else(|| {
        if scalars.is_empty() { class(v).to_string() } else { format!("{} {}", class(v), scalars.join(" ")) }
    });
    TNode { label, role: String::new(), color, children }
}

fn with_role(mut n: TNode, role: &str) -> TNode {
    n.role = role.to_string();
    n
}

fn kids(v: &Value, fields: &[(&str, &str)]) -> Vec<TNode> {
    let mut out = vec![];
    for (k, role) in fields {
        match v.get(*k) {
            Some(Value::Array(xs)) => out.extend(xs.iter().map(|x| with_role(node(x), role))),
            Some(x) if x.is_object() => out.push(with_role(node(x), role)),
            _ => {}
        }
    }
    out
}

fn node(v: &Value) -> TNode {
    let c = class(v);
    let n = |label: String, color: Color32, fields: &[(&str, &str)]| TNode { label, role: String::new(), color, children: kids(v, fields) };
    match c {
        "IntLit" => leaf(s(v, "val"), LIT),
        "CharLit" => leaf(format!("{:?}", s(v, "val")), LIT),
        "StrLit" => leaf(short(&s(v, "val")), LIT),
        "Var" => leaf(s(v, "name"), NAME),
        "Assign" => n("=".into(), OP, &[("target", ""), ("value", "")]),
        "CompAssign" => n(format!("{}=", op_symbol(&s(v, "op"))), OP, &[("target", ""), ("value", "")]),
        "PostIncDec" => n(if s(v, "delta").starts_with('-') { "x--".into() } else { "x++".into() }, OP, &[("target", "")]),
        "Sizeof" if is_type(&v["operand"]) => leaf(format!("sizeof({})", ctype(&v["operand"])), OP),
        "Sizeof" => n("sizeof".into(), OP, &[("operand", "")]),
        "VaArg" => leaf(format!("va_arg({})", ctype(&v["ty"])), OP),
        "BinOp" => n(op_symbol(&s(v, "op")).into(), OP, &[("lhs", ""), ("rhs", "")]),
        "UnOp" => n(op_symbol(&s(v, "op")).into(), OP, &[("operand", "")]),
        "Call" => n(format!("call {}", s(v, "name")), NAME, &[("args", "")]),
        "Index" => n("[ ]".into(), OP, &[("base", "base"), ("idx", "index")]),
        "Member" => n(format!(".{}", s(v, "name")), OP, &[("base", "")]),
        "Deref" => n("*".into(), OP, &[("operand", "")]),
        "AddrOf" => n("&".into(), OP, &[("operand", "")]),
        "SExpr" => node(&v["expr"]),
        "SBlock" => n("block".into(), STMT, &[("stmts", "")]),
        "SIf" => n("if".into(), STMT, &[("cond", "cond"), ("then", "then"), ("els", "else")]),
        "SWhile" => n("while".into(), STMT, &[("cond", "cond"), ("body", "body")]),
        "SFor" => n("for".into(), STMT, &[("init", "init"), ("cond", "cond"), ("step", "step"), ("body", "body")]),
        "SReturn" => n("return".into(), STMT, &[("expr", "")]),
        "SDecl" => n(format!("{} {}", ctype(&v["type"]), s(v, "name")), DECL, &[("init", "init")]),
        "SBreak" => leaf("break".into(), STMT),
        "SContinue" => leaf("continue".into(), STMT),
        "Func" => {
            let params: Vec<String> = v["params"].as_array().map(|ps| ps.iter().map(|p| format!("{} {}", ctype(&p[0]), p[1].as_str().unwrap_or("?"))).collect()).unwrap_or_default();
            let va = if v["varargs"].as_bool().unwrap_or(false) { ", ..." } else { "" };
            let label = format!("{} {}({}{})", ctype(&v["ret"]), s(v, "name"), params.join(", "), va);
            // show the body's statements directly under the function
            let body = &v["body"];
            let children = if class(body) == "SBlock" { kids(body, &[("stmts", "")]) } else { vec![node(body)] };
            TNode { label, role: String::new(), color: DECL, children }
        }
        "Global" => n(format!("global {} {}", ctype(&v["type"]), s(v, "name")), DECL, &[("init", "")]),
        "StructDef" => leaf(format!("struct {}", s(v, "tag")), DECL),
        "TypedefDef" => leaf(format!("typedef {}", s(v, "name")), DECL),
        _ if is_type(v) => leaf(ctype(v), NAME),
        _ => generic(v, None, theme::WARN),
    }
}

fn decl_name(d: &Value) -> String {
    match class(d) {
        "Func" => format!("{}()", s(d, "name")),
        "Global" => s(d, "name"),
        "StructDef" => format!("struct {}", s(d, "tag")),
        "TypedefDef" => format!("typedef {}", s(d, "name")),
        other => other.to_string(),
    }
}

struct Placed {
    rect: Rect,
    label: String,
    role: String,
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
    out.push(Placed { rect: Rect::NOTHING, label: n.label.clone(), role: n.role.clone(), color: n.color, parent });
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
        self.refit = true;
    }

    pub fn show(&mut self, ui: &mut egui::Ui, b: &Snapshot) {
        let Some(decls) = &b.ast else {
            widgets::empty(ui, "no syntax tree: this is an assembly file, or an earlier stage failed");
            return;
        };
        widgets::heading(ui, "parser", theme::AST);
        widgets::caption(ui, "the dataclasses compiler.py's Parser builds. drag to pan, scroll to zoom. orange nodes are classes this view has no special label for.");

        let names: Vec<String> = decls.iter().map(decl_name).collect();
        if self.selected.as_ref().map_or(true, |s| !names.contains(s)) {
            self.selected = names.iter().find(|n| *n == "main()").or(names.first()).cloned();
        }
        ui.horizontal_wrapped(|ui| {
            for n in &names {
                if ui.selectable_label(self.selected.as_ref() == Some(n), RichText::new(n).monospace()).clicked() {
                    self.selected = Some(n.clone());
                    self.scene = Rect::ZERO;
                    self.refit = true;
                }
            }
        });
        let Some(i) = names.iter().position(|n| Some(n) == self.selected.as_ref()) else { return };
        let decl = &decls[i];

        if class(decl) == "StructDef" {
            if let Some(l) = b.structs.iter().find(|l| l.tag == s(decl, "tag")) {
                struct_layout(ui, l);
            }
        } else if !b.structs.is_empty() {
            egui::CollapsingHeader::new(RichText::new("struct layouts").color(theme::DIM)).show(ui, |ui| {
                for l in &b.structs {
                    struct_layout(ui, l);
                }
            });
        }

        let tree = node(decl);
        let mut placed = Vec::new();
        let mut cursor = 0.0;
        place(&tree, 0, &mut cursor, &mut placed, None);
        let bounds = placed.iter().fold(Rect::NOTHING, |a, p| a.union(p.rect)).expand(20.0);

        egui::Frame::NONE.fill(theme::PANEL).stroke(Stroke::new(1.0, theme::BORDER)).corner_radius(CornerRadius::same(6)).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            let view = ui.available_size();
            let mut root = None;
            egui::Scene::new().zoom_range(0.1..=2.5).show(ui, &mut self.scene, |ui| {
                let (resp, painter) = ui.allocate_painter(bounds.size(), Sense::hover());
                let off = resp.rect.min - bounds.min;
                root = placed.first().map(|p| p.rect.translate(off).center_top());
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
                        painter.text(r.center_top() - egui::vec2(0.0, 3.0), Align2::CENTER_BOTTOM, &p.role, egui::FontId::proportional(10.0), theme::DIM);
                    }
                }
            });
            if self.refit {
                self.refit = false;
                if let Some(root) = root.filter(|_| bounds.width() > view.x * 1.4) {
                    // scene rect == view size means 100% zoom
                    self.scene = Rect::from_min_size(root - egui::vec2(view.x / 2.0, 24.0), view);
                    ui.ctx().request_repaint();
                }
            }
        });
    }
}

fn struct_layout(ui: &mut egui::Ui, l: &StructLayout) {
    widgets::card(ui, |ui| {
        ui.label(RichText::new(format!("struct {}  {} bytes", l.tag, l.size)).monospace());
        let total = l.size.max(1) as f32;
        let w = ui.available_width().min(900.0);
        let (r, _) = ui.allocate_exact_size(egui::vec2(w, 34.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(r, CornerRadius::same(3), theme::BG);
        // padding shows as the gaps between member boxes
        for m in &l.members {
            let x0 = r.left() + m.off as f32 / total * w;
            let x1 = r.left() + (m.off + m.size) as f32 / total * w;
            let mr = Rect::from_x_y_ranges(x0..=x1.max(x0 + 2.0), r.y_range()).shrink2(egui::vec2(0.5, 0.0));
            let col = theme::hashed(&m.name);
            p.rect(mr, CornerRadius::same(3), col.gamma_multiply(0.22), Stroke::new(1.0, col), StrokeKind::Inside);
            if mr.width() > 30.0 {
                p.text(mr.center(), Align2::CENTER_CENTER, format!("{} @{}", m.name, m.off), theme::mono(11.0), theme::TEXT);
            }
        }
        ui.horizontal_wrapped(|ui| {
            for m in &l.members {
                ui.label(RichText::new(format!("+{:<3} {} {} ({}B, align {})", m.off, m.ty, m.name, m.size, m.align)).monospace().size(11.5).color(theme::hashed(&m.name)));
            }
        });
    });
}
