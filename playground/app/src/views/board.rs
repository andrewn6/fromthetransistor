// the simulated fpga board: power it on, watch the boot rom come up on the
// serial terminal, send the program from the editor over the uart, and
// follow the boot sequence the os chapters will extend.

use crate::python::{self, Roms};
use crate::snapshot::Snapshot;
use crate::{theme, widgets};
use eframe::egui::{self, pos2, vec2, Align2, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, StrokeKind};
use toolchain::board::{Board, Framing, Rom, Stage, BAUDS, CLOCK_HZ, LED_BASE, ROM_BASE, UART_BASE};
use toolchain::future::{EthernetStub, SdCardStub};
use toolchain::machine::SRAM_BASE;

const SPEEDS: [(u64, &str); 4] = [(20_000, "20k cycles / frame"), (100_000, "100k cycles / frame"), (500_000, "500k cycles / frame"), (2_000_000, "2M cycles / frame")];
// serial traffic within this many cycles lights the activity leds
const BLINK: u64 = 250_000;
const TERM_MAX: usize = 20_000;

pub struct BoardView {
    pub board: Board,
    roms: Roms,
    // python version the roms were built with
    rom_version: u32,
    speed: usize,
    line: String,
    // bytes on the wire for the last download, and what was sent
    sent: usize,
    sent_name: String,
    // measured simulation speed
    rate: f64,
    rate_at: (f64, u64),
}

impl Default for BoardView {
    fn default() -> Self {
        BoardView {
            board: Board::default(),
            roms: Vec::new(),
            rom_version: 0,
            speed: 1,
            line: String::new(),
            sent: 0,
            sent_name: String::new(),
            rate: 0.0,
            rate_at: (0.0, 0),
        }
    }
}

fn cycles(b: &Board) -> u64 {
    b.machine.as_ref().map_or(0, |m| m.cycles)
}

pub fn stage_text(s: &Stage) -> String {
    match s {
        Stage::Off => "power off".into(),
        Stage::WaitingForImage => "rom waiting for an image".into(),
        Stage::Receiving => "receiving over uart".into(),
        Stage::Running => "program running".into(),
        Stage::Halted => "halted on svc".into(),
        Stage::Exited(c) => format!("exited ({:#x})", c),
        Stage::Fault(_) => "fault".into(),
    }
}

impl BoardView {
    // rebuild the boot roms whenever the .py files (re)load. the board picks
    // the first one that builds; a running board keeps its rom until the next
    // power cycle, like flashing a new bitstream.
    pub fn refresh_roms(&mut self, py: &python::State) {
        if !py.ready || py.version == self.rom_version {
            return;
        }
        self.rom_version = py.version;
        self.roms = python::roms(ROM_BASE);
        self.board.rom = self.roms.iter().find_map(|(name, r)| {
            Some(Rom {
                name: name.clone(),
                image: r.flat.clone()?,
                base: r.base,
                entry: r.entry,
                framing: if name.ends_with("bootrom.asm") { Framing::Length3 } else { Framing::Header },
            })
        });
    }

    pub fn tick(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let Some(m) = &self.board.machine else { return };
        if !m.running() {
            return;
        }
        self.board.run(SPEEDS[self.speed].0);
        let c = cycles(&self.board);
        if now - self.rate_at.0 >= 0.5 {
            self.rate = (c - self.rate_at.1.min(c)) as f64 / (now - self.rate_at.0);
            self.rate_at = (now, c);
        }
        ctx.request_repaint();
    }

    pub fn metric(&self) -> String {
        stage_text(&self.board.stage())
    }

