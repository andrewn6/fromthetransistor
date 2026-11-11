// port of section3/assembler.py: two-pass arm assembler for the compiler's
// output. pass 1 lays out .text/.data, records labels and reserves a word
// per instruction plus literal pool space; pass 2 encodes with resolved
// labels and records relocations for the linker.
//
// on top of the python version it keeps a `listing` (which source line
// produced which bytes) so the playground can draw the mapping.

use crate::py::{self, int0, repr, split_ws1, value_error, OrderedMap, Result};

// load address of the flat binary (matches linker.py DEFAULT_LOAD_ADDR)
pub const BASE: i128 = 0x10000;

// condition suffixes the compiler emits, in python dict order
pub const COND: [(&str, u32); 11] = [
    ("eq", 0x0), ("ne", 0x1), ("cs", 0x2), ("lo", 0x3), ("hi", 0x8),
    ("ls", 0x9), ("ge", 0xA), ("lt", 0xB), ("gt", 0xC), ("le", 0xD), ("al", 0xE),
];

pub const DP_OPS: [(&str, u32); 15] = [
    ("and", 0x0), ("eor", 0x1), ("sub", 0x2), ("rsb", 0x3), ("add", 0x4),
    ("sbc", 0x6), ("rsc", 0x7), ("tst", 0x8), ("teq", 0x9),
    ("cmp", 0xA), ("cmn", 0xB), ("orr", 0xC), ("mov", 0xD), ("bic", 0xE), ("mvn", 0xF),
];

// mnemonics that take a condition suffix
const COND_BASES: [&str; 15] = [
    "b", "mov", "add", "sub", "rsb", "mul", "and", "eor",
    "orr", "bic", "cmp", "cmn", "tst", "ldr", "str",
];

pub const R_ARM_ABS32: u32 = 2;
pub const R_ARM_CALL: u32 = 28;

fn cond_code(c: &str) -> u32 {
    COND.iter().find(|(k, _)| *k == c).map(|(_, v)| *v).unwrap()
}

fn dp_op(o: &str) -> Option<u32> {
    DP_OPS.iter().find(|(k, _)| *k == o).map(|(_, v)| *v)
}

pub fn reg(tok: &str) -> Result<u32> {
    let t = tok.trim().to_lowercase();
    let n = match t.as_str() {
        "r0" => 0, "r1" => 1, "r2" => 2, "r3" => 3, "r4" => 4, "r5" => 5, "r6" => 6, "r7" => 7,
        "r8" => 8, "r9" => 9, "r10" => 10, "fp" => 11, "ip" => 12, "sp" => 13, "lr" => 14, "pc" => 15,
        "r11" => 11, "r12" => 12, "r13" => 13, "r14" => 14, "r15" => 15,
        _ => return Err(value_error(format!("asm: bad register {}", repr(&t)))),
    };
    Ok(n)
}

pub fn parse_reglist(s: &str) -> Result<u32> {
    // '{fp, lr}' or '{r0-r3, r5}' -> bitmask
    let body = s.trim().trim_matches(|c| c == '{' || c == '}');
    let mut mask = 0u32;
    for part in body.split(',') {
        let part = part.trim();
        if part.contains('-') {
            let pieces: Vec<&str> = part.split('-').collect();
            if pieces.len() != 2 {
                return Err(value_error(format!("too many values to unpack (expected 2, got {})", pieces.len())));
            }
            for n in reg(pieces[0])?..reg(pieces[1])? + 1 {
                mask |= 1 << n;
            }
        } else {
            mask |= 1 << reg(part)?;
        }
    }
    Ok(mask)
}

