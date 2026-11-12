// arm assembly codegen, ported from compiler.py's CodeGen.
// model: each expression computes into r0; temporaries are pushed/popped.
// aapcs calling convention (args r0-r3, return in r0, fp=r11, lr=r14, sp=r13).

use super::ast::*;
use crate::py::{key_error, not_impl, syntax_error, type_error, Error, OrderedMap, Result};
use std::collections::HashMap;

// target is 32-bit arm: int, char* and every pointer occupy one word
pub const WORD: i128 = 4;

pub struct Layout<'a> {
    pub structs: &'a Structs,
}

impl<'a> Layout<'a> {
    fn members(&self, tag: &str) -> Result<&'a Vec<(String, Type)>> {
        self.structs.get(tag).ok_or_else(|| key_error(tag))
    }

    pub fn align_of(&self, t: &Type) -> Result<i128> {
        match t {
            Type::Char => Ok(1),
            Type::Int | Type::UInt | Type::Ptr(_) => Ok(WORD),
            Type::Arr(e, _) => self.align_of(e),
            Type::Struct(tag) => {
                let mut best = None;
                for (_, mt) in self.members(tag)? {
                    let a = self.align_of(mt)?;
                    best = Some(best.map_or(a, |b: i128| b.max(a)));
                }
                Ok(best.unwrap_or(1))
            }
            Type::Void => Err(type_error(format!("layout: unknown type {}", t.repr()))),
        }
    }

    // (name, type, offset) per member, plus the padded total size
    pub fn struct_layout(&self, tag: &str) -> Result<(Vec<(String, Type, i128)>, i128)> {
        let mut out = Vec::new();
        let mut off = 0;
        let mut align = 1;
        for (name, mt) in self.members(tag)? {
            let a = self.align_of(mt)?;
            off = (off + a - 1) & !(a - 1);
            out.push((name.clone(), mt.clone(), off));
            off += self.type_size(mt)?;
            align = align.max(a);
        }
        Ok((out, (off + align - 1) & !(align - 1)))
    }

    pub fn member_type(&self, tag: &str, name: &str) -> Result<Type> {
        for (n, mt) in self.members(tag)? {
            if n == name {
                return Ok(mt.clone());
            }
        }
        Err(Error::new("KeyError", format!("\"layout: struct {} has no member {}\"", crate::py::repr(tag), crate::py::repr(name))))
    }

    pub fn member_offset(&self, tag: &str, name: &str) -> Result<i128> {
        for (n, _, off) in self.struct_layout(tag)?.0 {
            if n == name {
                return Ok(off);
            }
        }
        Err(Error::new("KeyError", format!("\"layout: struct {} has no member {}\"", crate::py::repr(tag), crate::py::repr(name))))
    }

    pub fn type_size(&self, t: &Type) -> Result<i128> {
        match t {
            Type::Char => Ok(1),
            Type::Int | Type::UInt | Type::Ptr(_) => Ok(WORD),
            Type::Arr(e, n) => Ok(n * self.type_size(e)?),
            Type::Struct(tag) => Ok(self.struct_layout(tag)?.1),
            Type::Void => Err(type_error("codegen: void has no size")),
        }
    }

    pub fn scale_of(&self, t: &Type) -> Result<i128> {
        // pointer arithmetic step: *(p + i) advances by the size of *p
        match t {
            Type::Ptr(inner) => self.type_size(inner),
            _ => Ok(1),
        }
    }
}

pub fn decay(t: &Type) -> Type {
    match t {
        Type::Arr(e, _) => Type::ptr((**e).clone()),
        t => t.clone(),
    }
}

fn is_ptr(t: &Type) -> bool {
    matches!(t, Type::Ptr(_))
}
fn is_uint(t: &Type) -> bool {
    matches!(t, Type::UInt)
}
fn is_uint_or_ptr(t: &Type) -> bool {
    matches!(t, Type::UInt | Type::Ptr(_))
}

#[derive(Default)]
pub struct Frame {
    slots: HashMap<String, i128>,
    types: HashMap<String, Type>,
    pub size: i128,
}

impl Frame {
    fn declare(&mut self, layout: &Layout, name: &str, t: &Type) -> Result<i128> {
        if !self.slots.contains_key(name) {
            self.size += (layout.type_size(t)? + 3) & !3;
            self.slots.insert(name.to_string(), -self.size);
            self.types.insert(name.to_string(), t.clone());
        }
        Ok(self.slots[name])
    }

    fn offset(&self, name: &str) -> Result<i128> {
        self.slots
            .get(name)
            .copied()
            .ok_or_else(|| Error::new("KeyError", format!("\"codegen: undeclared local {}\"", crate::py::repr(name))))
    }

    fn has(&self, name: &str) -> bool {
        self.slots.contains_key(name)
    }

    fn frame_bytes(&self) -> i128 {
        // rounded to 8 bytes so sp stays 8-byte aligned per the aapcs
        (self.size + 7) & !7
    }
}