    fn power(&mut self) {
        if self.board.powered() {
            self.board.power_off();
        } else {
            let _ = self.board.power_on();
            self.sent = 0;
            self.rate = 0.0;
            self.rate_at = (0.0, 0);
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, b: &Snapshot, program: &str) {
        self.toolbar(ui);
        ui.add_space(10.0);
        let h = 280.0f32.min(ui.available_height() * 0.45);
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
        self.drawing(ui, rect);
        ui.add_space(10.0);
        let avail = ui.available_height();
        ui.horizontal_top(|ui| {
            let side = (ui.available_width() * 0.38).clamp(320.0, 460.0);
            let term_w = ui.available_width() - side - ui.spacing().item_spacing.x - 2.0;
            widgets::column(ui, term_w, avail, |ui| {
                widgets::card(ui, |ui| {
                    ui.set_min_size(vec2(term_w - 22.0, avail - 22.0));
                    self.terminal(ui, b, program);
                });
            });
            widgets::column(ui, side, avail, |ui| {
                widgets::card(ui, |ui| {
                    ui.set_min_size(vec2(side - 22.0, avail - 22.0));
                    egui::ScrollArea::vertical().id_salt("board-side").auto_shrink([false, false]).show(ui, |ui| {
                        self.sequence(ui);
                        ui.add_space(12.0);
                        self.devices(ui);
                    });
                });
            });
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            widgets::heading(ui, "fpga board", theme::ACCENT);
            ui.add_space(8.0);
            let on = self.board.powered();
            let label = if on { "power off" } else { "power on" };
            if ui.add_enabled(on || self.board.rom.is_some(), egui::Button::new(label)).on_disabled_hover_text("no boot rom built yet").clicked() {
                self.power();
            }
            if ui.add_enabled(on, egui::Button::new("reset")).on_hover_text("restart the cpu at the reset vector; memory keeps its contents").clicked() {
                self.board.reset();
            }
            ui.separator();
            ui.add_enabled_ui(!on, |ui| {
                egui::ComboBox::from_id_salt("baud")
                    .selected_text(format!("{} baud", self.board.baud))
                    .show_ui(ui, |ui| {
                        for b in BAUDS {
                            ui.selectable_value(&mut self.board.baud, b, format!("{} baud", b));
                        }
                    })
                    .response
                    .on_disabled_hover_text("the uart's baud rate is fixed while the board is on");
            });
            let mut dbg = self.board.debugger;
            if ui.checkbox(&mut dbg, "jtag debugger").on_hover_text("a debugger on the jtag header turns svc into semihosting (putchar, exit), the way real arm boards do it. without one, svc halts the cpu like the verilog.").changed() {
                self.board.set_debugger(dbg);
            }
            ui.separator();
            egui::ComboBox::from_id_salt("board-speed").selected_text(SPEEDS[self.speed].1).show_ui(ui, |ui| {
                for (i, s) in SPEEDS.iter().enumerate() {
                    ui.selectable_value(&mut self.speed, i, s.1);
                }
            });
            if on {
                let mhz = self.rate / 1e6;
                ui.label(RichText::new(format!("{:.1} MHz simulated ({:.1}% of the board's {} MHz)", mhz, self.rate / CLOCK_HZ as f64 * 100.0, CLOCK_HZ / 1_000_000)).color(theme::DIM).size(12.5));
            }
        });
    }

    // ----- the board drawing -----

    fn drawing(&mut self, ui: &mut egui::Ui, area: Rect) {
        let p = ui.painter_at(area);
        let r = area.shrink2(vec2(40.0, 6.0));
        let on = self.board.powered();
        let c = cycles(&self.board);
        let w = self.board.wires.borrow().clone();
        let stage = self.board.stage();
        // a stopped cpu means a quiet serial line
        let live = self.board.machine.as_ref().is_some_and(|m| m.running());

        // pcb outline with mounting holes
        p.rect(r, CornerRadius::same(10), Color32::from_rgb(0x1a, 0x1d, 0x23), Stroke::new(1.0, theme::BORDER), StrokeKind::Inside);
        for q in [r.left_top() + vec2(14.0, 14.0), r.right_top() + vec2(-14.0, 14.0), r.left_bottom() + vec2(14.0, -14.0), r.right_bottom() + vec2(-14.0, -14.0)] {
            p.circle(q, 5.0, theme::BG, Stroke::new(1.0, theme::BORDER));
        }
        p.text(r.right_top() + vec2(-28.0, 10.0), Align2::RIGHT_TOP, "ftt-1 rev a", theme::mono(10.5), theme::FAINT);

        let cy = r.center().y;
        let fpga = Rect::from_center_size(pos2(r.left() + r.width() * 0.42, cy - 6.0), vec2(150.0, 150.0).min(vec2(r.height() - 40.0, r.height() - 40.0)));
        let usb = Rect::from_center_size(pos2(r.left() + 4.0, cy), vec2(58.0, 52.0));
        let jtag = Rect::from_min_size(pos2(r.left() + 70.0, r.top() + 22.0), vec2(84.0, 38.0));
        let osc = Rect::from_center_size(pos2(fpga.left() - 70.0, fpga.top() + 18.0), vec2(56.0, 30.0));
        let sram = Rect::from_center_size(pos2(fpga.right() + 110.0, fpga.center().y - 30.0), vec2(120.0, 62.0));
        let eth = Rect::from_center_size(pos2(r.right() - 6.0, cy - 30.0), vec2(70.0, 62.0));
        let sd = Rect::from_center_size(pos2(r.right() - 120.0, r.bottom() - 30.0), vec2(90.0, 36.0));
        let leds = Rect::from_center_size(pos2(fpga.center().x, r.bottom() - 26.0), vec2(8.0 * 22.0, 30.0));
        let reset = pos2(r.left() + 90.0, r.bottom() - 34.0);
        let power = Rect::from_center_size(pos2(r.left() + 160.0, r.bottom() - 34.0), vec2(46.0, 22.0));

        // traces first, parts on top
        let uart_busy = live && (c.saturating_sub(w.last_rx) < BLINK && w.rx_bytes > 0 || c.saturating_sub(w.last_tx) < BLINK && w.tx_bytes > 0);
        let trace = |a: Pos2, b: Pos2, hot: bool| {
            let mid = pos2(b.x, a.y);
            let s = Stroke::new(1.5, if hot { theme::ACCENT } else { theme::BORDER });
            p.line_segment([a, mid], s);
            p.line_segment([mid, b], s);
        };
        trace(usb.right_center(), fpga.left_center() + vec2(0.0, 20.0), uart_busy);
        trace(osc.right_center(), fpga.left_top() + vec2(30.0, 0.0), on);
        trace(jtag.right_center(), fpga.left_top() + vec2(60.0, 0.0), self.board.debugger);
        trace(fpga.right_center() + vec2(0.0, -30.0), sram.left_center(), on);
        trace(fpga.right_center() + vec2(0.0, 30.0), eth.left_center() + vec2(0.0, 20.0), false);
        trace(fpga.right_bottom() + vec2(-20.0, 0.0), sd.center_top(), false);
        trace(fpga.center_bottom(), leds.center_top(), on && w.leds != 0);

        let hover = ui.input(|i| i.pointer.hover_pos());
        let part = |rect: Rect, title: &str, sub: &str, lit: bool| {
            let hot = hover.is_some_and(|h| rect.contains(h));
            p.rect(rect, CornerRadius::same(4), Color32::from_rgb(0x24, 0x28, 0x30), Stroke::new(1.0, if hot { theme::DIM } else if lit { theme::ACCENT.gamma_multiply(0.7) } else { theme::BORDER }), StrokeKind::Inside);
            p.text(rect.center() - vec2(0.0, 7.0), Align2::CENTER_CENTER, title, egui::FontId::proportional(12.5), if lit { theme::TEXT } else { theme::DIM });
            p.text(rect.center() + vec2(0.0, 9.0), Align2::CENTER_CENTER, sub, theme::mono(10.0), theme::FAINT);
        };
        let led = |at: Pos2, lit: bool, col: Color32| {
            if lit {
                p.circle_filled(at, 7.0, col.gamma_multiply(0.18));
                p.circle_filled(at, 3.5, col);
            } else {
                p.circle(at, 3.5, Color32::from_rgb(0x2a, 0x2e, 0x35), Stroke::new(1.0, theme::BORDER));
            }
        };

        part(usb, "usb", "serial", on);
        led(usb.right_top() + vec2(14.0, 2.0), live && c.saturating_sub(w.last_tx) < BLINK && w.tx_bytes > 0, theme::WARN);
        led(usb.right_bottom() + vec2(14.0, -2.0), live && c.saturating_sub(w.last_rx) < BLINK && w.rx_bytes > 0, theme::OK);
        p.text(usb.right_top() + vec2(24.0, 2.0), Align2::LEFT_CENTER, "tx", theme::mono(9.5), theme::FAINT);
        p.text(usb.right_bottom() + vec2(24.0, -2.0), Align2::LEFT_CENTER, "rx", theme::mono(9.5), theme::FAINT);

        // jtag: a 2x5 header
        part(jtag, "", "", self.board.debugger);
        for i in 0..5 {
            for j in 0..2 {
                let at = jtag.left_top() + vec2(14.0 + i as f32 * 14.0, 12.0 + j as f32 * 14.0);
                p.circle_filled(at, 2.5, if self.board.debugger { theme::ACCENT } else { theme::FAINT });
            }
        }
        p.text(jtag.center_bottom() + vec2(0.0, 6.0), Align2::CENTER_TOP, if self.board.debugger { "jtag  debugger attached" } else { "jtag" }, theme::mono(10.0), theme::FAINT);

        part(osc, "50 MHz", "osc", on);

        let pc = self.board.machine.as_ref().map(|m| format!("pc {:08x}", m.cpu.pc)).unwrap_or_else(|| "unconfigured".into());
        p.rect(fpga, CornerRadius::same(6), Color32::from_rgb(0x20, 0x24, 0x2b), Stroke::new(1.0, if on { theme::ACCENT.gamma_multiply(0.7) } else { theme::BORDER }), StrokeKind::Inside);
        // a ring of bga balls hinting at the package
        for k in 0..12 {
            let t = k as f32 / 11.0;
            for (a, b) in [(fpga.left_top(), fpga.right_top()), (fpga.left_bottom(), fpga.right_bottom())] {
                let at = a + (b - a) * (0.08 + t * 0.84);
                p.circle_filled(at + vec2(0.0, if a.y == fpga.top() { 8.0 } else { -8.0 }), 1.5, theme::BORDER);
            }
        }
        p.text(fpga.center() - vec2(0.0, 30.0), Align2::CENTER_CENTER, "FPGA", egui::FontId::proportional(16.0), if on { theme::TEXT } else { theme::DIM });
        p.text(fpga.center() - vec2(0.0, 8.0), Align2::CENTER_CENTER, "arm7_cpu.v", theme::mono(10.5), theme::DIM);
        p.text(fpga.center() + vec2(0.0, 6.0), Align2::CENTER_CENTER, "soc.v  uart.v", theme::mono(10.5), theme::DIM);
        p.text(fpga.center() + vec2(0.0, 28.0), Align2::CENTER_CENTER, pc, theme::mono(11.0), if on { theme::ACCENT } else { theme::FAINT });
        // DONE goes high once the bitstream is loaded
        led(fpga.right_top() + vec2(14.0, 8.0), on, theme::OK);
        p.text(fpga.right_top() + vec2(24.0, 8.0), Align2::LEFT_CENTER, "done", theme::mono(9.5), theme::FAINT);

        let rom = self.board.rom.as_ref().map_or("no rom".to_string(), |r| format!("rom @ {:05x}", r.base));
        part(sram, "sram 1 MB", &rom, on);
        part(eth, "ethernet", "stub", false);
        part(sd, "sd card", "no card", false);

        // eight user leds
        for i in 0..8 {
            let at = pos2(leds.left() + 11.0 + i as f32 * 22.0, leds.top() + 8.0);
            led(at, on && w.leds >> i & 1 == 1, theme::OK);
            p.text(at + vec2(0.0, 9.0), Align2::CENTER_TOP, format!("ld{}", i), theme::mono(9.0), theme::FAINT);
        }

        // reset button and power switch, both clickable
        let reset_resp = ui.interact(Rect::from_center_size(reset, vec2(32.0, 32.0)), ui.scope_id().with("reset-btn"), Sense::click());
        let pressed = reset_resp.is_pointer_button_down_on();
        p.circle(reset, 12.0, Color32::from_rgb(0x2a, 0x2e, 0x35), Stroke::new(1.0, if reset_resp.hovered() && on { theme::DIM } else { theme::BORDER }));
        p.circle_filled(reset, if pressed { 6.0 } else { 7.5 }, if on { Color32::from_rgb(0x8a, 0x3a, 0x3a) } else { Color32::from_rgb(0x4a, 0x2e, 0x2e) });
        p.text(reset + vec2(0.0, 17.0), Align2::CENTER_TOP, "reset", theme::mono(9.5), theme::FAINT);
        if reset_resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            self.board.reset();
        }

        let power_resp = ui.interact(power, ui.scope_id().with("power-sw"), Sense::click());
        p.rect(power, CornerRadius::same(11), if on { theme::ACCENT.gamma_multiply(0.35) } else { Color32::from_rgb(0x2a, 0x2e, 0x35) }, Stroke::new(1.0, if power_resp.hovered() { theme::DIM } else { theme::BORDER }), StrokeKind::Inside);
        let knob = if on { power.right_center() - vec2(11.0, 0.0) } else { power.left_center() + vec2(11.0, 0.0) };
        p.circle_filled(knob, 8.0, if on { theme::TEXT } else { theme::DIM });
        p.text(power.center_bottom() + vec2(0.0, 5.0), Align2::CENTER_TOP, "power", theme::mono(9.5), theme::FAINT);
        led(power.right_center() + vec2(16.0, 0.0), on, theme::ERR);
        if power_resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(if on { "power off" } else { "power on: load the bitstream and start the boot rom" }).clicked() && (on || self.board.rom.is_some()) {
            self.power();
        }

        // what the board is doing, in the corner
        let col = match &stage {
            Stage::Off => theme::FAINT,
            Stage::Fault(_) => theme::ERR,
            Stage::Halted => theme::WARN,
            _ => theme::OK,
        };
        p.text(r.left_top() + vec2(28.0, 10.0), Align2::LEFT_TOP, stage_text(&stage), theme::mono(10.5), col);
    }