pub fn parse_string(s: &str) -> Result<Vec<u8>> {
    // '"hi\n"' -> bytes with escapes resolved
    let body: Vec<char> = s.trim().chars().collect();
    if body.is_empty() {
        return Err(py::index_error());
    }
    if body[0] != '"' || body[body.len() - 1] != '"' {
        return Err(py::Error::new("AssertionError", format!("asm: bad string {}", repr(s))));
    }
    let mut out = Vec::new();
    let mut i = 1;
    let push = |out: &mut Vec<u8>, c: u32| -> Result<()> {
        if c > 255 {
            return Err(value_error("byte must be in range(0, 256)"));
        }
        out.push(c as u8);
        Ok(())
    };
    while i + 1 < body.len() {
        let c = body[i];
        if c == '\\' {
            let nxt = body[i + 1];
            let v = match nxt {
                'n' => 10, 't' => 9, '0' => 0, '\\' => 92, '"' => 34, '\'' => 39,
                c => c as u32,
            };
            push(&mut out, v)?;
            i += 2;
        } else {
            push(&mut out, c as u32)?;
            i += 1;
        }
    }
    Ok(out)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sec {
    Text,
    Data,
}

impl Sec {
    pub fn name(self) -> &'static str {
        match self {
            Sec::Text => ".text",
            Sec::Data => ".data",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Reloc {
    pub sec: Sec,
    pub off: usize,
    pub rtype: u32,
    pub sym: String,
    pub addend: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Label,
    Directive,
    Insn,
    Pool,
}

// one source line's footprint in the output, for the playground listing
#[derive(Clone, Debug)]
pub struct ListingEntry {
    pub line: usize,
    pub kind: LineKind,
    pub sec: Sec,
    pub off: usize,
    pub len: usize,
}

#[derive(Clone, Debug)]
struct Pending {
    sec: Sec,
    off: usize,
    mnem: String,
    cond: String,
    ops: String,
}

#[derive(Default, Clone, Debug)]
pub struct Asm {
    pub text: Vec<u8>,
    pub data: Vec<u8>,
    pub symbols: OrderedMap<(Sec, usize)>,
    pub relocs: Vec<Reloc>,
    pub pool: Vec<String>,
    pool_at: std::collections::HashMap<usize, usize>,
    pub chunks: Vec<(usize, usize, usize)>, // (text offset, first entry, count)
    chunk_first: usize,
    cur: Option<Sec>,
    pending: Vec<Pending>,
    pub listing: Vec<ListingEntry>,
}

impl Asm {
    pub fn new() -> Self {
        Asm { cur: Some(Sec::Text), ..Default::default() }
    }

    fn cur(&self) -> Sec {
        self.cur.unwrap_or(Sec::Text)
    }

    fn sec_mut(&mut self, s: Sec) -> &mut Vec<u8> {
        match s {
            Sec::Text => &mut self.text,
            Sec::Data => &mut self.data,
        }
    }

    fn size(&self) -> usize {
        match self.cur() {
            Sec::Text => self.text.len(),
            Sec::Data => self.data.len(),
        }
    }

    // ----- pass 1 -----

    pub fn pass1(&mut self, src: &str) -> Result<()> {
        for (lineno, raw) in py::splitlines(src).into_iter().enumerate() {
            let mut line: String = raw.split('@').next().unwrap_or("").trim().to_string();
            if line.is_empty() {
                continue;
            }
            // label (may be followed by an instruction on the same line)
            loop {
                let first = match line.split_whitespace().next() {
                    Some(w) if w.ends_with(':') => w.to_string(),
                    _ => break,
                };
                let name = &first[..first.len() - 1];
                let at = (self.cur(), self.size());
                self.symbols.insert(name, at);
                self.listing.push(ListingEntry { line: lineno, kind: LineKind::Label, sec: at.0, off: at.1, len: 0 });
                let skip = name.chars().count() + 1;
                line = line.chars().skip(skip).collect::<String>().trim().to_string();
                if line.is_empty() {
                    break;
                }
            }
            if line.is_empty() {
                continue;
            }
            let (sec, before) = (self.cur(), self.size());
            let kind = if line.starts_with('.') {
                self.directive(&line)?;
                if line.starts_with(".pool") { LineKind::Pool } else { LineKind::Directive }
            } else {
                self.instruction_pass1(&line)?;
                LineKind::Insn
            };
            // a section switch changes cur; report the bytes the line added
            let len = if self.cur() == sec { self.size() - before } else { 0 };
            self.listing.push(ListingEntry { line: lineno, kind, sec, off: before, len });
        }
        Ok(())
    }

    fn directive(&mut self, line: &str) -> Result<()> {
        let (d, rest) = split_ws1(line);
        let cur = self.cur();
        match d {
            ".text" => self.cur = Some(Sec::Text),
            ".data" => self.cur = Some(Sec::Data),
            ".global" | ".globl" => {} // linker concern, ignore here
            ".word" => {
                let sec = self.sec_mut(cur);
                while sec.len() % 4 != 0 {
                    sec.push(0); // keep words aligned
                }
                for v in rest.split(',') {
                    let v = v.trim();
                    match int0(v) {
                        Ok(n) => {
                            let w = (n & 0xFFFF_FFFF) as u32;
                            self.sec_mut(cur).extend_from_slice(&w.to_le_bytes());
                        }
                        Err(_) => {
                            // symbol reference in data: needs an ABS32 reloc
                            let off = self.sec_mut(cur).len();
                            self.relocs.push(Reloc { sec: cur, off, rtype: R_ARM_ABS32, sym: v.to_string(), addend: 0 });
                            self.sec_mut(cur).extend_from_slice(&[0, 0, 0, 0]);
                        }
                    }
                }
            }
            ".byte" => {
                for v in rest.split(',') {
                    let n = int0(v)?;
                    self.sec_mut(cur).push((n & 0xFF) as u8);
                }
            }
            ".zero" => {
                let n = int0(rest)?.max(0) as usize;
                self.sec_mut(cur).extend(std::iter::repeat(0).take(n));
            }
            ".asciz" | ".ascii" => {
                let data = parse_string(rest)?;
                let sec = self.sec_mut(cur);
                sec.extend_from_slice(&data);
                if d == ".asciz" {
                    sec.push(0);
                }
            }
            ".pool" => {
                // flush pending literals inline: reserve their space here so
                // each entry stays within ldr range of the code that uses it
                if cur == Sec::Text {
                    let n = self.pool.len() - self.chunk_first;
                    let base = self.text.len();
                    self.text.extend(std::iter::repeat(0).take(4 * n));
                    self.chunks.push((base, self.chunk_first, n));
                    self.chunk_first = self.pool.len();
                }
            }
            _ => return Err(value_error(format!("asm: unknown directive {}", d))),
        }
        Ok(())
    }

    fn instruction_pass1(&mut self, line: &str) -> Result<()> {
        // split 'mnemonic[cond] operands'
        let (m, ops) = split_ws1(line);
        let mut mnem = m.to_lowercase();
        let mut cond = "al".to_string();
        for (suf, _) in COND.iter() {
            if *suf != "al" && mnem.ends_with(suf) && COND_BASES.contains(&&mnem[..mnem.len() - suf.len()]) {
                mnem.truncate(mnem.len() - suf.len());
                cond = suf.to_string();
                break;
            }
        }
        let (sec, off) = (self.cur(), self.size());
        if mnem == "ldr" && ops.contains('=') {
            // pseudo: ldr rd, =value/symbol -> pool entry
            let (rd, val) = ops.split_once('=').unwrap();
            let rd = rd.trim().trim_end_matches(',');
            self.pool_at.insert(self.pending.len(), self.pool.len());
            self.pool.push(val.trim().to_string());
            self.pending.push(Pending { sec, off, mnem: "LDR=".into(), cond, ops: rd.to_string() });
        } else {
            self.pending.push(Pending { sec, off, mnem, cond, ops: ops.to_string() });
        }
        self.sec_mut(sec).extend_from_slice(&[0, 0, 0, 0]); // reserve the word
        Ok(())
    }

    // ----- pass 2 -----

    pub fn pass2(&mut self) -> Result<()> {
        // literals still pending (source had no trailing .pool) flush at the
        // very end, matching the old single-pool behavior
        if self.pool.len() > self.chunk_first {
            let n = self.pool.len() - self.chunk_first;
            let base = self.text.len();
            self.text.extend(std::iter::repeat(0).take(4 * n));
            self.chunks.push((base, self.chunk_first, n));
            self.chunk_first = self.pool.len();
        }

        let pending = std::mem::take(&mut self.pending);
        for (idx, p) in pending.iter().enumerate() {
            let word = self.encode(&p.mnem, &p.cond, &p.ops, p.sec, p.off, idx)?;
            self.sec_mut(p.sec)[p.off..p.off + 4].copy_from_slice(&word.to_le_bytes());
        }
        self.pending = pending;

        // emit the pool chunks themselves into the space reserved in pass 1
        for &(base, first, n) in &self.chunks.clone() {
            for k in 0..n {
                let off = base + 4 * k;
                let val = self.pool[first + k].clone();
                match int0(&val) {
                    Ok(v) => {
                        let w = (v & 0xFFFF_FFFF) as u32;
                        self.text[off..off + 4].copy_from_slice(&w.to_le_bytes());
                    }
                    Err(_) => self.relocs.push(Reloc { sec: Sec::Text, off, rtype: R_ARM_ABS32, sym: val, addend: 0 }),
                }
            }
        }
        Ok(())
    }

    pub fn sym_addr(&self, name: &str) -> Result<i128> {
        let (sec, off) = self
            .symbols
            .get(name)
            .ok_or_else(|| value_error(format!("asm: undefined symbol {}", repr(name))))?;
        let base = if *sec == Sec::Text { 0 } else { self.text.len() };
        Ok(BASE + (base + off) as i128)
    }

    fn encode(&mut self, mnem: &str, cond: &str, ops: &str, secname: Sec, at: usize, idx: usize) -> Result<u32> {
        let c = cond_code(cond) as i128;

        if mnem == "LDR=" {
            let rd = reg(ops)? as i128;
            let i = self.pool_at[&idx];
            // find the flushed pool chunk holding this entry
            let entry = self
                .chunks
                .iter()
                .find(|&&(_, first, n)| first <= i && i < first + n)
                .map(|&(base, first, _)| base + 4 * (i - first))
                .ok_or_else(|| value_error("asm: literal not flushed into any pool"))?;
            let off12 = entry as i128 - (at as i128 + 8); // pc reads as addr+8
            if !(0..=4095).contains(&off12) {
                return Err(value_error("asm: literal pool out of ldr range"));
            }
            return py::u32_checked((c << 28) | 0x059F0000 | (rd << 12) | off12);
        }

        if matches!(mnem, "ldr" | "str" | "ldrb" | "strb") {
            let (rd, mem) = ops
                .split_once(',')
                .ok_or_else(|| value_error("not enough values to unpack (expected 2, got 1)"))?;
            let byte = if mnem.ends_with('b') { 1 } else { 0 };
            let load = if mnem.starts_with("ldr") { 1 } else { 0 };
            let mem: Vec<char> = mem.trim().chars().collect();
            let mem: String = if mem.len() >= 2 { mem[1..mem.len() - 1].iter().collect() } else { String::new() };
            let (rn, imm) = match mem.split_once(',') {
                Some((rn, imm)) => (rn.trim().to_string(), int0(&imm.trim().replace('#', ""))?),
                None => (mem.clone(), 0),
            };
            let u = if imm >= 0 { 1 } else { 0 };
            return py::u32_checked(
                (c << 28) | 0x05000000 | (1 << 24) | (u << 23) | (byte << 22) | (load << 20)
                    | ((reg(&rn)? as i128) << 16) | ((reg(rd)? as i128) << 12) | imm.abs(),
            );
        }

        if mnem == "push" {
            return Ok(0xE92D0000 | parse_reglist(ops)?); // stmdb sp!, {...}
        }
        if mnem == "pop" {
            return Ok(0xE8BD0000 | parse_reglist(ops)?); // ldmia sp!, {...}
        }

        if mnem == "b" || mnem == "bl" {
            let target = ops.trim();
            let base_word: i128 = if mnem == "b" { 0x0A000000 } else { 0x0B000000 };
            let taddr = match int0(target) {
                Ok(t) => t,
                Err(_) => {
                    self.relocs.push(Reloc { sec: secname, off: at, rtype: R_ARM_CALL, sym: target.to_string(), addend: 0 });
                    return py::u32_checked((c << 28) | base_word | 0xFFFFFE);
                }
            };
            let delta = taddr - (BASE + at as i128 + 8);
            if delta & 3 != 0 {
                return Err(value_error("asm: branch target not word-aligned"));
            }
            return py::u32_checked((c << 28) | base_word | ((delta >> 2) & 0xFFFFFF));
        }

        if mnem == "bx" {
            return Ok(0xE12FFF10 | reg(ops.trim())?);
        }

        if mnem == "svc" {
            let imm = int0(&ops.replace('#', ""))? & 0xFFFFFF;
            return py::u32_checked((c << 28) | 0x0F000000 | imm);
        }

        if mnem == "mul" {
            let regs: Vec<&str> = ops.split(',').collect();
            if regs.len() != 3 {
                return Err(value_error(if regs.len() < 3 {
                    format!("not enough values to unpack (expected 3, got {})", regs.len())
                } else {
                    format!("too many values to unpack (expected 3, got {})", regs.len())
                }));
            }
            let (rd, rm, rs) = (reg(regs[0])? as i128, reg(regs[1])? as i128, reg(regs[2])? as i128);
            return py::u32_checked((c << 28) | (rd << 16) | (rs << 8) | 0x90 | rm);
        }

        if matches!(mnem, "lsl" | "lsr" | "asr") {
            // both forms encode as mov rd, rm, <shift>:
            //   'lsl rd, rm, #imm'  -> shift by imm5 (operand2 bit 4 = 0)
            //   'lsl rd, rm, rs'    -> shift by register (bit 4 = 1)
            let parts: Vec<&str> = ops.split(',').map(|x| x.trim()).collect();
            let sh: i128 = match mnem { "lsl" => 0, "lsr" => 1, _ => 2 };
            if parts.len() != 3 {
                return Err(value_error(format!("asm: bad shift operands {}", repr(ops))));
            }
            let (rd, rm, third) = (parts[0], parts[1], parts[2]);
            let mov = dp_op("mov").unwrap() as i128;
            if third.starts_with('#') {
                let imm = int0(&third.replace('#', ""))?;
                if !(0..=31).contains(&imm) {
                    return Err(value_error("asm: bad shift amount"));
                }
                return py::u32_checked(
                    (c << 28) | (mov << 21) | ((reg(rd)? as i128) << 12) | (imm << 7) | (sh << 5) | reg(rm)? as i128,
                );
            }
            return py::u32_checked(
                (c << 28) | (mov << 21) | ((reg(rd)? as i128) << 12) | ((reg(third)? as i128) << 8)
                    | (sh << 5) | (1 << 4) | reg(rm)? as i128,
            );
        }

        // data processing: op rd, rn, operand2   (mov/cmp take one operand)
        let o = mnem;
        let mut opcode = dp_op(o).ok_or_else(|| value_error(format!("asm: unknown instruction {}", repr(mnem))))? as i128;
        let parts: Vec<&str> = ops.split(',').map(|x| x.trim()).collect();
        let part = |i: usize| parts.get(i).copied().ok_or_else(|| py::Error::new("IndexError", "list index out of range"));
        let is_cmp = matches!(o, "cmp" | "cmn" | "tst" | "teq");
        let s_bit: i128 = if is_cmp { 1 } else { 0 };
        let (rd, rn, op2): (i128, i128, &str) = if o == "mov" || o == "mvn" {
            let (rd, op2) = (part(0)?, part(1)?);
            (reg(rd)? as i128, 0, op2)
        } else if is_cmp {
            let op2 = part(1)?;
            (0, reg(part(0)?)? as i128, op2)
        } else {
            let (rd, rn, op2) = (part(0)?, part(1)?, part(2)?);
            (reg(rd)? as i128, reg(rn)? as i128, op2)
        };
        // negative immediates flip add<->sub the way gas does
        let (operand2, i_bit) = if let Some(imm_s) = op2.strip_prefix('#') {
            let mut imm = int0(imm_s)?;
            if imm < 0 {
                if o == "add" {
                    opcode = dp_op("sub").unwrap() as i128;
                    imm = -imm;
                } else if o == "sub" {
                    opcode = dp_op("add").unwrap() as i128;
                    imm = -imm;
                } else {
                    return Err(value_error(format!("asm: negative imm not allowed for {}", o)));
                }
            }
            if imm > 255 {
                return Err(value_error(format!("asm: imm {} not encodable (use ldr =)", imm)));
            }
            (imm, 1) // I=1, rotate=0
        } else {
            (reg(op2)? as i128, 0) // I=0, no shift
        };
        py::u32_checked((c << 28) | (i_bit << 25) | (opcode << 21) | (s_bit << 20) | (rn << 16) | (rd << 12) | operand2)
    }

    // ----- output -----

    pub fn flat_binary(&self) -> Result<Vec<u8>> {
        // .text then .data at BASE; apply relocations so the image is
        // position-fixed and ready to run/flash
        let mut img = self.text.clone();
        img.extend_from_slice(&self.data);
        let data_start = self.text.len();
        for r in &self.relocs {
            let s_addr = self.sym_addr(&r.sym)?;
            let off = if r.sec == Sec::Data { r.off + data_start } else { r.off };
            let old = u32::from_le_bytes(img[off..off + 4].try_into().unwrap());
            let new = if r.rtype == R_ARM_ABS32 {
                ((old as i128 + s_addr + r.addend as i128) & 0xFFFF_FFFF) as u32
            } else {
                let delta = s_addr - (BASE + off as i128 + 8);
                if delta & 3 != 0 {
                    return Err(value_error("asm: bl target not aligned"));
                }
                (old & 0xFF000000) | (((delta >> 2) & 0xFFFFFF) as u32)
            };
            img[off..off + 4].copy_from_slice(&new.to_le_bytes());
        }
        Ok(img)
    }
}

// assemble a whole source text (pass1 + pass2)
pub fn assemble(src: &str) -> Result<Asm> {
    let mut a = Asm::new();
    a.pass1(src)?;
    a.pass2()?;
    Ok(a)
}
