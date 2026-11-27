// compiler output next to the machine code the assembler made from it.
// pick a line to see its 32 bits split into fields, its relocation, and
// where its literal pool entry lives.

use crate::{highlight, theme, widgets};
use eframe::egui::{self, Align2, Color32, CornerRadius, Rect, RichText, Sense, Stroke, StrokeKind};
use crate::snapshot::{LineKind, Snapshot, R_ARM_ABS32};
use crate::srcmap::{Loc, SourceMap};
use toolchain::disasm;

pub struct AsmView {
    selected: Option<usize>,
    show_runtime: bool,
    jump: Option<usize>,
    // the followed line this view last jumped to
    seen: Option<Loc>,
}

impl Default for AsmView {
    fn default() -> Self {
        AsmView { selected: None, show_runtime: false, jump: None, seen: None }
    }
}

// one row of the listing
struct Row<'a> {
    line: usize,
    text: &'a str,
    place: Option<Place<'a>>,
}

#[derive(Clone, Copy)]
struct Place<'a> {
    sec: &'a str,
    off: usize,
    len: usize,
    kind: LineKind,
}

fn word(bytes: &[u8], off: usize) -> Option<u32> {
    bytes.get(off..off + 4).map(|w| u32::from_le_bytes(w.try_into().unwrap()))
}

impl AsmView {
    pub fn on_rebuild(&mut self) {}

