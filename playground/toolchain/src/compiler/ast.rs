// ast for the toy c dialect, mirroring the dataclasses in compiler.py

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    Int,
    UInt,
    Char,
    Void,
    Arr(Box<Type>, i128),
    Ptr(Box<Type>),
    Struct(String),
}

impl Type {
    pub fn ptr(t: Type) -> Type {
        Type::Ptr(Box::new(t))
    }

    // python dataclass repr, used in error messages
    pub fn repr(&self) -> String {
        match self {
            Type::Int => "TInt()".into(),
            Type::UInt => "TUInt()".into(),
            Type::Char => "TChar()".into(),
            Type::Void => "TVoid()".into(),
            Type::Arr(e, n) => format!("TArr(elem={}, count={})", e.repr(), n),
            Type::Ptr(t) => format!("TPtr(inner={})", t.repr()),
            Type::Struct(tag) => format!("TStruct(tag={})", crate::py::repr(tag)),
        }
    }

    // c-ish spelling for the playground ui
    pub fn c_name(&self) -> String {
        match self {
            Type::Int => "int".into(),
            Type::UInt => "unsigned int".into(),
            Type::Char => "char".into(),
            Type::Void => "void".into(),
            Type::Arr(e, n) => format!("{}[{}]", e.c_name(), n),
            Type::Ptr(t) => format!("{}*", t.c_name()),
            Type::Struct(tag) => format!("struct {}", tag),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod,
    Eq, Ne, Lt, Le, Gt, Ge,
    And, Or,
    BAnd, BOr, BXor,
    Shl, Shr,
}

impl BinOp {
    pub fn name(self) -> &'static str {
        use BinOp::*;
        match self {
            Add => "add", Sub => "sub", Mul => "mul", Div => "div", Mod => "mod",
            Eq => "eq", Ne => "ne", Lt => "lt", Le => "le", Gt => "gt", Ge => "ge",
            And => "and", Or => "or", BAnd => "band", BOr => "bor", BXor => "bxor",
            Shl => "shl", Shr => "shr",
        }
    }

    pub fn symbol(self) -> &'static str {
        use BinOp::*;
        match self {
            Add => "+", Sub => "-", Mul => "*", Div => "/", Mod => "%",
            Eq => "==", Ne => "!=", Lt => "<", Le => "<=", Gt => ">", Ge => ">=",
            And => "&&", Or => "||", BAnd => "&", BOr => "|", BXor => "^",
            Shl => "<<", Shr => ">>",
        }
    }
}

impl Type {
    // python class name, for AttributeError messages
    pub fn class_name(&self) -> &'static str {
        match self {
            Type::Int => "TInt",
            Type::UInt => "TUInt",
            Type::Char => "TChar",
            Type::Void => "TVoid",
            Type::Arr(..) => "TArr",
            Type::Ptr(_) => "TPtr",
            Type::Struct(_) => "TStruct",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
    BNot,
}

impl UnOp {
    pub fn name(self) -> &'static str {
        match self {
            UnOp::Neg => "neg",
            UnOp::Not => "not",
            UnOp::BNot => "bnot",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            UnOp::Neg => "-",
            UnOp::Not => "!",
            UnOp::BNot => "~",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SizeofArg {
    Type(Type),
    Expr(Box<Expr>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    IntLit(i128),
    CharLit(char),
    StrLit(String),
    Var(String),
    Assign(Box<Expr>, Box<Expr>),
    CompAssign(BinOp, Box<Expr>, Box<Expr>),
    PostIncDec(Box<Expr>, i128),
    Sizeof(SizeofArg),
    VaArg(Type),
    BinOp(BinOp, Box<Expr>, Box<Expr>),
    UnOp(UnOp, Box<Expr>),
    Call(String, Vec<Expr>),
    Index(Box<Expr>, Box<Expr>),
    Member(Box<Expr>, String),
    Deref(Box<Expr>),
    AddrOf(Box<Expr>),
}

impl Expr {
    // python dataclass repr, used in error messages
    pub fn repr(&self) -> String {
        use crate::py::repr as r;
        match self {
            Expr::IntLit(v) => format!("IntLit(val={})", v),
            Expr::CharLit(c) => format!("CharLit(val={})", r(&c.to_string())),
            Expr::StrLit(v) => format!("StrLit(val={})", r(v)),
            Expr::Var(n) => format!("Var(name={})", r(n)),
            Expr::Assign(t, v) => format!("Assign(target={}, value={})", t.repr(), v.repr()),
            Expr::CompAssign(op, t, v) => format!("CompAssign(op={}, target={}, value={})", r(op.name()), t.repr(), v.repr()),
            Expr::PostIncDec(t, d) => format!("PostIncDec(target={}, delta={})", t.repr(), d),
            Expr::Sizeof(SizeofArg::Type(t)) => format!("Sizeof(operand={})", t.repr()),
            Expr::Sizeof(SizeofArg::Expr(e)) => format!("Sizeof(operand={})", e.repr()),
            Expr::VaArg(t) => format!("VaArg(ty={})", t.repr()),
            Expr::BinOp(op, l, rr) => format!("BinOp(op={}, lhs={}, rhs={})", r(op.name()), l.repr(), rr.repr()),
            Expr::UnOp(op, o) => format!("UnOp(op={}, operand={})", r(op.name()), o.repr()),
            Expr::Call(n, args) => {
                let a: Vec<String> = args.iter().map(|x| x.repr()).collect();
                format!("Call(name={}, args=[{}])", r(n), a.join(", "))
            }
            Expr::Index(b, i) => format!("Index(base={}, idx={})", b.repr(), i.repr()),
            Expr::Member(b, n) => format!("Member(base={}, name={})", b.repr(), r(n)),
            Expr::Deref(o) => format!("Deref(operand={})", o.repr()),
            Expr::AddrOf(o) => format!("AddrOf(operand={})", o.repr()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    Expr(Expr),
    Block(Vec<Stmt>),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    While(Expr, Box<Stmt>),
    Return(Option<Expr>),
    Decl(Type, String, Option<Expr>),
    For(Option<Box<Stmt>>, Option<Expr>, Option<Expr>, Box<Stmt>),
    Break,
    Continue,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Func {
    pub ret: Type,
    pub name: String,
    pub params: Vec<(Type, String)>,
    pub body: Stmt,
    pub varargs: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Global {
    pub ty: Type,
    pub name: String,
    pub init: Option<Expr>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TopDecl {
    Func(Func),
    Global(Global),
    StructDef(String),
    TypedefDef(String),
}

// struct tag -> ordered (member name, type) list, filled while parsing
pub type Structs = crate::py::OrderedMap<Vec<(String, Type)>>;
