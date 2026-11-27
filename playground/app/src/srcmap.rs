// which source line every stage's output came from, so one line can be
// followed from the editor through preprocessing, tokens, asm, machine words
// and the cpu. built once per build from what bridge.py reported.

use crate::snapshot::{LineKind, Snapshot};
use std::collections::BTreeMap;

pub const RUNTIME: &str = "build.py runtime";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Loc {
    pub file: String,
    pub line: usize,
}

impl Loc {
    pub fn new(file: &str, line: usize) -> Self {
        Loc { file: file.to_string(), line }
    }

    pub fn label(&self) -> String {
        format!("{}:{}", self.file, self.line + 1)
    }
}

#[derive(Default)]
pub struct SourceMap {
    files: BTreeMap<String, Vec<String>>,
    // full asm line (runtime + compiler output) -> source line
    asm: Vec<Option<Loc>>,
    // instruction address -> source line
    addr: BTreeMap<u32, Loc>,
    pre: Vec<Option<Loc>>,
    pre_starts: Vec<usize>,
}

impl SourceMap {
    // name is the program being built; for a .s file the asm is the source
    pub fn build(b: &Snapshot, name: &str) -> Self {
        let mut m = SourceMap::default();
        let Some(full) = &b.full_asm else { return m };
        let lines = toolchain::py::splitlines(full);
        let off = b.asm_line_offset.min(lines.len());
        if b.kind == "c" {
            m.files.insert(RUNTIME.into(), lines[..off].iter().map(|s| s.to_string()).collect());
            for (f, text) in &b.sources {
                m.files.insert(f.clone(), text.lines().map(String::from).collect());
            }
            m.asm = (0..lines.len())
                .map(|i| {
                    if i < off {
                        Some(Loc::new(RUNTIME, i))
                    } else {
                        b.asm_src.get(i - off).cloned().flatten().map(|(f, l)| Loc { file: f, line: l })
                    }
                })
                .collect();
        } else {
            m.files.insert(name.into(), lines.iter().map(|s| s.to_string()).collect());
            m.asm = (0..lines.len()).map(|i| Some(Loc::new(name, i))).collect();
        }
        for e in &b.listing {
            if e.kind == LineKind::Insn && e.len > 0 {
                if let Some(Some(l)) = m.asm.get(e.line) {
                    m.addr.insert(b.addr_of(&e.sec, e.off), l.clone());
                }
            }
        }
        m.pre = b.pre_lines.iter().map(|x| x.clone().map(|(f, l)| Loc { file: f, line: l })).collect();
        if let Some(pre) = &b.preprocessed {
            // token spans are python string indexes, so count chars not bytes
            m.pre_starts = std::iter::once(0).chain(pre.chars().enumerate().filter(|(_, c)| *c == '\n').map(|(i, _)| i + 1)).collect();
        }
        m
    }

    pub fn has(&self, l: &Loc) -> bool {
        self.files.get(&l.file).map_or(false, |f| l.line < f.len())
    }

    pub fn text(&self, l: &Loc) -> &str {
        self.files.get(&l.file).and_then(|f| f.get(l.line)).map_or("", |s| s.as_str())
    }

    pub fn at(&self, addr: u32) -> Option<&Loc> {
        self.addr.get(&addr)
    }

    pub fn asm_loc(&self, full_line: usize) -> Option<&Loc> {
        self.asm.get(full_line).and_then(|x| x.as_ref())
    }

    pub fn addrs(&self, l: &Loc) -> Vec<u32> {
        self.addr.iter().filter(|(_, x)| *x == l).map(|(a, _)| *a).collect()
    }

    pub fn asm_lines(&self, l: &Loc) -> Vec<usize> {
        (0..self.asm.len()).filter(|&i| self.asm[i].as_ref() == Some(l)).collect()
    }

    pub fn pre_loc(&self, pre_line: usize) -> Option<&Loc> {
        self.pre.get(pre_line).and_then(|x| x.as_ref())
    }

    pub fn pre_lines(&self, l: &Loc) -> Vec<usize> {
        (0..self.pre.len()).filter(|&i| self.pre[i].as_ref() == Some(l)).collect()
    }

    // preprocessed line holding a char offset (token spans are offsets)
    pub fn pre_line_of(&self, offset: usize) -> usize {
        self.pre_starts.partition_point(|&s| s <= offset).saturating_sub(1)
    }
}
