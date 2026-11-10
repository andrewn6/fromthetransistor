"""
C Compiler, written in Python

Source text -> Tokenized -> Parsed -> Codegen -> ARM assembly text

supported: int/char/void + pointers, arrays (char buf[512] locals and
globals, decay to element pointer, string init), globals, string literals,
full binary op set (div/mod via __divsi3/__modsi3/__udivsi3), unary - ! * &,
a[i], calls (max 4 args), if/else, while, for, break/continue, local decls,
compound assignment (+= -= *= /= %=), x++/x--, structs (member access,
->, nested types, c layout with alignment/padding), typedef, sizeof,
const/unsigned/signed/long/short qualifiers, unsigned compares + division,
varargs (printf), and a minimal preprocessor (#include "...", object-like
#define). proven by build.py + test.c + vatest.c.
"""
from __future__ import annotations
import sys
from dataclasses import dataclass, field
import os 
import re 

# ---------- ast: types ----------

# type covers the c primitive types; TPtr nests for pointers
class Type: pass

@dataclass(frozen=True)
class TInt(Type): pass
@dataclass(frozen=True)
class TUInt(Type): pass
@dataclass(frozen=True)
class TChar(Type): pass
@dataclass(frozen=True)
class TVoid(Type): pass
@dataclass(frozen=True)
class TArr(Type):
    elem: Type 
    count: int 
@dataclass(frozen=True)
class TPtr(Type):
    inner: Type
@dataclass(frozen=True)
class TStruct(Type):
    tag: str

# ---------- ast: operators ----------

## operator constants
BINOPS = (
    "add", "sub", "mul", "div", "mod", # arithmetic 
    "eq", "ne", "lt", "le", "gt", "ge",
    "and", "or",
    "band", "bor", "bxor",
    "shl", "shr",
)
ADD, SUB, MUL, DIV, MOD, EQ, NE, LT, LE, GT, GE, AND, OR, \
    BAND, BOR, BXOR, SHL, SHR = BINOPS

UNOPS = ("neg", "not", "bnot")
NEG, NOT, BNOT = UNOPS

# compound assignment token -> the underlying binary op
COMPOUND = {"+=": ADD, "-=": SUB, "*=": MUL, "/=": DIV, "%=": MOD,
            "|=": BOR, "&=": BAND, "^=": BXOR, "<<=": SHL, ">>=": SHR}

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
class CompAssign(Expr):
    op: str 
    target: Expr
    value: Expr
@dataclass 
class PostIncDec(Expr):
    target: Expr 
    delta: int
@dataclass 
class Sizeof(Expr):
    operand: object

@dataclass
class VaArg(Expr):
    ty: Type

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
class Member(Expr):
    base: Expr 
    name: str 
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
@dataclass 
class SFor(Stmt):
    init: Stmt | None 
    cond: Expr | None 
    step: Expr | None 
    body: Stmt
@dataclass 
class SBreak(Stmt): pass 
@dataclass
class SContinue(Stmt): pass

# ---------- ast: top level ----------

# topdecl distinguishes function definitions from global variables
@dataclass
class Func:
    ret: Type
    name: str
    params: list   # list of (Type, str)
    body: Stmt
    varargs: bool = False
@dataclass
class Global:
    type: Type
    name: str
    init: Expr | None
@dataclass 
class StructDef:
    tag: str
@dataclass 
class TypedefDef:
    name: str 

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
KEYWORDS = ["int", "char", "void", "if", "while", "return", "else", "for", "break", "continue", "struct", "typedef", "sizeof", "const", "unsigned", "signed", "long", "short"]

# ordered so multi-char symbols match before single chars
OPERATORS = [
    "==", "!=", "<=", ">=", "&&", "||",
    "<<=", ">>=",
    "+=", "-=", "*=", "/=", "%=", "|=", "&=", "^=", "++", "--", "->",
    "<<", ">>",
    "+", "-", "*", "/", "%", "<", ">", "=", "!", "&", "|", "^", "~",
]

PUNCT = "(){}[];,."

def esc(c: str) -> str:
    # convert an escape code into the actual character
    return {"n": "\n", "t": "\t", "0": "\0", "\\": "\\", "'": "'", '"': '"'}.get(c, c)

DEFINES: dict = {}

