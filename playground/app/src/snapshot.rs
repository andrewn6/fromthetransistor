// everything the python toolchain reported for one build (see
// playground/py/bridge.py), plus helpers the views derive from it

use serde::{Deserialize, Deserializer};

pub fn hex<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    let s = String::deserialize(d)?;
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(serde::de::Error::custom)).collect()
}

fn opt_hex<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
    Ok(Some(hex(d)?))
}

#[derive(Deserialize, Clone, Debug)]
pub struct Token {
    pub kind: String,
    pub text: String,
    pub repr: String,
    pub start: Option<usize>,
    pub end: Option<usize>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Member {
    pub name: String,
    pub ty: String,
    pub off: i64,
    pub size: i64,
    pub align: i64,
}

#[derive(Deserialize, Clone, Debug)]
pub struct StructLayout {
    pub tag: String,
    pub size: i64,
    pub members: Vec<Member>,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LineKind {
    Label,
    Insn,
    Directive,
    Pool,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ListingEntry {
    pub line: usize,
    pub kind: LineKind,
    pub sec: String,
    pub off: usize,
    pub len: usize,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Section {
    pub name: String,
    pub addr: u32,
    #[serde(deserialize_with = "hex")]
    pub content: Vec<u8>,
}

impl Section {
    pub fn size(&self) -> u32 {
        self.content.len() as u32
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct Symbol {
    pub name: String,
    pub value: u32,
    pub section: usize,
    #[serde(rename = "type")]
    pub stype: u32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Patch {
    pub section: usize,
    pub offset: u32,
    pub place: u32,
    pub symbol: String,
    pub target: u32,
    pub rtype: u32,
    pub before: u32,
    pub after: u32,
}

pub const R_ARM_ABS32: u32 = 2;

#[derive(Deserialize, Clone, Debug)]
pub struct Link {
    pub load_addr: u32,
    pub entry: u32,
    pub sections: Vec<Section>,
    pub symbols: Vec<Symbol>,
    pub patches: Vec<Patch>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct PyError {
    pub stage: String,
    pub kind: String,
    pub msg: String,
    pub traceback: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Snapshot {
    pub kind: String,
    pub preprocessed: Option<String>,
    pub defines: Vec<(String, String)>,
    pub tokens: Option<Vec<Token>>,
    pub ast: Option<Vec<serde_json::Value>>,
    pub structs: Vec<StructLayout>,
    pub asm: Option<String>,
    pub full_asm: Option<String>,
    pub asm_line_offset: usize,
    pub listing: Vec<ListingEntry>,
    #[serde(deserialize_with = "opt_hex")]
    pub text: Option<Vec<u8>>,
    #[serde(deserialize_with = "opt_hex")]
    pub data: Option<Vec<u8>>,
    pub pool_count: usize,
    pub reloc_count: usize,
    pub chunks: Vec<(usize, usize, usize)>,
    pub link: Option<Link>,
    #[serde(deserialize_with = "opt_hex")]
    pub elf: Option<Vec<u8>>,
    #[serde(deserialize_with = "opt_hex")]
    pub flat: Option<Vec<u8>>,
    pub base: u32,
    pub error: Option<PyError>,
    pub warnings: Vec<String>,
}

// a named byte range of the elf file, read back from its own headers
#[derive(Clone, Debug)]
pub struct Region {
    pub name: String,
    pub offset: u32,
    pub size: u32,
}

#[derive(Clone, Debug)]
pub struct Func {
    pub name: String,
    pub start: u32,
    pub end: u32,
}

impl Snapshot {
    pub fn failed(msg: impl Into<String>) -> Self {
        Snapshot {
            error: Some(PyError { stage: "bridge".into(), kind: "Error".into(), msg: msg.into(), traceback: String::new() }),
            ..Default::default()
        }
    }

    // the first section's address; before linking, where linker.py would put it
    pub fn text_base(&self) -> u32 {
        self.link.as_ref().and_then(|l| l.sections.first()).map_or(0x10000 + 52 + 32, |s| s.addr)
    }

    pub fn text_len(&self) -> usize {
        self.text.as_ref().map_or(0, |t| t.len())
    }

    pub fn addr_of(&self, sec: &str, off: usize) -> u32 {
        self.text_base() + if sec == ".text" { off } else { self.text_len() + off } as u32
    }

    // final bytes of a section: linked when available, else what the assembler emitted
    pub fn section_bytes(&self, sec: &str) -> &[u8] {
        if let Some(s) = self.link.as_ref().and_then(|l| l.sections.iter().find(|s| s.name == sec)) {
            return &s.content;
        }
        let v = if sec == ".text" { &self.text } else { &self.data };
        v.as_deref().unwrap_or(&[])
    }

    pub fn insn_count(&self) -> usize {
        self.listing.iter().filter(|l| l.kind == LineKind::Insn).count()
    }

    // ".global f" then "f:" in the compiler output marks a function
    pub fn func_lines(&self) -> Vec<(String, usize)> {
        let Some(asm) = &self.asm else { return vec![] };
        let lines: Vec<&str> = asm.lines().collect();
        let mut in_text = false;
        let mut out = vec![];
        for (i, l) in lines.iter().enumerate() {
            match l.trim() {
                ".text" => in_text = true,
                ".data" => in_text = false,
                _ => {}
            }
            if let Some(name) = l.strip_prefix(".global ") {
                if in_text && lines.get(i + 1).map_or(false, |n| *n == format!("{}:", name)) {
                    out.push((name.to_string(), i));
                }
            }
        }
        out
    }

    // functions = global symbols in .text (local .L labels are jump targets)
    pub fn functions(&self) -> Vec<Func> {
        let Some(l) = &self.link else { return vec![] };
        let Some(text) = l.sections.first() else { return vec![] };
        let mut v: Vec<(String, u32)> =
            l.symbols.iter().filter(|s| s.section == 0 && !s.name.starts_with(".L")).map(|s| (s.name.clone(), s.value)).collect();
        v.sort_by_key(|(_, a)| *a);
        let end = text.addr + text.size();
        (0..v.len()).map(|i| Func { name: v[i].0.clone(), start: v[i].1, end: v.get(i + 1).map_or(end, |n| n.1) }).collect()
    }

    // file layout, decoded from the elf's own headers
    pub fn regions(&self) -> Vec<Region> {
        let Some(b) = &self.elf else { return vec![] };
        let u16_at = |o: usize| b.get(o..o + 2).map_or(0, |w| u16::from_le_bytes([w[0], w[1]]) as u32);
        let u32_at = |o: usize| b.get(o..o + 4).map_or(0, |w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]));
        if b.len() < 52 {
            return vec![];
        }
        let (phoff, shoff) = (u32_at(28), u32_at(32) as usize);
        let (phsize, phnum, shnum, shstr) = (u16_at(42), u16_at(44), u16_at(48) as usize, u16_at(50) as usize);
        let mut out = vec![
            Region { name: "elf header".into(), offset: 0, size: u16_at(40) },
            Region { name: "program header".into(), offset: phoff, size: phsize * phnum },
        ];
        let sh = |i: usize, f: usize| u32_at(shoff + i * 40 + f * 4);
        let names = sh(shstr, 4) as usize;
        for i in 1..shnum {
            let n: String = b.get(names + sh(i, 0) as usize..).unwrap_or(&[]).iter().take_while(|&&c| c != 0).map(|&c| c as char).collect();
            out.push(Region { name: n, offset: sh(i, 4), size: sh(i, 5) });
        }
        out.push(Region { name: "section headers".into(), offset: shoff as u32, size: shnum as u32 * 40 });
        out
    }
}

pub fn func_at(funcs: &[Func], addr: u32) -> Option<usize> {
    funcs.iter().position(|f| f.start <= addr && addr < f.end)
}