fn collect_locals(frame: &mut Frame, layout: &Layout, stmt: &Stmt) -> Result<()> {
    match stmt {
        Stmt::Decl(t, name, _) => {
            frame.declare(layout, name, t)?;
        }
        Stmt::Block(stmts) => {
            for s in stmts {
                collect_locals(frame, layout, s)?;
            }
        }
        Stmt::If(_, then, els) => {
            collect_locals(frame, layout, then)?;
            if let Some(e) = els {
                collect_locals(frame, layout, e)?;
            }
        }
        Stmt::While(_, body) => collect_locals(frame, layout, body)?,
        Stmt::For(init, _, _, body) => {
            if let Some(i) = init {
                collect_locals(frame, layout, i)?;
            }
            collect_locals(frame, layout, body)?;
        }
        _ => {}
    }
    Ok(())
}

// string literal -> label, numbered in first-seen order
pub fn collect_strings(decls: &[TopDecl]) -> OrderedMap<String> {
    fn visit_expr(e: &Expr, out: &mut OrderedMap<String>) {
        match e {
            Expr::StrLit(v) => {
                if !out.contains(v) {
                    let lbl = format!(".Lstr{}", out.len());
                    out.insert(v, lbl);
                }
            }
            Expr::Assign(t, v) | Expr::CompAssign(_, t, v) => {
                visit_expr(t, out);
                visit_expr(v, out);
            }
            Expr::PostIncDec(t, _) => visit_expr(t, out),
            Expr::BinOp(_, l, r) => {
                visit_expr(l, out);
                visit_expr(r, out);
            }
            Expr::UnOp(_, o) | Expr::Deref(o) | Expr::AddrOf(o) => visit_expr(o, out),
            Expr::Call(_, args) => {
                for a in args {
                    visit_expr(a, out);
                }
            }
            Expr::Index(b, i) => {
                visit_expr(b, out);
                visit_expr(i, out);
            }
            Expr::Member(b, _) => visit_expr(b, out),
            Expr::Sizeof(SizeofArg::Expr(o)) => visit_expr(o, out),
            _ => {}
        }
    }
    fn visit_stmt(s: &Stmt, out: &mut OrderedMap<String>) {
        match s {
            Stmt::Expr(e) => visit_expr(e, out),
            Stmt::Block(xs) => {
                for x in xs {
                    visit_stmt(x, out);
                }
            }
            Stmt::If(c, t, e) => {
                visit_expr(c, out);
                visit_stmt(t, out);
                if let Some(e) = e {
                    visit_stmt(e, out);
                }
            }
            Stmt::While(c, b) => {
                visit_expr(c, out);
                visit_stmt(b, out);
            }
            Stmt::For(init, cond, step, body) => {
                if let Some(i) = init {
                    visit_stmt(i, out);
                }
                if let Some(c) = cond {
                    visit_expr(c, out);
                }
                // matches compiler.py: the body is only visited when the
                // loop has a step expression
                if let Some(s) = step {
                    visit_expr(s, out);
                    visit_stmt(body, out);
                }
            }
            Stmt::Return(Some(e)) => visit_expr(e, out),
            Stmt::Decl(_, _, Some(e)) => visit_expr(e, out),
            _ => {}
        }
    }
    let mut out = OrderedMap::new();
    for d in decls {
        match d {
            TopDecl::Func(f) => visit_stmt(&f.body, &mut out),
            TopDecl::Global(g) => {
                if let Some(e) = &g.init {
                    visit_expr(e, &mut out);
                }
            }
            _ => {}
        }
    }
    out
}

pub fn c_string_literal(s: &str) -> String {
    let mut out = String::from("\"");
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\000"),
            c if (32..127).contains(&(c as u32)) => out.push(c),
            c => out.push_str(&format!("\\{:03o}", c as u32)),
        }
    }
    out.push('"');
    out
}

pub struct CodeGen<'a> {
    pub lines: Vec<String>,
    strings: OrderedMap<String>,
    globals: Vec<Global>,
    label_count: usize,
    frame: Option<Frame>,
    epilogue: String,
    loops: Vec<(String, String)>, // (break target, continue target)
    pool_need: Option<usize>,
    layout: Layout<'a>,
    // first output line of each function, for the playground
    pub func_lines: Vec<(String, usize)>,
}

impl<'a> CodeGen<'a> {
    pub fn new(structs: &'a Structs) -> Self {
        CodeGen {
            lines: Vec::new(),
            strings: OrderedMap::new(),
            globals: Vec::new(),
            label_count: 0,
            frame: None,
            epilogue: String::new(),
            loops: Vec::new(),
            pool_need: None,
            layout: Layout { structs },
            func_lines: Vec::new(),
        }
    }

