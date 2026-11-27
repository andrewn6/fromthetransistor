// the system around the cpu: memory map, mmio devices and the harness
// behavior of section3/cpu_tb.cpp (image load + reset-vector trampoline,
// one bus access per cycle, fatal on unmapped access).
//
// two additions over the testbench, both off by default in cpu_tb mode:
//   - semihosting: handle svc like qemu does (SYS_WRITEC / SYS_WRITE0 /
//     SYS_EXIT) instead of halting, so build.py programs print output
//   - devices: mmio peripherals (the soc.v uart, later ethernet / sd) that
//     sit in the address map next to sram

use crate::cpu::{BusReq, Cpu, EdgeEvent, Phase};
use crate::future::{IdentityMmu, Mmu};
use std::collections::{HashSet, VecDeque};

pub const SRAM_BASE: u32 = 0x0001_0000;
pub const SRAM_SIZE: u32 = 0x0010_0000;
pub const STK_BASE: u32 = 0x007F_0000;
pub const STK_SIZE: u32 = 0x0001_0000;
pub const RESULTS: u32 = 0x0010_0000;

// a memory-mapped peripheral. offsets are relative to `base()`.
pub trait Device {
    fn name(&self) -> &str;
    fn base(&self) -> u32;
    fn size(&self) -> u32;
    // `side_effects` is false when the cpu is writing (the bus still reads)
    fn read(&mut self, off: u32, side_effects: bool) -> u32;
    fn write(&mut self, off: u32, val: u32, byte: bool);
    // bytes the device wants printed on the console (uart tx)
    fn drain_output(&mut self) -> Vec<u8> {
        Vec::new()
    }
    // a byte typed into the console (uart rx)
    fn input(&mut self, _b: u8) {}
    // one clock edge, for devices with timing (the board's uart)
    fn tick(&mut self) {}
}

pub struct Ram {
    pub name: &'static str,
    pub base: u32,
    pub data: Vec<u8>,
}

impl Ram {
    fn contains(&self, a: u32, n: u32) -> bool {
        a >= self.base && (a as u64 + n as u64) <= self.base as u64 + self.data.len() as u64
    }
}

pub struct Bus {
    pub rams: Vec<Ram>,
    pub devices: Vec<Box<dyn Device>>,
    pub mmu: Box<dyn Mmu>,
}

impl Default for Bus {
    fn default() -> Self {
        Bus {
            rams: vec![
                Ram { name: "sram", base: SRAM_BASE, data: vec![0; SRAM_SIZE as usize] },
                Ram { name: "stack", base: STK_BASE, data: vec![0; STK_SIZE as usize] },
            ],
            devices: Vec::new(),
            mmu: Box::new(IdentityMmu),
        }
    }
}

impl Bus {
    fn ram(&mut self, a: u32, n: u32) -> Option<&mut [u8]> {
        let r = self.rams.iter_mut().find(|r| r.contains(a, n))?;
        let o = (a - r.base) as usize;
        Some(&mut r.data[o..o + n as usize])
    }

    fn device(&mut self, a: u32) -> Option<(&mut Box<dyn Device>, u32)> {
        let d = self.devices.iter_mut().find(|d| a >= d.base() && a - d.base() < d.size())?;
        let off = a - d.base();
        Some((d, off))
    }

    // one cpu-side read; Err is cpu_tb's "FATAL: unmapped" message
    pub fn read(&mut self, a: u32, byte: bool, side_effects: bool) -> Result<u32, String> {
        let a = self.mmu.translate(a).map_err(|e| e.to_string())?;
        if let Some((d, off)) = self.device(a) {
            let v = d.read(off, side_effects);
            return Ok(if byte { v & 0xFF } else { v });
        }
        let n = if byte { 1 } else { 4 };
        match self.ram(a, n) {
            Some(p) if byte => Ok(p[0] as u32),
            Some(p) => Ok(u32::from_le_bytes([p[0], p[1], p[2], p[3]])),
            None => Err(format!("FATAL: unmapped read access at 0x{:08x}", a)),
        }
    }

    pub fn write(&mut self, a: u32, v: u32, byte: bool) -> Result<(), String> {
        let a = self.mmu.translate(a).map_err(|e| e.to_string())?;
        if let Some((d, off)) = self.device(a) {
            d.write(off, v, byte);
            return Ok(());
        }
        let n = if byte { 1 } else { 4 };
        match self.ram(a, n) {
            Some(p) if byte => {
                p[0] = v as u8;
                Ok(())
            }
            Some(p) => {
                p.copy_from_slice(&v.to_le_bytes());
                Ok(())
            }
            None => Err(format!("FATAL: unmapped write access at 0x{:08x}", a)),
        }
    }

    // debugger view: no faults, no device side effects
    pub fn peek8(&mut self, a: u32) -> Option<u8> {
        self.ram(a, 1).map(|p| p[0])
    }

