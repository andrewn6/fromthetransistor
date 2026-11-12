// the output file byte by byte: which region every byte belongs to, the
// decoded headers, and a colored hexdump

use crate::{theme, widgets};
use eframe::egui::{self, Align2, Color32, CornerRadius, Rect, RichText, Sense, Stroke, StrokeKind};
use toolchain::linker::Region;
use toolchain::pipeline::Build;

#[derive(Default)]
pub struct ElfView {
    hovered: Option<usize>,
}

fn region_color(name: &str) -> Color32 {
    match name {
        "elf header" => theme::CPU,
        "program header" => theme::AST,
        ".text" => theme::ASM,
        ".data" => theme::LINK,
        ".shstrtab" | ".strtab" => theme::TOKENS,
        ".symtab" => theme::ELF,
        _ => theme::SOURCE,
    }
}

fn u16_at(b: &[u8], o: usize) -> u32 {
    u16::from_le_bytes([b[o], b[o + 1]]) as u32
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

fn cstr(b: &[u8], o: usize) -> String {
    b.get(o..).map(|s| s.iter().take_while(|&&c| c != 0).map(|&c| c as char).collect()).unwrap_or_default()
}

impl ElfView {
    pub fn show(&mut self, ui: &mut egui::Ui, b: &Build) {
        let Some(elf) = &b.elf else {
            widgets::empty(ui, "no elf yet: an earlier stage failed");
            return;
        };
        widgets::heading(ui, "elf32 executable", theme::ELF);
        widgets::caption(ui, "the exact bytes linker.py writes. qemu-system-arm -M versatilepb -nographic -semihosting -kernel prog.elf runs it.");
        ui.add_space(4.0);
        let regions = &b.regions;
        widgets::card(ui, |ui| {
            ui.set_width(ui.available_width());
            self.byte_map(ui, elf.len(), regions);
        });
        ui.add_space(8.0);
        ui.columns(2, |cols| {
            widgets::card(&mut cols[0], |ui| {
                ui.set_min_height(ui.available_height());
                egui::ScrollArea::vertical().id_salt("elf-hdr").auto_shrink([false, false]).show(ui, |ui| headers(ui, elf));
            });
            widgets::card(&mut cols[1], |ui| {
                ui.set_min_height(ui.available_height());
                self.hexdump(ui, elf, regions);
            });
        });
    }

    fn byte_map(&mut self, ui: &mut egui::Ui, total: usize, regions: &[Region]) {
        let w = ui.available_width();
        let (r, resp) = ui.allocate_exact_size(egui::vec2(w, 34.0), Sense::hover());
        let p = ui.painter();
        let mut hovered = None;
        for (i, reg) in regions.iter().enumerate() {
            let x0 = r.left() + reg.offset as f32 / total as f32 * w;
            let x1 = r.left() + (reg.offset + reg.size) as f32 / total as f32 * w;
            let rr = Rect::from_x_y_ranges(x0..=x1.max(x0 + 1.0), r.y_range());
            let col = region_color(&reg.name);
            let on = self.hovered == Some(i);
            p.rect(rr, CornerRadius::same(2), col.gamma_multiply(if on { 0.5 } else { 0.22 }), Stroke::new(1.0, col.gamma_multiply(0.8)), StrokeKind::Inside);
            if rr.width() > 50.0 {
                p.text(rr.center(), Align2::CENTER_CENTER, &reg.name, egui::FontId::proportional(11.0), theme::TEXT);
            }
            if resp.hover_pos().map_or(false, |h| rr.contains(h)) {
                hovered = Some(i);
            }
        }
        ui.horizontal_wrapped(|ui| {
            for (i, reg) in regions.iter().enumerate() {
                let resp = widgets::pill(ui, &format!("{} @{:#x} {}", reg.name, reg.offset, widgets::bytes(reg.size as usize)), region_color(&reg.name));
                if resp.hovered() {
                    hovered = Some(i);
                }
            }
        });
        if hovered.is_some() || !ui.ui_contains_pointer() {
            self.hovered = hovered;
        }
    }

    fn hexdump(&mut self, ui: &mut egui::Ui, elf: &[u8], regions: &[Region]) {
        let region_of = |o: usize| regions.iter().position(|r| (r.offset as usize..(r.offset + r.size) as usize).contains(&o));
        let row_h = 16.0;
        let rows = elf.len().div_ceil(16);
        let mut hovered = None;
        egui::ScrollArea::vertical().id_salt("hex").auto_shrink([false, false]).show_rows(ui, row_h, rows, |ui, range| {
            for row in range {
                let (r, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), row_h), Sense::hover());
                let p = ui.painter();
                let font = theme::mono(11.5);
                p.text(egui::pos2(r.left(), r.center().y), Align2::LEFT_CENTER, format!("{:05x}", row * 16), font.clone(), theme::FAINT);
                for k in 0..16 {
                    let o = row * 16 + k;
                    let Some(&byte) = elf.get(o) else { break };
                    let reg = region_of(o);
                    let col = reg.map_or(theme::DIM, |i| region_color(&regions[i].name));
                    let x = r.left() + 52.0 + k as f32 * 22.0 + if k >= 8 { 6.0 } else { 0.0 };
                    let cell = Rect::from_min_size(egui::pos2(x - 2.0, r.top()), egui::vec2(21.0, row_h));
                    if reg.is_some() && reg == self.hovered {
                        p.rect_filled(cell, CornerRadius::same(2), col.gamma_multiply(0.25));
                    }
                    p.text(egui::pos2(x, r.center().y), Align2::LEFT_CENTER, format!("{:02x}", byte), font.clone(), if byte == 0 { col.gamma_multiply(0.35) } else { col });
                    let ax = r.left() + 52.0 + 16.0 * 22.0 + 18.0 + k as f32 * 8.0;
                    let ch = if (32..127).contains(&byte) { byte as char } else { '.' };
                    p.text(egui::pos2(ax, r.center().y), Align2::LEFT_CENTER, ch.to_string(), font.clone(), col.gamma_multiply(0.7));
                    if ui.rect_contains_pointer(cell) {
                        hovered = reg;
                    }
                }
            }
        });
        if hovered.is_some() {
            self.hovered = hovered;
        }
    }
}

