"""
The linker
"""
from __future__ import annotations 
import sys, os, struct 

# ELF 32 header  constants 
ELFCLASS32 = 1 
ELFDATA2LSB = 1
ET_EXEC = 2 
EM_ARM = 40 
EF_ARM_EABI_VER5 = 0x05000000

# Segments, kernel loads these segments into memory when running executable 
PT_LOAD = 1
PF_X, PF_W, PF_r = 1, 2, 4 

# Section header used at link time 
SHT_NULL, SHT_PROGBITS, SHT_SYMTAB, SHT_STRTAB, SHT_NOBITS = 0, 1, 2, 3, 8 
SHF_WRITE, SHF_ALLOC, SHF_EXECINSTR = 0x1, 0x2, 0x4 

# ARM relocations, essentially says at this address in tghe output patch the final value of this symbol - Assembler emits, we resolve these! 
STB_LOCAL, STB_GLOBAL = 0, 1 
STT_NOTYPE, STT_OBJECT, STT_FUNC, STT_SECTION = 0, 1, 2, 3 
R_ARM_NONE = 0 
R_ARM_ABS32 = 2 
R_ARM_CALL = 28 
R_ARM_JUMP24 = 29 

class Section:
    def __init__(self, name, content=b'', flags=SHF_ALLOC | SHF_EXECINSTR, type_=SHT_PROGBITS):
        self.name = name 
        self.content = bytearray(content)
        self.flags = flags 
        self.type = type_
        self.addr = 0 
        self.offset = 0 

    @property 
    def size(self):
        return len(self.content)

    
class Symbol:
    def __init__(self, name, value, section, bind=STB_GLOBAL, type_=STT_FUNC):
        self.name = name
        self.value = value 
        self.section = section 
        self.bind = bind
        self.type = type_ 

class Relocation:
    def __init__(self, offset, symbol, section, type_=R_ARM_CALL, addend=0):
        self.offset = offset
        self.symbol = symbol
        self.section = section
        self.type = type_
        self.addend = addend