def preprocess(src: str, path: str = ".") -> str:
    out = []
    for line in src.splitlines():
        s = line.strip()
        if s.startswith("#include"):
            fn = s[len("#include"):].strip()
            if not (fn.startswith('"') and fn.endswith('"')):
                raise SyntaxError(f'preprocess: only #include "file" is supported: {s!r}')
            fn = fn[1:-1]
            base = os.path.dirname(os.path.abspath(path))
            inc = os.path.join(base, fn)
            with open(inc) as f:
                out.append(preprocess(f.read(), inc))
        elif s.startswith("#define"):
            parts = s.split(None, 2)
            if len(parts) < 2:
                raise SyntaxError(f"preprocess: bad #define: {s!r}")
            name = parts[1]
            if "(" in name:
                raise SyntaxError(f"preprocess: function-like macros not supported: {s!r}")
            DEFINES[name] = parts[2] if len(parts) > 2 else ""
        elif s.startswith("#"):
            raise SyntaxError(f"preprocess: unsupported directive: {s!r}")
        else:
            out.append(line)
    text = "\n".join(out)
    for name, val in DEFINES.items():
        text = re.sub(rf"\b{re.escape(name)}\b", val.replace("\\", "\\\\"), text)
    return text

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
        # integer literal (decimal or 0x hex)
        if c.isdigit():
            j = i
            if c == "0" and i + 1 < n and src[i + 1] in "xX" \
                    and i + 2 < n and src[i + 2] in "0123456789abcdefABCDEF":
                j = i + 2
                while j < n and src[j] in "0123456789abcdefABCDEF":
                    j += 1
                toks.append(Tok(TINTL, int(src[i:j], 16)))
            else:
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
        self.typedefs = {}

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

        unsigned = False 
        while True:
            t = self.peek()
            if t == Tok(TKW, "const"):
                self.pos += 1 
            elif t == Tok(TKW, "unsigned"):
                unsigned = True 
                self.pos += 1
            elif t in (Tok(TKW, "signed"), Tok(TKW, "long"), Tok(TKW, "short")):
                self.pos += 1
            else:
                break

        t = self.peek()
        if t == Tok(TKW, "int"):
            self.pos += 1
            base = TUInt() if unsigned else TInt()
        elif t == Tok(TKW, "char"):
            self.pos += 1
            base = TChar()          # loads zero-extend, so char == unsigned char
        elif t == Tok(TKW, "void"):
            self.pos += 1
            base = TVoid()
        elif t == Tok(TKW, "struct"):
            self.pos += 1
            tag_tok = self.peek()
            if tag_tok is None or tag_tok.kind != TID:
                raise SyntaxError(f"parse: expected struct tag, got {self.preview()}")
            self.pos += 1
            if self.peek() == Tok(TP, "{"):
                # definition: 'struct tag { members }', registers the layout
                STRUCTS[tag_tok.val] = self.parse_struct_members()
            return self.parse_ptr(TStruct(tag_tok.val))
        elif t is not None and t.kind == TID and t.val in self.typedefs:
            # a typedef name: resolve to its aliased type
            self.pos += 1
            return self.parse_ptr(self.typedefs[t.val])
        elif unsigned and t is not None and t.kind == TID:
            base = TUInt()          # bare 'unsigned x' == 'unsigned int x'
        else:
            raise SyntaxError(f"parse: expected type, got {self.preview()}")
        return self.parse_ptr(base)

    
    def parse_array_suffix(self, t: Type) -> Type:
        if self.peek() == Tok(TP, "["):
            self.pos += 1 
            n = self.peek()
            if n is None or n.kind != TINTL:
                raise SyntaxError(f"parse: expected array size, got {self.preview()}")
            self.pos += 1 
            self.expect(TP, "]")
            return TArr(t, n.val)
        return t 

    def parse_ptr(self, t: Type) -> Type:
        # consume trailing stars, nesting TPtr for each pointer level
        while self.peek() == Tok(TO, "*"):
            self.pos += 1
            t = TPtr(t)
        return t

    def parse_struct_members(self) -> list:
        # '{' type declarator [array suffix] ';')* '}'
        self.expect(TP, "{")
        members = []
        while self.peek() != Tok(TP, "}"):
            if self.at_end():
                raise SyntaxError("parse: unterminated struct body, expected '}'")
            ty = self.parse_type()
            name_tok = self.peek()
            if name_tok is None or name_tok.kind != TID:
                raise SyntaxError(f"parse: expected member name, got {self.preview()}")
            self.pos += 1 
            ty = self.parse_array_suffix(ty)
            self.expect(TP, ";")
            members.append((name_tok.val, ty))
        self.expect(TP, "}")
        return members
    
    def parse_top(self):
        # one top-level decl: function or global. both start type + name.
        # a '(' after the name means function.
        if self.peek() == Tok(TKW, "struct") and self.peek(1) is not None \
                and self.peek(1).kind == TID and self.peek(2) == Tok(TP, "{"):
            ty = self.parse_type()
            self.expect(TP, ";")
            return StructDef(ty.tag)

        if self.peek() == Tok(TKW, "typedef"):
            self.pos += 1 
            ty = self.parse_type()
            name_tok = self.peek()
            if name_tok is None or name_tok.kind != TID:
                raise SyntaxError(f"parse: expected typedef name, got {self.preview()}")
            self.pos += 1 
            self.expect(TP, ";")
            self.typedefs[name_tok.val] = ty 
            return TypedefDef(name_tok.val)
        
        ty = self.parse_type()
        name_tok = self.peek()
        if name_tok is None or name_tok.kind != TID:
            raise SyntaxError(f"parse: expected declarator, got {self.preview()}")
        name = name_tok.val
        self.pos += 1
        if self.peek() == Tok(TP, "("):
            self.pos += 1
            params, varargs = self.parse_params()
            body = self.parse_block()
            return Func(ty, name, params, body, varargs)
        return self.parse_global(ty, name)

    def parse_global(self, ty: Type, name: str):
        # finish a global: optional initializer then ';'
        ty = self.parse_array_suffix(ty)
        if self.peek() == Tok(TO, "="):
            self.pos += 1
            e = self.parse_expr()
            self.expect(TP, ";")
            return Global(ty, name, e)
        self.expect(TP, ";")
        return Global(ty, name, None)

    def parse_params(self):
        # comma separated params up to ')'. a trailing '...' marks a
        # varargs function; the anonymous args arrive in r{len(params)}..r3.
        if self.peek() == Tok(TP, ")"):
            self.pos += 1
            return [], False
        if self.peek() == Tok(TKW, "void") and self.peek(1) == Tok(TP, ")"):
            self.pos += 2
            return [], False
        params = []
        while True:
            if self.peek() == Tok(TP, "."):
                if self.peek(1) == Tok(TP, ".") and self.peek(2) == Tok(TP, "."):
                    self.pos += 3 
                    self.expect(TP, ")")
                    return params, True 
                raise SyntaxError(f"parse: bad parameter, got {self.preview()}")

            ty = self.parse_type()
            name_tok = self.peek()
            if name_tok is None or name_tok.kind != TID:
                raise SyntaxError(f"parse: bad parameter, got {self.preview()}")
            params.append((ty, name_tok.val))
            self.pos += 1
            sep = self.next()
            if sep == Tok(TP, ")"):
                return params, False 
            if sep != Tok(TP, ","):
                raise SyntaxError(f"parse: bad parameter, got {self.preview()}")

    # ----- statements -----

    def is_type_start(self) -> bool:
        # true when the cursor sits on a type keyword or typedef name
        t = self.peek()
        if t is None:
            return False
        if t == Tok(TKW, "int") or t == Tok(TKW, "char") \
                or t == Tok(TKW, "void") or t == Tok(TKW, "struct") \
                or t in (Tok(TKW, "const"), Tok(TKW, "unsigned"),
                         Tok(TKW, "signed"), Tok(TKW, "long"), Tok(TKW, "short")):
            return True
        return t.kind == TID and t.val in self.typedefs
    
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
        if t == Tok(TKW, "for"):
            return self.parse_for()
        if t == Tok(TKW, "return"):
            return self.parse_return()
        if t == Tok(TKW, "break"):
            return self.parse_break()
        if t == Tok(TKW, "continue"):
            return self.parse_continue()
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

    def parse_for(self) -> Stmt:
        self.expect(TKW, "for")
        self.expect(TP, "(")
        if self.peek() == Tok(TP, ";"):
            self.pos += 1 
            init = None 
        elif self.is_type_start():
            init = self.parse_decl()
        else:
            e = self.parse_expr() 
            self.expect(TP, ";")
            init = SExpr(e)
        cond = None 
        if self.peek() != Tok(TP, ";"):
            cond = self.parse_expr()
        self.expect(TP, ";")
        step = None 
        if self.peek() != Tok(TP, ")"):
            step = self.parse_expr()
        self.expect(TP, ")")
        body = self.parse_stmt()
        return SFor(init, cond, step, body)

    def parse_break(self) -> Stmt:
        self.expect(TKW, "break")
        self.expect(TP, ";")
        return SBreak()

    def parse_continue(self) -> Stmt:
        self.expect(TKW, "continue")
        self.expect(TP, ";")
        return SContinue()
    
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
        ty = self.parse_array_suffix(ty)
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
        t = self.peek()
        if t == Tok(TO, "="):
            self.pos += 1 
            rhs = self.parse_assign()
            return Assign(lhs, rhs)
        if t is not None and t.kind == TO and t.val in COMPOUND:
            self.pos += 1 
            rhs = self.parse_assign()
            return CompAssign(COMPOUND[t.val], lhs, rhs)
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

    def parse_logic_and(self) -> Expr:
        return self.parse_binop({"&&": AND}, self.parse_bit_or)

    def parse_bit_or(self) -> Expr:
        # '|' is the loosest of the bitwise family
        return self.parse_binop({"|": BOR}, self.parse_bit_xor)

    def parse_bit_xor(self) -> Expr:
        return self.parse_binop({"^": BXOR}, self.parse_bit_and)

    def parse_bit_and(self) -> Expr:
        return self.parse_binop({"&": BAND}, self.parse_equality)
    
    def parse_relational(self) -> Expr:
        return self.parse_binop({"<": LT, "<=": LE, ">": GT, ">=": GE}, self.parse_shift)

    def parse_shift(self) -> Expr:
        return self.parse_binop({"<<": SHL, ">>": SHR}, self.parse_additive)
    
    def parse_logic_or(self) -> Expr:
        return self.parse_binop({"||": OR}, self.parse_logic_and)

    def parse_equality(self) -> Expr:
        return self.parse_binop({"==": EQ, "!=": NE}, self.parse_relational)

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
        if t == Tok(TO, "~"):
            self.pos += 1 
            return UnOp(BNOT, self.parse_unary())
        if t == Tok(TO, "!"):
            self.pos += 1
            return UnOp(NOT, self.parse_unary())
        if t == Tok(TO, "*"):
            self.pos += 1
            return Deref(self.parse_unary())
        if t == Tok(TO, "&"):
            self.pos += 1
            return AddrOf(self.parse_unary())
        if t == Tok(TKW, "sizeof"):
            self.pos += 1
            if self.peek() == Tok(TP, "(") and self.type_after_lparen():
                self.expect(TP, "(")
                ty = self.parse_type()
                self.expect(TP, ")")
                return Sizeof(ty)
            return Sizeof(self.parse_unary())
        
        return self.parse_postfix()

    def type_after_lparen(self) -> bool:
        # true when the token after '(' starts a type, so 'sizeof(' is a
        # cast-style type operand rather than a parenthesized expression
        n = self.peek(1)
        if n is None:
            return False
        if n in (Tok(TKW, "int"), Tok(TKW, "char"), Tok(TKW, "void"), Tok(TKW, "struct")) \
                or n in (Tok(TKW, "const"), Tok(TKW, "unsigned"), Tok(TKW, "signed"),
                         Tok(TKW, "long"), Tok(TKW, "short")):
            return True
        return n.kind == TID and n.val in self.typedefs

    def parse_postfix(self) -> Expr:
        # primary followed by zero or more call '(' args ')' or index '[' i ']'
        e = self.parse_primary()
        while True:
            t = self.peek()

            if isinstance(e, Var) and e.name == "va_arg" and t == Tok(TP, "("):
                self.pos += 1
                self.parse_expr()
                self.expect(TP, ",")
                ty = self.parse_type()
                self.expect(TP, ")")
                return VaArg(ty)
            
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
            elif t == Tok(TO, "++"):
                self.pos += 1 
                e = PostIncDec(e, 1)
            elif t == Tok(TO, "--"):
                self.pos += 1 
                e = PostIncDec(e, -1)
            elif t == Tok(TP, "."):
                self.pos += 1 
                name_tok = self.next()
                if name_tok is None or name_tok.kind != TID:
                    raise SyntaxError(f"parse: expected member name after '.', got {self.preview()}")
                e = Member(e, name_tok.val)
            elif t == Tok(TO, "->"):
                self.pos += 1
                name_tok = self.next()
                if name_tok is None or name_tok.kind != TID:
                    raise SyntaxError(f"parse: expected member name after '->', got {self.preview()}")
                e = Member(Deref(e), name_tok.val)
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

