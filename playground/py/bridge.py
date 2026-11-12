"""
playground bridge: runs the real toolchain (section4/compiler.py,
section3/assembler.py, section4/linker.py, section4/build.py) and reports
every intermediate as json for the web ui.

the toolchain files are imported as-is. this file only calls into them:
- the final elf + flat image come from build.build() itself
- the per-stage views come from running build.build()'s steps one at a
  time; if that ever disagrees with build.py the ui gets a warning

works under pyodide (browser) and plain cpython (for testing):
    python3 bridge.py /path/to/repo section4/test.c
"""
import dataclasses
import importlib
import json
import os
import re
import sys
import traceback

REPO = "/repo"
MODULES = ("compiler", "assembler", "linker", "build")


def setup(repo):
    # point imports at the repo and drop any stale copies so edits are picked up
    global REPO
    REPO = repo
    for p in (os.path.join(repo, "section3"), os.path.join(repo, "section4")):
        if p not in sys.path:
            sys.path.insert(0, p)
    for m in MODULES:
        sys.modules.pop(m, None)
    importlib.invalidate_caches()


def _mods():
    import compiler, assembler, linker, build
    return compiler, assembler, linker, build


# ---------- json helpers ----------

def _node(o, compiler):
    # dataclass tree -> {"_": class, field: ...}; types are tagged so the ui
    # can print them inline
    if dataclasses.is_dataclass(o) and not isinstance(o, type):
        d = {"_": type(o).__name__}
        if isinstance(o, compiler.Type):
            d["_type"] = True
        for f in dataclasses.fields(o):
            d[f.name] = _node(getattr(o, f.name), compiler)
        return d
    if isinstance(o, (list, tuple)):
        return [_node(x, compiler) for x in o]
    if isinstance(o, (str, int, float, bool)) or o is None:
        return o
    return repr(o)


def _type_str(t, compiler):
    try:
        return _ctype(_node(t, compiler))
    except Exception:
        return repr(t)


def _ctype(d):
    if not isinstance(d, dict):
        return str(d)
    c = d.get("_", "?")
    names = {"TInt": "int", "TUInt": "unsigned int", "TChar": "char", "TVoid": "void"}
    if c in names:
        return names[c]
    if c == "TPtr":
        return _ctype(d.get("inner")) + "*"
    if c == "TArr":
        return "%s[%s]" % (_ctype(d.get("elem")), d.get("count"))
    if c == "TStruct":
        return "struct " + str(d.get("tag"))
    return c


def _spans(src, toks):
    # where each token sits in the preprocessed text. uses plain c lexical
    # shapes, not compiler.py internals; stops quietly at the first surprise.
    out = []
    i, n = 0, len(src)
    for t in toks:
        while i < n:
            if src.startswith("//", i):
                j = src.find("\n", i)
                i = n if j < 0 else j
            elif src.startswith("/*", i):
                j = src.find("*/", i + 2)
                i = n if j < 0 else j + 2
            elif src[i].isspace():
                i += 1
            else:
                break
        kind = getattr(t, "kind", "")
        val = getattr(t, "val", None)
        if kind in ("strl", "chrl"):
            q = src[i] if i < n else ""
            j = i + 1
            while j < n and src[j] != q:
                j += 2 if src[j] == "\\" else 1
            end = j + 1
        elif kind == "intl":
            m = re.compile(r"0[xX][0-9a-fA-F]+|\d+").match(src, i)
            end = m.end() if m else -1
        else:
            text = str(val)
            end = i + len(text) if src.startswith(text, i) else -1
        if end < 0 or end > n:
            break
        out.append((i, end))
        i = end
    return out + [None] * (len(toks) - len(out))


def _hex(b):
    return bytes(b).hex()


def _line_kind(line):
    body = line.split("@")[0].strip()
    while body and body.split()[0].endswith(":"):
        name = body.split()[0][:-1]
        body = body[len(name) + 1:].strip()
    if not body:
        return "label" if line.split("@")[0].strip() else None
    if body.startswith(".pool"):
        return "pool"
    return "directive" if body.startswith(".") else "insn"


def _listing(assembler, full):
    # which bytes each source line produced: feed pass 1 one line at a time
    # into a scratch Asm (its state lives on the object, so this matches)
    a = assembler.Asm()
    out = []
    for i, raw in enumerate(full.splitlines()):
        kind = _line_kind(raw)
        if kind is None:
            continue
        sec = a.cur
        before = len(a.sections[sec])
        a.pass1(raw)
        grown = len(a.sections[sec]) - before if a.cur == sec else 0
        if kind == "label":
            out.append({"line": i, "kind": "label", "sec": sec, "off": before, "len": 0})
        else:
            out.append({"line": i, "kind": kind, "sec": sec, "off": before, "len": grown})
    return out


