// cycle-accurate model of section3/arm7_cpu.v.
//
// the verilog is a 3-phase machine (fetch, exec, block) doing one memory
// access per clock. this file mirrors it signal for signal: `bus()` is the
// combinational always @(*) block that drives sram_addr/we/byte/data_out,
// and `edge()` is the always @(posedge clk) block. nonblocking assignments
// are modeled by reading only the pre-edge state and writing a copy.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Fetch,
    Exec,
    Block,
}

impl Phase {
    pub fn name(self) -> &'static str {
        match self {
            Phase::Fetch => "FETCH",
            Phase::Exec => "EXEC",
            Phase::Block => "BLOCK",
        }
    }
}

// the memory request the cpu drives during a cycle
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BusReq {
    pub addr: u32,
    pub data_out: u32,
    pub we: bool,
    pub byte: bool,
}

// what happened on a clock edge, for tracing
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeEvent {
    None,
    Fetched,      // instr latched, pc += 4
    Executed,     // one instruction retired in the exec phase
    Svc,          // svc trapped: halted <= 1
    BlockStart,   // push/pop started its block-transfer phase
    BlockXfer,    // one register moved
    BlockDone,    // last register moved, sp written back
}

#[derive(Clone, Debug)]
pub struct Cpu {
    pub pc: u32,
    pub instr: u32,
    pub phase: Phase,
    pub regs: [u32; 16],
    pub flags: u8, // {n, z, c, v} = bits 3..0
    pub halted: bool,
    pub blk_mask: u16,
    pub blk_addr: u32,
}

pub const RESET_PC: u32 = 0x0001_0000;

pub const FLAG_N: u8 = 8;
pub const FLAG_Z: u8 = 4;
pub const FLAG_C: u8 = 2;
pub const FLAG_V: u8 = 1;

fn bit(v: u32, n: u32) -> bool {
    (v >> n) & 1 == 1
}

impl Default for Cpu {
    fn default() -> Self {
        Self::reset()
    }
}

impl Cpu {
    pub fn reset() -> Self {
        Cpu {
            pc: RESET_PC,
            instr: 0,
            phase: Phase::Fetch,
            regs: [0; 16],
            flags: 0,
            halted: false,
            blk_mask: 0,
            blk_addr: 0,
        }
    }

    fn n(&self) -> bool {
        self.flags & FLAG_N != 0
    }
    fn z(&self) -> bool {
        self.flags & FLAG_Z != 0
    }
    fn c(&self) -> bool {
        self.flags & FLAG_C != 0
    }
    fn v(&self) -> bool {
        self.flags & FLAG_V != 0
    }

    // ----- decode (the wires at the top of the module) -----

    pub fn cond_pass(&self) -> bool {
        match self.instr >> 28 {
            0x0 => self.z(),
            0x1 => !self.z(),
            0x2 => self.c(),
            0x3 => !self.c(),
            0x4 => self.n(),
            0x5 => !self.n(),
            0x6 => self.v(),
            0x7 => !self.v(),
            0x8 => self.c() && !self.z(),
            0x9 => !self.c() || self.z(),
            0xA => self.n() == self.v(),
            0xB => self.n() != self.v(),
            0xC => !self.z() && (self.n() == self.v()),
            0xD => self.z() || (self.n() != self.v()),
            0xE => true,
            _ => false,
        }
    }

    fn pc_read(&self) -> u32 {
        self.pc.wrapping_add(4)
    }
    fn rn(&self) -> usize {
        ((self.instr >> 16) & 0xF) as usize
    }
    fn rd(&self) -> usize {
        ((self.instr >> 12) & 0xF) as usize
    }
    fn reg_or_pc(&self, r: usize) -> u32 {
        if r == 15 { self.pc_read() } else { self.regs[r] }
    }

    pub fn is_dp(&self) -> bool {
        (self.instr >> 26) & 3 == 0b00
    }
    pub fn is_ls(&self) -> bool {
        (self.instr >> 26) & 3 == 0b01
    }
    pub fn is_blk(&self) -> bool {
        (self.instr >> 25) & 7 == 0b100
    }
    pub fn is_br(&self) -> bool {
        (self.instr >> 25) & 7 == 0b101
    }
    pub fn is_svc(&self) -> bool {
        (self.instr >> 24) & 0xF == 0b1111
    }
    pub fn is_bx(&self) -> bool {
        let i = self.instr;
        self.is_dp() && (i >> 20) & 0xFF == 0x12 && (i >> 12) & 0xFF == 0xFF && (i >> 4) & 0xFF == 0xF1
    }
    pub fn is_mul(&self) -> bool {
        let i = self.instr;
        self.is_dp() && !bit(i, 25) && (i >> 21) & 0x7F == 0 && (i >> 12) & 0xF == 0 && (i >> 4) & 0xF == 0b1001
    }

