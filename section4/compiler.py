"""
toy c compiler, converted from compiler.hs

pipeline: source text -> tokenize -> parse (ast) -> codegen -> arm assembly text

so far converted from haskell: ast node classes, lexer, and the partial
parser (program, type, ptr, top-level decl, global, params). the rest
(expressions, statements, blocks, codegen) is stubbed with todos below.
"""
from __future__ import annotations
import sys
from dataclasses import dataclass, field

# ---------- ast: types ----------

# type covers the c primitive types; TPtr nests for pointers
class Type: pass

@dataclass(frozen=True)
class TInt(Type): pass
@dataclass(frozen=True)
class TChar(Type): pass
@dataclass(frozen=True)
class TVoid(Type): pass
@dataclass(frozen=True)
class TPtr(Type):
    inner: Type

# ---------- ast: operators ----------

# binary operators our parser understands
ADD, SUB, MUL, DIV, MOD = "add", "sub", "mul", "div", "mod"
EQ, NE, LT, LE, GT, GE = "eq", "ne", "lt", "le", "gt", "ge"
AND, OR = "and", "or"

# unary operators such as negate and logical not
NEG, NOT = "neg", "not"

# ---------- ast: expressions ----------

# expr models all expression forms in our toy c language
class Expr: pass

@dataclass
class IntLit(Expr):
    val: int
@dataclass
class CharLit(Expr):
    val: str
@dataclass
class StrLit(Expr):
    val: str
@dataclass
class Var(Expr):
    name: str
@dataclass
class Assign(Expr):
    target: Expr
    value: Expr
@dataclass
class BinOp(Expr):
    op: str
    lhs: Expr
    rhs: Expr
@dataclass
class UnOp(Expr):
    op: str
    operand: Expr
@dataclass
class Call(Expr):
    name: str
    args: list
@dataclass
class Index(Expr):      # a[i] == *(a + i)
    base: Expr
    idx: Expr
@dataclass
class Deref(Expr):      # *p
    operand: Expr
@dataclass
class AddrOf(Expr):     # &x
    operand: Expr

# ---------- ast: statements ----------

# stmt captures statement forms: blocks, conditionals, loops, declarations
class Stmt: pass

@dataclass
class SExpr(Stmt):
    expr: Expr
@dataclass
class SBlock(Stmt):
    stmts: list
@dataclass
class SIf(Stmt):
    cond: Expr
    then: Stmt
    els: Stmt | None
@dataclass
class SWhile(Stmt):
    cond: Expr
    body: Stmt
@dataclass
class SReturn(Stmt):
    expr: Expr | None
@dataclass
class SDecl(Stmt):
    type: Type
    name: str
    init: Expr | None

# ---------- ast: top level ----------

# topdecl distinguishes function definitions from global variables
@dataclass
class Func:
    ret: Type
    name: str
    params: list   # list of (Type, str)
    body: Stmt
@dataclass
class Global:
    type: Type
    name: str
    init: Expr | None

# ---------- lexer ----------

# tok kinds mirror the haskell Tok variants
TKW, TID, TINTL, TCHRL, TSTRL, TP, TO = "kw", "id", "intl", "chrl", "strl", "p", "o"

@dataclass
class Tok:
    kind: str
    val: object
    def __eq__(self, other):
        return isinstance(other, Tok) and self.kind == other.kind and self.val == other.val
    def __repr__(self):
        return f"{self.kind}:{self.val!r}"

# reserved words so identifiers do not capture them
KEYWORDS = ["int", "char", "void", "if", "while", "return", "else"]

# ordered so multi-char symbols match before single chars
OPERATORS = [
    "==", "!=", "<=", ">=", "&&", "||",
    "+", "-", "*", "/", "%", "<", ">", "=", "!", "&",
]

PUNCT = "(){}[];,"

def esc(c: str) -> str:
    # convert an escape code into the actual character
    return {"n": "\n", "t": "\t", "0": "\0", "\\": "\\", "'": "'", '"': '"'}.get(c, c)

