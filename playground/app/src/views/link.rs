// what the linker did: where every section and function landed, which
// words it patched (relocations drawn as arcs), and the call graph those
// patches imply.

use crate::{theme, widgets};
use eframe::egui::{self, epaint::QuadraticBezierShape, Align2, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, StrokeKind};
use std::collections::{BTreeMap, HashMap, VecDeque};
use toolchain::linker::{ElfLinker, Patch, R_ARM_ABS32};
use toolchain::pipeline::Build;

pub struct LinkView {
    hovered_patch: Option<usize>,
    scene: Rect,
    show_local: bool,
}

impl Default for LinkView {
    fn default() -> Self {
        LinkView { hovered_patch: None, scene: Rect::ZERO, show_local: false }
    }
}

pub struct Func {
    pub name: String,
    pub start: u32,
    pub end: u32,
}

// functions = global symbols in .text (local .L labels are jump targets)
pub fn functions(ld: &ElfLinker) -> Vec<Func> {
    let Some(text) = ld.sections.first() else { return vec![] };
    let mut v: Vec<(String, u32)> = ld
        .symbols
        .iter()
        .filter(|(n, s)| s.section == 0 && !n.starts_with(".L"))
        .map(|(n, s)| (n.to_string(), s.value))
        .collect();
    v.sort_by_key(|(_, a)| *a);
    let end = text.addr + text.size();
    (0..v.len())
        .map(|i| Func { name: v[i].0.clone(), start: v[i].1, end: v.get(i + 1).map_or(end, |n| n.1) })
        .collect()
}

pub fn func_at(funcs: &[Func], addr: u32) -> Option<usize> {
    funcs.iter().position(|f| f.start <= addr && addr < f.end)
}

impl LinkView {
    pub fn on_rebuild(&mut self) {
        self.scene = Rect::ZERO;
        self.hovered_patch = None;
    }

    pub fn show(&mut self, ui: &mut egui::Ui, b: &Build) {
        let Some(ld) = &b.linker else {
            widgets::empty(ui, "nothing linked yet: an earlier stage failed");
            return;
        };
        widgets::heading(ui, "linker", theme::LINK);
        widgets::caption(
            ui,
            "sections are placed right after the 52-byte elf header and 32-byte program header at 0x10000, \
             symbols become addresses, then every relocation is patched: ABS32 writes an address, CALL rewrites \
             a branch offset.",
        );
        ui.add_space(4.0);
        let funcs = functions(ld);

        widgets::card(ui, |ui| {
            ui.set_width(ui.available_width());
            self.memory_map(ui, ld, &funcs);
        });
        ui.add_space(8.0);

        ui.columns(2, |cols| {
            widgets::card(&mut cols[0], |ui| {
                ui.label(RichText::new("call graph (from R_ARM_CALL relocations)").color(theme::DIM));
                ui.set_min_height(ui.available_height());
                self.call_graph(ui, ld, &funcs);
            });
            widgets::card(&mut cols[1], |ui| {
                ui.set_min_height(ui.available_height());
                self.tables(ui, ld);
            });
        });
    }