STRUCTS: dict = {}

def align_of(t: Type) -> int:
    if isinstance(t, TChar):
        return 1
    if isinstance(t, (TInt, TUInt, TPtr)):
        return WORD
    if isinstance(t, TArr):
        return align_of(t.elem)
    if isinstance(t, TStruct):
        return max((align_of(mt) for _, mt in STRUCTS[t.tag]), default=1)
    raise TypeError(f"layout: unknown type {t}")

def struct_layout(tag: str):
    out = []
    off = 0 
    align = 1 
    for (name, mt) in STRUCTS[tag]:
        a = align_of(mt)
        off = (off + a - 1) & ~(a- 1)
        out.append((name, mt, off))
        off += type_size(mt)
        align = max(align, a)
    return out, (off + align - 1) & ~(align - 1)

def member_type(tag: str, name: str) -> Type:
    for (n, mt) in STRUCTS[tag]:
        if n == name:
            return mt 
    raise KeyError(f"layout: struct {tag!r} has no member {name!r}")

def member_offset(tag: str, name: str) -> int:
    # byte offset of one member within the struct
    for (n, mt, off) in struct_layout(tag)[0]:
        if n == name:
            return off
    raise KeyError(f"layout: struct {tag!r} has no member {name!r}")
def type_size(t: Type) -> int:
    if isinstance(t, TChar):
        return 1
    if isinstance(t, (TInt, TUInt, TPtr)):
        return WORD
    if isinstance(t, TArr):
        return t.count * type_size(t.elem)
    if isinstance(t, TStruct):
        return struct_layout(t.tag)[1]
    if isinstance(t, TVoid):
        raise TypeError("codegen: void has no size")
    raise TypeError(f"codegen: unknown type {t}")