def tokenize(src: str) -> list:
    # consume the input characters and return a token list
    toks = []
    i, n = 0, len(src)
    while i < n:
        c = src[i]
        # line comment
        if src.startswith("//", i):
            while i < n and src[i] != "\n":
                i += 1
            continue
        # block comment
        if src.startswith("/*", i):
            i += 2
            while i < n and not src.startswith("*/", i):
                i += 1
            if i >= n:
                raise SyntaxError("unterminated /* comment")
            i += 2
            continue
        # whitespace
        if c.isspace():
            i += 1
            continue
        # integer literal
        if c.isdigit():
            j = i
            while j < n and src[j].isdigit():
                j += 1
            toks.append(Tok(TINTL, int(src[i:j])))
            i = j
            continue
        # char literal
        if c == "'":
            i += 1
            if src[i] == "\\":
                ch = esc(src[i + 1])
                i += 2
            else:
                ch = src[i]
                i += 1
            if src[i] != "'":
                raise SyntaxError("lex: malformed character literal")
            i += 1
            toks.append(Tok(TCHRL, ch))
            continue
        # string literal
        if c == '"':
            i += 1
            buf = []
            while i < n and src[i] != '"':
                if src[i] == "\\":
                    buf.append(esc(src[i + 1]))
                    i += 2
                else:
                    buf.append(src[i])
                    i += 1
            if i >= n:
                raise SyntaxError("unterminated string")
            i += 1
            toks.append(Tok(TSTRL, "".join(buf)))
            continue
        # identifier or keyword
        if c.isalpha() or c == "_":
            j = i
            while j < n and (src[j].isalnum() or src[j] == "_"):
                j += 1
            word = src[i:j]
            toks.append(Tok(TKW, word) if word in KEYWORDS else Tok(TID, word))
            i = j
            continue
        # punctuation
        if c in PUNCT:
            toks.append(Tok(TP, c))
            i += 1
            continue
        # operator (longest match first)
        op = next((o for o in OPERATORS if src.startswith(o, i)), None)
        if op is None:
            raise SyntaxError(f"lex: unexpected char {c!r}")
        toks.append(Tok(TO, op))
        i += len(op)
    return toks


