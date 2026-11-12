// the fpga dev board from section 7 of the outline, simulated: the soc
// (arm7_cpu.v plus soc.v's uart) and what sits around it on a real board,
// a 50 MHz clock, power and reset, leds, a usb-serial bridge, a jtag header,
// an sd slot and an ethernet jack.
//
// it differs from the cpu tab the way hardware differs from a testbench:
// nothing is preloaded except the boot rom, the program has to come in over
// the uart at a real baud rate, and svc only reaches semihosting when a
// debugger is attached to the jtag header.

use crate::cpu::Cpu;
use crate::future::{EthernetStub, SdCardStub};
use crate::machine::{Device, Machine, Status, SRAM_BASE};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

pub const CLOCK_HZ: u64 = 50_000_000;
// the boot rom sits at the top of sram, like block ram the bitstream fills in
pub const ROM_BASE: u32 = 0x000F_0000;
pub const ROM_SIZE: u32 = 0x0001_0000;
pub const LED_BASE: u32 = 0x4000_0000;
pub const UART_BASE: u32 = 0x4000_1000;
pub const BAUDS: [u32; 3] = [115_200, 921_600, 3_000_000];
// where section3/bootrom.asm copies the image
pub const BOOTROM_LOAD: u32 = 0x0002_0000;

// how a boot rom wants the image framed on the wire
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Framing {
    // playground/boot/rom.s: load address, length, entry (4 bytes each, le), image
    Header,
    // section3/bootrom.asm: a 3-byte length, then the image, copied to 0x20000
    Length3,
}

#[derive(Clone, Debug)]
pub struct Rom {
    pub name: String,
    pub image: Vec<u8>,
    pub base: u32,
    pub entry: u32,
    pub framing: Framing,
}

// signals the ui watches: leds, serial traffic, transfer progress
#[derive(Clone, Debug, Default)]
pub struct Wires {
    pub leds: u32,
    // host bytes still waiting to go out on the wire
    pub rx_pending: usize,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    // cycle of the last byte each way, for the activity leds
    pub last_rx: u64,
    pub last_tx: u64,
    // rx bytes lost because the cpu read too late
    pub overruns: u64,
    // bytes written while the transmitter was still busy
    pub tx_dropped: u64,
}

pub type Shared = Rc<RefCell<Wires>>;

// soc.v's uart window behind a real serial line. a byte takes 10 bit times
// (start, 8 data, stop) each way. like the verilog, a new rx byte overwrites
// an unread one, and a write while tx is busy is lost.
pub struct SerialUart {
    cycles_per_byte: u64,
    now: u64,
    host: VecDeque<u8>,
    next_rx: u64,
    rx_byte: u8,
    rx_valid: bool,
    tx_free_at: u64,
    tx: Vec<u8>,
    wires: Shared,
}

impl SerialUart {
    pub fn new(baud: u32, wires: Shared) -> Self {
        SerialUart {
            cycles_per_byte: CLOCK_HZ * 10 / baud as u64,
            now: 0,
            host: VecDeque::new(),
            next_rx: 0,
            rx_byte: 0,
            rx_valid: false,
            tx_free_at: 0,
            tx: Vec::new(),
            wires,
        }
    }
}