    pub fn peek32(&mut self, a: u32) -> Option<u32> {
        self.ram(a, 4).map(|p| u32::from_le_bytes([p[0], p[1], p[2], p[3]]))
    }

    pub fn poke32(&mut self, a: u32, v: u32) -> bool {
        match self.ram(a, 4) {
            Some(p) => {
                p.copy_from_slice(&v.to_le_bytes());
                true
            }
            None => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Running,
    // svc with semihosting off: the verilog cpu just halts
    Halted,
    // semihosting SYS_EXIT with its reason code
    Exited(u32),
    Fault(String),
    Breakpoint(u32),
}

// one retired instruction, for the timeline and call graph views
#[derive(Clone, Copy, Debug)]
pub struct Retired {
    pub cycle: u64,
    pub addr: u32,
    pub instr: u32,
    pub executed: bool, // false when its condition failed
}

#[derive(Clone, Debug)]
pub struct Frame {
    pub func: u32,
    pub ret: u32,
    pub sp: u32,
}

pub struct Machine {
    pub cpu: Cpu,
    pub bus: Bus,
    pub cycles: u64,
    pub retired: u64,
    pub status: Status,
    pub semihosting: bool,
    pub console: String,
    pub last_bus: Option<BusReq>,
    pub trace: VecDeque<Retired>,
    pub trace_cap: usize,
    // retire counts per sram word, for the hot-spot view
    pub heat: Vec<u32>,
    pub call_stack: Vec<Frame>,
    pub breakpoints: HashSet<u32>,
    pub phase_cycles: [u64; 3],
}

impl Default for Machine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine {
    pub fn new() -> Self {
        Machine {
            cpu: Cpu::reset(),
            bus: Bus::default(),
            cycles: 0,
            retired: 0,
            status: Status::Running,
            semihosting: false,
            console: String::new(),
            last_bus: None,
            trace: VecDeque::new(),
            trace_cap: 200_000,
            heat: vec![0; (SRAM_SIZE / 4) as usize],
            call_stack: Vec::new(),
            breakpoints: HashSet::new(),
            phase_cycles: [0; 3],
        }
    }

    // cpu_tb.cpp: load the flat image at `base`, plant `b entry` at the
    // reset vector, then one clock edge under reset. the testbench uses
    // one address for both; build.py programs have _start at the base.
    pub fn load(&mut self, image: &[u8], base: u32, entry: u32) -> Result<(), String> {
        let end = SRAM_BASE + SRAM_SIZE;
        if !(base >= SRAM_BASE && base + 4 <= end) {
            return Err(format!("FATAL: unmapped image access at 0x{:08x}", base));
        }
        let n = image.len().min((end - base) as usize);
        for (i, b) in image[..n].iter().enumerate() {
            self.bus.ram(base + i as u32, 1).unwrap()[0] = *b;
        }
        let off = entry.wrapping_sub(0x10000 + 8) >> 2;
        self.bus.poke32(0x10000, 0xEA00_0000 | (off & 0xFF_FFFF));
        self.cpu = Cpu::reset();
        self.cycles = 0;
        self.retired = 0;
        self.status = Status::Running;
        self.console.clear();
        self.trace.clear();
        self.heat.iter_mut().for_each(|h| *h = 0);
        self.call_stack.clear();
        self.phase_cycles = [0; 3];
        Ok(())
    }

    pub fn running(&self) -> bool {
        matches!(self.status, Status::Running | Status::Breakpoint(_))
    }

    // one clock: drive the bus, read, edge, then commit the write
    pub fn cycle(&mut self) {
        if !self.running() {
            return;
        }
        self.status = Status::Running;
        let req = self.cpu.bus();
        let data_in = match self.bus.read(req.addr, req.byte, !req.we) {
            Ok(v) => v,
            Err(e) => {
                self.status = Status::Fault(e);
                return;
            }
        };
        let phase = self.cpu.phase;
        let exec_addr = self.cpu.pc.wrapping_sub(4);
        let instr = self.cpu.instr;
        let passed = self.cpu.cond_pass();
        let was_bl = phase == Phase::Exec && passed && self.cpu.is_br() && (instr >> 24) & 1 == 1;
        let was_bx_lr = phase == Phase::Exec && passed && self.cpu.is_bx() && instr & 0xF == 14;
        let sp = self.cpu.regs[13];

        let ev = self.cpu.edge(data_in);
        self.phase_cycles[phase as usize] += 1;
        if req.we {
            if let Err(e) = self.bus.write(req.addr, req.data_out, req.byte) {
                self.status = Status::Fault(e);
            }
        }
        self.last_bus = Some(req);
        self.cycles += 1;

        if phase == Phase::Exec {
            self.retired += 1;
            if exec_addr >= SRAM_BASE && exec_addr < SRAM_BASE + SRAM_SIZE {
                let h = &mut self.heat[((exec_addr - SRAM_BASE) / 4) as usize];
                *h = h.saturating_add(1);
            }
            if self.trace.len() >= self.trace_cap {
                self.trace.pop_front();
            }
            self.trace.push_back(Retired { cycle: self.cycles, addr: exec_addr, instr, executed: passed });
            if was_bl {
                self.call_stack.push(Frame { func: self.cpu.pc, ret: exec_addr + 4, sp });
            } else if was_bx_lr {
                self.call_stack.pop();
            }
        }

        for d in self.bus.devices.iter_mut() {
            d.tick();
            for b in d.drain_output() {
                self.console.push(b as char);
            }
        }

        if ev == EdgeEvent::Svc {
            if self.semihosting {
                self.semihost();
            } else {
                self.status = Status::Halted;
            }
        }
    }

    // arm angel semihosting, the subset build.py's runtime uses
    fn semihost(&mut self) {
        let (op, arg) = (self.cpu.regs[0], self.cpu.regs[1]);
        match op {
            0x03 => match self.bus.peek8(arg) {
                // SYS_WRITEC: r1 points at the char
                Some(b) => self.console.push(b as char),
                None => self.status = Status::Fault(format!("semihosting: bad SYS_WRITEC pointer 0x{:08x}", arg)),
            },
            0x04 => {
                // SYS_WRITE0: r1 points at a nul-terminated string
                let mut a = arg;
                loop {
                    match self.bus.peek8(a) {
                        Some(0) => break,
                        Some(b) => self.console.push(b as char),
                        None => {
                            self.status = Status::Fault(format!("semihosting: bad SYS_WRITE0 pointer 0x{:08x}", a));
                            return;
                        }
                    }
                    a = a.wrapping_add(1);
                }
            }
            0x18 => {
                self.status = Status::Exited(arg);
                return;
            }
            _ => {
                self.status = Status::Fault(format!("semihosting: unsupported operation 0x{:x}", op));
                return;
            }
        }
        if self.running() {
            self.cpu.halted = false; // resume after the svc
        }
    }

    // run until halt, fault, breakpoint or the cycle budget runs out
    pub fn run(&mut self, max_cycles: u64) {
        let stop = self.cycles + max_cycles;
        let mut first = true;
        while self.running() && self.cycles < stop {
            if !first && self.cpu.phase == Phase::Fetch && self.breakpoints.contains(&self.cpu.pc) {
                self.status = Status::Breakpoint(self.cpu.pc);
                return;
            }
            first = false;
            self.cycle();
        }
    }

    // run until the next instruction boundary (back in fetch)
    pub fn step_instruction(&mut self) {
        self.cycle();
        while self.running() && self.cpu.phase != Phase::Fetch {
            self.cycle();
        }
    }

    pub fn type_input(&mut self, b: u8) {
        for d in self.bus.devices.iter_mut() {
            d.input(b);
        }
    }
}

// the soc.v uart window: DATA at +0x0, STATUS at +0x4 = {rx_valid, tx_ready}
pub struct UartMmio {
    pub base: u32,
    rx: VecDeque<u8>,
    rx_valid: bool,
    rx_byte: u8,
    tx: Vec<u8>,
}

impl UartMmio {
    pub const BASE: u32 = 0x4000_1000;

    pub fn new() -> Self {
        UartMmio { base: Self::BASE, rx: VecDeque::new(), rx_valid: false, rx_byte: 0, tx: Vec::new() }
    }

    fn pump(&mut self) {
        // the uart's rx_pulse: latch the next typed byte when idle
        if !self.rx_valid {
            if let Some(b) = self.rx.pop_front() {
                self.rx_byte = b;
                self.rx_valid = true;
            }
        }
    }
}

impl Default for UartMmio {
    fn default() -> Self {
        Self::new()
    }
}

impl Device for UartMmio {
    fn name(&self) -> &str {
        "uart (soc.v)"
    }
    fn base(&self) -> u32 {
        self.base
    }
    fn size(&self) -> u32 {
        16
    }
    fn read(&mut self, off: u32, side_effects: bool) -> u32 {
        self.pump();
        match off {
            0x0 => {
                let v = self.rx_byte as u32;
                if side_effects {
                    self.rx_valid = false;
                }
                v
            }
            0x4 => ((self.rx_valid as u32) << 1) | 1, // tx always ready
            _ => 0,
        }
    }
    fn write(&mut self, off: u32, val: u32, _byte: bool) {
        if off == 0 {
            self.tx.push(val as u8);
        }
    }
    fn drain_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.tx)
    }
    fn input(&mut self, b: u8) {
        self.rx.push_back(b);
    }
}