class Parser:
    def __init__(self, toks: list):
        self.toks = toks
        self.pos = 0

    def peek(self, k=0):
        i = self.pos + k
        return self.toks[i] if i < len(self.toks) else None

    def next(self):
        t = self.peek()
        self.pos += 1
        return t

    def at_end(self):
        return self.pos >= len(self.toks)

    def preview(self):
        return repr(self.toks[self.pos:self.pos + 3])

    def expect(self, kind, val):
        t = self.peek()
        if t is not None and t.kind == kind and t.val == val:
            self.pos += 1
            return t
        raise SyntaxError(f"parse: expected {kind}:{val!r}, got {self.preview()}")

    # ----- top level -----

    def parse_program(self) -> list:
        decls = []
        while not self.at_end():
            decls.append(self.parse_top())
        return decls

    def parse_type(self) -> Type:
        t = self.peek()
        base = None
        if t == Tok(TKW, "int"):
            base = TInt()
        elif t == Tok(TKW, "char"):
            base = TChar()
        elif t == Tok(TKW, "void"):
            base = TVoid()
        else:
            raise SyntaxError(f"parse: expected type, got {self.preview()}")
        self.pos += 1
        return self.parse_ptr(base)

    def parse_ptr(self, t: Type) -> Type:
        # consume trailing stars, nesting TPtr for each pointer level
        while self.peek() == Tok(TO, "*"):
            self.pos += 1
            t = TPtr(t)
        return t

    def parse_top(self):
        # one top-level decl: function or global. both start type + name.
        # a '(' after the name means function.
        ty = self.parse_type()
        name_tok = self.peek()
        if name_tok is None or name_tok.kind != TID:
            raise SyntaxError(f"parse: expected declarator, got {self.preview()}")
        name = name_tok.val
        self.pos += 1
        if self.peek() == Tok(TP, "("):
            self.pos += 1
            params = self.parse_params()
            body = self.parse_block()
            return Func(ty, name, params, body)
        return self.parse_global(ty, name)

    def parse_global(self, ty: Type, name: str):
        # finish a global: optional initializer then ';'
        if self.peek() == Tok(TO, "="):
            self.pos += 1
            e = self.parse_expr()
            self.expect(TP, ";")
            return Global(ty, name, e)
        self.expect(TP, ";")
        return Global(ty, name, None)

    def parse_params(self) -> list:
        # comma separated params up to ')'
        if self.peek() == Tok(TP, ")"):
            self.pos += 1
            return []
        if self.peek() == Tok(TKW, "void") and self.peek(1) == Tok(TP, ")"):
            self.pos += 2
            return []
        params = []
        while True:
            ty = self.parse_type()
            name_tok = self.peek()
            if name_tok is None or name_tok.kind != TID:
                raise SyntaxError(f"parse: bad parameter, got {self.preview()}")
            params.append((ty, name_tok.val))
            self.pos += 1
            sep = self.next()
            if sep == Tok(TP, ")"):
                return params
            if sep != Tok(TP, ","):
                raise SyntaxError(f"parse: bad parameter, got {self.preview()}")

    # ----- statements -----

    def is_type_start(self) -> bool:
        # true when the cursor sits on a type keyword starting a declaration
        t = self.peek()
        return t == Tok(TKW, "int") or t == Tok(TKW, "char") or t == Tok(TKW, "void")

    def parse_block(self) -> Stmt:
        # '{' stmt* '}' into SBlock
        self.expect(TP, "{")
        stmts = []
        while self.peek() != Tok(TP, "}"):
            if self.at_end():
                raise SyntaxError("parse: unterminated block, expected '}'")
            stmts.append(self.parse_stmt())
        self.expect(TP, "}")
        return SBlock(stmts)

    def parse_stmt(self) -> Stmt:
        # dispatch on the leading token to the matching statement form
        t = self.peek()
        if t == Tok(TP, "{"):
            return self.parse_block()
        if t == Tok(TKW, "if"):
            return self.parse_if()
        if t == Tok(TKW, "while"):
            return self.parse_while()
        if t == Tok(TKW, "return"):
            return self.parse_return()
        if self.is_type_start():
            return self.parse_decl()
        # fallthrough: expression statement, 'expr ;'
        e = self.parse_expr()
        self.expect(TP, ";")
        return SExpr(e)

    def parse_if(self) -> Stmt:
        # 'if' '(' cond ')' then ['else' els]
        self.expect(TKW, "if")
        self.expect(TP, "(")
        cond = self.parse_expr()
        self.expect(TP, ")")
        then = self.parse_stmt()
        els = None
        if self.peek() == Tok(TKW, "else"):
            self.pos += 1
            els = self.parse_stmt()
        return SIf(cond, then, els)

    def parse_while(self) -> Stmt:
        # 'while' '(' cond ')' body
        self.expect(TKW, "while")
        self.expect(TP, "(")
        cond = self.parse_expr()
        self.expect(TP, ")")
        body = self.parse_stmt()
        return SWhile(cond, body)

    def parse_return(self) -> Stmt:
        # 'return' [expr] ';'
        self.expect(TKW, "return")
        if self.peek() == Tok(TP, ";"):
            self.pos += 1
            return SReturn(None)
        e = self.parse_expr()
        self.expect(TP, ";")
        return SReturn(e)

    def parse_decl(self) -> Stmt:
        # local declaration: type name ['=' init] ';'
        ty = self.parse_type()
        name_tok = self.peek()
        if name_tok is None or name_tok.kind != TID:
            raise SyntaxError(f"parse: expected declarator, got {self.preview()}")
        name = name_tok.val
        self.pos += 1
        init = None
        if self.peek() == Tok(TO, "="):
            self.pos += 1
            init = self.parse_expr()
        self.expect(TP, ";")
        return SDecl(ty, name, init)

    # ----- expressions -----
    # a ladder of parse functions, lowest precedence first. each level parses
    # the next-tighter level then folds its own operators left-to-right (except
    # assignment, which is right associative).

    def parse_expr(self) -> Expr:
        return self.parse_assign()

    def parse_assign(self) -> Expr:
        # right associative: lhs '=' rhs
        lhs = self.parse_logic_or()
        if self.peek() == Tok(TO, "="):
            self.pos += 1
            rhs = self.parse_assign()
            return Assign(lhs, rhs)
        return lhs

    def parse_binop(self, ops: dict, lower) -> Expr:
        # generic left-associative fold: ops maps token value -> op constant,
        # lower is the next-tighter precedence parser.
        lhs = lower()
        while True:
            t = self.peek()
            if t is not None and t.kind == TO and t.val in ops:
                self.pos += 1
                rhs = lower()
                lhs = BinOp(ops[t.val], lhs, rhs)
            else:
                return lhs

    def parse_logic_or(self) -> Expr:
        return self.parse_binop({"||": OR}, self.parse_logic_and)

    def parse_logic_and(self) -> Expr:
        return self.parse_binop({"&&": AND}, self.parse_equality)

    def parse_equality(self) -> Expr:
        return self.parse_binop({"==": EQ, "!=": NE}, self.parse_relational)

    def parse_relational(self) -> Expr:
        return self.parse_binop({"<": LT, "<=": LE, ">": GT, ">=": GE}, self.parse_additive)

    def parse_additive(self) -> Expr:
        return self.parse_binop({"+": ADD, "-": SUB}, self.parse_term)

    def parse_term(self) -> Expr:
        return self.parse_binop({"*": MUL, "/": DIV, "%": MOD}, self.parse_unary)

    def parse_unary(self) -> Expr:
        # prefix operators: '-' negate, '!' logical not, '*' deref, '&' addr-of
        t = self.peek()
        if t == Tok(TO, "-"):
            self.pos += 1
            return UnOp(NEG, self.parse_unary())
        if t == Tok(TO, "!"):
            self.pos += 1
            return UnOp(NOT, self.parse_unary())
        if t == Tok(TO, "*"):
            self.pos += 1
            return Deref(self.parse_unary())
        if t == Tok(TO, "&"):
            self.pos += 1
            return AddrOf(self.parse_unary())
        return self.parse_postfix()

    def parse_postfix(self) -> Expr:
        # primary followed by zero or more call '(' args ')' or index '[' i ']'
        e = self.parse_primary()
        while True:
            t = self.peek()
            if t == Tok(TP, "("):
                # a call only makes sense directly on a named function
                if not isinstance(e, Var):
                    raise SyntaxError(f"parse: call target must be a name, got {e}")
                self.pos += 1
                args = self.parse_args()
                e = Call(e.name, args)
            elif t == Tok(TP, "["):
                self.pos += 1
                idx = self.parse_expr()
                self.expect(TP, "]")
                e = Index(e, idx)
            else:
                return e

    def parse_args(self) -> list:
        # comma separated argument expressions up to ')'
        if self.peek() == Tok(TP, ")"):
            self.pos += 1
            return []
        args = []
        while True:
            args.append(self.parse_expr())
            sep = self.next()
            if sep == Tok(TP, ")"):
                return args
            if sep != Tok(TP, ","):
                raise SyntaxError(f"parse: bad argument list, got {self.preview()}")

    def parse_primary(self) -> Expr:
        # atoms: literals, a bare name, or a parenthesized expression
        t = self.peek()
        if t is None:
            raise SyntaxError("parse: unexpected end of input in expression")
        if t.kind == TINTL:
            self.pos += 1
            return IntLit(t.val)
        if t.kind == TCHRL:
            self.pos += 1
            return CharLit(t.val)
        if t.kind == TSTRL:
            self.pos += 1
            return StrLit(t.val)
        if t.kind == TID:
            self.pos += 1
            return Var(t.val)
        if t == Tok(TP, "("):
            self.pos += 1
            e = self.parse_expr()
            self.expect(TP, ")")
            return e
        raise SyntaxError(f"parse: expected expression, got {self.preview()}")