    fn memory_map(&mut self, ui: &mut egui::Ui, ld: &ElfLinker, funcs: &[Func]) {
        let start = ld.load_addr;
        let end = ld.sections.iter().map(|s| s.addr + s.size()).max().unwrap_or(start + 1);
        let span = (end - start).max(1) as f32;
        let w = ui.available_width();
        let (r, resp) = ui.allocate_exact_size(egui::vec2(w, 150.0), Sense::hover());
        let p = ui.painter_at(r);
        let bar = Rect::from_min_size(egui::pos2(r.left(), r.top() + 70.0), egui::vec2(w, 30.0));
        let x_of = |a: u32| bar.left() + (a.saturating_sub(start)) as f32 / span * w;

        // headers, then sections
        let hdr = Rect::from_x_y_ranges(bar.left()..=x_of(ld.sections.first().map_or(end, |s| s.addr)), bar.y_range());
        p.rect_filled(hdr, CornerRadius::same(2), theme::FAINT.gamma_multiply(0.5));
        for (i, s) in ld.sections.iter().enumerate() {
            let sr = Rect::from_x_y_ranges(x_of(s.addr)..=x_of(s.addr + s.size()).max(x_of(s.addr) + 1.0), bar.y_range());
            let col = if i == 0 { theme::ASM } else { theme::LINK };
            p.rect(sr, CornerRadius::same(2), col.gamma_multiply(0.12), Stroke::new(1.0, col.gamma_multiply(0.6)), StrokeKind::Inside);
            p.text(egui::pos2(sr.left() + 4.0, bar.bottom() + 4.0), Align2::LEFT_TOP, format!("{} 0x{:x}", s.name, s.addr), theme::mono(11.0), col);
        }
        // functions as blocks inside .text
        let mut hover_func = None;
        for f in funcs {
            let fr = Rect::from_x_y_ranges(x_of(f.start)..=x_of(f.end), (bar.top() + 4.0)..=(bar.bottom() - 4.0));
            let col = theme::hashed(&f.name);
            p.rect_filled(fr.shrink2(egui::vec2(0.5, 0.0)), CornerRadius::same(2), col.gamma_multiply(0.45));
            if fr.width() > 44.0 {
                p.text(fr.center(), Align2::CENTER_CENTER, &f.name, theme::mono(10.5), theme::TEXT);
            }
            if resp.hover_pos().map_or(false, |h| fr.contains(h)) {
                hover_func = Some(f);
            }
        }
        p.text(egui::pos2(bar.left(), bar.bottom() + 20.0), Align2::LEFT_TOP, format!("0x{:x}", start), theme::mono(10.5), theme::FAINT);
        p.text(egui::pos2(bar.right(), bar.bottom() + 20.0), Align2::RIGHT_TOP, format!("0x{:x}", end), theme::mono(10.5), theme::FAINT);
        p.text(egui::pos2(hdr.left() + 2.0, bar.top() - 4.0), Align2::LEFT_BOTTOM, "hdrs", egui::FontId::proportional(10.0), theme::DIM);

        // relocation arcs above the bar, from patched word to target
        for (i, pt) in ld.patches.iter().enumerate() {
            let (a, z) = (x_of(pt.place), x_of(pt.target));
            if (a - z).abs() < 1.0 {
                continue;
            }
            let on = self.hovered_patch == Some(i);
            let col = if pt.rtype == R_ARM_ABS32 { theme::ELF } else { theme::CPU };
            let lift = ((a - z).abs() * 0.35).clamp(8.0, 62.0);
            let mid = Pos2::new((a + z) / 2.0, bar.top() - lift);
            let alpha = if on { 1.0 } else if self.hovered_patch.is_some() { 0.08 } else { 0.28 };
            p.add(QuadraticBezierShape::from_points_stroke(
                [Pos2::new(a, bar.top()), mid, Pos2::new(z, bar.top())],
                false,
                Color32::TRANSPARENT,
                Stroke::new(if on { 2.0 } else { 1.0 }, col.gamma_multiply(alpha)),
            ));
        }
        if let Some(f) = hover_func {
            resp.on_hover_text(format!("{}  0x{:x}..0x{:x}  ({} bytes)", f.name, f.start, f.end, f.end - f.start));
        }
        ui.horizontal(|ui| {
            widgets::pill(ui, "CALL: branch offset", theme::CPU);
            widgets::pill(ui, "ABS32: absolute address", theme::ELF);
            widgets::caption(ui, "hover a row in the relocation table to isolate its arc");
        });
    }

