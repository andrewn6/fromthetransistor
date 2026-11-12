// the course outline: what runs here today, what is scaffolding, and the
// interfaces the next stages will plug into

use crate::views::cpu::CpuView;
use crate::{theme, widgets};
use eframe::egui::{self, Color32, RichText};
use toolchain::future::{EthernetStub, SdCardStub, Syscall};
use toolchain::machine::UartMmio;
use toolchain::roadmap::{Status, SECTIONS};

fn status_color(s: Status) -> Color32 {
    match s {
        Status::Done => theme::OK,
        Status::Partial => theme::WARN,
        Status::Scaffold => theme::TOKENS,
        Status::NotStarted => theme::FAINT,
        Status::OutOfScope => theme::FAINT,
    }
}

pub fn show(ui: &mut egui::Ui, cpu: &CpuView) {
    widgets::heading(ui, "from the transistor to the web browser", theme::SOURCE);
    widgets::caption(ui, "ORIGINAL_OUTLINE.md as a checklist. green parts run in this playground (the python toolchain itself, through pyodide); blue parts exist as interfaces in toolchain/src/future.rs, wired where the real thing will plug in.");
    ui.add_space(6.0);
    egui::ScrollArea::vertical().id_salt("roadmap").auto_shrink([false, false]).show(ui, |ui| {
        ui.columns(2, |cols| {
            let ui = &mut cols[0];
            for s in SECTIONS {
                widgets::card(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(format!("section {}: {}", s.number, s.title)).strong());
                    for it in s.items {
                        ui.horizontal_wrapped(|ui| {
                            widgets::pill(ui, it.status.label(), status_color(it.status));
                            ui.label(RichText::new(it.title).color(if it.status == Status::NotStarted || it.status == Status::OutOfScope { theme::DIM } else { theme::TEXT }));
                            if !it.lang.is_empty() {
                                ui.label(RichText::new(it.lang).size(11.5).color(theme::FAINT));
                            }
                        });
                        if !it.note.is_empty() {
                            ui.label(RichText::new(format!("    {}", it.note)).size(12.0).color(theme::DIM));
                        }
                        if !it.files.is_empty() {
                            ui.label(RichText::new(format!("    {}", it.files.join("  "))).monospace().size(11.0).color(theme::FAINT));
                        }
                    }
                });
                ui.add_space(6.0);
            }
            let ui = &mut cols[1];
            widgets::card(ui, |ui| {
                ui.set_width(ui.available_width());
                widgets::heading(ui, "address map", theme::CPU);
                widgets::caption(ui, "sram + stack come from cpu_tb.cpp. mmio windows are devices on the bus; the stubs reserve where ethernet and the sd card will live.");
                egui::Grid::new("map").num_columns(3).striped(true).show(ui, |ui| {
                    let mut rows = cpu.device_map();
                    for (name, base, size) in [
                        ("uart (soc.v), when enabled", UartMmio::BASE, 16),
                        ("ethernet (planned)", EthernetStub::BASE, EthernetStub::RXBUF + EthernetStub::BUF_SIZE),
                        ("sd card (planned)", SdCardStub::BASE, 0x1000),
                    ] {
                        if !rows.iter().any(|(_, b, _)| *b == base) {
                            rows.push((name.to_string(), base, size));
                        }
                    }
                    rows.sort_by_key(|r| r.1);
                    for (name, base, size) in rows {
                        ui.label(RichText::new(format!("0x{:08x}", base)).monospace());
                        ui.label(RichText::new(format!("0x{:08x}", base as u64 + size as u64 - 1)).monospace().color(theme::DIM));
                        ui.label(name);
                        ui.end_row();
                    }
                });
                ui.add_space(4.0);
                widgets::caption(ui, "every access goes through the Mmu trait first. today that is IdentityMmu; section 5's mmu replaces it.");
            });
            ui.add_space(6.0);
            widgets::card(ui, |ui| {
                ui.set_width(ui.available_width());
                widgets::heading(ui, "planned syscall abi", theme::TOKENS);
                widgets::caption(ui, "what the os chapter asks for. the numbers are a proposal; the kernel (c, built by this compiler) gets the final say. the cpu still needs svc mode and an exception vector before user code can trap into it.");
                egui::Grid::new("abi").num_columns(3).striped(true).show(ui, |ui| {
                    for s in Syscall::ALL {
                        ui.label(RichText::new(s.name()).monospace());
                        ui.label(RichText::new(format!("{}", s as u32)).monospace().color(theme::DIM));
                        ui.label(RichText::new(s.group()).size(12.0).color(theme::FAINT));
                        ui.end_row();
                    }
                });
            });
            ui.add_space(6.0);
            widgets::card(ui, |ui| {
                ui.set_width(ui.available_width());
                widgets::heading(ui, "running the os here", theme::AST);
                for line in [
                    "1. bootloader.c builds with this toolchain and pulls a kernel image through the ethernet window",
                    "2. the cpu grows svc/irq modes, banked sp/lr and a vector table (arm7_cpu.v, then cpu.rs)",
                    "3. the mmu replaces IdentityMmu; page faults become a new machine Status",
                    "4. the sd card stub gets a disk image, fat reads it, init runs the shell",
                    "5. tcp + telnetd use the ethernet device; the browser is a user program",
                ] {
                    ui.label(RichText::new(line).size(12.5));
                }
            });
        });
    });
}