# Target is 32bit ARM, int, char and every pointer all occupy one 
WORD = 4 

def type_size(t: Type) -> int:
    if isinstance(t, TChar):
        return 1 
    if isinstance(t, (TInt, TPtr)):
        return WORD 
    if isinstance(t, TVoid):
        raise TypeError("codegen: void has no size")
    raise TypeError(f"codegen: unknown type {t}")

def scale_of(t: Type) -> int:
    # pointer arithmetic step: *(p + i) advances by the size of *p.
    # p + i ==  p + i * scale_of(typeof(p)). for a non-pointer we scale by 1 
    if isinstance(t, TPtr):
        return type_size(t.inner)
    return 1 

class Frame:
    def __init__(self):
        self.slots = {}
        self.types = {}
        self.size = 0 

    def declare(self, name: str, t: Type) -> int:
        if name not in self.slots:
            self.size += WORD 
            self.slots[name] = -self.size 
            self.types[name] = t 
        return self.slots[name]

    def offset(self, name: str) -> int:
        if name not in self.slots:
            raise KeyError(f"codegen: undeclared local {name!r}")
        return self.slots[name]

    def type_of(self, name: str) -> Type:
        return self.types[name]

    def has(self, name: str) -> bool:
        return name in self.slots 

    def frame_bytes(self) -> int:
        # total locals size, rounded to 8 byte so the stack 
        # pointer stays 8 byte aligned per the ARM aapcs
        return (self.size + 7) & ~7