    // ----- serial terminal -----

    fn terminal(&mut self, ui: &mut egui::Ui, b: &Snapshot, program: &str) {
        let on = self.board.powered();
        ui.horizontal(|ui| {
            widgets::heading(ui, "serial terminal", theme::ACCENT);
            widgets::caption(ui, &format!("usb-serial, {} baud 8N1", self.board.baud));
        });
        ui.add_space(4.0);

        // the host's download tool
        let pending = self.board.wires.borrow().rx_pending;
        ui.horizontal(|ui| {
            let image = b.flat.as_ref().filter(|_| b.error.is_none());
            let size = image.map_or(String::new(), |i| format!(" ({})", widgets::bytes(i.len())));
            let can = on && image.is_some() && pending == 0;
            let resp = ui.add_enabled(can, egui::Button::new(format!("send {}{} over serial", program, size)));
            let resp = if !on {
                resp.on_disabled_hover_text("power the board on first")
            } else if image.is_none() {
                resp.on_disabled_hover_text("the build failed, so there is no image to send")
            } else {
                resp.on_hover_text("frames the image the way the boot rom expects and pushes it down the cable")
            };
            if resp.clicked() {
                if let Some(img) = image {
                    let entry = b.link.as_ref().map_or(b.base, |l| l.entry);
                    if let Ok(n) = self.board.send_image(img, b.base, entry) {
                        self.sent = n;
                        self.sent_name = program.to_string();
                    }
                }
            }
            if pending > 0 && self.sent > 0 {
                let done = self.sent - pending.min(self.sent);
                ui.add(egui::ProgressBar::new(done as f32 / self.sent as f32).desired_width(180.0).text(format!("{} / {}", widgets::bytes(done), widgets::bytes(self.sent))));
            }
        });
        if let Some(rom) = &self.board.rom {
            if rom.framing == Framing::Length3 && b.base != toolchain::board::BOOTROM_LOAD {
                widgets::caption(ui, &format!("bootrom.asm copies images to 0x{:x}, but this program is linked at 0x{:x}", toolchain::board::BOOTROM_LOAD, b.base));
            }
        }
        ui.add_space(4.0);

        let out_h = (ui.available_height() - 34.0).max(80.0);
        egui::Frame::NONE.fill(theme::BG).stroke(Stroke::new(1.0, theme::BORDER)).corner_radius(CornerRadius::same(3)).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
            ui.set_min_size(vec2(ui.available_width(), out_h));
            ui.set_max_height(out_h);
            egui::ScrollArea::vertical().id_salt("term").stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                let text = self.board.machine.as_ref().map_or("", |m| m.console.as_str());
                let start = text.len().saturating_sub(TERM_MAX);
                let start = (start..text.len()).find(|&i| text.is_char_boundary(i)).unwrap_or(text.len());
                let shown = if on { &text[start..] } else { "" };
                ui.label(RichText::new(if shown.is_empty() && !on { "(board is off)" } else { shown }).monospace().color(if on { theme::TEXT } else { theme::FAINT }));
            });
        });
        ui.horizontal(|ui| {
            let resp = ui.add_enabled(on, egui::TextEdit::singleline(&mut self.line).hint_text("type here, enter sends the line").desired_width(ui.available_width() - 70.0).font(egui::TextStyle::Monospace));
            let send = ui.add_enabled(on, egui::Button::new("send")).clicked();
            if send || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                for c in self.line.bytes().chain(std::iter::once(b'\n')) {
                    self.board.type_byte(c);
                }
                self.line.clear();
                resp.request_focus();
            }
        });
    }

    // ----- boot sequence -----

    fn sequence(&mut self, ui: &mut egui::Ui) {
        widgets::heading(ui, "boot sequence", theme::ACCENT);
        ui.add_space(4.0);
        let stage = self.board.stage();
        let on = self.board.powered();
        let w = self.board.wires.borrow().clone();
        let downloaded = self.board.payload.is_some();
        let past_rom = matches!(stage, Stage::Running | Stage::Halted | Stage::Exited(_) | Stage::Fault(_)) && downloaded;

        let rom_detail = match &self.board.rom {
            None => "no rom builds yet".to_string(),
            Some(r) => {
                let mut s = format!("{} at 0x{:x}", r.name, r.base);
                if !r.name.ends_with("bootrom.asm") {
                    if let Some(e) = self.roms.iter().find(|(n, _)| n.ends_with("bootrom.asm")).and_then(|(_, b)| b.error.as_ref()) {
                        s += &format!("\nstand-in: section3/bootrom.asm fails to {}: {}", e.stage, e.msg);
                    }
                }
                s
            }
        };
        let program_detail = match &stage {
            Stage::Running => format!("{} is running", self.sent_name),
            Stage::Halted if !self.board.debugger => "halted on svc: no debugger attached for semihosting".into(),
            Stage::Halted => "halted".into(),
            Stage::Exited(c) => format!("exited through semihosting ({:#x})", c),
            Stage::Fault(e) => e.clone(),
            _ => "jumps to the image's entry point once it arrives".into(),
        };
        let steps: [(&str, String, Step); 6] = [
            ("power + fpga config", "bitstream: arm7_cpu.v, soc.v, uart.v at 50 MHz".into(), if on { Step::Done } else { Step::Pending }),
            ("boot rom", rom_detail, if !on { Step::Pending } else if downloaded || past_rom { Step::Done } else { Step::Active }),
            (
                "download over uart",
                format!("{} bytes in, {} overruns", w.rx_bytes, w.overruns),
                if stage == Stage::Receiving { Step::Active } else if past_rom { Step::Done } else if w.overruns > 0 { Step::Failed } else { Step::Pending },
            ),
            (
                "program",
                program_detail,
                match stage {
                    Stage::Running => Step::Active,
                    Stage::Exited(_) if past_rom => Step::Done,
                    Stage::Halted | Stage::Fault(_) => Step::Failed,
                    _ => Step::Pending,
                },
            ),
            ("kernel", "section 5, not written yet. needs svc and irq modes in arm7_cpu.v and an mmu".into(), Step::Missing),
            ("init + shell", "section 5, not written yet. init, shell, cat, ls, rm on fat".into(), Step::Missing),
        ];
        let mut centers = Vec::new();
        for (i, (title, detail, step)) in steps.iter().enumerate() {
            let c = step_row(ui, i + 1, title, detail, *step);
            centers.push(c);
        }
        for w in centers.windows(2) {
            ui.painter().line_segment([w[0] + vec2(0.0, 11.0), w[1] - vec2(0.0, 11.0)], Stroke::new(1.0, theme::FAINT.gamma_multiply(0.6)));
        }
    }

    fn devices(&mut self, ui: &mut egui::Ui) {
        widgets::heading(ui, "address map", theme::ACCENT);
        ui.add_space(4.0);
        let w = self.board.wires.borrow().clone();
        let payload = self.board.payload.map_or("empty".to_string(), |(a, b)| format!("image {:x}..{:x}", a, b));
        let rows: [(u32, &str, String); 7] = [
            (SRAM_BASE, "sram", payload),
            (ROM_BASE, "boot rom (in sram)", self.board.rom.as_ref().map_or("none".into(), |r| r.name.clone())),
            (LED_BASE, "leds (playground only)", format!("{:08b}", w.leds)),
            (UART_BASE, "uart (soc.v)", format!("rx {} tx {}, {} lost", w.rx_bytes, w.tx_bytes, w.overruns + w.tx_dropped)),
            (EthernetStub::BASE, "ethernet (stub)", "stub, never receives".into()),
            (SdCardStub::BASE, "sd card (stub)", "empty slot".into()),
            (0, "mmu", "identity (none yet)".into()),
        ];
        egui::Grid::new("board-map").num_columns(3).striped(true).spacing(vec2(10.0, 4.0)).show(ui, |ui| {
            for (base, name, state) in rows {
                ui.label(RichText::new(if name == "mmu" { "-".into() } else { format!("{:08x}", base) }).monospace().size(11.5).color(theme::DIM));
                ui.label(RichText::new(name).size(12.5));
                ui.label(RichText::new(state).monospace().size(11.0).color(theme::DIM));
                ui.end_row();
            }
        });
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Done,
    Active,
    Pending,
    Failed,
    // nothing exists for this step yet
    Missing,
}