class ELFLinker:
    DEFAULT_LOAD_ADDR = 0x10000 
    EHDR_SIZE, PHDR_SIZE, SHDR_SIZE = 52, 32, 40

    def __init__(self, load_addr=DEFAULT_LOAD_ADDR):
        self.load_addr = load_addr 
        self.sections: list[Section] = []
        self.symbols: dict[str, Symbol] = {}
        self.relocations: list[Relocation] = []
        self.entry = 0

    def _section(self, name):
        for s in self.sections:
            if s.name == name:
                return s 
        return None 


    def add_section(self, name, content=b'', flags=SHF_ALLOC | SHF_EXECINSTR):
        s = self._section(name)
        if s is None:
            s = Section(name, content, flags)
            self.sections.append(s)

        else: 
            s.content.extend(content)
        return s 

    def read_bin_file(self, file, section_name=".text"):
        with open(file, "rb") as f:
            self.add_section(section_name, f.read())


    def read_bitstring_file(self, file, section_name=".text"):
        out = bytearray()
        with open(file) as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue 
                out += struct.pack('<I', int(line, 2))

        self.add_section(section_name, bytes(out))

    def add_symbol(self, name, value, section_name=".text", bind=STB_GLOBAL, type_=STT_FUNC):
        sec = self._section(section_name)
        if sec is None:
            raise ValueError(f"section {section_name!r} not found")    
        self.symbols[name] = Symbol(name, value, sec, bind, type_)

    def add_relocation(self, offset, symbol_name, section_name=".text", type_=R_ARM_CALL, addend=0):
        sec = self._section(section_name)
        if sec is None:
            raise ValueError(f"section {section_name!r} not found")
        self.relocations.append(Relocation(offset, symbol_name, sec, type_, addend))

    def link(self, entry="_start"):
        addr = self.load_addr + self.EHDR_SIZE + self.PHDR_SIZE 
        for s in self.sections:
            s.addr = addr 
            addr += s.size 
        for sym in self.symbols.values():
            sym.value = sym.section.addr + sym.value 
        for rel in self.relocations:
            self._apply(rel)
        if entry not in self.symbols:
            raise ValueError(f"entry symbol {entry!r} not found")
        self.entry = self.symbols[entry].value


    @staticmethod 
    def _read32(buf, off):
        return struct.unpack_from("<I", buf, off)[0]

    @staticmethod
    def _write32(buf, off, val):
        struct.pack_into("<I", buf, off, val & 0XFFFFFFFF)

    def _apply(self, rel):
        sym = self.symbols.get(rel.symbol)
        if sym is None:
            raise ValueError(f"undefined symbol {rel.symbol!r}")
        S, A = sym.value, rel.addend
        P = rel.section.addr + rel.offset
        buf = rel.section.content 

        if rel.type == R_ARM_ABS32:
            self._write32(buf, rel.offset, self._read32(buf, rel.offset) + S + A)

        elif rel.type in (R_ARM_CALL, R_ARM_JUMP24):
            instr = self._read32(buf, rel.offset)
            delta = (S + A) - (P + 8)
            if delta & 3:
                raise ValueError("ARM branch target not 4-byte aligned")
            imm = (delta >> 2) & 0X00FFFFFF
            self._write32(buf, rel.offset, (instr & 0XFF000000) | imm)

        elif rel.type == R_ARM_NONE:
            pass
        else:
            raise NotImplementedError(f"reloc type {rel.type}")


    def write_elf(self, output_filename):
        shstrtab = bytearray(b'\x00')
        def add_str(tab, s):
            off = len(tab); tab.extend(s.encode()); tab.append(0); return off 

        section_name_offs = {s.name: add_str(shstrtab, s.name) for s in self.sections}
        off_shstr  = add_str(shstrtab, '.shstrtab')
        off_symtab = add_str(shstrtab, '.symtab')
        off_strtab = add_str(shstrtab, '.strtab')

        strtab = bytearray(b'\x00')

        # Elf32_Sym = name(4) value(4) size(4) info(1) other(1) shndx(2) = 16 bytes
        entries = [struct.pack('<IIIBBH', 0, 0, 0, 0, 0, 0)]    # STN_UNDEF
        for i, s in enumerate(self.sections, start=1):
            info = (STB_LOCAL << 4) | STT_SECTION
            entries.append(struct.pack('<IIIBBH', 0, s.addr, 0, info, 0, i))
        first_global = len(entries)
        for sym in self.symbols.values():
            shndx = self.sections.index(sym.section) + 1
            info = (sym.bind << 4) | (sym.type & 0xF)
            entries.append(struct.pack('<IIIBBH',
                add_str(strtab, sym.name), sym.value, 0, info, 0, shndx))
        symtab_blob = b''.join(entries)

        # File offsets: [ehdr][phdr][sections][shstrtab][symtab][strtab][shdrs]
        section_data_off = self.EHDR_SIZE + self.PHDR_SIZE
        cur = section_data_off
        for s in self.sections:
            s.offset = cur
            cur += s.size
        shstr_off = cur; cur += len(shstrtab)
        sym_off   = cur; cur += len(symtab_blob)
        str_off   = cur; cur += len(strtab)
        sh_off    = cur

        load_size = sum(s.size for s in self.sections)

        # Elf32_Shdr = name type flags addr offset size link info addralign entsize (10 words)
        shdrs = [struct.pack('<IIIIIIIIII', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)]
        for s in self.sections:
            shdrs.append(struct.pack('<IIIIIIIIII',
                section_name_offs[s.name], s.type, s.flags,
                s.addr, s.offset, s.size,
                0, 0, 4, 0))
        shstr_idx = len(shdrs)
        shdrs.append(struct.pack('<IIIIIIIIII',
            off_shstr, SHT_STRTAB, 0, 0, shstr_off, len(shstrtab),
            0, 0, 1, 0))
        sym_idx = len(shdrs)
        str_idx = sym_idx + 1
        shdrs.append(struct.pack('<IIIIIIIIII',
            off_symtab, SHT_SYMTAB, 0, 0, sym_off, len(symtab_blob),
            str_idx, first_global, 4, 16))
        shdrs.append(struct.pack('<IIIIIIIIII',
            off_strtab, SHT_STRTAB, 0, 0, str_off, len(strtab),
            0, 0, 1, 0))

        # ELF header
        e_ident = b'\x7fELF' + bytes([ELFCLASS32, ELFDATA2LSB, 1, 0]) + b'\x00' * 8
        ehdr = e_ident + struct.pack('<HHIIIIIHHHHHH',
            ET_EXEC,                # e_type
            EM_ARM,                 # e_machine
            1,                      # e_version
            self.entry,             # e_entry
            self.EHDR_SIZE,         # e_phoff
            sh_off,                 # e_shoff
            EF_ARM_EABI_VER5,       # e_flags
            self.EHDR_SIZE,         # e_ehsize
            self.PHDR_SIZE, 1,      # e_phentsize, e_phnum
            self.SHDR_SIZE, len(shdrs),   # e_shentsize, e_shnum
            shstr_idx)              # e_shstrndx

        # Single PT_LOAD R+X segment covering ELF + PHDR + all loaded sections.
        phdr = struct.pack('<IIIIIIII',
            PT_LOAD,
            0,                                # p_offset
            self.load_addr,                   # p_vaddr
            self.load_addr,                   # p_paddr
            section_data_off + load_size,     # p_filesz
            section_data_off + load_size,     # p_memsz
            PF_X | PF_r,                      # p_flags
            0x1000)                           # p_align

        with open(output_filename, 'wb') as f:
            f.write(ehdr)
            f.write(phdr)
            for s in self.sections:
                f.write(bytes(s.content))
            f.write(bytes(shstrtab))
            f.write(symtab_blob)
            f.write(bytes(strtab))
            for h in shdrs:
                f.write(h)
        os.chmod(output_filename, 0o755)