def collect_locals(frame: Frame, stmt: Stmt):
    if isinstance(stmt, SDecl):
        frame.declare(stmt.name, stmt.type)
    elif isinstance(stmt, SBlock):
        for s in stmt.stmts:
            collect_locals(frame, s)
    elif isinstance(stmt, SIf):
        collect_locals(frame, stmt.then)
        if stmt.els is not None:
            collect_locals(frame, stmt.els)
    elif isinstance(stmt, SWhile):
        collect_locals(frame, stmt.body)

def collect_strings(decls: list) -> dict:
    out = {}
    counter = [0]

    def visit_expr(e):
        if e is None:
            return
        if isinstance(e, StrLit):
            if e.val not in out:
                out[e.val] = f".Lstr{counter[0]}"
                counter[0] += 1
        elif isinstance(e, Assign):
            visit_expr(e.target); visit_expr(e.value)
        elif isinstance(e, BinOp):
            visit_expr(e.lhs); visit_expr(e.rhs)
        elif isinstance(e, UnOp):
            visit_expr(e.operand)
        elif isinstance(e, Call):
            for a in e.args:
                visit_expr(a)
        elif isinstance(e, Index):
            visit_expr(e.base); visit_expr(e.idx)
        elif isinstance(e, (Deref, AddrOf)):
            visit_expr(e.operand)

    def visit_stmt(s):
        if isinstance(s, SExpr):
            visit_expr(s.expr)
        elif isinstance(s, SBlock):
            for x in s.stmts:
                visit_stmt(x)
        elif isinstance(s, SIf):
            visit_expr(s.cond); visit_stmt(s.then)
            if s.els is not None:
                visit_stmt(s.els)
        elif isinstance(s, SWhile):
            visit_expr(s.cond); visit_stmt(s.body)
        elif isinstance(s, SReturn):
            visit_expr(s.expr)
        elif isinstance(s, SDecl):
            visit_expr(s.init)

    for d in decls:
        if isinstance(d, Func):
            visit_stmt(d.body)
        elif isinstance(d, Global):
            visit_expr(d.init)
    return out

def c_string_literal(s: str) -> str:
    # render a python string as a .asciz operand, wrap in quotes and
    # escape the characters that matter
    out = ['"']
    for ch in s:
        if ch == '\\':
            out.append('\\\\')
        elif ch == '"':
            out.append('\\"')
        elif ch == '\n':
            out.append('\\n')
        elif ch == '\t':
            out.append('\\t')
        elif ch == '\0':
            out.append('\\000')
        elif 32 <= ord(ch) < 127:
            out.append(ch)
        else:
            out.append(f'\\{ord(ch):03o}')
    out.append('"')
    return "".join(out)
# ---------- codegen ----------
# target: gnu-style arm assembly text (gas syntax). a later pass / our own
# extended assembler consumes this. model: each expression computes into r0;
# temporaries are pushed/popped. aapcs calling convention (args r0-r3 then
# stack, return in r0, fp=r11, lr=r14, sp=r13).