    // barrel shifter: (op2_val, op2_cout)
    fn shifter(&self) -> (u32, bool) {
        let op2 = self.instr & 0xFFF;
        let rm = (op2 & 0xF) as usize;
        let sh_reg = bit(op2, 4);
        let rs = ((op2 >> 8) & 0xF) as usize;
        let sh_amt = if sh_reg { self.regs[rs] & 0x1F } else { (op2 >> 7) & 0x1F };
        let sh_typ = (op2 >> 5) & 3;
        let rm_val = self.reg_or_pc(rm);
        let mut val = rm_val;
        let mut cout = self.c();
        match sh_typ {
            0 => {
                if sh_amt != 0 {
                    val = rm_val << sh_amt;
                    cout = bit(rm_val, 32 - sh_amt);
                }
            }
            1 => {
                if sh_amt == 0 {
                    val = 0;
                    cout = bit(rm_val, 31);
                } else {
                    val = rm_val >> sh_amt;
                    cout = bit(rm_val, sh_amt - 1);
                }
            }
            2 => {
                if sh_amt != 0 {
                    val = ((rm_val as i32) >> sh_amt) as u32;
                    cout = bit(rm_val, 31);
                }
            }
            _ => {}
        }
        (val, cout)
    }

    fn op2_final(&self) -> (u32, bool) {
        if bit(self.instr, 25) {
            (self.instr & 0xFF, self.c())
        } else {
            self.shifter()
        }
    }

    // the alu: (alu_out, new_flags)
    pub fn alu(&self) -> (u32, u8) {
        let op = (self.instr >> 21) & 0xF;
        let a = self.reg_or_pc(self.rn());
        let (b, op2_carry) = self.op2_final();
        let (a64, b64, c64) = (a as u64, b as u64, self.c() as u64);
        let m33 = 0x1_FFFF_FFFFu64;
        let add_ext = (a64 + b64) & m33;
        let sub_ext = (a64 + (!b as u64) + 1) & m33;
        let rsb_ext = (b64 + (!a as u64) + 1) & m33;
        let sbc_ext = (a64 + (!b as u64) + c64) & m33;
        let rsc_ext = (b64 + (!a as u64) + c64) & m33;
        let b31 = |x: u64| (x >> 31) & 1 == 1;
        let (a31, op31) = (bit(a, 31), bit(b, 31));
        let add_v = a31 == op31 && b31(add_ext) != a31;
        let sub_v = a31 != op31 && b31(sub_ext) != a31;
        let rsb_v = a31 != op31 && b31(rsb_ext) != op31;
        let sbc_v = a31 != op31 && b31(sbc_ext) != a31;
        let rsc_v = a31 != op31 && b31(rsc_ext) != op31;
        let carry = |x: u64| (x >> 32) & 1 == 1;

        let (mut out, mut c, mut v) = (0u32, self.c(), self.v());
        match op {
            0x0 | 0x8 => out = a & b,
            0x1 | 0x9 => out = a ^ b,
            0x2 | 0xA => (out, c, v) = (sub_ext as u32, carry(sub_ext), sub_v),
            0x3 => (out, c, v) = (rsb_ext as u32, carry(rsb_ext), rsb_v),
            0x4 | 0xB => (out, c, v) = (add_ext as u32, carry(add_ext), add_v),
            0x5 => {} // adc: not wired in the verilog
            0x6 => (out, c, v) = (sbc_ext as u32, carry(sbc_ext), sbc_v),
            0x7 => (out, c, v) = (rsc_ext as u32, carry(rsc_ext), rsc_v),
            0xC => out = a | b,
            0xD => out = b,
            0xE => out = a & !b,
            _ => out = !b,
        }
        let logical = matches!(op, 0x0 | 0x1 | 0x8 | 0x9 | 0xC | 0xD | 0xE | 0xF);
        let mut f = 0u8;
        if bit(out, 31) {
            f |= FLAG_N;
        }
        if out == 0 {
            f |= FLAG_Z;
        }
        if if logical { op2_carry } else { c } {
            f |= FLAG_C;
        }
        if if logical { self.v() } else { v } {
            f |= FLAG_V;
        }
        (out, f)
    }

    fn is_compare(&self) -> bool {
        matches!((self.instr >> 21) & 0xF, 0x8 | 0x9 | 0xA | 0xB)
    }

    fn ls_ea(&self) -> u32 {
        let base = self.reg_or_pc(self.rn());
        let off = self.instr & 0xFFF;
        if bit(self.instr, 23) { base.wrapping_add(off) } else { base.wrapping_sub(off) }
    }

    fn blk_bytes(&self) -> u32 {
        (self.instr & 0xFFFF).count_ones() * 4
    }

    fn blk_idx(&self) -> usize {
        // lowest set bit of the pending mask (0 if empty)
        if self.blk_mask == 0 { 0 } else { self.blk_mask.trailing_zeros() as usize }
    }

    // ----- combinational bus outputs -----