    fn emit(&mut self, line: &str) {
        self.lines.push(line.to_string());
        if line == ".pool" {
            // a function boundary just flushed all pending literals
            self.pool_need = None;
            return;
        }
        if line.starts_with("    ldr ") && line.contains(", =") && self.pool_need.is_none() {
            // first literal since the last pool flush
            self.pool_need = Some(self.lines.len());
        }
        // if that literal is drifting beyond ldr's 4kb pc-relative range,
        // drop a pool right here and jump over it
        if let Some(p) = self.pool_need {
            if self.lines.len() - p > 600 {
                let lbl = self.new_label("Lp");
                self.lines.push(format!("    b {}", lbl));
                self.lines.push(".pool".into());
                self.lines.push(format!("{}:", lbl));
                self.pool_need = None;
            }
        }
    }

    fn label(&mut self, name: &str) {
        self.lines.push(format!("{}:", name));
    }

    fn new_label(&mut self, prefix: &str) -> String {
        self.label_count += 1;
        format!(".{}{}", prefix, self.label_count)
    }

    fn string_label(&self, v: &str) -> Result<String> {
        self.strings.get(v).cloned().ok_or_else(|| key_error(v))
    }

    fn frame(&self) -> &Frame {
        self.frame.as_ref().unwrap()
    }

    fn frame_has(&self, name: &str) -> bool {
        self.frame.as_ref().map(|f| f.has(name)).unwrap_or(false)
    }

    // ----- program -----

    pub fn gen_program(&mut self, decls: &[TopDecl]) -> Result<String> {
        self.strings = collect_strings(decls);
        let funcs: Vec<&Func> = decls.iter().filter_map(|d| if let TopDecl::Func(f) = d { Some(f) } else { None }).collect();
        self.globals = decls.iter().filter_map(|d| if let TopDecl::Global(g) = d { Some(g.clone()) } else { None }).collect();

        // .data section: globals with initializers + all string literals
        self.emit(".data");
        for g in self.globals.clone() {
            self.emit(&format!(".global {}", g.name));
            self.label(&g.name);
            match &g.init {
                None => {
                    // uninitialized global: reserve its size, zeroed
                    let n = self.layout.type_size(&g.ty)?;
                    self.emit(&format!("    .zero {}", n));
                }
                Some(Expr::IntLit(v)) => self.emit(&format!("    .word {}", v)),
                Some(Expr::CharLit(c)) => {
                    let d = if g.ty == Type::Char { "byte" } else { "word" };
                    self.emit(&format!("    .{} {}", d, c));
                }
                Some(Expr::StrLit(v)) => {
                    if let Type::Arr(_, count) = &g.ty {
                        // char a[N] = "s": bytes inline, zero padded
                        let pad = count - v.chars().count() as i128 - 1;
                        if pad < 0 {
                            return Err(not_impl("codegen: array initializer too long"));
                        }
                        self.emit(&format!("    .asciz {}", c_string_literal(v)));
                        if pad != 0 {
                            self.emit(&format!("    .zero {}", pad));
                        }
                    } else {
                        // a global char* initialized to a string literal points at
                        // the literal's label; the bytes are emitted below
                        let l = self.string_label(v)?;
                        self.emit(&format!("    .word {}", l));
                    }
                }
                Some(e) => {
                    if matches!(g.ty, Type::Arr(..)) {
                        return Err(not_impl(format!("codegen: array global {} only takes a string init", crate::py::repr(&g.name))));
                    }
                    return Err(not_impl(format!("codegen: global initializer must be a constant, got {}", e.repr())));
                }
            }
        }

        // emit each unique string literal once, as a label + nul-terminated bytes
        let strings: Vec<(String, String)> = self.strings.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
        for (value, lbl) in strings {
            self.label(&lbl);
            self.emit(&format!("    .asciz {}", c_string_literal(&value)));
        }

        // .text section: the functions
        self.emit("");
        self.emit(".text");
        for f in funcs {
            self.func_lines.push((f.name.clone(), self.lines.len()));
            self.gen_func(f)?;
            // flush this function's literals into a nearby pool chunk so
            // ldr = stays within its 4kb pc-relative range
            self.emit(".pool");
            self.emit("");
        }
        Ok(self.lines.join("\n") + "\n")
    }

    // ----- functions -----

