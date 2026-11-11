// port of section4/linker.py: places sections after the elf + program
// headers, resolves symbols, patches relocations and writes an elf32 le
// arm executable with a single PT_LOAD segment (qemu -kernel / semihosting).

use crate::py::{repr, value_error, Error, OrderedMap, Result};

pub const ELFCLASS32: u8 = 1;
pub const ELFDATA2LSB: u8 = 1;
pub const ET_EXEC: u16 = 2;
pub const EM_ARM: u16 = 40;
pub const EF_ARM_EABI_VER5: u32 = 0x05000000;

pub const PT_LOAD: u32 = 1;
pub const PF_X: u32 = 1;
pub const PF_W: u32 = 2;
pub const PF_R: u32 = 4;

pub const SHT_NULL: u32 = 0;
pub const SHT_PROGBITS: u32 = 1;
pub const SHT_SYMTAB: u32 = 2;
pub const SHT_STRTAB: u32 = 3;
pub const SHT_NOBITS: u32 = 8;
pub const SHF_WRITE: u32 = 0x1;
pub const SHF_ALLOC: u32 = 0x2;
pub const SHF_EXECINSTR: u32 = 0x4;

pub const STB_LOCAL: u8 = 0;
pub const STB_GLOBAL: u8 = 1;
pub const STT_NOTYPE: u8 = 0;
pub const STT_OBJECT: u8 = 1;
pub const STT_FUNC: u8 = 2;
pub const STT_SECTION: u8 = 3;
pub const R_ARM_NONE: u32 = 0;
pub const R_ARM_ABS32: u32 = 2;
pub const R_ARM_CALL: u32 = 28;
pub const R_ARM_JUMP24: u32 = 29;

pub const DEFAULT_LOAD_ADDR: u32 = 0x10000;
pub const EHDR_SIZE: u32 = 52;
pub const PHDR_SIZE: u32 = 32;
pub const SHDR_SIZE: u32 = 40;

#[derive(Clone, Debug)]
pub struct Section {
    pub name: String,
    pub content: Vec<u8>,
    pub flags: u32,
    pub stype: u32,
    pub addr: u32,
    pub offset: u32,
}

impl Section {
    pub fn size(&self) -> u32 {
        self.content.len() as u32
    }
}

#[derive(Clone, Debug)]
pub struct Symbol {
    pub name: String,
    pub value: u32,
    pub section: usize, // index into sections
    pub bind: u8,
    pub stype: u8,
}

#[derive(Clone, Debug)]
pub struct Relocation {
    pub offset: u32,
    pub symbol: String,
    pub section: usize,
    pub rtype: u32,
    pub addend: i64,
}

// what one relocation did, for the playground's linker view
#[derive(Clone, Debug)]
pub struct Patch {
    pub section: usize,
    pub offset: u32,
    pub place: u32, // P: the address being patched
    pub symbol: String,
    pub target: u32, // S + A
    pub rtype: u32,
    pub before: u32,
    pub after: u32,
}

// a named byte range in the output file, for the elf layout view
#[derive(Clone, Debug)]
pub struct Region {
    pub name: String,
    pub offset: u32,
    pub size: u32,
}

#[derive(Clone, Debug)]
pub struct ElfLinker {
    pub load_addr: u32,
    pub sections: Vec<Section>,
    pub symbols: OrderedMap<Symbol>,
    pub relocations: Vec<Relocation>,
    pub entry: u32,
    pub patches: Vec<Patch>,
}

impl Default for ElfLinker {
    fn default() -> Self {
        Self::new(DEFAULT_LOAD_ADDR)
    }
}

impl ElfLinker {
    pub fn new(load_addr: u32) -> Self {
        ElfLinker {
            load_addr,
            sections: Vec::new(),
            symbols: OrderedMap::new(),
            relocations: Vec::new(),
            entry: 0,
            patches: Vec::new(),
        }
    }

