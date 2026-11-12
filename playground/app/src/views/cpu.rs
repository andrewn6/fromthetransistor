// the arm7 model running the build: step by cycle or instruction, watch
// the fetch/exec/block state machine, registers, bus, stack and output

use crate::snapshot::{func_at, Func, Snapshot};
use crate::{theme, widgets};
use eframe::egui::{self, Align2, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, StrokeKind};
use std::collections::HashSet;
use toolchain::cpu::{Phase, FLAG_C, FLAG_N, FLAG_V, FLAG_Z};
use toolchain::disasm;
use toolchain::machine::{Machine, Status, UartMmio, RESULTS, SRAM_BASE, SRAM_SIZE};

const SPEEDS: [(u64, &str); 7] = [
    (1, "1 cycle / frame"),
    (10, "10 / frame"),
    (100, "100 / frame"),
    (1_000, "1k / frame"),
    (10_000, "10k / frame"),
    (100_000, "100k / frame"),
    (1_000_000, "1M / frame"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum MemView {
    Stack,
    Results,
    Data,
}

pub struct CpuView {
    pub machine: Option<Machine>,
    image: Option<(Vec<u8>, u32, u32)>,
    funcs: Vec<Func>,
    labels: Vec<(u32, String)>,
    pool_words: HashSet<u32>,
    text_range: (u32, u32),
    data_range: (u32, u32),
    running: bool,
    speed: usize,
    pub semihost: bool,
    pub uart: bool,
    autorun: bool,
    prev_regs: [u32; 16],
    follow: bool,
    uart_line: String,
    mem: MemView,
}

impl Default for CpuView {
    fn default() -> Self {
        CpuView {
            machine: None,
            image: None,
            funcs: vec![],
            labels: vec![],
            pool_words: HashSet::new(),
            text_range: (0, 0),
            data_range: (0, 0),
            running: false,
            speed: 4,
            semihost: true,
            uart: false,
            autorun: true,
            prev_regs: [0; 16],
            follow: true,
            uart_line: String::new(),
            mem: MemView::Stack,
        }
    }
}

impl CpuView {
    pub fn configure(&mut self, semihost: bool, uart: bool) {
        self.semihost = semihost;
        self.uart = uart;
    }

    pub fn load(&mut self, b: &Snapshot) {
        let (Some(flat), Some(ld)) = (&b.flat, &b.link) else {
            self.machine = None;
            self.image = None;
            self.running = false;
            return;
        };
        self.image = Some((flat.clone(), b.base, ld.entry));
        self.funcs = b.functions();
        let mut labels: Vec<(u32, String)> = ld.symbols.iter().filter(|s| s.section == 0).map(|s| (s.value, s.name.clone())).collect();
        labels.sort();
        self.labels = labels;
        self.pool_words.clear();
        for &(base, _, n) in &b.chunks {
            for k in 0..n {
                self.pool_words.insert(b.base + (base + 4 * k) as u32);
            }
        }
        let text = &ld.sections[0];
        self.text_range = (text.addr, text.addr + text.size());
        self.data_range = ld.sections.get(1).map_or((0, 0), |s| (s.addr, s.addr + s.size()));
        self.reset();
        self.running = self.autorun;
    }

    fn reset(&mut self) {
        let Some((img, base, entry)) = &self.image else { return };
        let mut m = Machine::new();
        m.semihosting = self.semihost;
        if self.uart {
            m.bus.devices.push(Box::new(UartMmio::new()));
        }
        if let Err(e) = m.load(img, *base, *entry) {
            m.status = Status::Fault(e);
        }
        self.prev_regs = m.cpu.regs;
        self.machine = Some(m);
        self.running = false;
    }

    pub fn metric(&self) -> String {
        match &self.machine {
            None => "-".into(),
            Some(m) => match &m.status {
                Status::Exited(_) => format!("exit, {} cyc", m.cycles),
                Status::Halted => format!("halt, {} cyc", m.cycles),
                Status::Fault(_) => "fault".into(),
                _ => format!("{} cycles", m.cycles),
            },
        }
    }

    pub fn tick(&mut self, ctx: &egui::Context) {
        if !self.running {
            return;
        }
        let Some(m) = &mut self.machine else { return };
        if !m.running() {
            self.running = false;
            return;
        }
        self.prev_regs = m.cpu.regs;
        m.run(SPEEDS[self.speed].0);
        if !m.running() || matches!(m.status, Status::Breakpoint(_)) {
            self.running = false;
        }
        ctx.request_repaint();
    }

    fn func_name(&self, addr: u32) -> &str {
        func_at(&self.funcs, addr).map_or("?", |i| self.funcs[i].name.as_str())
    }

    pub fn show(&mut self, ui: &mut egui::Ui, _b: &Snapshot) {
        if self.machine.is_none() {
            widgets::empty(ui, "nothing to run: the build did not produce an image");
            return;
        }
        self.controls(ui);
        ui.add_space(6.0);
        if ui.available_width() < 1050.0 {
            self.show_stacked(ui);
            return;
        }
        let avail = ui.available_height();
        let bottom_h = 190.0;
        let top_h = (avail - bottom_h - 10.0).max(300.0);
        ui.horizontal_top(|ui| {
            ui.set_height(top_h);
            // the side columns give up width first when the view is narrow
            let total = ui.available_width();
            let left_w = (total * 0.28).clamp(240.0, 300.0);
            let right_w = (total * 0.32).clamp(260.0, 380.0);
            let mid_w = (ui.available_width() - left_w - right_w - 20.0).max(260.0);
            panel(ui, left_w, top_h, |ui| {
                egui::ScrollArea::vertical().id_salt("cpu-left").show(ui, |ui| self.state_panel(ui));
            });
            panel(ui, mid_w, top_h, |ui| self.disassembly(ui));
            widgets::column(ui, right_w, top_h, |ui| {
                let h = (top_h - 10.0) / 2.0;
                panel(ui, right_w, h, |ui| self.console(ui));
                ui.add_space(6.0);
                panel(ui, right_w, h - 4.0, |ui| self.memory(ui));
            });
        });
        ui.add_space(8.0);
        ui.horizontal_top(|ui| {
            let hot_w = 300.0;
            let tl_w = ui.available_width() - hot_w - 10.0;
            panel(ui, tl_w, bottom_h, |ui| self.timeline(ui));
            panel(ui, hot_w, bottom_h, |ui| self.hot(ui));
        });
    }

    // narrow screens: two columns at most, the page scrolls instead of clipping
    fn show_stacked(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().id_salt("cpu-stacked").auto_shrink([false, false]).show(ui, |ui| {
            let gap = ui.spacing().item_spacing.x + 2.0;
            let full = ui.available_width();
            let half = (full - gap) / 2.0;
            if full >= 760.0 {
                ui.horizontal_top(|ui| {
                    panel(ui, 250.0, 460.0, |ui| {
                        egui::ScrollArea::vertical().id_salt("cpu-left").show(ui, |ui| self.state_panel(ui));
                    });
                    panel(ui, ui.available_width() - 2.0, 460.0, |ui| self.disassembly(ui));
                });
            } else {
                // too narrow for registers beside the listing: one above the other
                panel(ui, full, 460.0, |ui| {
                    egui::ScrollArea::vertical().id_salt("cpu-left").show(ui, |ui| self.state_panel(ui));
                });
                ui.add_space(8.0);
                panel(ui, full, 460.0, |ui| self.disassembly(ui));
            }
            ui.add_space(8.0);
            ui.horizontal_top(|ui| {
                panel(ui, half, 260.0, |ui| self.console(ui));
                panel(ui, half, 260.0, |ui| self.memory(ui));
            });
            ui.add_space(8.0);
            panel(ui, full, 190.0, |ui| self.timeline(ui));
            ui.add_space(8.0);
            panel(ui, full, 190.0, |ui| self.hot(ui));
        });
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            widgets::heading(ui, "arm7 cpu", theme::CPU);
            if ui.button("reset").on_hover_text("reload the image, pc = 0x10000").clicked() {
                self.reset();
            }
            let m = self.machine.as_mut().unwrap();
            let can = m.running();
            if ui.add_enabled(can, egui::Button::new("step cycle")).on_hover_text("one clock edge").clicked() {
                self.prev_regs = m.cpu.regs;
                m.cycle();
                self.running = false;
            }
            if ui.add_enabled(can, egui::Button::new("step instr")).on_hover_text("clock until the cpu is back in FETCH").clicked() {
                self.prev_regs = m.cpu.regs;
                m.step_instruction();
                self.running = false;
            }
            let run_label = if self.running { "pause" } else { "run" };
            if ui.add_enabled(can || self.running, egui::Button::new(RichText::new(run_label).strong().color(theme::CPU))).clicked() {
                self.running = !self.running;
            }
            egui::ComboBox::from_id_salt("speed").selected_text(SPEEDS[self.speed].1).show_ui(ui, |ui| {
                for (i, (_, l)) in SPEEDS.iter().enumerate() {
                    ui.selectable_value(&mut self.speed, i, *l);
                }
            });
            ui.separator();
            let mut changed = false;
            changed |= ui.checkbox(&mut self.semihost, "semihosting").on_hover_text("handle svc like qemu (putchar, putstr, exit). off = halt on the first svc, exactly like cpu_tb.cpp").changed();
            changed |= ui.checkbox(&mut self.uart, "uart mmio").on_hover_text("map the soc.v uart at 0x40001000").changed();
            ui.checkbox(&mut self.autorun, "run on build");
            if changed {
                self.reset();
            }
        });
        let m = self.machine.as_ref().unwrap();
        ui.horizontal_wrapped(|ui| {
            let (text, col) = match &m.status {
                Status::Running if self.running => ("running".to_string(), theme::OK),
                Status::Running => ("paused".to_string(), theme::WARN),
                Status::Halted => ("halted on svc (cpu_tb.cpp behavior)".to_string(), theme::WARN),
                Status::Exited(c) => (format!("exited via SYS_EXIT (0x{:x})", c), theme::OK),
                Status::Fault(e) => (e.clone(), theme::ERR),
                Status::Breakpoint(a) => (format!("breakpoint at 0x{:x}", a), theme::WARN),
            };
            widgets::pill(ui, &text, col);
            let cpi = if m.retired > 0 { m.cycles as f32 / m.retired as f32 } else { 0.0 };
            // separate labels so the line wraps on narrow screens
            for stat in [
                format!("{} cycles", m.cycles),
                format!("{} instructions", m.retired),
                format!("{:.2} cycles/instr", cpi),
                format!("fetch {}  exec {}  block {}", m.phase_cycles[0], m.phase_cycles[1], m.phase_cycles[2]),
            ] {
                ui.label(RichText::new(stat).monospace().color(theme::DIM));
            }
        });
    }

    fn state_panel(&mut self, ui: &mut egui::Ui) {
        let m = self.machine.as_ref().unwrap();
        ui.label(RichText::new("state machine").color(theme::DIM));
        phase_diagram(ui, m);
        if let Some(req) = m.last_bus {
            let kind = if req.we { "write" } else { "read" };
            let size = if req.byte { "byte" } else { "word" };
            let data = if req.we { format!(" = 0x{:08x}", req.data_out) } else { String::new() };
            ui.label(RichText::new(format!("last bus: {} {} 0x{:08x}{}", kind, size, req.addr, data)).monospace().size(11.5).color(if req.we { theme::CPU } else { theme::DIM }));
        }
        ui.add_space(8.0);
        ui.label(RichText::new("registers").color(theme::DIM));
        egui::Grid::new("regs").num_columns(4).spacing(egui::vec2(10.0, 3.0)).show(ui, |ui| {
            for row in 0..8 {
                for col in 0..2 {
                    let i = row + col * 8;
                    let v = if i == 15 { m.cpu.pc } else { m.cpu.regs[i] };
                    let changed = i != 15 && v != self.prev_regs[i];
                    ui.label(RichText::new(disasm::reg_name(i as u32)).monospace().color(theme::DIM));
                    let rt = RichText::new(format!("{:08x}", v)).monospace();
                    let resp = ui.label(if changed { rt.color(theme::CPU).strong() } else { rt });
                    resp.on_hover_text(format!("{} = {} (signed {})", disasm::reg_name(i as u32), v, v as i32));
                }
                ui.end_row();
            }
        });
        widgets::caption(ui, "pc shows the fetch address register; during EXEC it is already instruction + 4.");
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            for (bit, name) in [(FLAG_N, "N"), (FLAG_Z, "Z"), (FLAG_C, "C"), (FLAG_V, "V")] {
                let on = m.cpu.flags & bit != 0;
                let (r, _) = ui.allocate_exact_size(egui::vec2(30.0, 22.0), Sense::hover());
                ui.painter().rect(r, CornerRadius::same(4), if on { theme::OK.gamma_multiply(0.3) } else { theme::BG }, Stroke::new(1.0, if on { theme::OK } else { theme::BORDER }), StrokeKind::Inside);
                ui.painter().text(r.center(), Align2::CENTER_CENTER, name, theme::mono(12.0), if on { theme::TEXT } else { theme::FAINT });
            }
        });
        ui.add_space(8.0);
        ui.label(RichText::new("call stack").color(theme::DIM));
        if m.call_stack.is_empty() {
            widgets::caption(ui, "(empty)");
        }
        for f in m.call_stack.iter().rev().take(16) {
            let name = self.func_name(f.func).to_string();
            ui.label(RichText::new(format!("{}  ret 0x{:x}", name, f.ret)).monospace().size(12.0).color(theme::hashed(&name)));
        }
    }

    fn disassembly(&mut self, ui: &mut egui::Ui) {
        let (start, end) = self.text_range;
        let n = ((end - start) / 4) as usize;
        let m = self.machine.as_mut().unwrap();
        let pc_exec = if m.cpu.phase == Phase::Fetch { m.cpu.pc } else { m.cpu.pc.wrapping_sub(4) };
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("memory as instructions (.text)").color(theme::DIM));
            ui.checkbox(&mut self.follow, "follow pc");
            widgets::caption(ui, "click the gutter to set a breakpoint");
        });
        let row_h = 17.0;
        let mut area = egui::ScrollArea::vertical().id_salt("disasm").auto_shrink([false, false]);
        let cur_row = pc_exec.wrapping_sub(start) / 4;
        if self.follow && (cur_row as usize) < n {
            let y = cur_row as f32 * (row_h + ui.spacing().item_spacing.y);
            area = area.vertical_scroll_offset((y - 140.0).max(0.0));
        }
        let max_heat = m.heat.iter().copied().max().unwrap_or(1).max(1) as f32;
        let mut toggle = None;
        let labels = &self.labels;
        let pool = &self.pool_words;
        let funcs = &self.funcs;
        area.show_rows(ui, row_h, n, |ui, range| {
            for k in range {
                let a = start + 4 * k as u32;
                let w = m.bus.peek32(a).unwrap_or(0);
                let (r, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), row_h), Sense::click());
                let p = ui.painter();
                let is_pc = a == pc_exec;
                if is_pc {
                    p.rect_filled(r, CornerRadius::same(3), theme::CPU.gamma_multiply(0.2));
                } else if resp.hovered() {
                    p.rect_filled(r, CornerRadius::same(3), theme::RAISED);
                }
                let y = r.center().y;
                let bp = m.breakpoints.contains(&a);
                let gutter = Rect::from_min_size(r.min, egui::vec2(16.0, row_h));
                if bp {
                    p.circle_filled(gutter.center(), 4.5, theme::ERR);
                } else if resp.hovered() && ui.rect_contains_pointer(gutter) {
                    p.circle_stroke(gutter.center(), 4.5, Stroke::new(1.0, theme::ERR.gamma_multiply(0.6)));
                }
                if resp.clicked() && resp.interact_pointer_pos().map_or(false, |pp| gutter.contains(pp)) {
                    toggle = Some(a);
                }
                // retire count as a heat bar
                let h = m.heat.get(((a - SRAM_BASE) / 4) as usize).copied().unwrap_or(0);
                if h > 0 {
                    let bw = 34.0 * ((h as f32).ln_1p() / max_heat.ln_1p());
                    p.rect_filled(Rect::from_min_size(egui::pos2(r.left() + 18.0, y - 3.0), egui::vec2(bw.max(1.0), 6.0)), CornerRadius::same(2), theme::WARN.gamma_multiply(0.6));
                }
                let font = theme::mono(12.0);
                p.text(egui::pos2(r.left() + 58.0, y), Align2::LEFT_CENTER, format!("{:06x}", a), font.clone(), theme::FAINT);
                p.text(egui::pos2(r.left() + 112.0, y), Align2::LEFT_CENTER, widgets::hex32(w), font.clone(), theme::DIM);
                let label = labels.binary_search_by_key(&a, |(x, _)| *x).ok().map(|i| labels[i].1.as_str());
                if let Some(l) = label {
                    let col = if l.starts_with(".L") { theme::FAINT } else { theme::hashed(l) };
                    p.text(egui::pos2(r.left() + 184.0, y), Align2::LEFT_CENTER, format!("{}:", l), font.clone(), col);
                }
                let (text, col) = if pool.contains(&a) {
                    (format!(".word 0x{:x}", w), theme::token_color("intl"))
                } else {
                    let mut t = disasm::disasm(a, w);
                    if disasm::classify(w) == disasm::Class::Branch {
                        let target = disasm::branch_target(a, w);
                        let name = labels.binary_search_by_key(&target, |(x, _)| *x).ok().map(|i| labels[i].1.clone())
                            .or_else(|| func_at(funcs, target).map(|i| funcs[i].name.clone()));
                        if let Some(nm) = name {
                            t = format!("{}  <{}>", t, nm);
                        }
                    }
                    (t, if is_pc { theme::TEXT } else { theme::ASM })
                };
                p.text(egui::pos2(r.left() + 300.0, y), Align2::LEFT_CENTER, text, font, col);
            }
        });
        if let Some(a) = toggle {
            if !m.breakpoints.remove(&a) {
                m.breakpoints.insert(a);
            }
        }
    }

    fn console(&mut self, ui: &mut egui::Ui) {
        let m = self.machine.as_mut().unwrap();
        ui.horizontal(|ui| {
            ui.label(RichText::new("console").color(theme::DIM));
            widgets::caption(ui, if self.semihost { "semihosting svc + uart" } else { "uart only" });
        });
        if self.uart {
            ui.horizontal(|ui| {
                let resp = ui.add(egui::TextEdit::singleline(&mut self.uart_line).hint_text("type, enter sends to uart rx").desired_width(ui.available_width() - 60.0));
                let send = ui.button("send").clicked() || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if send {
                    for b in self.uart_line.bytes() {
                        m.type_input(b);
                    }
                    self.uart_line.clear();
                    resp.request_focus();
                }
            });
        }
        egui::ScrollArea::vertical().id_salt("console").stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
            if m.console.is_empty() {
                widgets::caption(ui, "(no output yet)");
            }
            ui.label(RichText::new(&m.console).monospace().color(theme::OK));
        });
    }

    fn memory(&mut self, ui: &mut egui::Ui) {
        let m = self.machine.as_mut().unwrap();
        ui.horizontal(|ui| {
            ui.label(RichText::new("memory").color(theme::DIM));
            ui.selectable_value(&mut self.mem, MemView::Stack, "stack");
            ui.selectable_value(&mut self.mem, MemView::Data, ".data");
            ui.selectable_value(&mut self.mem, MemView::Results, "results");
        });
        let sp = m.cpu.regs[13];
        let fp = m.cpu.regs[11];
        let base = match self.mem {
            // after a pop-everything exit sp sits one past the stack page
            MemView::Stack => (sp & !3).min(toolchain::machine::STK_BASE + toolchain::machine::STK_SIZE - 4 * 32),
            MemView::Results => RESULTS,
            MemView::Data => self.data_range.0 & !3,
        };
        egui::ScrollArea::vertical().id_salt("mem").auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("memgrid").num_columns(3).spacing(egui::vec2(10.0, 2.0)).show(ui, |ui| {
                for k in 0..32u32 {
                    let a = base.wrapping_add(4 * k);
                    let Some(w) = m.bus.peek32(a) else {
                        ui.label(RichText::new(format!("{:08x}", a)).monospace().color(theme::FAINT));
                        ui.label(RichText::new("unmapped").color(theme::FAINT));
                        ui.end_row();
                        break;
                    };
                    let mut tag = String::new();
                    if a == sp {
                        tag.push_str("<- sp ");
                    }
                    if a == fp {
                        tag.push_str("<- fp");
                    }
                    let ascii: String = w.to_le_bytes().iter().map(|&c| if (32..127).contains(&c) { c as char } else { '.' }).collect();
                    ui.label(RichText::new(format!("{:08x}", a)).monospace().color(theme::FAINT));
                    ui.label(RichText::new(format!("{:08x}  {}", w, ascii)).monospace());
                    ui.label(RichText::new(tag).monospace().size(11.5).color(theme::CPU));
                    ui.end_row();
                }
            });
            if self.mem == MemView::Results {
                widgets::caption(ui, "cputest.c stores its 18 results here; cpu_tb.cpp checks them after the halt.");
            }
        });
    }

    fn timeline(&mut self, ui: &mut egui::Ui) {
        let m = self.machine.as_ref().unwrap();
        ui.horizontal(|ui| {
            ui.label(RichText::new("pc over time").color(theme::DIM));
            widgets::caption(ui, &format!("last {} retired instructions; each function is a band, loops show up as stripes", m.trace.len()));
        });
        let (r, resp) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
        let p = ui.painter_at(r);
        p.rect_filled(r, CornerRadius::same(4), theme::BG);
        let (lo, hi) = self.text_range;
        if m.trace.is_empty() || hi <= lo {
            p.text(r.center(), Align2::CENTER_CENTER, "run the program to fill this in", egui::FontId::proportional(12.0), theme::FAINT);
            return;
        }
        let label_w = 90.0;
        let plot = Rect::from_min_max(egui::pos2(r.left() + label_w, r.top() + 4.0), egui::pos2(r.right() - 4.0, r.bottom() - 4.0));
        let y_of = |a: u32| plot.bottom() - (a.saturating_sub(lo)) as f32 / (hi - lo) as f32 * plot.height();
        for f in &self.funcs {
            let (y0, y1) = (y_of(f.end), y_of(f.start));
            let col = theme::hashed(&f.name);
            p.rect_filled(Rect::from_x_y_ranges(plot.x_range(), y0..=y1), 0.0, col.gamma_multiply(0.05));
            if y1 - y0 > 9.0 {
                p.text(egui::pos2(r.left() + 4.0, (y0 + y1) / 2.0), Align2::LEFT_CENTER, &f.name, theme::mono(10.0), col);
            }
        }
        let cols = plot.width().max(1.0) as usize;
        let n = m.trace.len();
        for c in 0..cols {
            let a = c * n / cols;
            let b = ((c + 1) * n / cols).max(a + 1).min(n);
            let x = plot.left() + c as f32;
            for e in m.trace.range(a..b).step_by(((b - a) / 6).max(1)) {
                let col = func_at(&self.funcs, e.addr).map_or(theme::DIM, |i| theme::hashed(&self.funcs[i].name));
                p.rect_filled(Rect::from_center_size(Pos2::new(x, y_of(e.addr)), egui::vec2(1.5, 2.0)), 0.0, if e.executed { col } else { col.gamma_multiply(0.4) });
            }
        }
        if let Some(h) = resp.hover_pos().filter(|h| plot.contains(*h)) {
            let i = (((h.x - plot.left()) / plot.width()) * n as f32) as usize;
            if let Some(e) = m.trace.get(i.min(n - 1)) {
                p.line_segment([Pos2::new(h.x, plot.top()), Pos2::new(h.x, plot.bottom())], Stroke::new(1.0, theme::FAINT));
                let name = self.func_name(e.addr).to_string();
                resp.on_hover_text(format!("cycle {}: 0x{:x} {}  {}", e.cycle, e.addr, disasm::disasm(e.addr, e.instr), name));
            }
        }
    }

    fn hot(&mut self, ui: &mut egui::Ui) {
        let m = self.machine.as_ref().unwrap();
        ui.label(RichText::new("where the instructions went").color(theme::DIM));
        let mut per: Vec<(String, u64)> = self
            .funcs
            .iter()
            .map(|f| {
                let s = ((f.start - SRAM_BASE) / 4) as usize;
                let e = ((f.end.min(SRAM_BASE + SRAM_SIZE) - SRAM_BASE) / 4) as usize;
                (f.name.clone(), m.heat[s..e.max(s)].iter().map(|&x| x as u64).sum())
            })
            .filter(|(_, n)| *n > 0)
            .collect();
        per.sort_by(|a, b| b.1.cmp(&a.1));
        let total: u64 = per.iter().map(|x| x.1).sum::<u64>().max(1);
        for (name, n) in per.iter().take(7) {
            let (r, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 18.0), Sense::hover());
            let frac = *n as f32 / total as f32;
            let col = theme::hashed(name);
            let bar = Rect::from_min_size(egui::pos2(r.left() + 96.0, r.top() + 3.0), egui::vec2((r.width() - 150.0) * frac, 12.0));
            ui.painter().rect_filled(bar, CornerRadius::same(2), col.gamma_multiply(0.7));
            ui.painter().text(egui::pos2(r.left(), r.center().y), Align2::LEFT_CENTER, name, theme::mono(11.0), col);
            ui.painter().text(egui::pos2(r.right(), r.center().y), Align2::RIGHT_CENTER, format!("{:.0}%", frac * 100.0), theme::mono(11.0), theme::DIM);
        }
        if per.is_empty() {
            widgets::caption(ui, "nothing retired yet");
        }
    }
}

