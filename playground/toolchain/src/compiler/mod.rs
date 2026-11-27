// port of section4/compiler.py: source text -> preprocess -> tokens -> ast
// -> arm assembly text

pub mod ast;
pub mod codegen;
pub mod lexer;
pub mod parser;

use crate::py::{OrderedMap, Result};
pub use lexer::{DiskFs, SourceFs, Spanned, Tok};

// every intermediate the compiler produces, so the playground can show them
#[derive(Default, Clone)]
pub struct Compiled {
    pub preprocessed: Option<String>,
    pub defines: Vec<(String, String)>,
    pub tokens: Option<Vec<Spanned>>,
    pub ast: Option<Vec<ast::TopDecl>>,
    pub structs: ast::Structs,
    pub asm: Option<String>,
    pub func_lines: Vec<(String, usize)>,
}

// run the compiler stage by stage, keeping whatever finished before an error
pub fn compile_stages(src: &str, path: &str, fs: &dyn SourceFs) -> (Compiled, Result<()>) {
    let mut out = Compiled::default();
    let res = (|| {
        let mut defines = OrderedMap::new();
        let pre = lexer::preprocess(src, path, fs, &mut defines)?;
        out.defines = defines.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
        out.preprocessed = Some(pre.clone());
        let toks = lexer::tokenize(&pre)?;
        out.tokens = Some(toks.clone());
        let plain: Vec<Tok> = toks.into_iter().map(|s| s.tok).collect();
        let mut structs = ast::Structs::new();
        let parsed = {
            let mut p = parser::Parser::new(&plain, &mut structs);
            p.parse_program()
        };
        out.structs = structs;
        let decls = parsed?;
        out.ast = Some(decls.clone());
        let mut cg = codegen::CodeGen::new(&out.structs);
        let asm = cg.gen_program(&decls)?;
        out.func_lines = cg.func_lines.clone();
        out.asm = Some(asm);
        Ok(())
    })();
    (out, res)
}

// compiler.py compile_text: the whole thing, assembly text out
pub fn compile_text(src: &str, path: &str, fs: &dyn SourceFs) -> Result<String> {
    let (c, r) = compile_stages(src, path, fs);
    r?;
    Ok(c.asm.unwrap())
}