    fn call_graph(&mut self, ui: &mut egui::Ui, ld: &ElfLinker, funcs: &[Func]) {
        let names: HashMap<u32, usize> = funcs.iter().enumerate().map(|(i, f)| (f.start, i)).collect();
        let mut edges: BTreeMap<(usize, usize), usize> = BTreeMap::new();
        for pt in &ld.patches {
            if pt.rtype == R_ARM_ABS32 {
                continue;
            }
            if let (Some(from), Some(&to)) = (func_at(funcs, pt.place), names.get(&pt.target)) {
                if from != to {
                    *edges.entry((from, to)).or_default() += 1;
                }
            }
        }
        // layer by bfs depth from _start
        let mut depth = vec![usize::MAX; funcs.len()];
        let mut q = VecDeque::new();
        if let Some(s) = funcs.iter().position(|f| f.name == "_start") {
            depth[s] = 0;
            q.push_back(s);
        }
        while let Some(n) = q.pop_front() {
            // edges are sorted by (from, to), so this range is n's callees
            for (&(_, z), _) in edges.range((n, 0)..(n + 1, 0)) {
                if depth[z] == usize::MAX {
                    depth[z] = depth[n] + 1;
                    q.push_back(z);
                }
            }
        }
        let max_d = depth.iter().filter(|d| **d != usize::MAX).max().copied().unwrap_or(0);
        let mut layers: Vec<Vec<usize>> = vec![vec![]; max_d + 2];
        for (i, d) in depth.iter().enumerate() {
            layers[if *d == usize::MAX { max_d + 1 } else { *d }].push(i);
        }
        let (bw, bh, gx, gy) = (120.0, 26.0, 18.0, 64.0);
        let mut pos = vec![Pos2::ZERO; funcs.len()];
        for (d, layer) in layers.iter().enumerate() {
            for (k, &f) in layer.iter().enumerate() {
                pos[f] = Pos2::new(k as f32 * (bw + gx), d as f32 * gy);
            }
        }
        let width = layers.iter().map(|l| l.len()).max().unwrap_or(1) as f32 * (bw + gx);
        let height = layers.len() as f32 * gy;

        egui::Scene::new().zoom_range(0.2..=2.0).show(ui, &mut self.scene, |ui| {
            let (resp, p) = ui.allocate_painter(egui::vec2(width.max(200.0), height.max(80.0)), Sense::hover());
            let o = resp.rect.min.to_vec2();
            for (&(a, z), &n) in &edges {
                let from = pos[a] + o + egui::vec2(bw / 2.0, bh);
                let to = pos[z] + o + egui::vec2(bw / 2.0, 0.0);
                let col = theme::hashed(&funcs[a].name).gamma_multiply(0.7);
                let ctrl = if to.y <= from.y { Pos2::new((from.x + to.x) / 2.0 + 40.0, from.y.min(to.y) - 30.0) } else { Pos2::new((from.x + to.x) / 2.0, (from.y + to.y) / 2.0) };
                p.add(QuadraticBezierShape::from_points_stroke([from, ctrl, to], false, Color32::TRANSPARENT, Stroke::new(1.0 + (n as f32).ln_1p(), col)));
                p.circle_filled(to, 2.5, col);
                if n > 1 {
                    p.text(ctrl, Align2::CENTER_CENTER, format!("x{}", n), egui::FontId::proportional(10.0), theme::DIM);
                }
            }
            for (i, f) in funcs.iter().enumerate() {
                let r = Rect::from_min_size(pos[i] + o, egui::vec2(bw, bh));
                let col = theme::hashed(&f.name);
                let unreachable = depth[i] == usize::MAX;
                p.rect(r, CornerRadius::same(5), col.gamma_multiply(if unreachable { 0.05 } else { 0.15 }), Stroke::new(1.0, col.gamma_multiply(if unreachable { 0.3 } else { 0.8 })), StrokeKind::Inside);
                p.text(r.center(), Align2::CENTER_CENTER, &f.name, theme::mono(11.5), if unreachable { theme::DIM } else { theme::TEXT });
            }
            if !layers[max_d + 1].is_empty() {
                p.text(o.to_pos2() + egui::vec2(0.0, (max_d + 1) as f32 * gy - 14.0), Align2::LEFT_BOTTOM, "never called from _start:", egui::FontId::proportional(10.5), theme::FAINT);
            }
        });
    }

    fn tables(&mut self, ui: &mut egui::Ui, ld: &ElfLinker) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("relocations ({})", ld.patches.len())).color(theme::DIM));
        });
        let mut hovered = None;
        egui::ScrollArea::vertical().id_salt("patches").max_height(ui.available_height() * 0.55).auto_shrink([false, true]).show(ui, |ui| {
            egui::Grid::new("patch-grid").striped(true).num_columns(4).show(ui, |ui| {
                for h in ["place", "type", "symbol -> target", "before -> after"] {
                    ui.label(RichText::new(h).size(11.5).color(theme::FAINT));
                }
                ui.end_row();
                for (i, pt) in ld.patches.iter().enumerate() {
                    let row = patch_row(ui, pt);
                    if row {
                        hovered = Some(i);
                    }
                    ui.end_row();
                }
            });
        });
        if hovered.is_some() || !ui.ui_contains_pointer() {
            self.hovered_patch = hovered;
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("symbols ({})", ld.symbols.len())).color(theme::DIM));
            ui.checkbox(&mut self.show_local, "include .L labels");
        });
        egui::ScrollArea::vertical().id_salt("syms").auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("sym-grid").striped(true).num_columns(3).show(ui, |ui| {
                for (name, s) in ld.symbols.iter() {
                    if !self.show_local && name.starts_with(".L") {
                        continue;
                    }
                    ui.label(RichText::new(name).monospace().color(if s.section == 0 { theme::hashed(name) } else { theme::LINK }));
                    ui.label(RichText::new(format!("0x{:08x}", s.value)).monospace().color(theme::DIM));
                    ui.label(RichText::new(if s.section == 0 { ".text func" } else { ".data object" }).size(11.5).color(theme::FAINT));
                    ui.end_row();
                }
            });
        });
    }
}

fn patch_row(ui: &mut egui::Ui, pt: &Patch) -> bool {
    let a = ui.label(RichText::new(format!("0x{:06x}", pt.place)).monospace());
    let t = ui.label(RichText::new(if pt.rtype == R_ARM_ABS32 { "ABS32" } else { "CALL" }).monospace().color(if pt.rtype == R_ARM_ABS32 { theme::ELF } else { theme::CPU }));
    let s = ui.label(RichText::new(format!("{} 0x{:x}", pt.symbol, pt.target)).monospace());
    let d = ui.label(RichText::new(format!("{:08x} -> {:08x}", pt.before, pt.after)).monospace().color(theme::DIM));
    a.hovered() || t.hovered() || s.hovered() || d.hovered()
}