impl Device for SerialUart {
    fn name(&self) -> &str {
        "uart (soc.v)"
    }
    fn base(&self) -> u32 {
        UART_BASE
    }
    fn size(&self) -> u32 {
        16
    }
    fn read(&mut self, off: u32, side_effects: bool) -> u32 {
        match off {
            0x0 => {
                if side_effects {
                    self.rx_valid = false;
                }
                self.rx_byte as u32
            }
            0x4 => ((self.rx_valid as u32) << 1) | (self.now >= self.tx_free_at) as u32,
            _ => 0,
        }
    }
    fn write(&mut self, off: u32, val: u32, _byte: bool) {
        if off != 0 {
            return;
        }
        let mut w = self.wires.borrow_mut();
        if self.now >= self.tx_free_at {
            self.tx.push(val as u8);
            self.tx_free_at = self.now + self.cycles_per_byte;
            w.tx_bytes += 1;
            w.last_tx = self.now;
        } else {
            w.tx_dropped += 1;
        }
    }
    fn drain_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.tx)
    }
    fn input(&mut self, b: u8) {
        if self.host.is_empty() {
            self.next_rx = self.next_rx.max(self.now + self.cycles_per_byte);
        }
        self.host.push_back(b);
        self.wires.borrow_mut().rx_pending = self.host.len();
    }
    fn tick(&mut self) {
        self.now += 1;
        if self.now < self.next_rx {
            return;
        }
        if let Some(b) = self.host.pop_front() {
            let mut w = self.wires.borrow_mut();
            if self.rx_valid {
                w.overruns += 1;
            }
            self.rx_byte = b;
            self.rx_valid = true;
            self.next_rx = self.now + self.cycles_per_byte;
            w.rx_bytes += 1;
            w.last_rx = self.now;
            w.rx_pending = self.host.len();
        }
    }
}

// eight leds on a write-only register. soc.v has no led window yet; this
// one is the playground's, so the boot rom can show where it is.
pub struct Leds {
    wires: Shared,
}

impl Device for Leds {
    fn name(&self) -> &str {
        "leds (playground only)"
    }
    fn base(&self) -> u32 {
        LED_BASE
    }
    fn size(&self) -> u32 {
        16
    }
    fn read(&mut self, _off: u32, _side_effects: bool) -> u32 {
        self.wires.borrow().leds
    }
    fn write(&mut self, off: u32, val: u32, _byte: bool) {
        if off == 0 {
            self.wires.borrow_mut().leds = val & 0xFF;
        }
    }
}

// where the board is in bringing itself up
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stage {
    Off,
    // the rom is running and nothing has arrived yet
    WaitingForImage,
    Receiving,
    // the cpu left the rom
    Running,
    // svc without a debugger: the verilog cpu stops
    Halted,
    // semihosting SYS_EXIT through the debugger
    Exited(u32),
    Fault(String),
}

pub struct Board {
    // None while the power is off
    pub machine: Option<Machine>,
    pub rom: Option<Rom>,
    pub baud: u32,
    // a debugger on the jtag header: svc goes to semihosting instead of halting
    pub debugger: bool,
    pub wires: Shared,
    // address range of the last image sent, once the rom has been told
    pub payload: Option<(u32, u32)>,
}

impl Default for Board {
    fn default() -> Self {
        Board { machine: None, rom: None, baud: BAUDS[1], debugger: false, wires: Shared::default(), payload: None }
    }
}

impl Board {
    pub fn powered(&self) -> bool {
        self.machine.is_some()
    }

    // configure the fpga (the "bitstream" is the soc plus the rom contents)
    // and release reset
    pub fn power_on(&mut self) -> Result<(), String> {
        let rom = self.rom.as_ref().ok_or("no boot rom to put in block ram")?;
        *self.wires.borrow_mut() = Wires::default();
        let mut m = Machine::new();
        m.load(&rom.image, rom.base, rom.entry)?;
        m.semihosting = self.debugger;
        m.trace_cap = 4096;
        m.bus.devices.push(Box::new(Leds { wires: self.wires.clone() }));
        m.bus.devices.push(Box::new(SerialUart::new(self.baud, self.wires.clone())));
        m.bus.devices.push(Box::new(EthernetStub::new()));
        m.bus.devices.push(Box::new(SdCardStub { base: SdCardStub::BASE, image: Vec::new() }));
        self.payload = None;
        self.machine = Some(m);
        Ok(())
    }

    pub fn power_off(&mut self) {
        self.machine = None;
        self.payload = None;
        *self.wires.borrow_mut() = Wires::default();
    }