# ---------- source map ----------
# compiler.py keeps no line numbers. these subclasses only watch: the parser
# notes which tokens each statement used, codegen notes which statement it
# was working on when each asm line was emitted. the asm is unchanged (the
# elf check against build.build() would catch it otherwise).

def _pre_lines(src, path):
    # (file, line) for every line compiler.preprocess(src, path) returns, by
    # walking the same lines it walks: includes are inlined, directives drop
    out = []
    for i, line in enumerate(src.splitlines()):
        s = line.strip()
        if s.startswith("#include"):
            inc = os.path.join(os.path.dirname(os.path.abspath(path)), s[len("#include"):].strip()[1:-1])
            with open(inc) as f:
                sub = _pre_lines(f.read(), inc)
            # an include with nothing left still adds one empty line
            out.extend(sub or [None])
        elif not s.startswith("#"):
            out.append((os.path.basename(path), i))
    return out


def _mapped(compiler):
    class Parser(compiler.Parser):
        def __init__(self, toks):
            super().__init__(toks)
            self.spans = []   # (node, first token, last token)

        def parse_stmt(self):
            start = self.pos
            s = super().parse_stmt()
            self.spans.append((s, start, self.pos - 1))
            return s

        def parse_top(self):
            start = self.pos
            d = super().parse_top()
            self.spans.append((d, start, self.pos - 1))
            return d

    class CodeGen(compiler.CodeGen):
        def __init__(self, spans):
            super().__init__()
            self.where = {id(n): (a, b) for n, a, b in spans}
            self.stack = [None]
            self.marks = []   # (asm line index, token): lines from here on came from token's line
            self.closing = None

        def _at(self, tok):
            self.marks.append((len(self.lines), tok))

        def gen_stmt(self, s):
            w = self.where.get(id(s))
            # blocks emit nothing of their own, their statements map themselves
            if w is None or isinstance(s, compiler.SBlock):
                return super().gen_stmt(s)
            self.stack.append(w[0])
            self._at(w[0])
            try:
                return super().gen_stmt(s)
            finally:
                self.stack.pop()
                self._at(self.stack[-1])

        def gen_func(self, fn):
            w = self.where.get(id(fn))
            self.stack = [w[0] if w else None]
            self.closing = w[1] if w else None
            self._at(self.stack[0])
            try:
                return super().gen_func(fn)
            finally:
                self.stack = [None]
                self._at(None)

        def label(self, name):
            # the epilogue belongs to the function's closing brace
            if name == getattr(self, "epilogue", None) and self.closing is not None:
                self.stack = [self.closing]
                self._at(self.closing)
            return super().label(name)

    return Parser, CodeGen


def _srcmap(out, src, path, pre, spans, parser, cg, compiler):
    # asm line -> [file, line]: token -> offset in the preprocessed text ->
    # preprocessed line -> source line
    import bisect
    lines = _pre_lines(src, path)
    if len(lines) != len(pre.split("\n")):
        out["warnings"].append("source map unavailable: compiler.preprocess's line structure changed; "
                               "update _pre_lines in playground/py/bridge.py")
        return
    starts = [0] + [i + 1 for i, c in enumerate(pre) if c == "\n"]

    def loc(tok):
        if tok is None or tok >= len(spans) or spans[tok] is None:
            return None
        return lines[bisect.bisect_right(starts, spans[tok][0]) - 1]

    out["pre_lines"] = [list(l) if l else None for l in lines]
    n = len(cg.lines)
    asm_src = [None] * n
    marks = cg.marks + [(n, None)]
    for (a, tok), (b, _) in zip(marks, marks[1:]):
        l = loc(tok)
        for i in range(a, min(b, n)):
            asm_src[i] = list(l) if l else None
    out["asm_src"] = asm_src
    # what each line parsed into, for the lineage strip
    stmts = {}
    for node, a, _ in parser.spans:
        l = loc(a)
        if l and not isinstance(node, compiler.SBlock):
            stmts.setdefault("%s:%d" % l, []).append(type(node).__name__)
    out["stmts"] = stmts
    out["sources"] = {}
    for name in sorted({l[0] for l in lines if l}):
        try:
            with open(os.path.join(os.path.dirname(path), name)) as f:
                out["sources"][name] = f.read()
        except OSError:
            pass


# ---------- the pipeline ----------

class StageError(Exception):
    def __init__(self, stage, exc):
        self.stage, self.exc = stage, exc


def _stage(name, fn, *args):
    try:
        return fn(*args)
    except Exception as e:
        raise StageError(name, e) from e