    pub fn show(&mut self, ui: &mut egui::Ui, b: &Snapshot, sm: &SourceMap, focus: &mut Option<Loc>) {
        let Some(full) = &b.full_asm else {
            widgets::empty(ui, "no assembly yet: an earlier stage failed");
            return;
        };
        let lines = toolchain::py::splitlines(full);
        let mut place: Vec<Option<Place>> = vec![None; lines.len()];
        for e in &b.listing {
            if e.line < place.len() && (e.len > 0 || place[e.line].is_none()) {
                place[e.line] = Some(Place { sec: &e.sec, off: e.off, len: e.len, kind: e.kind });
            }
        }
        // a newly followed line: select its first instruction and scroll there
        let mut target = None;
        if *focus != self.seen {
            if let Some(l) = focus.as_ref() {
                let mine = sm.asm_lines(l);
                target = mine.iter().copied().find(|&i| place[i].map_or(false, |p| p.kind == LineKind::Insn)).or(mine.first().copied());
                if target.map_or(false, |t| t < b.asm_line_offset) {
                    self.show_runtime = true;
                }
            }
        }
        let first = if self.show_runtime { 0 } else { b.asm_line_offset.min(lines.len()) };
        let rows: Vec<Row> = (first..lines.len()).map(|i| Row { line: i, text: lines[i], place: place[i] }).collect();
        if let Some(t) = target {
            self.selected = Some(t);
            // a few rows of context above
            self.jump = rows.iter().position(|r| r.line == t).map(|j| j.saturating_sub(6));
        }

        widgets::heading(ui, "codegen + assembler", theme::ASM);
        widgets::caption(
            ui,
            "every expression lands in r0, temporaries go through push/pop. pass 1 lays out the sections and \
             reserves pool space for ldr =; pass 2 encodes each line into a 32-bit word.",
        );
        ui.horizontal_wrapped(|ui| {
            if b.text.is_some() {
                widgets::pill(ui, &format!(".text {}", widgets::bytes(b.text_len())), theme::ASM);
                widgets::pill(ui, &format!(".data {}", widgets::bytes(b.data.as_ref().map_or(0, |d| d.len()))), theme::LINK);
                widgets::pill(ui, &format!("{} pool literals", b.pool_count), theme::token_color("intl"));
                widgets::pill(ui, &format!("{} relocations", b.reloc_count), theme::ELF);
            }
            if b.asm_line_offset > 0 {
                ui.checkbox(&mut self.show_runtime, "show the build.py runtime");
            }
            ui.separator();
            for (name, l) in &b.func_lines() {
                if ui.small_button(RichText::new(name).monospace()).clicked() {
                    let target = l + b.asm_line_offset;
                    self.jump = rows.iter().position(|r| r.line == target);
                    self.selected = Some(target + 2);
                }
            }
        });
        ui.add_space(4.0);

        let detail_w = 430.0;
        ui.horizontal_top(|ui| {
            let list_w = (ui.available_width() - detail_w - 12.0).max(300.0);
            let h = ui.available_height();
            widgets::column(ui, list_w, h, |ui| {
                widgets::card(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    self.listing(ui, b, &rows, sm, focus);
                });
            });
            widgets::column(ui, detail_w, h, |ui| {
                widgets::card(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    egui::ScrollArea::vertical().id_salt("asm-detail").show(ui, |ui| self.detail(ui, b, &lines, &place));
                });
            });
        });
        self.seen = focus.clone();
    }

    fn listing(&mut self, ui: &mut egui::Ui, b: &Snapshot, rows: &[Row], sm: &SourceMap, focus: &mut Option<Loc>) {
        let mut picked = None;
        let row_h = 17.0;
        let mut area = egui::ScrollArea::vertical().id_salt("asm-list").auto_shrink([false, false]);
        if let Some(j) = self.jump.take() {
            area = area.vertical_scroll_offset((j as f32) * (row_h + ui.spacing().item_spacing.y));
        }
        area.show_rows(ui, row_h, rows.len(), |ui, range| {
            for row in &rows[range] {
                let (r, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), row_h), Sense::click());
                let selected = self.selected == Some(row.line);
                if selected {
                    ui.painter().rect_filled(r, CornerRadius::same(3), theme::ASM.gamma_multiply(0.15));
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(3), theme::RAISED);
                }
                let from = sm.asm_loc(row.line);
                if from.is_some() && from == focus.as_ref() {
                    ui.painter().rect_filled(egui::Rect::from_min_size(r.min, egui::vec2(2.0, row_h)), 0.0, theme::ACCENT);
                    if !selected {
                        ui.painter().rect_filled(r, CornerRadius::same(3), theme::ACCENT.gamma_multiply(0.08));
                    }
                }
                if resp.clicked() {
                    self.selected = Some(row.line);
                    picked = from.cloned();
                }
                let p = ui.painter();
                let y = r.center().y;
                let font = theme::mono(12.0);
                if let Some(Place { sec, off, len, kind }) = row.place {
                    let addr = b.addr_of(sec, off);
                    if len > 0 || kind == LineKind::Label {
                        p.text(egui::pos2(r.left() + 4.0, y), Align2::LEFT_CENTER, format!("{:06x}", addr), font.clone(), theme::FAINT);
                    }
                    if len > 0 {
                        let bytes = b.section_bytes(sec);
                        // instructions and pools are words; data directives show raw bytes
                        let first = if matches!(kind, LineKind::Insn | LineKind::Pool) {
                            word(bytes, off).map_or(String::new(), widgets::hex32)
                        } else {
                            bytes.get(off..off + len.min(4)).unwrap_or(&[]).iter().map(|x| format!("{:02x}", x)).collect()
                        };
                        let hex = if len > 4 { format!("{} +{}", first, len - 4) } else { first };
                        let col = if sec == ".data" { theme::LINK } else { theme::ASM };
                        p.text(egui::pos2(r.left() + 60.0, y), Align2::LEFT_CENTER, hex, font.clone(), col.gamma_multiply(0.85));
                    }
                }
                let mut job = highlight::asm(row.text, 12.0);
                job.wrap.max_width = f32::INFINITY;
                let galley = ui.painter().layout_job(job);
                ui.painter().galley(egui::pos2(r.left() + 168.0, y - galley.size().y / 2.0), galley, theme::TEXT);
            }
        });
        if let Some(l) = picked {
            *focus = Some(l);
        }
    }

    fn detail(&self, ui: &mut egui::Ui, b: &Snapshot, lines: &[&str], place: &[Option<Place>]) {
        let Some(line) = self.selected.filter(|l| *l < lines.len()) else {
            ui.label(RichText::new("click a line in the listing").color(theme::DIM));
            ui.add_space(10.0);
            legend(ui);
            return;
        };
        ui.label(RichText::new(lines[line].trim()).monospace().size(14.0).color(theme::ASM));
        let Some(Place { sec, off, len, kind }) = place[line] else {
            widgets::caption(ui, "this line emits no bytes (comment, .global or a section switch).");
            return;
        };
        let addr = b.addr_of(sec, off);
        ui.label(RichText::new(format!("{} + 0x{:x}  =  address 0x{:08x}, {} bytes", sec, off, addr, len)).monospace().color(theme::DIM));
        if kind == LineKind::Label {
            widgets::caption(ui, "a label: pass 1 records its section and offset, the linker turns it into an address.");
            return;
        }
        let bytes = b.section_bytes(sec);
        if kind == LineKind::Insn {
            let Some(w) = word(bytes, off) else { return };
            ui.add_space(6.0);
            ui.label(RichText::new(format!("0x{:08x}   {}", w, disasm::disasm(addr, w))).monospace().size(13.5));
            ui.add_space(4.0);
            bit_diagram(ui, w);
            // relocation patched into this word?
            let sec_idx = (sec == ".data") as usize;
            if let Some(p) = b.link.as_ref().and_then(|l| l.patches.iter().find(|p| p.section == sec_idx && p.offset as usize == off)) {
                ui.add_space(8.0);
                widgets::heading(ui, "relocation", theme::LINK);
                ui.label(RichText::new(format!(
                    "{} -> {}  (0x{:08x})\nassembler left 0x{:08x}, linker wrote 0x{:08x}",
                    if p.rtype == R_ARM_ABS32 { "R_ARM_ABS32" } else { "R_ARM_CALL" },
                    p.symbol, p.target, p.before, p.after
                )).monospace());
                if p.rtype != R_ARM_ABS32 {
                    widgets::caption(ui, &format!("offset24 = (target - (P + 8)) >> 2 = (0x{:x} - 0x{:x}) >> 2", p.target, p.place + 8));
                }
            }
            // ldr rd, =x reads its literal pc-relative
            if disasm::classify(w) == disasm::Class::LoadStore && (w >> 16) & 0xF == 15 {
                let lit = addr.wrapping_add(8).wrapping_add(w & 0xFFF);
                let lit_off = lit.wrapping_sub(b.text_base()) as usize;
                ui.add_space(8.0);
                widgets::heading(ui, "literal pool", theme::token_color("intl"));
                let val = word(b.section_bytes(".text"), lit_off).unwrap_or(0);
                ui.label(RichText::new(format!("pc + 8 + {} = 0x{:08x}\nholds 0x{:08x} ({})", w & 0xFFF, lit, val, val as i32)).monospace());
                widgets::caption(ui, "mov only encodes 8-bit immediates, so bigger constants and addresses live in a pool after the function and get loaded relative to pc.");
            }
        } else {
            let n = len.min(64);
            let data = bytes.get(off..off + n).unwrap_or(&[]);
            let hex: Vec<String> = data.iter().map(|x| format!("{:02x}", x)).collect();
            let ascii: String = data.iter().map(|&c| if (32..127).contains(&c) { c as char } else { '.' }).collect();
            ui.label(RichText::new(hex.chunks(8).map(|c| c.join(" ")).collect::<Vec<_>>().join("\n")).monospace());
            ui.label(RichText::new(ascii).monospace().color(theme::DIM));
            if kind == LineKind::Pool {
                widgets::caption(ui, ".pool flushes the literals collected since the last pool into this spot.");
            }
        }
    }
}