class CodeGen:
    def __init__(self):
        self.lines = []          # output assembly lines
        self.strings = {}        # string literal -> label, for .data
        self.globals = []        # list of Global nodes, emitted into .data/.bss
        self.label_count = 0
        self.frame = None        # current function's Frame (set in gen_func)
        self.epilogue = None     # current function's epilogue label (for return)

    # ----- emit helpers -----

    def emit(self, line: str = ""):
        # append one line of assembly. blank line for spacing when empty.
        self.lines.append(line)

    def label(self, name: str):
        # emit a label definition 'name:' at column 0
        self.lines.append(f"{name}:")

    def new_label(self, prefix="L") -> str:
        # unique local label, e.g. '.L3'. used for branch targets.
        self.label_count += 1
        return f".{prefix}{self.label_count}"

    # ----- program -----

    def gen_program(self, decls: list) -> str:
        # split top-level decls, gather strings, then emit .data then .text.
        self.strings = collect_strings(decls)
        funcs = [d for d in decls if isinstance(d, Func)]
        self.globals = [d for d in decls if isinstance(d, Global)]

        # .data section: globals with initializers + all string literals
        self.emit(".data")
        for g in self.globals:
            self.emit(f".global {g.name}")
            self.label(g.name)
            if g.init is None:
                # uninitialized global: reserve its size, zeroed
                self.emit(f"    .zero {type_size(g.type)}")
            elif isinstance(g.init, IntLit):
                self.emit(f"    .word {g.init.val}")
            elif isinstance(g.init, CharLit):
                self.emit(f"    .byte {ord(g.init.val)}")
            elif isinstance(g.init, StrLit):
                # a global char* initialized to a string literal points at
                # the literal's label; the bytes are emitted below.
                self.emit(f"    .word {self.strings[g.init.val]}")
            else:
                raise NotImplementedError(
                    f"codegen: global initializer must be a constant, got {g.init}")

        # emit each unique string literal once, as a label + nul-terminated bytes
        for value, lbl in self.strings.items():
            self.label(lbl)
            self.emit(f"    .asciz {c_string_literal(value)}")

        # .text section: the functions
        self.emit()
        self.emit(".text")
        for fn in funcs:
            self.gen_func(fn)
            self.emit()

        return "\n".join(self.lines) + "\n"

    # ----- functions -----

    def gen_func(self, fn: Func):
        # build the frame: params first (so their slots are deterministic),
        # then every local declared anywhere in the body.
        self.frame = Frame()
        for (ty, name) in fn.params:
            self.frame.declare(name, ty)
        collect_locals(self.frame, fn.body)
        frame_bytes = self.frame.frame_bytes()

        # a per-function epilogue label so 'return' anywhere can jump to the
        # single restore-and-exit sequence.
        self.epilogue = self.new_label("Lret")

        self.emit(f".global {fn.name}")
        self.label(fn.name)

        # prologue: save fp/lr, set fp to the new frame base, reserve locals.
        self.emit("    push {fp, lr}")
        self.emit("    mov fp, sp")
        if frame_bytes:
            self.emit(f"    sub sp, sp, #{frame_bytes}")

        # spill incoming register args (r0-r3) into their stack slots so the
        # rest of codegen can treat params like any other local. args beyond
        # the fourth were passed on the stack by the caller (handled later).
        for i, (ty, name) in enumerate(fn.params):
            if i < 4:
                off = self.frame.offset(name)
                self.emit(f"    str r{i}, [fp, #{off}]")
            else:
                raise NotImplementedError(
                    "codegen: more than 4 params (stack args) not yet supported")

        # body
        self.gen_stmt(fn.body)

        # epilogue: restore sp/fp/lr and return. a function that falls off the
        # end returns garbage in r0, matching c's undefined behavior.
        self.label(self.epilogue)
        self.emit("    mov sp, fp")
        self.emit("    pop {fp, lr}")
        self.emit("    bx lr")

    def gen_stmt(self, s: Stmt):
        # TODO: one case per Stmt subclass. loops/ifs emit branches + labels.
        raise NotImplementedError("gen_stmt not implemented yet")

    def gen_expr(self, e: Expr):
        # TODO: evaluate expr into r0 (simple accumulator model), push/pop
        #       r0 to a temp for binary ops. handle lvalues for Assign/AddrOf.
        raise NotImplementedError("gen_expr not implemented yet")


def compile_text(src: str) -> str:
    toks = tokenize(src)
    ast = Parser(toks).parse_program()
    return CodeGen().gen_program(ast)

def main(argv):
    if len(argv) < 2:
        print("usage: compiler.py input.c [-o out.s]", file=sys.stderr)
        return 1
    with open(argv[1]) as f:
        src = f.read()
    out = compile_text(src)
    if "-o" in argv:
        dst = argv[argv.index("-o") + 1]
        with open(dst, "w") as f:
            f.write(out)
    else:
        sys.stdout.write(out)
    return 0

if __name__ == "__main__":
    sys.exit(main(sys.argv))