def _link(linker, text, data, symbols, relocs, out, load_addr=None):
    # build.py's linking steps, plus the before/after of every patched word
    ld = linker.ELFLinker() if load_addr is None else linker.ELFLinker(load_addr=load_addr)
    ld.add_section(".text", text)
    ld.add_section(".data", data, flags=linker.SHF_ALLOC | linker.SHF_WRITE)
    for name, (sec, off) in symbols.items():
        ld.add_symbol(name, off, sec, type_=linker.STT_OBJECT if sec == ".data" else linker.STT_FUNC)
    for (sec, off, rtype, sym, addend) in relocs:
        ld.add_relocation(off, sym, sec, type_=rtype, addend=addend)
    before = {s.name: bytes(s.content) for s in ld.sections}
    try:
        ld.link(entry="_start")
    finally:
        out["link"] = _link_json(ld, before)
    return ld


def _link_json(ld, before):
    secs = ld.sections
    patches = []
    for r in ld.relocations:
        sym = ld.symbols.get(r.symbol)
        s = r.section
        o = r.offset
        patches.append({
            "section": secs.index(s), "offset": o, "place": s.addr + o,
            "symbol": r.symbol, "target": ((sym.value + r.addend) & 0xFFFFFFFF) if sym else 0,
            "rtype": r.type,
            "before": int.from_bytes(before[s.name][o:o + 4], "little"),
            "after": int.from_bytes(bytes(s.content[o:o + 4]), "little"),
        })
    return {
        "load_addr": ld.load_addr,
        "entry": ld.entry,
        "sections": [{"name": s.name, "addr": s.addr, "content": _hex(s.content)} for s in secs],
        "symbols": [{"name": n, "value": s.value & 0xFFFFFFFF, "section": secs.index(s.section), "type": s.type}
                    for n, s in ld.symbols.items()],
        "patches": patches,
    }


def _write_tmp(ld, name):
    path = os.path.join(REPO, "section4", "build", "_bridge_" + name)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    ld.write_elf(path)
    with open(path, "rb") as f:
        return f.read()


def run(request):
    """request json: {"name": "test.c", "kind": "c"|"asm", "files": {name: text}}"""
    req = json.loads(request)
    out = {"kind": req["kind"], "warnings": [], "error": None}
    try:
        compiler, assembler, linker, build = _stage("import", _mods)
    except StageError as e:
        out["error"] = _err(e)
        return json.dumps(out)

    # editor files live next to the real sources so #include "x.c" resolves
    play = os.path.join(REPO, "section4", "playground")
    os.makedirs(play, exist_ok=True)
    for fname, text in req["files"].items():
        with open(os.path.join(play, fname), "w") as f:
            f.write(text)
    path = os.path.join(play, req["name"])
    src = req["files"][req["name"]]

    try:
        if req["kind"] == "c":
            _run_c(out, src, path, compiler, assembler, linker, build)
        else:
            _run_asm(out, src, assembler, linker)
    except StageError as e:
        out["error"] = _err(e)
    except Exception as e:
        out["error"] = _err(StageError("bridge", e))
    return json.dumps(out)


def _run_c(out, src, path, compiler, assembler, linker, build):
    # compile_text's steps, one at a time
    compiler.STRUCTS.clear()
    compiler.DEFINES.clear()
    pre = _stage("preprocess", compiler.preprocess, src, path)
    out["preprocessed"] = pre
    out["defines"] = [[k, v] for k, v in compiler.DEFINES.items()]
    toks = _stage("lex", compiler.tokenize, pre)
    spans = _spans(pre, toks)
    out["tokens"] = [{"kind": t.kind, "text": str(t.val), "repr": repr(t),
                      "start": s[0] if s else None, "end": s[1] if s else None}
                     for t, s in zip(toks, spans)]
    Parser, CodeGen = _mapped(compiler)
    parser = Parser(toks)
    ast = _stage("parse", parser.parse_program)
    out["ast"] = _node(ast, compiler)
    out["structs"] = _structs(compiler)
    cg = CodeGen(parser.spans)
    asm = _stage("codegen", cg.gen_program, ast)
    out["asm"] = asm
    try:
        _srcmap(out, src, path, pre, spans, parser, cg, compiler)
    except Exception as e:
        out["warnings"].append("source map unavailable: %s: %s" % (type(e).__name__, e))

    full = build.RUNTIME + "\n" + asm
    out["full_asm"] = full
    out["asm_line_offset"] = len(build.RUNTIME.splitlines()) + 1
    a = _assemble(out, full, assembler)
    text, data, symbols, relocs = a.object()
    ld = _stage("link", _link, linker, text, data, symbols, relocs, out)
    mine = _write_tmp(ld, "play.elf")

    # the authoritative build: build.py itself, start to finish
    base = os.path.splitext(os.path.basename(path))[0]
    _stage("build.py", build.build, path, base + ".elf")
    with open(os.path.join(build.BUILD_DIR, base + ".elf"), "rb") as f:
        elf = f.read()
    with open(os.path.join(build.BUILD_DIR, base + ".bin"), "rb") as f:
        flat = f.read()
    if elf != mine:
        out["warnings"].append("build.py produced a different elf than the playground's step-by-step run; "
                               "the elf and cpu views use build.py's output, the other views may be stale. "
                               "update playground/py/bridge.py to match build.py.")
    out["elf"] = _hex(elf)
    out["flat"] = _hex(flat)
    # build.py loads the flat image at the first section's address
    out["base"] = ld.sections[0].addr