def decay(t: Type) -> Type:
    return TPtr(t.elem) if isinstance(t, TArr) else t 

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
            self.size += (type_size(t) + 3) & ~3 
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
    elif isinstance(stmt, SFor):
        if stmt.init is not None:
            collect_locals(frame, stmt.init)
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
        elif isinstance(e, CompAssign):
            visit_expr(e.target); visit_expr(e.value)
        elif isinstance(e, PostIncDec):
            visit_expr(e.target)
        elif isinstance(e, BinOp):
            visit_expr(e.lhs); visit_expr(e.rhs)
        elif isinstance(e, UnOp):
            visit_expr(e.operand)
        elif isinstance(e, Call):
            for a in e.args:
                visit_expr(a)
        elif isinstance(e, Index):
            visit_expr(e.base); visit_expr(e.idx)
        elif isinstance(e, Member):
            visit_expr(e.base)
        elif isinstance(e, (Deref, AddrOf)):
            visit_expr(e.operand)
        elif isinstance(e, Sizeof):
            if not isinstance(e.operand, Type):
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
            
        elif isinstance(s, SFor):
            if s.init is not None:
                visit_stmt(s.init)
            if s.cond is not None:
                visit_expr(s.cond)
            if s.step is not None:
                visit_expr(s.step)
                visit_stmt(s.body)
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
        self.loops = []         # stack of (break_target, continue_target)
        self.pool_need = None   # line index of the earliest unflushed ldr =

    def emit(self, line: str = ""):
        # append one line of assembly. blank line for spacing when empty.
        self.lines.append(line)
        if line == ".pool":
            # a function boundary just flushed all pending literals
            self.pool_need = None
            return
        if line.startswith("    ldr ") and ", =" in line and self.pool_need is None:
            # first literal since the last pool flush
            self.pool_need = len(self.lines)
        # if that literal is drifting beyond ldr's 4kb pc-relative range,
        # drop a pool right here and jump over it: the data is never
        # executed, and no existing label sits between the two instructions
        # surrounding the flush.
        if self.pool_need is not None and len(self.lines) - self.pool_need > 600:
            lbl = self.new_label("Lp")
            self.lines.append(f"    b {lbl}")
            self.lines.append(".pool")
            self.lines.append(f"{lbl}:")
            self.pool_need = None

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
                self.emit(f"    .{'byte' if isinstance(g.type, TChar) else 'word'} {g.init.val}")
            elif isinstance(g.init, StrLit):
                if isinstance(g.type, TArr):
                    # char a[N] = "s": bytes inline, zero padded
                    pad = g.type.count - len(g.init.val) - 1
                    if pad < 0:
                        raise NotImplementedError("codegen: array initializer too long")
                    self.emit(f"    .asciz {c_string_literal(g.init.val)}")
                    if pad:
                        self.emit(f"    .zero {pad}")
                else:
                    # a global char* initialized to a string literal points at
                    # the literal's label; the bytes are emitted below.
                    self.emit(f"    .word {self.strings[g.init.val]}")
            elif isinstance(g.type, TArr):
                raise NotImplementedError(
                    f"codegen: array global {g.name!r} only takes a string init")
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
            # flush this function's literals into a nearby pool chunk so
            # ldr = stays within its 4kb pc-relative range
            self.emit(".pool")
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
        if fn.varargs:
            self.frame.declare("__va", TInt())
            for i in range(len(fn.params), 4):
                self.frame.declare(f"__va{i - len(fn.params)}", TInt())
        frame_bytes = self.frame.frame_bytes()

        # a per-function epilogue label so 'return' anywhere can jump to the
        # single restore-and-exit sequence.
        self.epilogue = self.new_label("Lret")

        self.emit(f".global {fn.name}")
        self.label(fn.name)

        # prologue: save fp/lr, set fp to the new frame base.
        self.emit("    push {fp, lr}")
        self.emit("    mov fp, sp")

        # spill incoming register args (r0-r3) into their stack slots so the
        # rest of codegen can treat params like any other local. slots are
        # fp-relative so this works before sp is moved. args beyond the
        # fourth were passed on the stack by the caller (handled later).
        # spilling first also frees r0-r3 as scratch for the big-frame sub.
        for i, (ty, name) in enumerate(fn.params):
            if i < 4:
                off = self.frame.offset(name)
                self.emit(f"    str r{i}, [fp, #{off}]")
            else:
                raise NotImplementedError(
                    "codegen: more than 4 params (stack args) not yet supported")

        if fn.varargs:
            # stash the anonymous args after the named ones and reset the
            # fetch index
            for i in range(len(fn.params), 4):
                off = self.frame.offset(f"__va{i - len(fn.params)}")
                self.emit(f"    str r{i}, [fp, #{off}]")
            off = self.frame.offset("__va")
            self.emit("    mov r0, #0")
            self.emit(f"    str r0, [fp, #{off}]")

        # reserve locals. imm only encodes 0-255, big frames go via a register.
        if frame_bytes:
            if frame_bytes <= 255:
                self.emit(f"    sub sp, sp, #{frame_bytes}")
            else:
                self.emit(f"    ldr r1, ={frame_bytes}")
                self.emit("    sub sp, sp, r1")

        # body
        self.gen_stmt(fn.body)

        # epilogue: restore sp/fp/lr and return. a function that falls off the
        # end returns garbage in r0, matching c's undefined behavior.
        self.label(self.epilogue)
        self.emit("    mov sp, fp")
        self.emit("    pop {fp, lr}")
        self.emit("    bx lr")

    def gen_stmt(self, s: Stmt):
        # one case per Stmt subclass. control flow lowers to cmp + cond
        # branches + fresh labels; every other case recurses or stores.
        if isinstance(s, SExpr):
            # evaluate for side effects, discard the result in r0
            self.gen_expr(s.expr)

        elif isinstance(s, SBlock):
            for x in s.stmts:
                self.gen_stmt(x)

        elif isinstance(s, SReturn):
            if s.expr is not None:
                self.gen_expr(s.expr)          # result in r0
            else:
                self.emit("    mov r0, #0")    # bare 'return' returns 0
            self.emit(f"    b {self.epilogue}")

        elif isinstance(s, SDecl):
            # locals were pre-allocated by collect_locals; just store the
            # initializer if there is one. uninitialized locals hold garbage,
            # same as real c.
            if isinstance(s.type, TArr) and isinstance(s.init, StrLit):
                # char a[N] = "s": copy from the literal's label, zero the rest
                if not isinstance(s.type.elem, TChar):
                    raise NotImplementedError("codegen: only char arrays take a string init")
                n = len(s.init.val) + 1
                if n > s.type.count:
                    raise NotImplementedError("codegen: array initializer too long")
                off = self.frame.offset(s.name)
                self.emit(f"    ldr r1, ={self.strings[s.init.val]}")
                for i in range(n):
                    self.emit(f"    ldrb r0, [r1, #{i}]")
                    self.emit(f"    strb r0, [fp, #{off + i}]")
                if n < s.type.count:
                    self.emit("    mov r0, #0")
                    for i in range(n, s.type.count):
                        self.emit(f"    strb r0, [fp, #{off + i}]")
            elif s.init is not None:
                self.gen_expr(s.init)          # value in r0
                off = self.frame.offset(s.name)
                if isinstance(s.type, TChar):
                    self.emit(f"    strb r0, [fp, #{off}]")
                else:
                    self.emit(f"    str r0, [fp, #{off}]")

        elif isinstance(s, SIf):
            # cond -> branch on false over 'then', optional jump over 'else'
            self.gen_expr(s.cond)              # value in r0
            else_l = self.new_label("Lelse")
            end_l = self.new_label("Lend")
            self.emit("    cmp r0, #0")
            self.emit(f"    beq {else_l}")
            self.gen_stmt(s.then)
            if s.els is not None:
                self.emit(f"    b {end_l}")
                self.label(else_l)
                self.gen_stmt(s.els)
                self.label(end_l)
            else:
                # no else: 'then' falls through to here when cond was true
                self.label(else_l)

        elif isinstance(s, SWhile):
            # top-of-loop label, re-test cond each iteration
            top_l = self.new_label("Lwhile")
            end_l = self.new_label("Lwend")
            self.label(top_l)
            self.gen_expr(s.cond)              # value in r0
            self.emit("    cmp r0, #0")
            self.emit(f"    beq {end_l}")
            self.loops.append((end_l, top_l))  # continue -> re-test cond
            self.gen_stmt(s.body)
            self.loops.pop()
            self.emit(f"    b {top_l}")
            self.label(end_l)

        elif isinstance(s, SFor):
            # lower as: init; top: cond -> beq end; body; step:; step; b top; end:
            if s.init is not None:
                self.gen_stmt(s.init)
            top_l = self.new_label("Lfor")
            step_l = self.new_label("Lforstep")
            end_l = self.new_label("Lforend")
            self.label(top_l)
            if s.cond is not None:
                self.gen_expr(s.cond)          # value in r0
                self.emit("    cmp r0, #0")
                self.emit(f"    beq {end_l}")
            self.loops.append((end_l, step_l)) # continue -> run step
            self.gen_stmt(s.body)
            self.loops.pop()
            self.label(step_l)
            if s.step is not None:
                self.gen_expr(s.step)          # result discarded
            self.emit(f"    b {top_l}")
            self.label(end_l)

        elif isinstance(s, SBreak):
            if not self.loops:
                raise SyntaxError("gen_stmt: break outside a loop")
            self.emit(f"    b {self.loops[-1][0]}")

        elif isinstance(s, SContinue):
            if not self.loops:
                raise SyntaxError("gen_stmt: continue outside a loop")
            self.emit(f"    b {self.loops[-1][1]}")
        else:
            raise NotImplementedError(f"gen_stmt: unknown stmt {s}")

    # ----- type inference -----
    # minimal local inference: just enough to pick ldr vs ldrb/str vs strb
    # and to scale pointer arithmetic. undeclared names (callees) assume int.

    def declared_type(self, name: str) -> Type | None:
        # the type a name was declared with, local or global; none if undeclared
        if self.frame is not None and self.frame.has(name):
            return self.frame.type_of(name)
        for g in self.globals:
            if g.name == name:
                return g.type
        return None

    def infer_type(self, e: Expr) -> Type:
        if isinstance(e, IntLit):
            return TInt()
        if isinstance(e, CharLit):
            return TChar()
        if isinstance(e, StrLit):
            return TPtr(TChar())
        if isinstance(e, Var):
            t = self.declared_type(e.name)
            return decay(t) if t is not None else TInt()
        if isinstance(e, Deref):
            t = self.infer_type(e.operand)
            return t.inner if isinstance(t, TPtr) else TInt()
        if isinstance(e, Index):
            t = self.infer_type(e.base)
            return t.inner if isinstance(t, TPtr) else TInt()
        if isinstance(e, Member):
            bt = self.infer_type(e.base)
            if not isinstance(bt, TStruct):
                raise TypeError(f"codegen: member access on non-struct {bt}")
            return decay(member_type(bt.tag, e.name))
        if isinstance(e, Assign):
            return self.infer_type(e.target)
        if isinstance(e, CompAssign):
            return self.infer_type(e.target)
        if isinstance(e, PostIncDec):
            return self.infer_type(e.target)
        if isinstance(e, AddrOf):
            # &buf: same address as buf, treat as element pointer
            if isinstance(e.operand, Var):
                t = self.declared_type(e.operand.name)
                if t is not None and isinstance(t, TArr):
                    return TPtr(t.elem)
            return TPtr(self.infer_type(e.operand))
        if isinstance(e, UnOp):
            return TInt()          # !x and -x are ints
        if isinstance(e, VaArg):
            return decay(e.ty)
        if isinstance(e, Call):
            return TInt()          # we do not track return types
        if isinstance(e, BinOp):
            # pointer +/- int keeps the pointer type (for p+1 deref chains)
            if e.op in (ADD, SUB):
                lt = self.infer_type(e.lhs)
                if isinstance(lt, TPtr):
                    return lt
                rt = self.infer_type(e.rhs)
                if isinstance(rt, TPtr):
                    return rt
            # unsigned arithmetic stays unsigned, so chained compares and
            # divides keep the right flavor
            if e.op in (ADD, SUB, MUL, DIV, MOD, BAND, BOR, BXOR, SHL, SHR):
                if isinstance(self.infer_type(e.lhs), TUInt) \
                        or isinstance(self.infer_type(e.rhs), TUInt):
                    return TUInt()
            return TInt()
        if isinstance(e, Sizeof):
            return TInt()
        raise NotImplementedError(f"infer_type: unknown expr {e}")

    # ----- load/store picking -----

    def load_insn(self, t: Type) -> str:
        # word or byte load for reading a value of type t
        return "ldrb" if isinstance(t, TChar) else "ldr"

    def store_insn(self, t: Type) -> str:
        return "strb" if isinstance(t, TChar) else "str"

    # ----- lvalues -----
    # gen_lvalue evaluates the ADDRESS of an assignable expression into r0.

    def emit_local_addr(self, off: int):
        # r0 = fp + off. imm only encodes 0-255, big offsets go via a literal.
        if abs(off) <= 255:
            self.emit(f"    add r0, fp, #{off}")
        else:
            self.emit(f"    ldr r0, ={off}")
            self.emit("    add r0, fp, r0")

    def gen_lvalue(self, e: Expr):
        if isinstance(e, Var):
            if self.frame is not None and self.frame.has(e.name):
                # local: address = fp + slot offset
                self.emit_local_addr(self.frame.offset(e.name))
            else:
                # global: address = its linker-resolved symbol
                self.emit(f"    ldr r0, ={e.name}")
        elif isinstance(e, Deref):
            # *p as lvalue: p already IS the address
            self.gen_expr(e.operand)
        elif isinstance(e, Index):
            # a[i] as lvalue: address = base + i * scale
            self.gen_lvalue_index(e)
        elif isinstance(e, Member):
            # address = address of the base struct + member offset.
            # gen_lvalue(deref) yields the pointer value itself, so this
            # one case covers b.m, p->m (parsed as (*p).m), a.b.c, arr[i].m
            bt = self.infer_type(e.base)
            if not isinstance(bt, TStruct):
                raise TypeError(f"gen_lvalue: member access on non-struct {bt}")
            self.gen_lvalue(e.base)
            off = member_offset(bt.tag, e.name)
            if 0 <= off <= 255:
                self.emit(f"    add r0, r0, #{off}")
            else:
                self.emit(f"    ldr r1, ={off}")
                self.emit("    add r0, r0, r1")
        else:
            raise NotImplementedError(f"gen_lvalue: not assignable: {e}")

    def gen_lvalue_index(self, e: Index):
        # shared by Index-as-lvalue and Index-as-rvalue-load
        t = self.infer_type(e.base)
        scale = scale_of(t) if isinstance(t, TPtr) else 1
        self.gen_expr(e.base)          # base address in r0
        self.emit("    push {r0}")
        self.gen_expr(e.idx)           # index in r0
        if scale != 1:
            self.emit(f"    mov r1, #{scale}")
            self.emit("    mul r0, r0, r1")
        self.emit("    pop {r1}")      # r1 = base, r0 = scaled index
        self.emit("    add r0, r1, r0")

    # ----- expressions -----
    # gen_expr evaluates the VALUE of e into r0. temporaries go through the
    # stack (push/pop), so evaluation may freely clobber r0 between operands.

    def gen_expr(self, e: Expr):
        if isinstance(e, IntLit):
            # mov immediate only encodes 0-255 (and some rotated forms);
            # anything else goes through a literal via ldr =
            if 0 <= e.val <= 255:
                self.emit(f"    mov r0, #{e.val}")
            else:
                self.emit(f"    ldr r0, ={e.val}")

        elif isinstance(e, CharLit):
            self.emit(f"    mov r0, #{ord(e.val)}")

        elif isinstance(e, StrLit):
            # value of a string literal is its address
            self.emit(f"    ldr r0, ={self.strings[e.val]}")

        elif isinstance(e, Var):
            t = self.declared_type(e.name)
            if t is not None and isinstance(t, TArr):
                # array decays: value is its address
                if self.frame is not None and self.frame.has(e.name):
                    self.emit_local_addr(self.frame.offset(e.name))
                else:
                    self.emit(f"    ldr r0, ={e.name}")
            elif self.frame is not None and self.frame.has(e.name):
                off = self.frame.offset(e.name)
                insn = self.load_insn(self.frame.type_of(e.name))
                self.emit(f"    {insn} r0, [fp, #{off}]")
            else:
                # global: load directly off its symbol
                self.emit(f"    ldr r0, ={e.name}")
                insn = self.load_insn(decay(t))
                self.emit(f"    {insn} r0, [r0]")

        elif isinstance(e, Assign):
            # address of target, value of rhs, store. value stays in r0
            # so 'x = y = 5' chains work.
            if isinstance(e.target, Var):
                dt = self.declared_type(e.target.name)
                if dt is not None and isinstance(dt, TArr):
                    raise TypeError(f"codegen: cannot assign to array {e.target.name!r}")
            t = self.infer_type(e.target)
            self.gen_lvalue(e.target)      # address in r0
            self.emit("    push {r0}")
            self.gen_expr(e.value)         # value in r0
            self.emit("    pop {r1}")      # r1 = address, r0 = value
            self.emit(f"    {self.store_insn(t)} r0, [r1]")

        elif isinstance(e, CompAssign):
            # like Assign, but reloads the target and applies op. the target
            # address is computed once, so a[i] += 1 evaluates a and i once.
            if isinstance(e.target, Var):
                dt = self.declared_type(e.target.name)
                if dt is not None and isinstance(dt, TArr):
                    raise TypeError(f"codegen: cannot assign to array {e.target.name!r}")
            t = self.infer_type(e.target)
            self.gen_lvalue(e.target)                # address in r0
            self.emit("    push {r0}")
            self.emit(f"    {self.load_insn(t)} r0, [r0]")   # old value in r0
            self.emit("    push {r0}")
            self.gen_expr(e.value)                   # rhs in r0
            # pointer targets scale the rhs, like p + n
            if e.op in (ADD, SUB) and isinstance(t, TPtr):
                scale = scale_of(t)
                if scale != 1:
                    self.emit(f"    mov r1, #{scale}")
                    self.emit("    mul r0, r0, r1")
            self.emit("    pop {r1}")                # r1 = old value
            if e.op == ADD:
                self.emit("    add r0, r1, r0")
            elif e.op == SUB:
                self.emit("    sub r0, r1, r0")
            elif e.op == MUL:
                self.emit("    mul r0, r1, r0")
            elif e.op in (BAND, BOR, BXOR):
                insn = {BAND: "and", BOR: "orr", BXOR: "eor"}[e.op]
                self.emit(f"    {insn} r0, r1, r0")
            elif e.op in (SHL, SHR):
                signed = not isinstance(t, (TUInt, TPtr))
                if isinstance(e.value, IntLit):
                    n = e.value.val & 31
                    base = "lsl" if e.op == SHL else ("asr" if signed else "lsr")
                    self.emit(f"    {base} r0, r1, #{n}")
                else:
                    if e.op == SHL:
                        self.emit("    lsl r0, r1, r0")
                    elif signed:
                        self.emit("    asr r0, r1, r0")
                    else:
                        self.emit("    lsr r0, r1, r0")
            elif e.op in (DIV, MOD):
                # no divide in our arm subset; unsigned targets use
                # __udivsi3 (quotient r0, remainder r2)
                self.emit("    mov r2, r0")          # save rhs
                self.emit("    mov r0, r1")          # r0 = old value
                self.emit("    mov r1, r2")          # r1 = rhs
                if isinstance(t, TUInt):
                    self.emit("    bl __udivsi3")
                    if e.op == MOD:
                        self.emit("    mov r0, r2")
                else:
                    self.emit(f"    bl {'__divsi3' if e.op == DIV else '__modsi3'}")
            else:
                raise NotImplementedError(f"gen_expr: unknown compound op {e.op}")
            self.emit("    pop {r1}")                # r1 = address
            self.emit(f"    {self.store_insn(t)} r0, [r1]")
            # new value stays in r0 so 'x = y += 1' chains work

        elif isinstance(e, PostIncDec):
            # x++ / x--: memory gets old + delta, expression yields the OLD
            # value. pointer targets step by scale_of, so p++ advances a
            # whole element.
            if isinstance(e.target, Var):
                dt = self.declared_type(e.target.name)
                if dt is not None and isinstance(dt, TArr):
                    raise TypeError(f"codegen: cannot increment array {e.target.name!r}")
            t = self.infer_type(e.target)
            step = e.delta * (scale_of(t) if isinstance(t, TPtr) else 1)
            self.gen_lvalue(e.target)                # address in r0
            self.emit("    push {r0}")
            self.emit(f"    {self.load_insn(t)} r0, [r0]")   # old value in r0
            self.emit("    push {r0}")
            insn = "add" if step >= 0 else "sub"
            self.emit(f"    {insn} r0, r0, #{abs(step)}")
            self.emit("    pop {r1}")                # r1 = old value
            self.emit("    pop {r2}")                # r2 = address
            self.emit(f"    {self.store_insn(t)} r0, [r2]")
            self.emit("    mov r0, r1")              # result = old value

        elif isinstance(e, Sizeof):
            # the operand is never evaluated, only its type matters.
            # arrays keep their full size (no decay); pointers are a word.
            if isinstance(e.operand, Type):
                t = e.operand
            elif isinstance(e.operand, Var):
                t = self.declared_type(e.operand.name)
                if t is None:
                    t = TInt()
            elif isinstance(e.operand, Member):
                # raw member type: b.data stays char[512], not char*
                bt = self.infer_type(e.operand.base)
                if not isinstance(bt, TStruct):
                    raise TypeError(f"codegen: member access on non-struct {bt}")
                t = member_type(bt.tag, e.operand.name)
            else:
                t = self.infer_type(e.operand)
            size = type_size(t)
            if 0 <= size <= 255:
                self.emit(f"    mov r0, #{size}")
            else:
                self.emit(f"    ldr r0, ={size}")

        elif isinstance(e, VaArg):
            # runtime fetch: __va indexes the consecutive __va0.. slots,
            # slot i at offset(__va0) - 4*i, so va_arg inside loops works.
            # varargs are word-promoted, so always a full load.
            if self.frame is None or not self.frame.has("__va0"):
                raise TypeError("codegen: va_arg outside a varargs function")
            base = self.frame.offset("__va0")
            vaidx = self.frame.offset("__va")
            self.emit(f"    ldr r1, [fp, #{vaidx}]")   # i = __va
            self.emit("    mov r2, r1")
            self.emit("    add r1, r1, #1")
            self.emit(f"    str r1, [fp, #{vaidx}]")   # __va = i + 1
            self.emit("    lsl r2, r2, #2")            # 4*i
            if abs(base) <= 255:
                self.emit(f"    add r0, fp, #{base}")  # fp + base
            else:
                self.emit(f"    ldr r0, ={base}")
                self.emit("    add r0, fp, r0")
            self.emit("    sub r0, r0, r2")            # &slot = fp + base - 4*i
            self.emit("    ldr r0, [r0]")              # value in r0

        elif isinstance(e, AddrOf):
            self.gen_lvalue(e.operand)

        elif isinstance(e, Deref):
            t = self.infer_type(e)         # element type (char vs word)
            self.gen_expr(e.operand)       # address in r0
            self.emit(f"    {self.load_insn(t)} r0, [r0]")

        elif isinstance(e, Index):
            t = self.infer_type(e)
            self.gen_lvalue_index(e)       # address in r0
            self.emit(f"    {self.load_insn(t)} r0, [r0]")

        elif isinstance(e, Member):
            # member address in r0; load it unless the member is an array,
            # whose value is its address (decays like a plain array name)
            bt = self.infer_type(e.base)
            mt = member_type(bt.tag, e.name)
            self.gen_lvalue(e)
            if not isinstance(mt, TArr):
                self.emit(f"    {self.load_insn(decay(mt))} r0, [r0]")

        elif isinstance(e, UnOp):
            self.gen_expr(e.operand)
            if e.op == NEG:
                # rsb = reverse subtract: r0 = 0 - r0
                self.emit("    rsb r0, r0, #0")
            elif e.op == BNOT:
                # ~x -> r0 = ~r0
                self.emit("    mvn r0, r0")
            elif e.op == NOT:
                # !x -> r0 = (x == 0)
                self.emit("    cmp r0, #0")
                self.emit("    mov r0, #0")
                self.emit("    moveq r0, #1")
            else:
                raise NotImplementedError(f"gen_expr: unknown unop {e.op}")

        elif isinstance(e, BinOp):
            self.gen_binop(e)

        elif isinstance(e, Call):
            self.gen_call(e)

        else:
            raise NotImplementedError(f"gen_expr: unknown expr {e}")

    def gen_binop(self, e: BinOp):
        op = e.op

        # short-circuit logical operators lower to branches, not alu ops
        if op == AND:
            false_l = self.new_label("Landf")
            end_l = self.new_label("Land")
            self.gen_expr(e.lhs)
            self.emit("    cmp r0, #0")
            self.emit(f"    beq {false_l}")
            self.gen_expr(e.rhs)
            self.emit("    cmp r0, #0")
            self.emit(f"    beq {false_l}")
            self.emit("    mov r0, #1")
            self.emit(f"    b {end_l}")
            self.label(false_l)
            self.emit("    mov r0, #0")
            self.label(end_l)
            return
        if op == OR:
            true_l = self.new_label("Lort")
            end_l = self.new_label("Lor")
            self.gen_expr(e.lhs)
            self.emit("    cmp r0, #0")
            self.emit(f"    bne {true_l}")
            self.gen_expr(e.rhs)
            self.emit("    cmp r0, #0")
            self.emit(f"    bne {true_l}")
            self.emit("    mov r0, #0")
            self.emit(f"    b {end_l}")
            self.label(true_l)
            self.emit("    mov r0, #1")
            self.label(end_l)
            return

        # pointer arithmetic: if one side is a pointer, scale the other side
        scale = 1
        lt = self.infer_type(e.lhs)
        if isinstance(lt, TPtr) and op in (ADD, SUB):
            scale = scale_of(lt)

        # lhs -> r0, push; rhs -> r0; pop lhs into r1
        self.gen_expr(e.lhs)
        self.emit("    push {r0}")
        self.gen_expr(e.rhs)
        if scale != 1:
            self.emit(f"    mov r1, #{scale}")
            self.emit("    mul r0, r0, r1")
        self.emit("    pop {r1}")          # r1 = lhs, r0 = rhs

        if op == ADD:
            self.emit("    add r0, r1, r0")
        elif op == SUB:
            self.emit("    sub r0, r1, r0")
        elif op == MUL:
            self.emit("    mul r0, r1, r0")
        elif op in (BAND, BOR, BXOR):
            # r1 = lhs, r0 = rhs, one alu op
            insn = {BAND: "and", BOR: "orr", BXOR: "eor"}[op]
            self.emit(f"    {insn} r0, r1, r0")
        elif op in (SHL, SHR):
            # constant counts encode as imm shifts; variable counts use the
            # register-shift form. >> is arithmetic for signed lhs.
            signed = not isinstance(self.infer_type(e.lhs), (TUInt, TPtr))
            if isinstance(e.rhs, IntLit):
                n = e.rhs.val & 31
                base = "lsl" if op == SHL else ("asr" if signed else "lsr")
                self.emit(f"    {base} r0, r1, #{n}")
            else:
                if op == SHL:
                    self.emit("    lsl r0, r1, r0")
                elif signed:
                    self.emit("    asr r0, r1, r0")
                else:
                    self.emit("    lsr r0, r1, r0")
        elif op in (DIV, MOD):
            # no divide in our arm subset: call a runtime helper.
            # __udivsi3 returns quotient in r0 and remainder in r2, so it
            # covers both unsigned div and mod.
            uns = isinstance(self.infer_type(e.lhs), (TUInt, TPtr)) \
                or isinstance(self.infer_type(e.rhs), (TUInt, TPtr))
            self.emit("    mov r2, r0")    # save rhs
            self.emit("    mov r0, r1")    # r0 = lhs
            self.emit("    mov r1, r2")    # r1 = rhs
            if uns:
                self.emit("    bl __udivsi3")
                if op == MOD:
                    self.emit("    mov r0, r2")
            else:
                self.emit(f"    bl {'__divsi3' if op == DIV else '__modsi3'}")
        elif op in (EQ, NE, LT, LE, GT, GE):
            # cmp sets flags from lhs - rhs, then materialize 0/1 with
            # conditional movs. r1 = lhs, r0 = rhs. unsigned operands (or
            # pointers) compare with lo/ls/hi/hs instead of lt/le/gt/ge.
            uns = isinstance(self.infer_type(e.lhs), (TUInt, TPtr)) \
                or isinstance(self.infer_type(e.rhs), (TUInt, TPtr))
            if uns:
                cond = {EQ: "eq", NE: "ne", LT: "lo",
                        LE: "ls", GT: "hi", GE: "hs"}[op]
            else:
                cond = {EQ: "eq", NE: "ne", LT: "lt",
                        LE: "le", GT: "gt", GE: "ge"}[op]
            self.emit("    cmp r1, r0")
            self.emit("    mov r0, #0")
            self.emit(f"    mov{cond} r0, #1")
        else:
            raise NotImplementedError(f"gen_binop: unknown op {op}")

    def gen_call(self, e: Call):
        if e.name in ("va_start", "va_end"):
            if self.frame is None or not self.frame.has("__va"):
                raise TypeError(f"codegen: {e.name} outside a varargs function")
            return
        if len(e.args) > 4:
            raise NotImplementedError(
                "codegen: more than 4 call args (stack args) not yet supported")
        # evaluate args left to right, each pushed; then pop in reverse so
        # arg i lands in register r_i.
        for a in e.args:
            self.gen_expr(a)
            self.emit("    push {r0}")
        for i in range(len(e.args) - 1, -1, -1):
            self.emit(f"    pop {{r{i}}}")
        self.emit(f"    bl {e.name}")

def compile_text(src: str, path: str) -> str:
    STRUCTS.clear()
    DEFINES.clear()
    src = preprocess(src, path)
    toks = tokenize(src)
    ast = Parser(toks).parse_program()
    return CodeGen().gen_program(ast)

def main(argv):
    if len(argv) < 2:
        print("usage: compiler.py input.c [-o out.s]", file=sys.stderr)
        return 1
    with open(argv[1]) as f:
        src = f.read()
    out = compile_text(src, argv[1])
    if "-o" in argv:
        dst = argv[argv.index("-o") + 1]
        with open(dst, "w") as f:
            f.write(out)
    else:
        sys.stdout.write(out)
    return 0

if __name__ == "__main__":
    sys.exit(main(sys.argv))
