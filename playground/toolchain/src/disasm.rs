// disassembler + bitfield breakdown for the subset the cpu runs. decode
// order matches arm7_cpu.v (svc, bx, b/bl, ldm/stm, ldr/str, mul, dp).

const CONDS: [&str; 16] = ["eq", "ne", "cs", "lo", "mi", "pl", "vs", "vc", "hi", "ls", "ge", "lt", "gt", "le", "", "nv"];
const DP: [&str; 16] = ["and", "eor", "sub", "rsb", "add", "adc", "sbc", "rsc", "tst", "teq", "cmp", "cmn", "orr", "mov", "bic", "mvn"];

pub fn reg_name(r: u32) -> &'static str {
    ["r0", "r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9", "r10", "fp", "ip", "sp", "lr", "pc"][r as usize & 15]
}

fn reglist(mask: u32) -> String {
    let regs: Vec<&str> = (0..16).filter(|i| mask >> i & 1 == 1).map(reg_name).collect();
    format!("{{{}}}", regs.join(", "))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Svc,
    Bx,
    Branch,
    Block,
    LoadStore,
    Mul,
    DataProc,
    Unknown,
}

pub fn classify(i: u32) -> Class {
    let is_dp = (i >> 26) & 3 == 0;
    if (i >> 24) & 0xF == 0xF {
        Class::Svc
    } else if is_dp && (i >> 20) & 0xFF == 0x12 && (i >> 12) & 0xFF == 0xFF && (i >> 4) & 0xFF == 0xF1 {
        Class::Bx
    } else if (i >> 25) & 7 == 0b101 {
        Class::Branch
    } else if (i >> 25) & 7 == 0b100 {
        Class::Block
    } else if (i >> 26) & 3 == 1 {
        Class::LoadStore
    } else if is_dp && (i >> 25) & 1 == 0 && (i >> 21) & 0x7F == 0 && (i >> 12) & 0xF == 0 && (i >> 4) & 0xF == 9 {
        Class::Mul
    } else if is_dp {
        Class::DataProc
    } else {
        Class::Unknown
    }
}

// branch target of a b/bl at `addr`
pub fn branch_target(addr: u32, i: u32) -> u32 {
    let imm = (((i & 0xFF_FFFF) << 8) as i32 >> 6) as u32;
    addr.wrapping_add(8).wrapping_add(imm)
}

