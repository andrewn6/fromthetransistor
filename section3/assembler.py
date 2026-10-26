"""
two-pass arm assembler for the compiler's output

ARM assembler, two passed for compiler's output

First, I layout the sections (.text/.data), record label addresses, size, every instruction
and collect the literal-pool requests.

Second, I encode the instructions with resolved labels, emit the literal pool at the end of .text and record relocations for the linker to patchy

usage: uv run assembler.py input.s output.bin
"""
import sys, struct

# load address of the flat binary (matches linker.py DEFAULT_LOAD_ADDR)
BASE = 0x10000

# condition suffixes the compiler emits
COND = {"eq": 0x0, "ne": 0x1, "cs": 0x2, "lo": 0x3, "hi": 0x8,
        "ls": 0x9, "ge": 0xA, "lt": 0xB, "gt": 0xC, "le": 0xD, "al": 0xE}

# data processing opcodes
DP_OPS = {"and": 0x0, "eor": 0x1, "sub": 0x2, "rsb": 0x3, "add": 0x4,
          "sbc": 0x6, "rsc": 0x7, "tst": 0x8, "teq": 0x9,
          "cmp": 0xA, "cmn": 0xB, "orr": 0xC, "mov": 0xD, "bic": 0xE, "mvn": 0xF}

# register names -> numbers
REGS = {"r0":0,"r1":1,"r2":2,"r3":3,"r4":4,"r5":5,"r6":6,"r7":7,
        "r8":8,"r9":9,"r10":10,"fp":11,"ip":12,"sp":13,"lr":14,"pc":15,
        "r11":11,"r12":12,"r13":13,"r14":14,"r15":15}

R_ARM_ABS32, R_ARM_CALL = 2, 28

def reg(tok):
    tok = tok.strip().lower()
    if tok not in REGS:
        raise ValueError(f"asm: bad register {tok!r}")
    return REGS[tok]

def parse_reglist(s):
    # '{fp, lr}' or '{r0-r3, r5}' -> bitmask
    body = s.strip().strip("{}")
    mask = 0
    for part in body.split(","):
        part = part.strip()
        if "-" in part:
            lo, hi = part.split("-")
            for n in range(reg(lo), reg(hi) + 1):
                mask |= 1 << n
        else:
            mask |= 1 << reg(part)
    return mask

def parse_string(s):
    # '"hi\n"' -> bytes with escapes resolved
    body = s.strip()
    assert body[0] == '"' and body[-1] == '"', f"asm: bad string {s!r}"
    out = bytearray()
    i = 1
    while i < len(body) - 1:
        c = body[i]
        if c == "\\":
            nxt = body[i + 1]
            out.append({"n":10,"t":9,"0":0,"\\":92,'"':34,"'":39}.get(nxt, ord(nxt)))
            i += 2
        else:
            out.append(ord(c))
            i += 1
    return bytes(out)