    fn section_index(&self, name: &str) -> Option<usize> {
        self.sections.iter().position(|s| s.name == name)
    }

    pub fn add_section(&mut self, name: &str, content: &[u8], flags: u32) -> usize {
        match self.section_index(name) {
            Some(i) => {
                self.sections[i].content.extend_from_slice(content);
                i
            }
            None => {
                self.sections.push(Section {
                    name: name.to_string(),
                    content: content.to_vec(),
                    flags,
                    stype: SHT_PROGBITS,
                    addr: 0,
                    offset: 0,
                });
                self.sections.len() - 1
            }
        }
    }

    // read_bitstring_file: one ascii 32-bit word per line ('0101...')
    pub fn add_bitstring(&mut self, text: &str, section_name: &str) -> Result<()> {
        let mut out = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let w = u32::from_str_radix(line, 2)
                .map_err(|_| value_error(format!("invalid literal for int() with base 2: {}", repr(line))))?;
            out.extend_from_slice(&w.to_le_bytes());
        }
        self.add_section(section_name, &out, SHF_ALLOC | SHF_EXECINSTR);
        Ok(())
    }

    pub fn add_symbol(&mut self, name: &str, value: u32, section_name: &str, bind: u8, stype: u8) -> Result<()> {
        let sec = self
            .section_index(section_name)
            .ok_or_else(|| value_error(format!("section {} not found", repr(section_name))))?;
        self.symbols.insert(name, Symbol { name: name.to_string(), value, section: sec, bind, stype });
        Ok(())
    }

    pub fn add_relocation(&mut self, offset: u32, symbol_name: &str, section_name: &str, rtype: u32, addend: i64) -> Result<()> {
        let sec = self
            .section_index(section_name)
            .ok_or_else(|| value_error(format!("section {} not found", repr(section_name))))?;
        self.relocations.push(Relocation { offset, symbol: symbol_name.to_string(), section: sec, rtype, addend });
        Ok(())
    }

    pub fn link(&mut self, entry: &str) -> Result<()> {
        let mut addr = self.load_addr + EHDR_SIZE + PHDR_SIZE;
        for s in &mut self.sections {
            s.addr = addr;
            addr += s.size();
        }
        let addrs: Vec<u32> = self.sections.iter().map(|s| s.addr).collect();
        for sym in self.symbols.values_mut() {
            sym.value = addrs[sym.section].wrapping_add(sym.value);
        }
        self.patches.clear();
        for i in 0..self.relocations.len() {
            let rel = self.relocations[i].clone();
            self.apply(&rel)?;
        }
        let e = self
            .symbols
            .get(entry)
            .ok_or_else(|| value_error(format!("entry symbol {} not found", repr(entry))))?;
        self.entry = e.value;
        Ok(())
    }

    fn read32(buf: &[u8], off: u32) -> Result<u32> {
        let o = off as usize;
        buf.get(o..o + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            .ok_or_else(|| Error::new("error", "unpack_from requires a buffer of at least 4 bytes"))
    }

    fn write32(buf: &mut [u8], off: u32, val: i128) {
        let o = off as usize;
        buf[o..o + 4].copy_from_slice(&((val & 0xFFFF_FFFF) as u32).to_le_bytes());
    }

    fn apply(&mut self, rel: &Relocation) -> Result<()> {
        let sym = self
            .symbols
            .get(&rel.symbol)
            .ok_or_else(|| value_error(format!("undefined symbol {}", repr(&rel.symbol))))?;
        let (s, a) = (sym.value as i128, rel.addend as i128);
        let sec = &mut self.sections[rel.section];
        let p = sec.addr as i128 + rel.offset as i128;
        let before = Self::read32(&sec.content, rel.offset)?;

        match rel.rtype {
            R_ARM_ABS32 => Self::write32(&mut sec.content, rel.offset, before as i128 + s + a),
            R_ARM_CALL | R_ARM_JUMP24 => {
                let delta = (s + a) - (p + 8);
                if delta & 3 != 0 {
                    return Err(value_error("ARM branch target not 4-byte aligned"));
                }
                let imm = (delta >> 2) & 0x00FFFFFF;
                Self::write32(&mut sec.content, rel.offset, (before as i128 & 0xFF000000) | imm);
            }
            R_ARM_NONE => {}
            t => return Err(Error::new("NotImplementedError", format!("reloc type {}", t))),
        }
        let after = Self::read32(&sec.content, rel.offset)?;
        self.patches.push(Patch {
            section: rel.section,
            offset: rel.offset,
            place: p as u32,
            symbol: rel.symbol.clone(),
            target: ((s + a) & 0xFFFF_FFFF) as u32,
            rtype: rel.rtype,
            before,
            after,
        });
        Ok(())
    }

    // write_elf's byte layout: [ehdr][phdr][sections][shstrtab][symtab][strtab][shdrs]
    pub fn elf_image(&mut self) -> (Vec<u8>, Vec<Region>) {
        fn add_str(tab: &mut Vec<u8>, s: &str) -> u32 {
            let off = tab.len() as u32;
            tab.extend_from_slice(s.as_bytes());
            tab.push(0);
            off
        }
        fn word(out: &mut Vec<u8>, v: u32) {
            out.extend_from_slice(&v.to_le_bytes());
        }

        let mut shstrtab = vec![0u8];
        let mut section_name_offs = std::collections::HashMap::new();
        for s in &self.sections {
            // dict comprehension: a duplicate name keeps the last offset
            let off = add_str(&mut shstrtab, &s.name);
            section_name_offs.insert(s.name.clone(), off);
        }
        let off_shstr = add_str(&mut shstrtab, ".shstrtab");
        let off_symtab = add_str(&mut shstrtab, ".symtab");
        let off_strtab = add_str(&mut shstrtab, ".strtab");

        let mut strtab = vec![0u8];

        // Elf32_Sym = name(4) value(4) size(4) info(1) other(1) shndx(2) = 16 bytes
        let mut symtab = Vec::new();
        let sym = |out: &mut Vec<u8>, name: u32, value: u32, info: u8, shndx: u16| {
            word(out, name);
            word(out, value);
            word(out, 0);
            out.push(info);
            out.push(0);
            out.extend_from_slice(&shndx.to_le_bytes());
        };
        sym(&mut symtab, 0, 0, 0, 0); // STN_UNDEF
        for (i, s) in self.sections.iter().enumerate() {
            sym(&mut symtab, 0, s.addr, (STB_LOCAL << 4) | STT_SECTION, (i + 1) as u16);
        }
        let first_global = (self.sections.len() + 1) as u32;
        for (_, s) in self.symbols.iter() {
            let name = add_str(&mut strtab, &s.name);
            sym(&mut symtab, name, s.value, (s.bind << 4) | (s.stype & 0xF), (s.section + 1) as u16);
        }

        let section_data_off = EHDR_SIZE + PHDR_SIZE;
        let mut cur = section_data_off;
        for s in &mut self.sections {
            s.offset = cur;
            cur += s.size();
        }
        let shstr_off = cur;
        cur += shstrtab.len() as u32;
        let sym_off = cur;
        cur += symtab.len() as u32;
        let str_off = cur;
        cur += strtab.len() as u32;
        let sh_off = cur;

        let load_size: u32 = self.sections.iter().map(|s| s.size()).sum();

        // Elf32_Shdr = name type flags addr offset size link info addralign entsize
        let mut shdrs: Vec<[u32; 10]> = vec![[0; 10]];
        for s in &self.sections {
            shdrs.push([section_name_offs[&s.name], s.stype, s.flags, s.addr, s.offset, s.size(), 0, 0, 4, 0]);
        }
        let shstr_idx = shdrs.len() as u16;
        shdrs.push([off_shstr, SHT_STRTAB, 0, 0, shstr_off, shstrtab.len() as u32, 0, 0, 1, 0]);
        let sym_idx = shdrs.len() as u32;
        let str_idx = sym_idx + 1;
        shdrs.push([off_symtab, SHT_SYMTAB, 0, 0, sym_off, symtab.len() as u32, str_idx, first_global, 4, 16]);
        shdrs.push([off_strtab, SHT_STRTAB, 0, 0, str_off, strtab.len() as u32, 0, 0, 1, 0]);

        let mut out = Vec::new();
        // elf header
        out.extend_from_slice(b"\x7fELF");
        out.extend_from_slice(&[ELFCLASS32, ELFDATA2LSB, 1, 0]);
        out.extend_from_slice(&[0u8; 8]);
        out.extend_from_slice(&ET_EXEC.to_le_bytes());
        out.extend_from_slice(&EM_ARM.to_le_bytes());
        word(&mut out, 1); // e_version
        word(&mut out, self.entry);
        word(&mut out, EHDR_SIZE); // e_phoff
        word(&mut out, sh_off);
        word(&mut out, EF_ARM_EABI_VER5);
        for h in [EHDR_SIZE as u16, PHDR_SIZE as u16, 1, SHDR_SIZE as u16, shdrs.len() as u16, shstr_idx] {
            out.extend_from_slice(&h.to_le_bytes());
        }
        // single PT_LOAD R+X segment covering ELF + PHDR + all loaded sections
        for v in [
            PT_LOAD,
            0,
            self.load_addr,
            self.load_addr,
            section_data_off + load_size,
            section_data_off + load_size,
            PF_X | PF_R,
            0x1000,
        ] {
            word(&mut out, v);
        }
        for s in &self.sections {
            out.extend_from_slice(&s.content);
        }
        out.extend_from_slice(&shstrtab);
        out.extend_from_slice(&symtab);
        out.extend_from_slice(&strtab);
        for h in &shdrs {
            for v in h {
                word(&mut out, *v);
            }
        }

        let mut regions = vec![
            Region { name: "elf header".into(), offset: 0, size: EHDR_SIZE },
            Region { name: "program header".into(), offset: EHDR_SIZE, size: PHDR_SIZE },
        ];
        for s in &self.sections {
            regions.push(Region { name: s.name.clone(), offset: s.offset, size: s.size() });
        }
        regions.push(Region { name: ".shstrtab".into(), offset: shstr_off, size: shstrtab.len() as u32 });
        regions.push(Region { name: ".symtab".into(), offset: sym_off, size: symtab.len() as u32 });
        regions.push(Region { name: ".strtab".into(), offset: str_off, size: strtab.len() as u32 });
        regions.push(Region { name: "section headers".into(), offset: sh_off, size: shdrs.len() as u32 * SHDR_SIZE });
        (out, regions)
    }
}

// linker.py's _hello_demo: prints "hi\n" via semihosting and exits
pub fn hello_demo() -> Result<ElfLinker> {
    let mut code = Vec::new();
    for w in [
        0xe28f1014u32, // add  r1, pc, #20    -> r1 = msg
        0xe3a00004,    // mov  r0, #4         SYS_WRITE0
        0xef123456,    // svc  0x123456
        0xe3a00018,    // mov  r0, #0x18      SYS_EXIT
        0xe59f1004,    // ldr  r1, [pc, #4]
        0xef123456,    // svc  0x123456
        0xeafffffe,    // 1: b 1b
        0x00020026,    // ADP_Stopped_ApplicationExit
    ] {
        code.extend_from_slice(&w.to_le_bytes());
    }
    code.extend_from_slice(b"hi\n\x00");
    let mut ld = ElfLinker::new(0x10000);
    ld.add_section(".text", &code, SHF_ALLOC | SHF_EXECINSTR);
    ld.add_symbol("_start", 0, ".text", STB_GLOBAL, STT_FUNC)?;
    ld.link("_start")?;
    Ok(ld)
}
