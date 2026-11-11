// port of section4/build.py: c source -> compiler -> RUNTIME + asm ->
// assembler -> linker -> elf + flat image for the cpu. every stage's output
// is kept so the playground can show it, even when a later stage fails.

use crate::assembler::{Asm, Sec};
use crate::compiler::{self, Compiled, SourceFs};
use crate::linker::{self, ElfLinker, Region};
use crate::py::Error;
use crate::runtime::RUNTIME;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    Preprocess,
    Lex,
    Parse,
    Codegen,
    Assemble,
    Link,
}

impl Stage {
    pub fn name(self) -> &'static str {
        match self {
            Stage::Preprocess => "preprocess",
            Stage::Lex => "lex",
            Stage::Parse => "parse",
            Stage::Codegen => "codegen",
            Stage::Assemble => "assemble",
            Stage::Link => "link",
        }
    }
}

#[derive(Default, Clone)]
pub struct Build {
    pub compiled: Compiled,
    // RUNTIME + "\n" + compiler output, exactly what build.py assembles
    pub full_asm: Option<String>,
    // line in full_asm where the compiler's output starts
    pub asm_line_offset: usize,
    pub asm: Option<Asm>,
    pub linker: Option<ElfLinker>,
    pub elf: Option<Vec<u8>>,
    pub regions: Vec<Region>,
    // .text + .data concatenated, loaded at `base` (cpu_tb.cpp's image)
    pub flat: Option<Vec<u8>>,
    pub base: u32,
    pub error: Option<(Stage, Error)>,
}

impl Build {
    pub fn ok(&self) -> bool {
        self.error.is_none()
    }

    // the symbol whose address range holds `addr` (functions and labels)
    pub fn symbol_at(&self, addr: u32) -> Option<(&str, u32)> {
        let ld = self.linker.as_ref()?;
        let text = ld.sections.first()?;
        let mut best: Option<(&str, u32)> = None;
        for (name, s) in ld.symbols.iter() {
            if s.section != 0 || name.starts_with(".L") {
                continue;
            }
            if s.value <= addr && addr < text.addr + text.size() && best.map_or(true, |(_, v)| s.value >= v) {
                best = Some((name, s.value));
            }
        }
        best
    }
}

fn failed_stage(c: &Compiled) -> Stage {
    if c.preprocessed.is_none() {
        Stage::Preprocess
    } else if c.tokens.is_none() {
        Stage::Lex
    } else if c.ast.is_none() {
        Stage::Parse
    } else {
        Stage::Codegen
    }
}

pub fn build(src: &str, path: &str, fs: &dyn SourceFs) -> Build {
    let mut b = Build::default();
    let (compiled, res) = compiler::compile_stages(src, path, fs);
    b.compiled = compiled;
    if let Err(e) = res {
        b.error = Some((failed_stage(&b.compiled), e));
        return b;
    }
    let asm_text = b.compiled.asm.clone().unwrap();
    let full = format!("{}\n{}", RUNTIME, asm_text);
    b.asm_line_offset = crate::py::splitlines(RUNTIME).len() + 1;
    b.full_asm = Some(full.clone());
    if let Err((stage, e)) = assemble_and_link(&mut b, &full) {
        b.error = Some((stage, e));
    }
    b
}

// build an already-written assembly file (the playground's .s mode); no
// runtime is prepended, the source must define _start itself
pub fn build_asm(src: &str) -> Build {
    let mut b = Build::default();
    b.full_asm = Some(src.to_string());
    if let Err((stage, e)) = assemble_and_link(&mut b, src) {
        b.error = Some((stage, e));
    }
    b
}

fn assemble_and_link(b: &mut Build, full: &str) -> Result<(), (Stage, Error)> {
    let mut a = Asm::new();
    let asm_res = a.pass1(full).and_then(|_| a.pass2());
    // keep the partial assembler state: pass 1 layout is still worth showing
    b.asm = Some(a.clone());
    asm_res.map_err(|e| (Stage::Assemble, e))?;

    let link = |a: &Asm| -> crate::py::Result<ElfLinker> {
        let mut ld = ElfLinker::default();
        ld.add_section(".text", &a.text, linker::SHF_ALLOC | linker::SHF_EXECINSTR);
        ld.add_section(".data", &a.data, linker::SHF_ALLOC | linker::SHF_WRITE);
        for (name, (sec, off)) in a.symbols.iter() {
            let t = if *sec == Sec::Data { linker::STT_OBJECT } else { linker::STT_FUNC };
            ld.add_symbol(name, *off as u32, sec.name(), linker::STB_GLOBAL, t)?;
        }
        for r in &a.relocs {
            ld.add_relocation(r.off as u32, &r.sym, r.sec.name(), r.rtype, r.addend)?;
        }
        Ok(ld)
    };
    let mut ld = link(&a).map_err(|e| (Stage::Link, e))?;
    let res = ld.link("_start");
    if let Err(e) = res {
        b.linker = Some(ld);
        return Err((Stage::Link, e));
    }
    let (elf, regions) = ld.elf_image();
    b.flat = Some(ld.sections.iter().flat_map(|s| s.content.clone()).collect());
    b.base = ld.sections[0].addr;
    b.elf = Some(elf);
    b.regions = regions;
    b.linker = Some(ld);
    Ok(())
}