// a numbered step with a ring marker, the same shape as the sidebar
fn step_row(ui: &mut egui::Ui, n: usize, title: &str, detail: &str, step: Step) -> Pos2 {
    let font = theme::mono(11.0);
    let galley = ui.painter().layout(detail.to_string(), font, theme::DIM, (ui.available_width() - 42.0).max(80.0));
    let h = (galley.size().y + 30.0).max(44.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
    let p = ui.painter();
    let c = pos2(r.left() + 14.0, r.top() + 16.0);
    let (ring, fill, num) = match step {
        Step::Done => (theme::OK, theme::OK.gamma_multiply(0.18), theme::TEXT),
        Step::Active => (theme::ACCENT, theme::ACCENT.gamma_multiply(0.25), theme::TEXT),
        Step::Failed => (theme::ERR, theme::ERR.gamma_multiply(0.18), theme::ERR),
        Step::Pending => (theme::BORDER, Color32::TRANSPARENT, theme::FAINT),
        Step::Missing => (theme::BORDER, Color32::TRANSPARENT, theme::FAINT),
    };
    if step == Step::Missing {
        // a dashed ring: the slot exists, the code does not
        for k in 0..12 {
            let a0 = k as f32 / 12.0 * std::f32::consts::TAU;
            let a1 = a0 + std::f32::consts::TAU / 24.0;
            p.line_segment([c + vec2(a0.cos(), a0.sin()) * 10.0, c + vec2(a1.cos(), a1.sin()) * 10.0], Stroke::new(1.5, ring));
        }
    } else {
        p.circle(c, 10.0, fill, Stroke::new(1.5, ring));
    }
    p.text(c, Align2::CENTER_CENTER, n.to_string(), egui::FontId::proportional(11.5), num);
    let fg = if step == Step::Missing || step == Step::Pending { theme::DIM } else { theme::TEXT };
    p.text(pos2(r.left() + 34.0, r.top() + 8.0), Align2::LEFT_TOP, title, egui::FontId::proportional(13.5), fg);
    p.galley(pos2(r.left() + 34.0, r.top() + 26.0), galley, theme::DIM);
    c
}