    fn gen_func(&mut self, f: &Func) -> Result<()> {
        // params first (so their slots are deterministic), then every local
        let mut frame = Frame::default();
        for (ty, name) in &f.params {
            frame.declare(&self.layout, name, ty)?;
        }
        collect_locals(&mut frame, &self.layout, &f.body)?;
        if f.varargs {
            frame.declare(&self.layout, "__va", &Type::Int)?;
            for i in f.params.len()..4 {
                frame.declare(&self.layout, &format!("__va{}", i - f.params.len()), &Type::Int)?;
            }
        }
        let frame_bytes = frame.frame_bytes();
        self.frame = Some(frame);

        self.epilogue = self.new_label("Lret");

        self.emit(&format!(".global {}", f.name));
        self.label(&f.name);

        // prologue: save fp/lr, set fp to the new frame base
        self.emit("    push {fp, lr}");
        self.emit("    mov fp, sp");

        // spill incoming register args (r0-r3) into their stack slots
        for (i, (_, name)) in f.params.iter().enumerate() {
            if i < 4 {
                let off = self.frame().offset(name)?;
                self.emit(&format!("    str r{}, [fp, #{}]", i, off));
            } else {
                return Err(not_impl("codegen: more than 4 params (stack args) not yet supported"));
            }
        }

        if f.varargs {
            // stash the anonymous args after the named ones and reset the index
            for i in f.params.len()..4 {
                let off = self.frame().offset(&format!("__va{}", i - f.params.len()))?;
                self.emit(&format!("    str r{}, [fp, #{}]", i, off));
            }
            let off = self.frame().offset("__va")?;
            self.emit("    mov r0, #0");
            self.emit(&format!("    str r0, [fp, #{}]", off));
        }

        // reserve locals. imm only encodes 0-255, big frames go via a register
        if frame_bytes != 0 {
            if frame_bytes <= 255 {
                self.emit(&format!("    sub sp, sp, #{}", frame_bytes));
            } else {
                self.emit(&format!("    ldr r1, ={}", frame_bytes));
                self.emit("    sub sp, sp, r1");
            }
        }

        self.gen_stmt(&f.body)?;

        // epilogue: restore sp/fp/lr and return
        let ep = self.epilogue.clone();
        self.label(&ep);
        self.emit("    mov sp, fp");
        self.emit("    pop {fp, lr}");
        self.emit("    bx lr");
        Ok(())
    }

    fn gen_stmt(&mut self, s: &Stmt) -> Result<()> {
        match s {
            Stmt::Expr(e) => self.gen_expr(e)?,
            Stmt::Block(xs) => {
                for x in xs {
                    self.gen_stmt(x)?;
                }
            }
            Stmt::Return(e) => {
                match e {
                    Some(e) => self.gen_expr(e)?,
                    None => self.emit("    mov r0, #0"), // bare 'return' returns 0
                }
                let ep = self.epilogue.clone();
                self.emit(&format!("    b {}", ep));
            }
            Stmt::Decl(ty, name, init) => {
                if let (Type::Arr(elem, count), Some(Expr::StrLit(v))) = (ty, init) {
                    // char a[N] = "s": copy from the literal's label, zero the rest
                    if **elem != Type::Char {
                        return Err(not_impl("codegen: only char arrays take a string init"));
                    }
                    let n = v.chars().count() as i128 + 1;
                    if n > *count {
                        return Err(not_impl("codegen: array initializer too long"));
                    }
                    let off = self.frame().offset(name)?;
                    let l = self.string_label(v)?;
                    self.emit(&format!("    ldr r1, ={}", l));
                    for i in 0..n {
                        self.emit(&format!("    ldrb r0, [r1, #{}]", i));
                        self.emit(&format!("    strb r0, [fp, #{}]", off + i));
                    }
                    if n < *count {
                        self.emit("    mov r0, #0");
                        for i in n..*count {
                            self.emit(&format!("    strb r0, [fp, #{}]", off + i));
                        }
                    }
                } else if let Some(init) = init {
                    self.gen_expr(init)?;
                    let off = self.frame().offset(name)?;
                    if *ty == Type::Char {
                        self.emit(&format!("    strb r0, [fp, #{}]", off));
                    } else {
                        self.emit(&format!("    str r0, [fp, #{}]", off));
                    }
                }
            }
            Stmt::If(cond, then, els) => {
                self.gen_expr(cond)?;
                let else_l = self.new_label("Lelse");
                let end_l = self.new_label("Lend");
                self.emit("    cmp r0, #0");
                self.emit(&format!("    beq {}", else_l));
                self.gen_stmt(then)?;
                if let Some(els) = els {
                    self.emit(&format!("    b {}", end_l));
                    self.label(&else_l);
                    self.gen_stmt(els)?;
                    self.label(&end_l);
                } else {
                    self.label(&else_l);
                }
            }
            Stmt::While(cond, body) => {
                let top_l = self.new_label("Lwhile");
                let end_l = self.new_label("Lwend");
                self.label(&top_l);
                self.gen_expr(cond)?;
                self.emit("    cmp r0, #0");
                self.emit(&format!("    beq {}", end_l));
                self.loops.push((end_l.clone(), top_l.clone())); // continue -> re-test cond
                self.gen_stmt(body)?;
                self.loops.pop();
                self.emit(&format!("    b {}", top_l));
                self.label(&end_l);
            }
            Stmt::For(init, cond, step, body) => {
                if let Some(i) = init {
                    self.gen_stmt(i)?;
                }
                let top_l = self.new_label("Lfor");
                let step_l = self.new_label("Lforstep");
                let end_l = self.new_label("Lforend");
                self.label(&top_l);
                if let Some(c) = cond {
                    self.gen_expr(c)?;
                    self.emit("    cmp r0, #0");
                    self.emit(&format!("    beq {}", end_l));
                }
                self.loops.push((end_l.clone(), step_l.clone())); // continue -> run step
                self.gen_stmt(body)?;
                self.loops.pop();
                self.label(&step_l);
                if let Some(st) = step {
                    self.gen_expr(st)?;
                }
                self.emit(&format!("    b {}", top_l));
                self.label(&end_l);
            }
            Stmt::Break => {
                let t = self.loops.last().ok_or_else(|| syntax_error("gen_stmt: break outside a loop"))?.0.clone();
                self.emit(&format!("    b {}", t));
            }
            Stmt::Continue => {
                let t = self.loops.last().ok_or_else(|| syntax_error("gen_stmt: continue outside a loop"))?.1.clone();
                self.emit(&format!("    b {}", t));
            }
        }
        Ok(())
    }