    // the reset button: the cpu restarts at the reset vector, memory keeps
    // its contents. images load after the reset branch at 0x10000, so reset
    // lands back in the rom, ready for the next download.
    pub fn reset(&mut self) {
        if let Some(m) = &mut self.machine {
            m.cpu = Cpu::reset();
            m.status = Status::Running;
            m.call_stack.clear();
        }
    }

    pub fn set_debugger(&mut self, on: bool) {
        self.debugger = on;
        if let Some(m) = &mut self.machine {
            m.semihosting = on;
        }
    }

    // the host end of the usb-serial cable: frame the image the way the rom
    // expects it and queue every byte. returns the bytes on the wire.
    pub fn send_image(&mut self, image: &[u8], base: u32, entry: u32) -> Result<usize, String> {
        let (Some(rom), Some(m)) = (&self.rom, &mut self.machine) else {
            return Err("the board is off".into());
        };
        let n = image.len() as u32;
        let mut wire = Vec::with_capacity(image.len() + 12);
        let load = match rom.framing {
            Framing::Header => {
                wire.extend(base.to_le_bytes());
                wire.extend(n.to_le_bytes());
                wire.extend(entry.to_le_bytes());
                base
            }
            Framing::Length3 => {
                wire.extend(&n.to_le_bytes()[..3]);
                BOOTROM_LOAD
            }
        };
        wire.extend_from_slice(image);
        for b in &wire {
            m.type_input(*b);
        }
        self.payload = Some((load, load + n));
        Ok(wire.len())
    }

    // a key pressed in the serial terminal
    pub fn type_byte(&mut self, b: u8) {
        if let Some(m) = &mut self.machine {
            m.type_input(b);
        }
    }

    pub fn run(&mut self, cycles: u64) {
        if let Some(m) = &mut self.machine {
            m.run(cycles);
        }
    }