def _run_asm(out, src, assembler, linker):
    out["full_asm"] = src
    out["asm_line_offset"] = 0
    a = _assemble(out, src, assembler)
    text, data, symbols, relocs = a.object()
    ld = _stage("link", _link, linker, text, data, symbols, relocs, out)
    out["elf"] = _hex(_write_tmp(ld, "play_s.elf"))
    out["flat"] = _hex(b"".join(bytes(s.content) for s in ld.sections))
    out["base"] = ld.sections[0].addr


def rom(request):
    """board boot roms. request json: {"base": 0xf0000, "files": {name: text}}.
    each file is assembled and linked at base with the repo's own tools;
    returns {name: {"flat", "base", "entry"} or {"error": {...}}}"""
    req = json.loads(request)
    result = {}
    try:
        _, assembler, linker, _ = _stage("import", _mods)
    except StageError as e:
        return json.dumps({n: {"error": _err(e)} for n in req["files"]})
    for name, text in req["files"].items():
        out = {"warnings": []}
        try:
            a = assembler.Asm()
            _stage("assemble", a.pass1, text)
            _stage("assemble", a.pass2)
            t, d, symbols, relocs = a.object()
            ld = _stage("link", _link, linker, t, d, symbols, relocs, out, req["base"])
            result[name] = {
                "flat": _hex(b"".join(bytes(s.content) for s in ld.sections)),
                "base": ld.sections[0].addr,
                "entry": ld.entry,
            }
        except StageError as e:
            result[name] = {"error": _err(e)}
        except Exception as e:
            result[name] = {"error": _err(StageError("bridge", e))}
    return json.dumps(result)


def _assemble(out, full, assembler):
    try:
        out["listing"] = _listing(assembler, full)
    except Exception as e:
        out["listing"] = []
        out["warnings"].append("line listing unavailable: %s: %s" % (type(e).__name__, e))
    a = assembler.Asm()
    try:
        _stage("assemble", a.pass1, full)
        _stage("assemble", a.pass2)
    finally:
        out["text"] = _hex(a.sections.get(".text", b""))
        out["data"] = _hex(a.sections.get(".data", b""))
        out["pool_count"] = len(getattr(a, "pool", []))
        out["reloc_count"] = len(getattr(a, "relocs", []))
        out["chunks"] = [list(c) for c in getattr(a, "chunks", [])]
    return a


def _structs(compiler):
    out = []
    for tag in list(compiler.STRUCTS):
        try:
            members, size = compiler.struct_layout(tag)
            out.append({"tag": tag, "size": size, "members": [
                {"name": n, "ty": _type_str(t, compiler), "off": off,
                 "size": compiler.type_size(t), "align": compiler.align_of(t)}
                for (n, t, off) in members]})
        except Exception:
            out.append({"tag": tag, "size": 0, "members": []})
    return out


def _err(e):
    exc = e.exc
    return {"stage": e.stage, "kind": type(exc).__name__, "msg": str(exc),
            "traceback": "".join(traceback.format_exception(type(exc), exc, exc.__traceback__))}


if __name__ == "__main__":
    repo, name = sys.argv[1], sys.argv[2]
    setup(repo)
    p = os.path.join(repo, name)
    files = {os.path.basename(p): open(p).read()}
    d = os.path.dirname(p)
    for f in os.listdir(d):
        if f.endswith((".c", ".h")) and f not in files:
            files[f] = open(os.path.join(d, f)).read()
    res = json.loads(run(json.dumps({"name": os.path.basename(p), "kind": "asm" if name.endswith((".s", ".asm")) else "c", "files": files})))
    print(json.dumps({k: (v if not isinstance(v, str) or len(v) < 80 else v[:80] + "...") for k, v in res.items()
                      if k not in ("ast", "tokens", "listing")}, indent=1)[:3000])
    print("tokens", len(res.get("tokens") or []), "listing", len(res.get("listing") or []), "warnings", res["warnings"])