pub fn disasm(addr: u32, i: u32) -> String {
    let c = CONDS[(i >> 28) as usize];
    let rn = (i >> 16) & 0xF;
    let rd = (i >> 12) & 0xF;
    match classify(i) {
        Class::Svc => format!("svc{} 0x{:x}", c, i & 0xFF_FFFF),
        Class::Bx => format!("bx{} {}", c, reg_name(i & 0xF)),
        Class::Branch => {
            let l = if (i >> 24) & 1 == 1 { "l" } else { "" };
            format!("b{}{} 0x{:x}", l, c, branch_target(addr, i))
        }
        Class::Block => {
            let load = (i >> 20) & 1 == 1;
            if rn == 13 && load && (i >> 23) & 1 == 1 {
                format!("pop{} {}", c, reglist(i & 0xFFFF))
            } else if rn == 13 && !load && (i >> 24) & 1 == 1 {
                format!("push{} {}", c, reglist(i & 0xFFFF))
            } else {
                format!("{}{} {}!, {}", if load { "ldm" } else { "stm" }, c, reg_name(rn), reglist(i & 0xFFFF))
            }
        }
        Class::LoadStore => {
            let op = if (i >> 20) & 1 == 1 { "ldr" } else { "str" };
            let b = if (i >> 22) & 1 == 1 { "b" } else { "" };
            let off = i & 0xFFF;
            let sign = if (i >> 23) & 1 == 1 { "" } else { "-" };
            if off == 0 {
                format!("{}{}{} {}, [{}]", op, c, b, reg_name(rd), reg_name(rn))
            } else {
                format!("{}{}{} {}, [{}, #{}{}]", op, c, b, reg_name(rd), reg_name(rn), sign, off)
            }
        }
        Class::Mul => format!("mul{} {}, {}, {}", c, reg_name(rn), reg_name(i & 0xF), reg_name((i >> 8) & 0xF)),
        Class::DataProc => {
            let op = (i >> 21) & 0xF;
            let name = DP[op as usize];
            let s = if (i >> 20) & 1 == 1 && !(8..=11).contains(&op) { "s" } else { "" };
            let op2 = if (i >> 25) & 1 == 1 {
                format!("#{}", i & 0xFF)
            } else {
                let rm = reg_name(i & 0xF);
                let sh = ["lsl", "lsr", "asr", "ror"][((i >> 5) & 3) as usize];
                if (i >> 4) & 1 == 1 {
                    format!("{}, {} {}", rm, sh, reg_name((i >> 8) & 0xF))
                } else if (i >> 7) & 0x1F != 0 || (i >> 5) & 3 != 0 {
                    format!("{}, {} #{}", rm, sh, (i >> 7) & 0x1F)
                } else {
                    rm.to_string()
                }
            };
            // the assembler writes shifts as lsl/lsr/asr rd, rm, x
            if op == 0xD && (i >> 25) & 1 == 0 && (i & 0xFF0) != 0 {
                let sh = ["lsl", "lsr", "asr", "ror"][((i >> 5) & 3) as usize];
                let amt = if (i >> 4) & 1 == 1 { reg_name((i >> 8) & 0xF).to_string() } else { format!("#{}", (i >> 7) & 0x1F) };
                return format!("{}{} {}, {}, {}", sh, c, reg_name(rd), reg_name(i & 0xF), amt);
            }
            match op {
                0xD | 0xF => format!("{}{}{} {}, {}", name, c, s, reg_name(rd), op2),
                8..=11 => format!("{}{} {}, {}", name, c, reg_name(rn), op2),
                _ => format!("{}{}{} {}, {}, {}", name, c, s, reg_name(rd), reg_name(rn), op2),
            }
        }
        Class::Unknown => format!(".word 0x{:08x}", i),
    }
}

// one named bitfield of an instruction word, for the encoding diagram
#[derive(Clone, Debug)]
pub struct Field {
    pub name: &'static str,
    pub hi: u32,
    pub lo: u32,
    pub value: u32,
    pub meaning: String,
}

fn f(i: u32, name: &'static str, hi: u32, lo: u32, meaning: impl Into<String>) -> Field {
    let width = hi - lo + 1;
    let mask = if width == 32 { u32::MAX } else { (1 << width) - 1 };
    Field { name, hi, lo, value: (i >> lo) & mask, meaning: meaning.into() }
}