// a card of a fixed outer size
fn panel(ui: &mut egui::Ui, w: f32, h: f32, add: impl FnOnce(&mut egui::Ui)) {
    widgets::column(ui, w, h, |ui| {
        widgets::card(ui, |ui| {
            ui.set_min_size(egui::vec2(w - 22.0, h - 22.0));
            ui.set_max_height(h - 22.0);
            add(ui);
        });
    });
}

// FETCH -> EXEC -> (BLOCK ->) FETCH, current phase lit
fn phase_diagram(ui: &mut egui::Ui, m: &Machine) {
    let w = ui.available_width().min(270.0);
    let (r, _) = ui.allocate_exact_size(egui::vec2(w, 96.0), Sense::hover());
    let p = ui.painter();
    let centers = [
        Pos2::new(r.left() + 40.0, r.top() + 30.0),
        Pos2::new(r.right() - 40.0, r.top() + 30.0),
        Pos2::new(r.center().x, r.bottom() - 16.0),
    ];
    let phases = [Phase::Fetch, Phase::Exec, Phase::Block];
    let notes = ["mem[pc] -> instr", "decode, alu, ld/st", "1 reg / cycle"];
    let edge = |a: Pos2, b: Pos2, bend: f32| {
        let mid = Pos2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0 + bend);
        p.add(egui::epaint::QuadraticBezierShape::from_points_stroke([a, mid, b], false, Color32::TRANSPARENT, Stroke::new(1.0, theme::FAINT)));
        p.circle_filled(b, 2.5, theme::FAINT);
    };
    edge(centers[0] + egui::vec2(34.0, -6.0), centers[1] + egui::vec2(-34.0, -6.0), -14.0);
    edge(centers[1] + egui::vec2(-34.0, 6.0), centers[0] + egui::vec2(34.0, 6.0), 14.0);
    edge(centers[1] + egui::vec2(0.0, 14.0), centers[2] + egui::vec2(30.0, -6.0), 0.0);
    edge(centers[2] + egui::vec2(-30.0, -6.0), centers[0] + egui::vec2(0.0, 14.0), 0.0);
    for (i, c) in centers.iter().enumerate() {
        let on = m.cpu.phase == phases[i] && !m.cpu.halted;
        let rr = Rect::from_center_size(*c, egui::vec2(68.0, 26.0));
        p.rect(rr, CornerRadius::same(13), if on { theme::CPU.gamma_multiply(0.35) } else { theme::BG }, Stroke::new(1.0, if on { theme::CPU } else { theme::BORDER }), StrokeKind::Inside);
        p.text(*c, Align2::CENTER_CENTER, phases[i].name(), theme::mono(11.5), if on { theme::TEXT } else { theme::DIM });
        let note_pos = if i == 2 { *c + egui::vec2(40.0, 0.0) } else { *c + egui::vec2(0.0, 19.0) };
        let anchor = if i == 2 { Align2::LEFT_CENTER } else { Align2::CENTER_TOP };
        p.text(note_pos, anchor, notes[i], egui::FontId::proportional(9.5), theme::FAINT);
    }
}