class Asm:
    def __init__(self):
        self.sections = {".text": bytearray(), ".data": bytearray()}
        self.symbols = {}       # name -> (section, offset)
        self.relocs = []        # (section, offset, type, symbol, addend)
        self.pool = []          # literal pool entries: raw int or symbol name
        self.pool_at = {}       # insn index -> pool entry index
        self.cur = ".text"
        self.pending = []       # (section, offset, (mnemonic, cond, operands))

    # ----- pass 1 -----

    def size(self):
        return len(self.sections[self.cur])

    def pass1(self, src):
        for raw in src.splitlines():
            line = raw.split("@")[0].strip()
            if not line:
                continue
            # label (may be followed by an instruction on the same line)
            while line and line.split()[0].endswith(":"):
                name = line.split()[0][:-1]
                self.symbols[name] = (self.cur, self.size())
                rest = line[len(name) + 1:].strip()
                line = rest
            if not line:
                continue
            if line.startswith("."):
                self.directive(line)
            else:
                self.instruction_pass1(line)

    def directive(self, line):
        parts = line.split(None, 1)
        d = parts[0]
        rest = parts[1] if len(parts) > 1 else ""
        sec = self.sections[self.cur]
        if d == ".text" or d == ".data":
            self.cur = d
        elif d in (".global", ".globl"):
            pass                                  # linker concern, ignore here
        elif d == ".word":
            while len(sec) % 4:
                sec.append(0)                     # keep words aligned
            for v in rest.split(","):
                v = v.strip()
                try:
                    sec += struct.pack("<I", int(v, 0) & 0xFFFFFFFF)
                except ValueError:
                    # symbol reference in data: needs an ABS32 reloc
                    self.relocs.append((self.cur, len(sec), R_ARM_ABS32, v, 0))
                    sec += b"\x00\x00\x00\x00"
        elif d == ".byte":
            for v in rest.split(","):
                sec.append(int(v, 0) & 0xFF)
        elif d == ".zero":
            sec += b"\x00" * int(rest, 0)
        elif d in (".asciz", ".ascii"):
            data = parse_string(rest)
            sec += data
            if d == ".asciz":
                sec.append(0)
        else:
            raise ValueError(f"asm: unknown directive {d}")

    def instruction_pass1(self, line):
        # split 'mnemonic[cond] operands'
        m = line.split(None, 1)
        mnem = m[0].lower()
        ops = m[1] if len(m) > 1 else ""
        cond = "al"
        for suf in COND:
            if suf != "al" and mnem.endswith(suf) and mnem[: -len(suf)] in (
                    "b", "mov", "add", "sub", "rsb", "mul", "and", "eor",
                    "orr", "bic", "cmp", "cmn", "tst", "ldr", "str"):
                mnem = mnem[: -len(suf)]
                cond = suf
                break
        if mnem == "ldr" and "=" in ops:
            # pseudo: ldr rd, =value/symbol -> pool entry
            rd, val = [x.strip() for x in ops.split("=", 1)]
            rd = rd.rstrip(",")
            self.pool_at[len(self.pending)] = len(self.pool)
            self.pool.append(val.strip())
            self.pending.append((self.cur, self.size(), ("LDR=", cond, rd)))
        else:
            self.pending.append((self.cur, self.size(), (mnem, cond, ops)))
        self.sections[self.cur] += b"\x00\x00\x00\x00"   # reserve the word

    # ----- pass 2 -----

    def pass2(self):
        text = self.sections[".text"]
        pool_base = len(text)
        # literal pool lives at the end of .text
        text += b"\x00" * (4 * len(self.pool))

        for idx, (secname, off, (mnem, cond, ops)) in enumerate(self.pending):
            word = self.encode(mnem, cond, ops, secname, off,
                               pool_base if secname == ".text" else None, idx)
            struct.pack_into("<I", self.sections[secname], off, word)

        # emit the pool itself
        for i, val in enumerate(self.pool):
            off = pool_base + 4 * i
            try:
                struct.pack_into("<I", text, off, int(val, 0) & 0xFFFFFFFF)
            except ValueError:
                self.relocs.append((".text", off, R_ARM_ABS32, val, 0))

    def sym_addr(self, name):
        if name not in self.symbols:
            raise ValueError(f"asm: undefined symbol {name!r}")
        sec, off = self.symbols[name]
        return BASE + (0 if sec == ".text" else len(self.sections[".text"])) + off

    def encode(self, mnem, cond, ops, secname, at, pool_base, idx):
        c = COND[cond]

        if mnem == "LDR=":
            rd = reg(ops)
            entry = pool_base + 4 * self.pool_at[idx]
            off12 = entry - (at + 8)               # pc reads as addr+8
            if not (0 <= off12 <= 4095):
                raise ValueError("asm: literal pool out of ldr range")
            return (c << 28) | 0x059F0000 | (rd << 12) | off12   # ldr rd,[pc,#off]

        if mnem in ("ldr", "str", "ldrb", "strb"):
            rd, mem = [x.strip() for x in ops.split(",", 1)]
            byte = 1 if mnem.endswith("b") else 0
            load = 1 if mnem.startswith("ldr") else 0
            mem = mem.strip()[1:-1]                # strip [ ]
            if "," in mem:
                rn, imm = [x.strip() for x in mem.split(",", 1)]
                imm = int(imm.replace("#", ""), 0)
            else:
                rn, imm = mem, 0
            u = 1 if imm >= 0 else 0
            return ((c << 28) | 0x05000000 | (1 << 24) | (u << 23) |
                    (byte << 22) | (load << 20) | (reg(rn) << 16) |
                    (reg(rd) << 12) | abs(imm))

        if mnem == "push":
            return 0xE92D0000 | parse_reglist(ops)         # stmdb sp!, {...}
        if mnem == "pop":
            return 0xE8BD0000 | parse_reglist(ops)         # ldmia sp!, {...}

        if mnem in ("b", "bl"):
            target = ops.strip() # remove leading white space asm
            base_word = 0x0A000000 if mnem == "b" else 0x0B000000
            try:
                taddr = int(target, 0)
            except ValueError:
                self.relocs.append((secname, at, R_ARM_CALL, target, 0))
                return (c << 28) | base_word | 0xFFFFFE 
            delta = taddr - (BASE + at + 8)
            if delta & 3:
                raise ValueError("asm: branch target not word-aligned")
            return (c << 28) | base_word | ((delta >> 2) & 0XFFFFFF)


        if mnem == "bx":
            return 0xE12FFF10 | reg(ops.strip())

        if mnem == "svc":
            imm = int(ops.replace("#", ""), 0) & 0xFFFFFF
            return (c << 28) | 0x0F000000 | imm

        if mnem == "mul":
            rd, rm, rs = [reg(x) for x in ops.split(",")]
            return (c << 28) | (rd << 16) | (rs << 8) | 0x90 | rm

        if mnem in ("lsl", "lsr"):
            # gas encodes 'lsl rd, rm, #n' as mov rd, rm, <shift>
            rd, rm, imm = [x.strip() for x in ops.split(",")]
            imm = int(imm.replace("#", ""), 0)
            sh = 0 if mnem == "lsl" else 1          # lsl=00, lsr=01
            if not (0 <= imm <= 31):
                raise ValueError("asm: bad shift amount")
            return ((c << 28) | (DP_OPS["mov"] << 21) |
                    (reg(rd) << 12) | (imm << 7) | (sh << 5) | reg(rm))

        # data processing: op rd, rn, operand2   (mov/cmp take one operand)
        o = mnem
        if o not in DP_OPS:
            raise ValueError(f"asm: unknown instruction {mnem!r}")
        opcode = DP_OPS[o]
        parts = [x.strip() for x in ops.split(",")]
        s_bit = 1 if o in ("cmp", "cmn", "tst", "teq") else 0
        if o in ("mov", "mvn"):
            rd, op2 = parts[0], parts[1]
            rn = 0
        elif o in ("cmp", "cmn", "tst", "teq"):
            rd, op2 = 0, parts[1]
            rn = reg(parts[0])
        else:
            rd, rn, op2 = parts[0], parts[1], parts[2]
            rd, rn = reg(rd), reg(rn)
        if o in ("mov", "mvn"):
            rd = reg(rd)
        # negative immediates flip add<->sub the way gas does
        if op2.startswith("#"):
            imm = int(op2[1:], 0)
            if imm < 0:
                if o == "add":
                    o, opcode, imm = "sub", DP_OPS["sub"], -imm
                elif o == "sub":
                    o, opcode, imm = "add", DP_OPS["add"], -imm
                else:
                    raise ValueError(f"asm: negative imm not allowed for {o}")
            if imm > 255:
                raise ValueError(f"asm: imm {imm} not encodable (use ldr =)")
            operand2 = imm                      # I=1, rotate=0
            i_bit = 1
        else:
            operand2 = reg(op2)                 # I=0, no shift
            i_bit = 0
        return ((c << 28) | (i_bit << 25) | (opcode << 21) | (s_bit << 20) |
                (rn << 16) | (rd << 12) | operand2)

    # ----- output -----

    def flat_binary(self):
        # .text then .data at BASE; apply relocations so the image is
        # position-fixed and ready to run/flash
        img = bytearray(self.sections[".text"]) + bytearray(self.sections[".data"])
        data_start = len(self.sections[".text"])
        for (secname, off, rtype, sym, addend) in self.relocs:
            s_addr = self.sym_addr(sym)
            if secname == ".data":
                off += data_start
            if rtype == R_ARM_ABS32:
                old = struct.unpack_from("<I", img, off)[0]
                struct.pack_into("<I", img, off, (old + s_addr + addend) & 0xFFFFFFFF)
            elif rtype == R_ARM_CALL:
                instr = struct.unpack_from("<I", img, off)[0]
                delta = s_addr - (BASE + off + 8)
                if delta & 3:
                    raise ValueError("asm: bl target not aligned")
                struct.pack_into("<I", img, off,
                                 (instr & 0xFF000000) | ((delta >> 2) & 0xFFFFFF))
        return bytes(img), self.symbols

    def object(self):
        # raw sections + symbols + relocs, for the linker to place and patch
        return (bytes(self.sections[".text"]), bytes(self.sections[".data"]),
                self.symbols, self.relocs)

def assemble_file(src_path, bin_path):
    with open(src_path) as f:
        src = f.read()
    a = Asm()
    a.pass1(src)
    a.pass2()
    img, symbols = a.flat_binary()
    with open(bin_path, "wb") as f:
        f.write(img)
    print(f"assembled {src_path} -> {bin_path} ({len(img)} bytes, "
          f"{len(symbols)} symbols)")
    return a

if __name__ == "__main__":
    if len(sys.argv) != 3:
        print("usage: python assembler.py input.s output.bin", file=sys.stderr)
        sys.exit(1)
    assemble_file(sys.argv[1], sys.argv[2])