pub fn fields(i: u32) -> Vec<Field> {
    let cond = {
        let c = CONDS[(i >> 28) as usize];
        if c.is_empty() { "always".to_string() } else { c.to_string() }
    };
    let mut v = vec![f(i, "cond", 31, 28, cond)];
    match classify(i) {
        Class::Svc => {
            v.push(f(i, "1111", 27, 24, "svc"));
            v.push(f(i, "imm24", 23, 0, format!("0x{:x}", i & 0xFF_FFFF)));
        }
        Class::Bx => {
            v.push(f(i, "bx op", 27, 4, "branch and exchange"));
            v.push(f(i, "rm", 3, 0, reg_name(i & 0xF)));
        }
        Class::Branch => {
            v.push(f(i, "101", 27, 25, "branch"));
            v.push(f(i, "L", 24, 24, if (i >> 24) & 1 == 1 { "link: lr = pc" } else { "no link" }));
            let off = (((i & 0xFF_FFFF) << 8) as i32 >> 6) as i32;
            v.push(f(i, "offset24", 23, 0, format!("{:+} bytes from pc+8", off)));
        }
        Class::Block => {
            v.push(f(i, "100", 27, 25, "block transfer"));
            v.push(f(i, "P", 24, 24, if (i >> 24) & 1 == 1 { "pre" } else { "post" }));
            v.push(f(i, "U", 23, 23, if (i >> 23) & 1 == 1 { "up" } else { "down" }));
            v.push(f(i, "S", 22, 22, ""));
            v.push(f(i, "W", 21, 21, "writeback"));
            v.push(f(i, "L", 20, 20, if (i >> 20) & 1 == 1 { "load" } else { "store" }));
            v.push(f(i, "rn", 19, 16, reg_name((i >> 16) & 0xF)));
            v.push(f(i, "reglist", 15, 0, reglist(i & 0xFFFF)));
        }
        Class::LoadStore => {
            v.push(f(i, "01", 27, 26, "load/store"));
            v.push(f(i, "I", 25, 25, "immediate offset"));
            v.push(f(i, "P", 24, 24, "pre-index"));
            v.push(f(i, "U", 23, 23, if (i >> 23) & 1 == 1 { "add offset" } else { "subtract offset" }));
            v.push(f(i, "B", 22, 22, if (i >> 22) & 1 == 1 { "byte" } else { "word" }));
            v.push(f(i, "W", 21, 21, ""));
            v.push(f(i, "L", 20, 20, if (i >> 20) & 1 == 1 { "load" } else { "store" }));
            v.push(f(i, "rn", 19, 16, reg_name((i >> 16) & 0xF)));
            v.push(f(i, "rd", 15, 12, reg_name((i >> 12) & 0xF)));
            v.push(f(i, "offset12", 11, 0, format!("{}", i & 0xFFF)));
        }
        Class::Mul => {
            v.push(f(i, "000000", 27, 22, "multiply"));
            v.push(f(i, "A", 21, 21, ""));
            v.push(f(i, "S", 20, 20, ""));
            v.push(f(i, "rd", 19, 16, reg_name((i >> 16) & 0xF)));
            v.push(f(i, "rn", 15, 12, ""));
            v.push(f(i, "rs", 11, 8, reg_name((i >> 8) & 0xF)));
            v.push(f(i, "1001", 7, 4, ""));
            v.push(f(i, "rm", 3, 0, reg_name(i & 0xF)));
        }
        Class::DataProc => {
            let op = (i >> 21) & 0xF;
            v.push(f(i, "00", 27, 26, "data processing"));
            v.push(f(i, "I", 25, 25, if (i >> 25) & 1 == 1 { "immediate" } else { "register" }));
            v.push(f(i, "opcode", 24, 21, DP[op as usize]));
            v.push(f(i, "S", 20, 20, if (i >> 20) & 1 == 1 { "set flags" } else { "" }));
            v.push(f(i, "rn", 19, 16, reg_name((i >> 16) & 0xF)));
            v.push(f(i, "rd", 15, 12, reg_name((i >> 12) & 0xF)));
            if (i >> 25) & 1 == 1 {
                v.push(f(i, "rotate", 11, 8, "unused by the cpu"));
                v.push(f(i, "imm8", 7, 0, format!("#{}", i & 0xFF)));
            } else if (i >> 4) & 1 == 1 {
                v.push(f(i, "rs", 11, 8, format!("shift by {}", reg_name((i >> 8) & 0xF))));
                v.push(f(i, "0", 7, 7, ""));
                v.push(f(i, "type", 6, 5, ["lsl", "lsr", "asr", "ror"][((i >> 5) & 3) as usize]));
                v.push(f(i, "1", 4, 4, "register shift"));
                v.push(f(i, "rm", 3, 0, reg_name(i & 0xF)));
            } else {
                v.push(f(i, "shift", 11, 7, format!("#{}", (i >> 7) & 0x1F)));
                v.push(f(i, "type", 6, 5, ["lsl", "lsr", "asr", "ror"][((i >> 5) & 3) as usize]));
                v.push(f(i, "0", 4, 4, "immediate shift"));
                v.push(f(i, "rm", 3, 0, reg_name(i & 0xF)));
            }
        }
        Class::Unknown => v.push(f(i, "?", 27, 0, "not decoded by arm7_cpu.v")),
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_common_forms() {
        assert_eq!(disasm(0, 0xE92D4800), "push {fp, lr}");
        assert_eq!(disasm(0, 0xE8BD4800), "pop {fp, lr}");
        assert_eq!(disasm(0, 0xE12FFF1E), "bx lr");
        assert_eq!(disasm(0, 0xE3A00004), "mov r0, #4");
        assert_eq!(disasm(0, 0xE59F1004), "ldr r1, [pc, #4]");
        assert_eq!(disasm(0, 0xEAFFFFFE), "b 0x0");
    }
}