def _hello_demo(out='a.out'):
    """
    Smoke test: a tiny ARM executable that prints "hi\\n" via angel
    semihosting (SYS_WRITE0) and exits cleanly.

        python3 linker.py
        qemu-system-arm -M versatilepb -nographic -semihosting -kernel a.out
    """
    code = b''.join([
        struct.pack('<I', 0xe28f1014),  # add  r1, pc, #20    -> r1 = msg
        struct.pack('<I', 0xe3a00004),  # mov  r0, #4         SYS_WRITE0
        struct.pack('<I', 0xef123456),  # svc  0x123456
        struct.pack('<I', 0xe3a00018),  # mov  r0, #0x18      SYS_EXIT
        struct.pack('<I', 0xe59f1004),  # ldr  r1, [pc, #4]
        struct.pack('<I', 0xef123456),  # svc  0x123456
        struct.pack('<I', 0xeafffffe),  # 1: b 1b
        struct.pack('<I', 0x00020026),  # ADP_Stopped_ApplicationExit
        b'hi\n\x00',
    ])
    while len(code) % 4:
        code += b'\x00'

    ld = ELFLinker(load_addr=0x10000)
    ld.add_section('.text', code)
    ld.add_symbol('_start', 0, '.text', type_=STT_FUNC)
    ld.link(entry='_start')
    ld.write_elf(out)
    print(f"wrote {out} ({os.path.getsize(out)} bytes)")
    print(f"run: qemu-system-arm -M versatilepb -nographic -semihosting -kernel {out}")


def main(argv):
    if len(argv) <= 1:
        _hello_demo()
        return
    out, bins = argv[1], argv[2:]
    ld = ELFLinker()
    for b in bins:
        ld.read_bin_file(b)
    ld.add_symbol('_start', 0, '.text')
    ld.link(entry='_start')
    ld.write_elf(out)
    print(f"wrote {out}")


if __name__ == '__main__':
    main(sys.argv)