    // ----- type inference -----
    // minimal local inference: just enough to pick ldr vs ldrb/str vs strb
    // and to scale pointer arithmetic. undeclared names (callees) assume int.

    fn declared_type(&self, name: &str) -> Option<Type> {
        if let Some(f) = &self.frame {
            if f.has(name) {
                return Some(f.types[name].clone());
            }
        }
        self.globals.iter().find(|g| g.name == name).map(|g| g.ty.clone())
    }

    fn struct_tag(t: &Type) -> Result<&str> {
        match t {
            Type::Struct(tag) => Ok(tag),
            _ => Err(type_error(format!("codegen: member access on non-struct {}", t.repr()))),
        }
    }

    pub fn infer_type(&self, e: &Expr) -> Result<Type> {
        Ok(match e {
            Expr::IntLit(_) => Type::Int,
            Expr::CharLit(_) => Type::Char,
            Expr::StrLit(_) => Type::ptr(Type::Char),
            Expr::Var(n) => self.declared_type(n).map(|t| decay(&t)).unwrap_or(Type::Int),
            Expr::Deref(o) => match self.infer_type(o)? {
                Type::Ptr(inner) => *inner,
                _ => Type::Int,
            },
            Expr::Index(b, _) => match self.infer_type(b)? {
                Type::Ptr(inner) => *inner,
                _ => Type::Int,
            },
            Expr::Member(b, name) => {
                let bt = self.infer_type(b)?;
                let tag = Self::struct_tag(&bt)?;
                decay(&self.layout.member_type(tag, name)?)
            }
            Expr::Assign(t, _) | Expr::CompAssign(_, t, _) | Expr::PostIncDec(t, _) => self.infer_type(t)?,
            Expr::AddrOf(o) => {
                // &buf: same address as buf, treat as element pointer
                if let Expr::Var(n) = &**o {
                    if let Some(Type::Arr(elem, _)) = self.declared_type(n) {
                        return Ok(Type::ptr(*elem));
                    }
                }
                Type::ptr(self.infer_type(o)?)
            }
            Expr::UnOp(..) => Type::Int,
            Expr::VaArg(t) => decay(t),
            Expr::Call(..) => Type::Int,
            Expr::BinOp(op, l, r) => {
                use BinOp::*;
                // pointer +/- int keeps the pointer type
                if matches!(op, Add | Sub) {
                    let lt = self.infer_type(l)?;
                    if is_ptr(&lt) {
                        return Ok(lt);
                    }
                    let rt = self.infer_type(r)?;
                    if is_ptr(&rt) {
                        return Ok(rt);
                    }
                }
                // unsigned arithmetic stays unsigned
                if matches!(op, Add | Sub | Mul | Div | Mod | BAnd | BOr | BXor | Shl | Shr)
                    && (is_uint(&self.infer_type(l)?) || is_uint(&self.infer_type(r)?))
                {
                    return Ok(Type::UInt);
                }
                Type::Int
            }
            Expr::Sizeof(_) => Type::Int,
        })
    }