// the 32 bits of an instruction, grouped into its encoding fields
pub fn bit_diagram(ui: &mut egui::Ui, w: u32) {
    let fields = disasm::fields(w);
    let width = ui.available_width().min(420.0);
    let cell = width / 32.0;
    let (r, _) = ui.allocate_exact_size(egui::vec2(width, 44.0), Sense::hover());
    let p = ui.painter();
    for (k, f) in fields.iter().enumerate() {
        let x0 = r.left() + (31 - f.hi) as f32 * cell;
        let x1 = r.left() + (32 - f.lo) as f32 * cell;
        let fr = Rect::from_x_y_ranges(x0..=x1, (r.top() + 12.0)..=(r.top() + 32.0));
        let col = field_color(k);
        p.rect(fr.shrink(0.5), CornerRadius::same(2), col.gamma_multiply(0.18), Stroke::new(1.0, col.gamma_multiply(0.8)), StrokeKind::Inside);
        for bit in f.lo..=f.hi {
            let x = r.left() + (31 - bit) as f32 * cell + cell / 2.0;
            let v = (w >> bit) & 1;
            p.text(egui::pos2(x, fr.center().y), Align2::CENTER_CENTER, v.to_string(), theme::mono(10.5), if v == 1 { theme::TEXT } else { theme::FAINT });
        }
        p.text(egui::pos2(x0 + 1.0, r.top() + 1.0), Align2::LEFT_TOP, f.hi.to_string(), theme::mono(8.5), theme::FAINT);
        if fr.width() > 18.0 {
            p.text(egui::pos2(fr.center().x, r.top() + 34.0), Align2::CENTER_TOP, f.name, egui::FontId::proportional(9.5), col);
        }
    }
    egui::Grid::new(("fields", w)).striped(true).num_columns(3).show(ui, |ui| {
        for (k, f) in fields.iter().enumerate() {
            ui.label(RichText::new(f.name).monospace().color(field_color(k)));
            ui.label(RichText::new(format!("[{}:{}] = {:#x}", f.hi, f.lo, f.value)).monospace().size(11.5).color(theme::DIM));
            ui.label(RichText::new(&f.meaning).size(12.0));
            ui.end_row();
        }
    });
}

fn field_color(k: usize) -> Color32 {
    const C: [Color32; 6] = [theme::CPU, theme::ASM, theme::TOKENS, theme::LINK, theme::AST, theme::ELF];
    C[k % C.len()]
}

fn legend(ui: &mut egui::Ui) {
    widgets::caption(ui, "columns: address, encoded word (after linking), source line. data lines show their first bytes.");
}