fn kv(ui: &mut egui::Ui, k: &str, v: String, note: &str) {
    ui.label(RichText::new(k).monospace().color(theme::DIM));
    ui.label(RichText::new(v).monospace());
    ui.label(RichText::new(note).size(11.5).color(theme::FAINT));
    ui.end_row();
}

fn headers(ui: &mut egui::Ui, b: &[u8]) {
    if b.len() < 84 {
        return;
    }
    widgets::heading(ui, "elf header", theme::CPU);
    egui::Grid::new("ehdr").num_columns(3).striped(true).show(ui, |ui| {
        kv(ui, "e_ident", format!("{:02x?}", &b[..8]), "\\x7fELF, 32-bit, little endian, version 1");
        kv(ui, "e_type", u16_at(b, 16).to_string(), "2 = ET_EXEC");
        kv(ui, "e_machine", u16_at(b, 18).to_string(), "40 = EM_ARM");
        kv(ui, "e_entry", format!("0x{:x}", u32_at(b, 24)), "_start");
        kv(ui, "e_phoff", u32_at(b, 28).to_string(), "program headers start");
        kv(ui, "e_shoff", u32_at(b, 32).to_string(), "section headers start");
        kv(ui, "e_flags", format!("0x{:08x}", u32_at(b, 36)), "EABI version 5");
        kv(ui, "e_phnum", u16_at(b, 44).to_string(), "one PT_LOAD");
        kv(ui, "e_shnum", u16_at(b, 48).to_string(), "null + sections + 3 tables");
        kv(ui, "e_shstrndx", u16_at(b, 50).to_string(), "which section holds names");
    });
    ui.add_space(8.0);
    widgets::heading(ui, "program header", theme::AST);
    egui::Grid::new("phdr").num_columns(3).striped(true).show(ui, |ui| {
        let o = 52;
        kv(ui, "p_type", u32_at(b, o).to_string(), "1 = PT_LOAD");
        kv(ui, "p_offset", u32_at(b, o + 4).to_string(), "load from the file start");
        kv(ui, "p_vaddr", format!("0x{:x}", u32_at(b, o + 8)), "headers land at 0x10000 too");
        kv(ui, "p_filesz", u32_at(b, o + 16).to_string(), "headers + all sections");
        kv(ui, "p_flags", u32_at(b, o + 24).to_string(), "5 = R + X");
    });
    ui.add_space(8.0);
    widgets::heading(ui, "section headers", theme::SOURCE);
    let shoff = u32_at(b, 32) as usize;
    let shnum = u16_at(b, 48) as usize;
    let shstr = u16_at(b, 50) as usize;
    let sh = |i: usize, f: usize| -> u32 { b.get(shoff + i * 40 + f * 4..shoff + i * 40 + f * 4 + 4).map_or(0, |w| u32::from_le_bytes(w.try_into().unwrap())) };
    let names_off = sh(shstr, 4) as usize;
    egui::Grid::new("shdr").num_columns(5).striped(true).show(ui, |ui| {
        for h in ["name", "type", "addr", "offset", "size"] {
            ui.label(RichText::new(h).size(11.5).color(theme::FAINT));
        }
        ui.end_row();
        for i in 0..shnum {
            let name = cstr(b, names_off + sh(i, 0) as usize);
            let t = match sh(i, 1) {
                0 => "NULL",
                1 => "PROGBITS",
                2 => "SYMTAB",
                3 => "STRTAB",
                _ => "?",
            };
            ui.label(RichText::new(if name.is_empty() { "(null)".into() } else { name.clone() }).monospace().color(region_color(&name)));
            ui.label(RichText::new(t).monospace().size(11.5));
            ui.label(RichText::new(format!("0x{:x}", sh(i, 3))).monospace());
            ui.label(RichText::new(sh(i, 4).to_string()).monospace());
            ui.label(RichText::new(sh(i, 5).to_string()).monospace());
            ui.end_row();
        }
    });
}