    fn load_insn(t: Option<&Type>) -> &'static str {
        if t == Some(&Type::Char) { "ldrb" } else { "ldr" }
    }

    fn store_insn(t: &Type) -> &'static str {
        if *t == Type::Char { "strb" } else { "str" }
    }

    // ----- lvalues -----
    // gen_lvalue evaluates the ADDRESS of an assignable expression into r0

    fn emit_local_addr(&mut self, off: i128) {
        if off.abs() <= 255 {
            self.emit(&format!("    add r0, fp, #{}", off));
        } else {
            self.emit(&format!("    ldr r0, ={}", off));
            self.emit("    add r0, fp, r0");
        }
    }

    fn gen_lvalue(&mut self, e: &Expr) -> Result<()> {
        match e {
            Expr::Var(n) => {
                if self.frame_has(n) {
                    let off = self.frame().offset(n)?;
                    self.emit_local_addr(off);
                } else {
                    self.emit(&format!("    ldr r0, ={}", n));
                }
            }
            Expr::Deref(o) => self.gen_expr(o)?, // *p as lvalue: p already IS the address
            Expr::Index(b, i) => self.gen_lvalue_index(b, i)?,
            Expr::Member(b, name) => {
                // address of the base struct + member offset
                let bt = self.infer_type(b)?;
                let tag = Self::struct_tag(&bt)?.to_string();
                self.gen_lvalue(b)?;
                let off = self.layout.member_offset(&tag, name)?;
                if (0..=255).contains(&off) {
                    self.emit(&format!("    add r0, r0, #{}", off));
                } else {
                    self.emit(&format!("    ldr r1, ={}", off));
                    self.emit("    add r0, r0, r1");
                }
            }
            e => return Err(not_impl(format!("gen_lvalue: not assignable: {}", e.repr()))),
        }
        Ok(())
    }

    fn gen_lvalue_index(&mut self, base: &Expr, idx: &Expr) -> Result<()> {
        let t = self.infer_type(base)?;
        let scale = if is_ptr(&t) { self.layout.scale_of(&t)? } else { 1 };
        self.gen_expr(base)?; // base address in r0
        self.emit("    push {r0}");
        self.gen_expr(idx)?; // index in r0
        if scale != 1 {
            self.emit(&format!("    mov r1, #{}", scale));
            self.emit("    mul r0, r0, r1");
        }
        self.emit("    pop {r1}"); // r1 = base, r0 = scaled index
        self.emit("    add r0, r1, r0");
        Ok(())
    }

    fn reject_array_target(&self, target: &Expr, what: &str) -> Result<()> {
        if let Expr::Var(n) = target {
            if let Some(Type::Arr(..)) = self.declared_type(n) {
                return Err(type_error(format!("codegen: cannot {} array {}", what, crate::py::repr(n))));
            }
        }
        Ok(())
    }

    fn emit_shift(&mut self, op: BinOp, signed: bool, rhs: &Expr) {
        // constant counts encode as imm shifts; variable counts use the
        // register-shift form. >> is arithmetic for signed lhs.
        if let Expr::IntLit(v) = rhs {
            let n = v & 31;
            let base = if op == BinOp::Shl { "lsl" } else if signed { "asr" } else { "lsr" };
            self.emit(&format!("    {} r0, r1, #{}", base, n));
        } else if op == BinOp::Shl {
            self.emit("    lsl r0, r1, r0");
        } else if signed {
            self.emit("    asr r0, r1, r0");
        } else {
            self.emit("    lsr r0, r1, r0");
        }
    }

    // ----- expressions -----
    // gen_expr evaluates the VALUE of e into r0

    pub fn gen_expr(&mut self, e: &Expr) -> Result<()> {
        match e {
            Expr::IntLit(v) => {
                if (0..=255).contains(v) {
                    self.emit(&format!("    mov r0, #{}", v));
                } else {
                    self.emit(&format!("    ldr r0, ={}", v));
                }
            }
            Expr::CharLit(c) => self.emit(&format!("    mov r0, #{}", *c as u32)),
            Expr::StrLit(v) => {
                let l = self.string_label(v)?;
                self.emit(&format!("    ldr r0, ={}", l));
            }
            Expr::Var(n) => {
                let t = self.declared_type(n);
                if let Some(Type::Arr(..)) = t {
                    // array decays: value is its address
                    if self.frame_has(n) {
                        let off = self.frame().offset(n)?;
                        self.emit_local_addr(off);
                    } else {
                        self.emit(&format!("    ldr r0, ={}", n));
                    }
                } else if self.frame_has(n) {
                    let off = self.frame().offset(n)?;
                    let insn = Self::load_insn(t.as_ref());
                    self.emit(&format!("    {} r0, [fp, #{}]", insn, off));
                } else {
                    // global: load directly off its symbol
                    self.emit(&format!("    ldr r0, ={}", n));
                    let insn = Self::load_insn(t.map(|t| decay(&t)).as_ref());
                    self.emit(&format!("    {} r0, [r0]", insn));
                }
            }
            Expr::Assign(target, value) => {
                self.reject_array_target(target, "assign to")?;
                let t = self.infer_type(target)?;
                self.gen_lvalue(target)?;
                self.emit("    push {r0}");
                self.gen_expr(value)?;
                self.emit("    pop {r1}");
                self.emit(&format!("    {} r0, [r1]", Self::store_insn(&t)));
            }
            Expr::CompAssign(op, target, value) => {
                use BinOp::*;
                self.reject_array_target(target, "assign to")?;
                let t = self.infer_type(target)?;
                self.gen_lvalue(target)?;
                self.emit("    push {r0}");
                self.emit(&format!("    {} r0, [r0]", Self::load_insn(Some(&t))));
                self.emit("    push {r0}");
                self.gen_expr(value)?;
                // pointer targets scale the rhs, like p + n
                if matches!(op, Add | Sub) && is_ptr(&t) {
                    let scale = self.layout.scale_of(&t)?;
                    if scale != 1 {
                        self.emit(&format!("    mov r1, #{}", scale));
                        self.emit("    mul r0, r0, r1");
                    }
                }
                self.emit("    pop {r1}");
                match op {
                    Add => self.emit("    add r0, r1, r0"),
                    Sub => self.emit("    sub r0, r1, r0"),
                    Mul => self.emit("    mul r0, r1, r0"),
                    BAnd => self.emit("    and r0, r1, r0"),
                    BOr => self.emit("    orr r0, r1, r0"),
                    BXor => self.emit("    eor r0, r1, r0"),
                    Shl | Shr => {
                        let signed = !is_uint_or_ptr(&t);
                        self.emit_shift(*op, signed, value);
                    }
                    Div | Mod => {
                        self.emit("    mov r2, r0");
                        self.emit("    mov r0, r1");
                        self.emit("    mov r1, r2");
                        if is_uint(&t) {
                            self.emit("    bl __udivsi3");
                            if *op == Mod {
                                self.emit("    mov r0, r2");
                            }
                        } else {
                            self.emit(if *op == Div { "    bl __divsi3" } else { "    bl __modsi3" });
                        }
                    }
                    other => return Err(not_impl(format!("gen_expr: unknown compound op {}", other.name()))),
                }
                self.emit("    pop {r1}");
                self.emit(&format!("    {} r0, [r1]", Self::store_insn(&t)));
            }
            Expr::PostIncDec(target, delta) => {
                self.reject_array_target(target, "increment")?;
                let t = self.infer_type(target)?;
                let step = delta * if is_ptr(&t) { self.layout.scale_of(&t)? } else { 1 };
                self.gen_lvalue(target)?;
                self.emit("    push {r0}");
                self.emit(&format!("    {} r0, [r0]", Self::load_insn(Some(&t))));
                self.emit("    push {r0}");
                let insn = if step >= 0 { "add" } else { "sub" };
                self.emit(&format!("    {} r0, r0, #{}", insn, step.abs()));
                self.emit("    pop {r1}");
                self.emit("    pop {r2}");
                self.emit(&format!("    {} r0, [r2]", Self::store_insn(&t)));
                self.emit("    mov r0, r1");
            }
            Expr::Sizeof(arg) => {
                // the operand is never evaluated, only its type matters
                let t = match arg {
                    SizeofArg::Type(t) => t.clone(),
                    SizeofArg::Expr(o) => match &**o {
                        Expr::Var(n) => self.declared_type(n).unwrap_or(Type::Int),
                        Expr::Member(b, name) => {
                            // raw member type: b.data stays char[512], not char*
                            let bt = self.infer_type(b)?;
                            let tag = Self::struct_tag(&bt)?;
                            self.layout.member_type(tag, name)?
                        }
                        o => self.infer_type(o)?,
                    },
                };
                let size = self.layout.type_size(&t)?;
                if (0..=255).contains(&size) {
                    self.emit(&format!("    mov r0, #{}", size));
                } else {
                    self.emit(&format!("    ldr r0, ={}", size));
                }
            }
            Expr::VaArg(_) => {
                // runtime fetch: __va indexes the consecutive __va0.. slots,
                // slot i at offset(__va0) - 4*i
                if !self.frame_has("__va0") {
                    return Err(type_error("codegen: va_arg outside a varargs function"));
                }
                let base = self.frame().offset("__va0")?;
                let vaidx = self.frame().offset("__va")?;
                self.emit(&format!("    ldr r1, [fp, #{}]", vaidx));
                self.emit("    mov r2, r1");
                self.emit("    add r1, r1, #1");
                self.emit(&format!("    str r1, [fp, #{}]", vaidx));
                self.emit("    lsl r2, r2, #2");
                if base.abs() <= 255 {
                    self.emit(&format!("    add r0, fp, #{}", base));
                } else {
                    self.emit(&format!("    ldr r0, ={}", base));
                    self.emit("    add r0, fp, r0");
                }
                self.emit("    sub r0, r0, r2");
                self.emit("    ldr r0, [r0]");
            }
            Expr::AddrOf(o) => self.gen_lvalue(o)?,
            Expr::Deref(o) => {
                let t = self.infer_type(e)?;
                self.gen_expr(o)?;
                self.emit(&format!("    {} r0, [r0]", Self::load_insn(Some(&t))));
            }
            Expr::Index(b, i) => {
                let t = self.infer_type(e)?;
                self.gen_lvalue_index(b, i)?;
                self.emit(&format!("    {} r0, [r0]", Self::load_insn(Some(&t))));
            }
            Expr::Member(b, name) => {
                // load unless the member is an array, whose value is its address
                let bt = self.infer_type(b)?;
                let tag = match &bt {
                    Type::Struct(tag) => tag.clone(),
                    t => return Err(Error::new("AttributeError", format!("'{}' object has no attribute 'tag'", t.class_name()))),
                };
                let mt = self.layout.member_type(&tag, name)?;
                self.gen_lvalue(e)?;
                if !matches!(mt, Type::Arr(..)) {
                    self.emit(&format!("    {} r0, [r0]", Self::load_insn(Some(&decay(&mt)))));
                }
            }
            Expr::UnOp(op, o) => {
                self.gen_expr(o)?;
                match op {
                    UnOp::Neg => self.emit("    rsb r0, r0, #0"),
                    UnOp::BNot => self.emit("    mvn r0, r0"),
                    UnOp::Not => {
                        self.emit("    cmp r0, #0");
                        self.emit("    mov r0, #0");
                        self.emit("    moveq r0, #1");
                    }
                }
            }
            Expr::BinOp(op, l, r) => self.gen_binop(*op, l, r)?,
            Expr::Call(name, args) => self.gen_call(name, args)?,
        }
        Ok(())
    }

    fn gen_binop(&mut self, op: BinOp, l: &Expr, r: &Expr) -> Result<()> {
        use BinOp::*;
        // short-circuit logical operators lower to branches, not alu ops
        if op == And {
            let false_l = self.new_label("Landf");
            let end_l = self.new_label("Land");
            self.gen_expr(l)?;
            self.emit("    cmp r0, #0");
            self.emit(&format!("    beq {}", false_l));
            self.gen_expr(r)?;
            self.emit("    cmp r0, #0");
            self.emit(&format!("    beq {}", false_l));
            self.emit("    mov r0, #1");
            self.emit(&format!("    b {}", end_l));
            self.label(&false_l);
            self.emit("    mov r0, #0");
            self.label(&end_l);
            return Ok(());
        }
        if op == Or {
            let true_l = self.new_label("Lort");
            let end_l = self.new_label("Lor");
            self.gen_expr(l)?;
            self.emit("    cmp r0, #0");
            self.emit(&format!("    bne {}", true_l));
            self.gen_expr(r)?;
            self.emit("    cmp r0, #0");
            self.emit(&format!("    bne {}", true_l));
            self.emit("    mov r0, #0");
            self.emit(&format!("    b {}", end_l));
            self.label(&true_l);
            self.emit("    mov r0, #1");
            self.label(&end_l);
            return Ok(());
        }

        // pointer arithmetic: if one side is a pointer, scale the other side
        let mut scale = 1;
        let lt = self.infer_type(l)?;
        if is_ptr(&lt) && matches!(op, Add | Sub) {
            scale = self.layout.scale_of(&lt)?;
        }

        self.gen_expr(l)?;
        self.emit("    push {r0}");
        self.gen_expr(r)?;
        if scale != 1 {
            self.emit(&format!("    mov r1, #{}", scale));
            self.emit("    mul r0, r0, r1");
        }
        self.emit("    pop {r1}"); // r1 = lhs, r0 = rhs

        match op {
            Add => self.emit("    add r0, r1, r0"),
            Sub => self.emit("    sub r0, r1, r0"),
            Mul => self.emit("    mul r0, r1, r0"),
            BAnd => self.emit("    and r0, r1, r0"),
            BOr => self.emit("    orr r0, r1, r0"),
            BXor => self.emit("    eor r0, r1, r0"),
            Shl | Shr => {
                let signed = !is_uint_or_ptr(&self.infer_type(l)?);
                self.emit_shift(op, signed, r);
            }
            Div | Mod => {
                // no divide in our arm subset: call a runtime helper
                let uns = is_uint_or_ptr(&self.infer_type(l)?) || is_uint_or_ptr(&self.infer_type(r)?);
                self.emit("    mov r2, r0");
                self.emit("    mov r0, r1");
                self.emit("    mov r1, r2");
                if uns {
                    self.emit("    bl __udivsi3");
                    if op == Mod {
                        self.emit("    mov r0, r2");
                    }
                } else {
                    self.emit(if op == Div { "    bl __divsi3" } else { "    bl __modsi3" });
                }
            }
            Eq | Ne | Lt | Le | Gt | Ge => {
                // cmp sets flags from lhs - rhs, then materialize 0/1
                let uns = is_uint_or_ptr(&self.infer_type(l)?) || is_uint_or_ptr(&self.infer_type(r)?);
                let cond = match (op, uns) {
                    (Eq, _) => "eq",
                    (Ne, _) => "ne",
                    (Lt, true) => "lo",
                    (Le, true) => "ls",
                    (Gt, true) => "hi",
                    (Ge, true) => "hs",
                    (Lt, false) => "lt",
                    (Le, false) => "le",
                    (Gt, false) => "gt",
                    _ => "ge",
                };
                self.emit("    cmp r1, r0");
                self.emit("    mov r0, #0");
                self.emit(&format!("    mov{} r0, #1", cond));
            }
            And | Or => unreachable!(),
        }
        Ok(())
    }

    fn gen_call(&mut self, name: &str, args: &[Expr]) -> Result<()> {
        if name == "va_start" || name == "va_end" {
            if !self.frame_has("__va") {
                return Err(type_error(format!("codegen: {} outside a varargs function", name)));
            }
            return Ok(());
        }
        if args.len() > 4 {
            return Err(not_impl("codegen: more than 4 call args (stack args) not yet supported"));
        }
        // evaluate args left to right, each pushed; then pop in reverse so
        // arg i lands in register r_i
        for a in args {
            self.gen_expr(a)?;
            self.emit("    push {r0}");
        }
        for i in (0..args.len()).rev() {
            self.emit(&format!("    pop {{r{}}}", i));
        }
        self.emit(&format!("    bl {}", name));
        Ok(())
    }
}