    pub fn bus(&self) -> BusReq {
        let mut r = BusReq { addr: self.pc, ..Default::default() };
        if self.halted {
            return r;
        }
        match self.phase {
            Phase::Exec => {
                if self.is_ls() && self.cond_pass() {
                    let byte = bit(self.instr, 22);
                    let rd = self.rd();
                    let src = if rd == 15 { self.pc.wrapping_add(8) } else { self.regs[rd] };
                    r.addr = self.ls_ea();
                    r.data_out = if byte { src & 0xFF } else { src };
                    r.we = !bit(self.instr, 20);
                    r.byte = byte;
                }
            }
            Phase::Block => {
                if self.blk_mask != 0 {
                    let idx = self.blk_idx();
                    r.addr = self.blk_addr;
                    r.we = !bit(self.instr, 20);
                    r.data_out = if idx == 15 { self.pc.wrapping_add(8) } else { self.regs[idx] };
                }
            }
            Phase::Fetch => {}
        }
        r
    }

    // ----- clock edge -----

    pub fn edge(&mut self, data_in: u32) -> EdgeEvent {
        if self.halted {
            return EdgeEvent::None;
        }
        let old = self.clone();
        let mut ev = EdgeEvent::None;
        match old.phase {
            Phase::Fetch => {
                self.instr = data_in;
                self.pc = old.pc.wrapping_add(4);
                self.phase = Phase::Exec;
                ev = EdgeEvent::Fetched;
            }
            Phase::Exec => {
                self.phase = Phase::Fetch;
                ev = EdgeEvent::Executed;
                if old.cond_pass() {
                    let i = old.instr;
                    if old.is_svc() {
                        self.halted = true;
                        ev = EdgeEvent::Svc;
                    } else if old.is_bx() {
                        self.pc = old.reg_or_pc((i & 0xF) as usize) & 0xFFFF_FFFC;
                    } else if old.is_br() {
                        if bit(i, 24) {
                            self.regs[14] = old.pc;
                        }
                        let imm = (((i & 0xFF_FFFF) << 8) as i32 >> 6) as u32;
                        self.pc = old.pc.wrapping_add(4).wrapping_add(imm);
                    } else if old.is_blk() {
                        let list = (i & 0xFFFF) as u16;
                        if list != 0 {
                            self.blk_mask = list;
                            self.blk_addr = if bit(i, 20) { old.regs[13] } else { old.regs[13].wrapping_sub(old.blk_bytes()) };
                            self.phase = Phase::Block;
                            ev = EdgeEvent::BlockStart;
                        }
                    } else if old.is_ls() {
                        if bit(i, 20) {
                            let rdata = if bit(i, 22) { data_in & 0xFF } else { data_in };
                            let rd = old.rd();
                            if rd == 15 {
                                self.pc = rdata & 0xFFFF_FFFC;
                            } else {
                                self.regs[rd] = rdata;
                            }
                        }
                    } else if old.is_mul() {
                        let rm = old.reg_or_pc((i & 0xF) as usize);
                        let rs = old.reg_or_pc(((i >> 8) & 0xF) as usize);
                        let res = rm.wrapping_mul(rs);
                        self.regs[old.rn()] = res;
                        if bit(i, 20) {
                            let mut f = old.flags & (FLAG_C | FLAG_V);
                            if bit(res, 31) {
                                f |= FLAG_N;
                            }
                            if res == 0 {
                                f |= FLAG_Z;
                            }
                            self.flags = f;
                        }
                    } else if old.is_dp() {
                        let (out, f) = old.alu();
                        let rd = old.rd();
                        if !old.is_compare() {
                            if rd != 15 {
                                self.regs[rd] = out;
                            } else {
                                self.pc = out & 0xFFFF_FFFC;
                            }
                        }
                        if bit(i, 20) {
                            self.flags = f;
                        }
                    }
                }
            }
            Phase::Block => {
                if old.blk_mask == 0 {
                    self.phase = Phase::Fetch;
                } else {
                    let idx = old.blk_idx();
                    let load = bit(old.instr, 20);
                    if load {
                        if idx == 15 {
                            self.pc = data_in & 0xFFFF_FFFC;
                        } else {
                            self.regs[idx] = data_in;
                        }
                    }
                    let rest = old.blk_mask & !(1u16 << idx);
                    self.blk_mask = rest;
                    self.blk_addr = old.blk_addr.wrapping_add(4);
                    ev = EdgeEvent::BlockXfer;
                    if rest == 0 {
                        // a later nonblocking assignment to regs[13] wins
                        self.regs[13] = if load {
                            old.regs[13].wrapping_add(old.blk_bytes())
                        } else {
                            old.regs[13].wrapping_sub(old.blk_bytes())
                        };
                        self.phase = Phase::Fetch;
                        ev = EdgeEvent::BlockDone;
                    }
                }
            }
        }
        ev
    }
}
