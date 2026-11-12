// recursive descent parser, ported from compiler.py's Parser

use super::ast::*;
use super::lexer::Tok;
use crate::py::{syntax_error, Result};
use std::collections::HashMap;

pub struct Parser<'a> {
    toks: &'a [Tok],
    pos: usize,
    pub typedefs: HashMap<String, Type>,
    pub structs: &'a mut Structs,
}

impl<'a> Parser<'a> {
    pub fn new(toks: &'a [Tok], structs: &'a mut Structs) -> Self {
        Parser { toks, pos: 0, typedefs: HashMap::new(), structs }
    }

    fn peek(&self, k: usize) -> Option<&'a Tok> {
        self.toks.get(self.pos + k)
    }

    fn next(&mut self) -> Option<&'a Tok> {
        let t = self.peek(0);
        self.pos += 1;
        t
    }

    fn at_end(&self) -> bool {
        self.pos >= self.toks.len()
    }

    fn preview(&self) -> String {
        let end = (self.pos + 3).min(self.toks.len());
        let start = self.pos.min(end);
        let items: Vec<String> = self.toks[start..end].iter().map(|t| t.repr()).collect();
        format!("[{}]", items.join(", "))
    }

    fn peek_is(&self, k: usize, f: impl Fn(&Tok) -> bool) -> bool {
        self.peek(k).map(f).unwrap_or(false)
    }

    fn expect_p(&mut self, p: char) -> Result<()> {
        if self.peek_is(0, |t| t.is_p(p)) {
            self.pos += 1;
            return Ok(());
        }
        Err(syntax_error(format!("parse: expected p:{}, got {}", crate::py::repr(&p.to_string()), self.preview())))
    }

    fn expect_kw(&mut self, k: &str) -> Result<()> {
        if self.peek_is(0, |t| t.is_kw(k)) {
            self.pos += 1;
            return Ok(());
        }
        Err(syntax_error(format!("parse: expected kw:{}, got {}", crate::py::repr(k), self.preview())))
    }

    fn take_id(&mut self, what: &str) -> Result<String> {
        match self.peek(0) {
            Some(Tok::Id(name)) => {
                self.pos += 1;
                Ok(name.clone())
            }
            _ => Err(syntax_error(format!("parse: {}, got {}", what, self.preview()))),
        }
    }

    // ----- top level -----

    pub fn parse_program(&mut self) -> Result<Vec<TopDecl>> {
        let mut decls = Vec::new();
        while !self.at_end() {
            decls.push(self.parse_top()?);
        }
        Ok(decls)
    }

    fn is_qualifier(t: &Tok) -> bool {
        t.is_kw("const") || t.is_kw("unsigned") || t.is_kw("signed") || t.is_kw("long") || t.is_kw("short")
    }

    pub fn parse_type(&mut self) -> Result<Type> {
        let mut unsigned = false;
        loop {
            match self.peek(0) {
                Some(t) if t.is_kw("const") => self.pos += 1,
                Some(t) if t.is_kw("unsigned") => {
                    unsigned = true;
                    self.pos += 1;
                }
                Some(t) if t.is_kw("signed") || t.is_kw("long") || t.is_kw("short") => self.pos += 1,
                _ => break,
            }
        }

        let base = match self.peek(0) {
            Some(t) if t.is_kw("int") => {
                self.pos += 1;
                if unsigned { Type::UInt } else { Type::Int }
            }
            Some(t) if t.is_kw("char") => {
                self.pos += 1;
                Type::Char // loads zero-extend, so char == unsigned char
            }
            Some(t) if t.is_kw("void") => {
                self.pos += 1;
                Type::Void
            }
            Some(t) if t.is_kw("struct") => {
                self.pos += 1;
                let tag = self.take_id("expected struct tag")?;
                if self.peek_is(0, |t| t.is_p('{')) {
                    // definition: 'struct tag { members }', registers the layout
                    let members = self.parse_struct_members()?;
                    self.structs.insert(&tag, members);
                }
                return Ok(self.parse_ptr(Type::Struct(tag)));
            }
            Some(Tok::Id(name)) if self.typedefs.contains_key(name) => {
                // a typedef name: resolve to its aliased type
                self.pos += 1;
                let t = self.typedefs[name].clone();
                return Ok(self.parse_ptr(t));
            }
            Some(Tok::Id(_)) if unsigned => Type::UInt, // bare 'unsigned x' == 'unsigned int x'
            _ => return Err(syntax_error(format!("parse: expected type, got {}", self.preview()))),
        };
        Ok(self.parse_ptr(base))
    }

    fn parse_array_suffix(&mut self, t: Type) -> Result<Type> {
        if self.peek_is(0, |t| t.is_p('[')) {
            self.pos += 1;
            let n = match self.peek(0) {
                Some(Tok::IntL(n)) => *n,
                _ => return Err(syntax_error(format!("parse: expected array size, got {}", self.preview()))),
            };
            self.pos += 1;
            self.expect_p(']')?;
            return Ok(Type::Arr(Box::new(t), n));
        }
        Ok(t)
    }

    fn parse_ptr(&mut self, mut t: Type) -> Type {
        // consume trailing stars, nesting TPtr for each pointer level
        while self.peek_is(0, |t| t.is_o("*")) {
            self.pos += 1;
            t = Type::ptr(t);
        }
        t
    }

    fn parse_struct_members(&mut self) -> Result<Vec<(String, Type)>> {
        self.expect_p('{')?;
        let mut members = Vec::new();
        while !self.peek_is(0, |t| t.is_p('}')) {
            if self.at_end() {
                return Err(syntax_error("parse: unterminated struct body, expected '}'"));
            }
            let ty = self.parse_type()?;
            let name = self.take_id("expected member name")?;
            let ty = self.parse_array_suffix(ty)?;
            self.expect_p(';')?;
            members.push((name, ty));
        }
        self.expect_p('}')?;
        Ok(members)
    }

    fn parse_top(&mut self) -> Result<TopDecl> {
        // one top-level decl: function or global. both start type + name.
        // a '(' after the name means function.
        if self.peek_is(0, |t| t.is_kw("struct"))
            && matches!(self.peek(1), Some(Tok::Id(_)))
            && self.peek_is(2, |t| t.is_p('{'))
        {
            let ty = self.parse_type()?;
            self.expect_p(';')?;
            return match ty {
                Type::Struct(tag) => Ok(TopDecl::StructDef(tag)),
                t => Err(crate::py::Error::new("AttributeError", format!("'{}' object has no attribute 'tag'", t.class_name()))),
            };
        }

        if self.peek_is(0, |t| t.is_kw("typedef")) {
            self.pos += 1;
            let ty = self.parse_type()?;
            let name = self.take_id("expected typedef name")?;
            self.expect_p(';')?;
            self.typedefs.insert(name.clone(), ty);
            return Ok(TopDecl::TypedefDef(name));
        }

        let ty = self.parse_type()?;
        let name = self.take_id("expected declarator")?;
        if self.peek_is(0, |t| t.is_p('(')) {
            self.pos += 1;
            let (params, varargs) = self.parse_params()?;
            let body = self.parse_block()?;
            return Ok(TopDecl::Func(Func { ret: ty, name, params, body, varargs }));
        }
        self.parse_global(ty, name)
    }

    fn parse_global(&mut self, ty: Type, name: String) -> Result<TopDecl> {
        let ty = self.parse_array_suffix(ty)?;
        if self.peek_is(0, |t| t.is_o("=")) {
            self.pos += 1;
            let e = self.parse_expr()?;
            self.expect_p(';')?;
            return Ok(TopDecl::Global(Global { ty, name, init: Some(e) }));
        }
        self.expect_p(';')?;
        Ok(TopDecl::Global(Global { ty, name, init: None }))
    }

    fn parse_params(&mut self) -> Result<(Vec<(Type, String)>, bool)> {
        // comma separated params up to ')'. a trailing '...' marks a
        // varargs function; the anonymous args arrive in r{len(params)}..r3.
        if self.peek_is(0, |t| t.is_p(')')) {
            self.pos += 1;
            return Ok((vec![], false));
        }
        if self.peek_is(0, |t| t.is_kw("void")) && self.peek_is(1, |t| t.is_p(')')) {
            self.pos += 2;
            return Ok((vec![], false));
        }
        let mut params = Vec::new();
        loop {
            if self.peek_is(0, |t| t.is_p('.')) {
                if self.peek_is(1, |t| t.is_p('.')) && self.peek_is(2, |t| t.is_p('.')) {
                    self.pos += 3;
                    self.expect_p(')')?;
                    return Ok((params, true));
                }
                return Err(syntax_error(format!("parse: bad parameter, got {}", self.preview())));
            }
            let ty = self.parse_type()?;
            let name = self.take_id("bad parameter")?;
            params.push((ty, name));
            match self.next() {
                Some(t) if t.is_p(')') => return Ok((params, false)),
                Some(t) if t.is_p(',') => {}
                _ => return Err(syntax_error(format!("parse: bad parameter, got {}", self.preview()))),
            }
        }
    }

    // ----- statements -----

    fn is_type_start(&self) -> bool {
        match self.peek(0) {
            None => false,
            Some(t) if t.is_kw("int") || t.is_kw("char") || t.is_kw("void") || t.is_kw("struct") || Self::is_qualifier(t) => true,
            Some(Tok::Id(name)) => self.typedefs.contains_key(name),
            _ => false,
        }
    }

    fn parse_block(&mut self) -> Result<Stmt> {
        self.expect_p('{')?;
        let mut stmts = Vec::new();
        while !self.peek_is(0, |t| t.is_p('}')) {
            if self.at_end() {
                return Err(syntax_error("parse: unterminated block, expected '}'"));
            }
            stmts.push(self.parse_stmt()?);
        }
        self.expect_p('}')?;
        Ok(Stmt::Block(stmts))
    }

    fn parse_stmt(&mut self) -> Result<Stmt> {
        let t = self.peek(0);
        let is = |k: &str| t.map(|t| t.is_kw(k)).unwrap_or(false);
        if t.map(|t| t.is_p('{')).unwrap_or(false) {
            return self.parse_block();
        }
        if is("if") {
            return self.parse_if();
        }
        if is("while") {
            return self.parse_while();
        }
        if is("for") {
            return self.parse_for();
        }
        if is("return") {
            return self.parse_return();
        }
        if is("break") {
            self.expect_kw("break")?;
            self.expect_p(';')?;
            return Ok(Stmt::Break);
        }
        if is("continue") {
            self.expect_kw("continue")?;
            self.expect_p(';')?;
            return Ok(Stmt::Continue);
        }
        if self.is_type_start() {
            return self.parse_decl();
        }
        // fallthrough: expression statement, 'expr ;'
        let e = self.parse_expr()?;
        self.expect_p(';')?;
        Ok(Stmt::Expr(e))
    }

    fn parse_if(&mut self) -> Result<Stmt> {
        self.expect_kw("if")?;
        self.expect_p('(')?;
        let cond = self.parse_expr()?;
        self.expect_p(')')?;
        let then = self.parse_stmt()?;
        let mut els = None;
        if self.peek_is(0, |t| t.is_kw("else")) {
            self.pos += 1;
            els = Some(Box::new(self.parse_stmt()?));
        }
        Ok(Stmt::If(cond, Box::new(then), els))
    }

    fn parse_while(&mut self) -> Result<Stmt> {
        self.expect_kw("while")?;
        self.expect_p('(')?;
        let cond = self.parse_expr()?;
        self.expect_p(')')?;
        let body = self.parse_stmt()?;
        Ok(Stmt::While(cond, Box::new(body)))
    }

    fn parse_for(&mut self) -> Result<Stmt> {
        self.expect_kw("for")?;
        self.expect_p('(')?;
        let init = if self.peek_is(0, |t| t.is_p(';')) {
            self.pos += 1;
            None
        } else if self.is_type_start() {
            Some(Box::new(self.parse_decl()?))
        } else {
            let e = self.parse_expr()?;
            self.expect_p(';')?;
            Some(Box::new(Stmt::Expr(e)))
        };
        let mut cond = None;
        if !self.peek_is(0, |t| t.is_p(';')) {
            cond = Some(self.parse_expr()?);
        }
        self.expect_p(';')?;
        let mut step = None;
        if !self.peek_is(0, |t| t.is_p(')')) {
            step = Some(self.parse_expr()?);
        }
        self.expect_p(')')?;
        let body = self.parse_stmt()?;
        Ok(Stmt::For(init, cond, step, Box::new(body)))
    }

    fn parse_return(&mut self) -> Result<Stmt> {
        self.expect_kw("return")?;
        if self.peek_is(0, |t| t.is_p(';')) {
            self.pos += 1;
            return Ok(Stmt::Return(None));
        }
        let e = self.parse_expr()?;
        self.expect_p(';')?;
        Ok(Stmt::Return(Some(e)))
    }

    fn parse_decl(&mut self) -> Result<Stmt> {
        let ty = self.parse_type()?;
        let name = self.take_id("expected declarator")?;
        let ty = self.parse_array_suffix(ty)?;
        let mut init = None;
        if self.peek_is(0, |t| t.is_o("=")) {
            self.pos += 1;
            init = Some(self.parse_expr()?);
        }
        self.expect_p(';')?;
        Ok(Stmt::Decl(ty, name, init))
    }

    // ----- expressions -----
    // a ladder of parse functions, lowest precedence first. each level parses
    // the next-tighter level then folds its own operators left-to-right (except
    // assignment, which is right associative).

    pub fn parse_expr(&mut self) -> Result<Expr> {
        self.parse_assign()
    }

    fn parse_assign(&mut self) -> Result<Expr> {
        let lhs = self.parse_logic_or()?;
        if self.peek_is(0, |t| t.is_o("=")) {
            self.pos += 1;
            let rhs = self.parse_assign()?;
            return Ok(Expr::Assign(Box::new(lhs), Box::new(rhs)));
        }
        if let Some(Tok::O(o)) = self.peek(0) {
            let op = match *o {
                "+=" => Some(BinOp::Add), "-=" => Some(BinOp::Sub), "*=" => Some(BinOp::Mul),
                "/=" => Some(BinOp::Div), "%=" => Some(BinOp::Mod), "|=" => Some(BinOp::BOr),
                "&=" => Some(BinOp::BAnd), "^=" => Some(BinOp::BXor), "<<=" => Some(BinOp::Shl),
                ">>=" => Some(BinOp::Shr), _ => None,
            };
            if let Some(op) = op {
                self.pos += 1;
                let rhs = self.parse_assign()?;
                return Ok(Expr::CompAssign(op, Box::new(lhs), Box::new(rhs)));
            }
        }
        Ok(lhs)
    }

    // generic left-associative fold over one precedence level
    fn parse_binop(&mut self, ops: &[(&str, BinOp)], lower: fn(&mut Self) -> Result<Expr>) -> Result<Expr> {
        let mut lhs = lower(self)?;
        loop {
            let op = match self.peek(0) {
                Some(Tok::O(o)) => ops.iter().find(|(s, _)| s == o).map(|(_, b)| *b),
                _ => None,
            };
            match op {
                Some(op) => {
                    self.pos += 1;
                    let rhs = lower(self)?;
                    lhs = Expr::BinOp(op, Box::new(lhs), Box::new(rhs));
                }
                None => return Ok(lhs),
            }
        }
    }

    fn parse_logic_or(&mut self) -> Result<Expr> {
        self.parse_binop(&[("||", BinOp::Or)], Self::parse_logic_and)
    }
    fn parse_logic_and(&mut self) -> Result<Expr> {
        self.parse_binop(&[("&&", BinOp::And)], Self::parse_bit_or)
    }
    fn parse_bit_or(&mut self) -> Result<Expr> {
        self.parse_binop(&[("|", BinOp::BOr)], Self::parse_bit_xor)
    }
    fn parse_bit_xor(&mut self) -> Result<Expr> {
        self.parse_binop(&[("^", BinOp::BXor)], Self::parse_bit_and)
    }
    fn parse_bit_and(&mut self) -> Result<Expr> {
        self.parse_binop(&[("&", BinOp::BAnd)], Self::parse_equality)
    }
    fn parse_equality(&mut self) -> Result<Expr> {
        self.parse_binop(&[("==", BinOp::Eq), ("!=", BinOp::Ne)], Self::parse_relational)
    }
    fn parse_relational(&mut self) -> Result<Expr> {
        self.parse_binop(
            &[("<", BinOp::Lt), ("<=", BinOp::Le), (">", BinOp::Gt), (">=", BinOp::Ge)],
            Self::parse_shift,
        )
    }
    fn parse_shift(&mut self) -> Result<Expr> {
        self.parse_binop(&[("<<", BinOp::Shl), (">>", BinOp::Shr)], Self::parse_additive)
    }
    fn parse_additive(&mut self) -> Result<Expr> {
        self.parse_binop(&[("+", BinOp::Add), ("-", BinOp::Sub)], Self::parse_term)
    }
    fn parse_term(&mut self) -> Result<Expr> {
        self.parse_binop(&[("*", BinOp::Mul), ("/", BinOp::Div), ("%", BinOp::Mod)], Self::parse_unary)
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        let t = self.peek(0);
        let is_o = |o: &str| t.map(|t| t.is_o(o)).unwrap_or(false);
        if is_o("-") {
            self.pos += 1;
            return Ok(Expr::UnOp(UnOp::Neg, Box::new(self.parse_unary()?)));
        }
        if is_o("~") {
            self.pos += 1;
            return Ok(Expr::UnOp(UnOp::BNot, Box::new(self.parse_unary()?)));
        }
        if is_o("!") {
            self.pos += 1;
            return Ok(Expr::UnOp(UnOp::Not, Box::new(self.parse_unary()?)));
        }
        if is_o("*") {
            self.pos += 1;
            return Ok(Expr::Deref(Box::new(self.parse_unary()?)));
        }
        if is_o("&") {
            self.pos += 1;
            return Ok(Expr::AddrOf(Box::new(self.parse_unary()?)));
        }
        if t.map(|t| t.is_kw("sizeof")).unwrap_or(false) {
            self.pos += 1;
            if self.peek_is(0, |t| t.is_p('(')) && self.type_after_lparen() {
                self.expect_p('(')?;
                let ty = self.parse_type()?;
                self.expect_p(')')?;
                return Ok(Expr::Sizeof(SizeofArg::Type(ty)));
            }
            return Ok(Expr::Sizeof(SizeofArg::Expr(Box::new(self.parse_unary()?))));
        }
        self.parse_postfix()
    }

    fn type_after_lparen(&self) -> bool {
        // true when the token after '(' starts a type, so 'sizeof(' is a
        // cast-style type operand rather than a parenthesized expression
        match self.peek(1) {
            None => false,
            Some(n) if n.is_kw("int") || n.is_kw("char") || n.is_kw("void") || n.is_kw("struct") || Self::is_qualifier(n) => true,
            Some(Tok::Id(name)) => self.typedefs.contains_key(name),
            _ => false,
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr> {
        let mut e = self.parse_primary()?;
        loop {
            let t = self.peek(0);
            let is_p = |p: char| t.map(|t| t.is_p(p)).unwrap_or(false);
            let is_o = |o: &str| t.map(|t| t.is_o(o)).unwrap_or(false);

            if matches!(&e, Expr::Var(n) if n == "va_arg") && is_p('(') {
                self.pos += 1;
                self.parse_expr()?;
                self.expect_p(',')?;
                let ty = self.parse_type()?;
                self.expect_p(')')?;
                return Ok(Expr::VaArg(ty));
            }

            if is_p('(') {
                // a call only makes sense directly on a named function
                let name = match &e {
                    Expr::Var(n) => n.clone(),
                    other => return Err(syntax_error(format!("parse: call target must be a name, got {}", other.repr()))),
                };
                self.pos += 1;
                let args = self.parse_args()?;
                e = Expr::Call(name, args);
            } else if is_p('[') {
                self.pos += 1;
                let idx = self.parse_expr()?;
                self.expect_p(']')?;
                e = Expr::Index(Box::new(e), Box::new(idx));
            } else if is_o("++") {
                self.pos += 1;
                e = Expr::PostIncDec(Box::new(e), 1);
            } else if is_o("--") {
                self.pos += 1;
                e = Expr::PostIncDec(Box::new(e), -1);
            } else if is_p('.') {
                self.pos += 1;
                match self.next() {
                    Some(Tok::Id(name)) => e = Expr::Member(Box::new(e), name.clone()),
                    _ => return Err(syntax_error(format!("parse: expected member name after '.', got {}", self.preview()))),
                }
            } else if is_o("->") {
                self.pos += 1;
                match self.next() {
                    Some(Tok::Id(name)) => e = Expr::Member(Box::new(Expr::Deref(Box::new(e))), name.clone()),
                    _ => return Err(syntax_error(format!("parse: expected member name after '->', got {}", self.preview()))),
                }
            } else {
                return Ok(e);
            }
        }
    }

    fn parse_args(&mut self) -> Result<Vec<Expr>> {
        if self.peek_is(0, |t| t.is_p(')')) {
            self.pos += 1;
            return Ok(vec![]);
        }
        let mut args = Vec::new();
        loop {
            args.push(self.parse_expr()?);
            match self.next() {
                Some(t) if t.is_p(')') => return Ok(args),
                Some(t) if t.is_p(',') => {}
                _ => return Err(syntax_error(format!("parse: bad argument list, got {}", self.preview()))),
            }
        }
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        let t = match self.peek(0) {
            None => return Err(syntax_error("parse: unexpected end of input in expression")),
            Some(t) => t,
        };
        let e = match t {
            Tok::IntL(n) => Expr::IntLit(*n),
            Tok::ChrL(c) => Expr::CharLit(*c),
            Tok::StrL(s) => Expr::StrLit(s.clone()),
            Tok::Id(n) => Expr::Var(n.clone()),
            t if t.is_p('(') => {
                self.pos += 1;
                let e = self.parse_expr()?;
                self.expect_p(')')?;
                return Ok(e);
            }
            _ => return Err(syntax_error(format!("parse: expected expression, got {}", self.preview()))),
        };
        self.pos += 1;
        Ok(e)
    }
}