    pub fn stage(&self) -> Stage {
        let Some(m) = &self.machine else { return Stage::Off };
        match &m.status {
            Status::Fault(e) => return Stage::Fault(e.clone()),
            Status::Halted => return Stage::Halted,
            Status::Exited(c) => return Stage::Exited(*c),
            _ => {}
        }
        let pc = m.cpu.pc;
        let in_payload = self.payload.is_some_and(|(a, b)| pc >= a && pc < b);
        // the reset trampoline at the start of sram counts as rom until an image replaces it
        let in_rom = (ROM_BASE..ROM_BASE + ROM_SIZE).contains(&pc) || (!in_payload && pc < SRAM_BASE + 16);
        if !in_rom {
            Stage::Running
        } else if self.wires.borrow().rx_pending > 0 {
            Stage::Receiving
        } else {
            Stage::WaitingForImage
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembler::{Asm, Sec};
    use crate::linker::{self, ElfLinker};

    // assemble + link with the rust port, linked at `base`
    fn build(src: &str, base: u32) -> (Vec<u8>, u32, u32) {
        let mut a = Asm::new();
        a.pass1(src).unwrap();
        a.pass2().unwrap();
        let mut ld = ElfLinker::new(base);
        ld.add_section(".text", &a.text, linker::SHF_ALLOC | linker::SHF_EXECINSTR);
        ld.add_section(".data", &a.data, linker::SHF_ALLOC | linker::SHF_WRITE);
        for (name, (sec, off)) in a.symbols.iter() {
            let t = if *sec == Sec::Data { linker::STT_OBJECT } else { linker::STT_FUNC };
            ld.add_symbol(name, *off as u32, sec.name(), linker::STB_GLOBAL, t).unwrap();
        }
        for r in &a.relocs {
            ld.add_relocation(r.off as u32, &r.sym, r.sec.name(), r.rtype, r.addend).unwrap();
        }
        ld.link("_start").unwrap();
        let flat: Vec<u8> = ld.sections.iter().flat_map(|s| s.content.clone()).collect();
        (flat, ld.sections[0].addr, ld.entry)
    }

    const PROGRAM: &str = "
.text
.global _start
_start:
  ldr r4, =0x40001000
  mov r0, #104
  bl putc
  mov r0, #105
  bl putc
  svc 0x123456
putc:
  ldr r1, [r4, #4]
  tst r1, #1
  beq putc
  strb r0, [r4, #0]
  bx lr
";

    fn board() -> Board {
        let (image, base, entry) = build(include_str!("../../boot/rom.s"), ROM_BASE);
        let mut b = Board::default();
        b.baud = 3_000_000;
        b.rom = Some(Rom { name: "rom.s".into(), image, base, entry, framing: Framing::Header });
        b
    }

    fn console(b: &Board) -> String {
        b.machine.as_ref().unwrap().console.clone()
    }

    #[test]
    fn boots_downloads_and_runs() {
        let mut b = board();
        b.power_on().unwrap();
        b.run(200_000);
        assert_eq!(b.stage(), Stage::WaitingForImage);
        assert_eq!(console(&b), "rom: waiting for an image on the uart\n");
        assert_eq!(b.wires.borrow().leds, 1);

        let (image, base, entry) = build(PROGRAM, 0x10000);
        b.send_image(&image, base, entry).unwrap();
        b.run(2_000);
        assert_eq!(b.stage(), Stage::Receiving);
        b.run(2_000_000);
        assert_eq!(b.wires.borrow().overruns, 0);
        assert_eq!(b.wires.borrow().leds, 7);
        assert!(console(&b).ends_with("rom: image received, jumping\nhi"), "{:?}", console(&b));
        // no debugger: the svc halts the cpu like the verilog
        assert_eq!(b.stage(), Stage::Halted);
    }

    #[test]
    fn reset_goes_back_to_the_rom() {
        let mut b = board();
        b.power_on().unwrap();
        b.run(200_000);
        let (image, base, entry) = build(PROGRAM, 0x10000);
        b.send_image(&image, base, entry).unwrap();
        b.run(3_000_000);
        b.reset();
        b.run(200_000);
        assert!(console(&b).ends_with("hirom: waiting for an image on the uart\n"), "{:?}", console(&b));
        assert_eq!(b.stage(), Stage::WaitingForImage);
    }

    #[test]
    fn debugger_turns_svc_into_semihosting() {
        let mut b = board();
        b.set_debugger(true);
        b.power_on().unwrap();
        b.run(200_000);
        let (image, base, entry) = build(PROGRAM, 0x10000);
        b.send_image(&image, base, entry).unwrap();
        b.run(3_000_000);
        // 0x123456 with r0 = 105 ('i') is not a semihosting op the runtime uses
        assert!(matches!(b.stage(), Stage::Fault(_)), "{:?}", b.stage());
    }

    #[test]
    fn sending_before_the_rom_listens_overruns() {
        let mut b = board();
        b.power_on().unwrap();
        let (image, base, entry) = build(PROGRAM, 0x10000);
        b.send_image(&image, base, entry).unwrap();
        b.run(3_000_000);
        // the rom was still printing its banner, so the header bytes were lost
        assert!(b.wires.borrow().overruns > 0);
        assert_ne!(b.stage(), Stage::Halted);
    }

    #[test]
    fn uart_timing_follows_the_baud_rate() {
        let w = Shared::default();
        let mut u = SerialUart::new(115_200, w.clone());
        u.input(0x41);
        let cpb = CLOCK_HZ * 10 / 115_200;
        for _ in 0..cpb - 1 {
            u.tick();
        }
        assert_eq!(u.read(4, false) & 2, 0);
        u.tick();
        assert_eq!(u.read(4, false) & 2, 2);
        assert_eq!(u.read(0, true), 0x41);
        assert_eq!(u.read(4, false) & 2, 0);
        // a second write before the first byte is out is lost
        u.write(0, 1, true);
        u.write(0, 2, true);
        assert_eq!(w.borrow().tx_dropped, 1);
        assert_eq!(u.drain_output(), vec![1]);
    }
}
