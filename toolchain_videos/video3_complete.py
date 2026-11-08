"""video 3 (advanced): the complete toolchain.

compiler deep cuts (types, struct layout, lvalues, indexing, string
literals, calls), the two-pass assembler (encoding, literal pools,
relocations), the ELF linker (placement, patching, file layout), the
runtime, and qemu. ~6 min.
"""
from manim import *
from viz import ct, code_block, hl, tbox, caption, chapter

# ---------------------------------------------------------------- data

TYPE_ROWS = [
    ("char", "1 byte"),
    ("int", "4 bytes"),
    ("int *  /  char *", "4 bytes"),
    ("char buf[512]", "512 bytes"),
    ("struct P", "sum of members, padded"),
]

DATA_ASM = [
    ".data",
    "g:",
    "    .word 42",
    "msg:",
    '    .asciz "hello"',
    ".Lstr1:",
    '    .asciz "hi\\n"',
]

INDEX_ASM = [
    "add r0, fp, #-12     @ &a[0]",
    "mov r0, #1           @ i = 1",
    "mov r1, #4           @ scale: sizeof(int)",
    "mul r0, r0, r1",
    "add r0, r1, r0       @ &a[1]  in r0",
    "@ (push/pop of temporaries elided)",
]

CALL_ASM = [
    "ldr r0, =.Lstr1      @ arg: address of \"hi\\n\"",
    "push {r0}            @ args go on...",
    "pop {r0}             @ ...and into r0..r3",
    "bl putstr",
    "@ result comes back in r0",
]

DIV_ASM = [
    "return a[1] / 2;",
    "    ...",
    "    bl __divsi3      @ no divide instruction!",
]

PASS1_S = [
    ".text",
    "main:",
    "    push {fp, lr}",
    "    ...",
    ".Lret1:",
    "    mov sp, fp",
    ".data",
    "msg:",
    '    .asciz "hello"',
]

SYMTAB = [
    ("main", "(.text, +0xa8)"),
    (".Lret1", "(.text, +0xd0)"),
    ("msg", "(.data, +0x4)"),
    (".Lstr1", "(.data, +0xe)"),
]

BITS = [
    # (label, bits, note, color)
    ("cond", "1110", "always", YELLOW),
    ("", "00", "", GREY_B),
    ("I", "1", "imm", TEAL),
    ("opcode", "1101", "mov", ORANGE),
    ("S", "0", "", GREY_B),
    ("rn", "0000", "", GREY_B),
    ("rd", "0000", "r0", BLUE),
    ("operand2", "00000101", "#5", GREEN),
]

RUNTIME_ASM = [
    "_start:",
    "    ldr sp, =0x00800000   @ build the stack",
    "    bl main",
    "    mov r0, #0x18         @ SYS_EXIT",
    "    svc 0x123456          @ semihosting",
    "",
    "putstr:",
    "    mov r0, #4            @ SYS_WRITE0",
    "    svc 0x123456",
]

# ---------------------------------------------------------------- helpers


def byte_cell(text, color=None, w=0.62, h=0.62, tsize=15):
    b = RoundedRectangle(corner_radius=0.05, width=w, height=h)
    b.set_stroke(GREY_B, 1.5)
    if color:
        b.set_fill(color, 0.22)
    t = ct(text, size=tsize, color=color or GREY_A)
    return VGroup(b, t.move_to(b))


def mem_row(labels, color=BLUE, w=1.15, h=0.62, tsize=17, sublabels=None):
    cells = VGroup()
    for lab in labels:
        cells.add(byte_cell(lab, color, w=w, h=h, tsize=tsize))
    row = VGroup(*cells).arrange(RIGHT, buff=0.1)
    if sublabels:
        subs = VGroup(
            *[ct(s, size=14, color=GREY_A) for s in sublabels]
        ).arrange(RIGHT, buff=0.1)
        for c, s in zip(cells, subs):
            s.set_x(c.get_x())
        row_group = VGroup(row, subs.next_to(row, DOWN, buff=0.12))
        return row_group, cells
    return row, cells


# ---------------------------------------------------------------- scene


class FullToolchain(Scene):
    def construct(self):
        self.intro()
        self.beat_types()
        self.beat_struct()
        self.beat_lvalue()
        self.beat_strings_calls()
        self.beat_assembler_pass1()
        self.beat_encode()
        self.beat_relocs_pool()
        self.beat_linker()
        self.beat_elf()
        self.beat_run()
        self.outro()

    # ---------- intro ----------

    def intro(self):
        title = Text("the whole toolchain", font_size=42)
        sub = caption("compiler -> assembler -> linker -> qemu, end to end", 24)
        sub.next_to(title, DOWN, buff=0.4)
        self.play(Write(title), run_time=1.2)
        self.play(FadeIn(sub, shift=UP * 0.3))
        self.wait(1.2)
        self.play(FadeOut(title), FadeOut(sub))

    # ---------- types ----------

    def beat_types(self):
        chip = chapter("1", "types & sizes")
        chip.to_corner(UL, buff=0.4)
        self.play(FadeIn(chip, shift=RIGHT * 0.5))

        rows = VGroup()
        for name, size in TYPE_ROWS:
            nm = ct(name, size=24, color=TEAL)
            sz = ct(size, size=22)
            rows.add(VGroup(nm, sz).arrange(RIGHT, buff=0.6))
        rows.arrange(DOWN, aligned_edge=LEFT, buff=0.3).move_to([-0.6, 0.4, 0])
        brace = Brace(rows, RIGHT, color=GREY_A)
        brl = ct("type_size()  /  align_of()", size=20).next_to(brace, RIGHT, buff=0.2)
        self.play(LaggedStart(*[FadeIn(r, shift=RIGHT * 0.3) for r in rows], lag_ratio=0.12), run_time=2)
        self.play(Create(brace), FadeIn(brl))
        cap = caption(
            "every load, store and frame slot is sized by the type.  int, char and\n"
            "pointers are one machine word: 4 bytes on this 32-bit ARM target.",
            21,
        )
        cap.to_edge(DOWN, buff=0.4)
        self.play(FadeIn(cap))
        self.wait(3)
        self.play(FadeOut(chip), FadeOut(rows), FadeOut(brace), FadeOut(brl), FadeOut(cap))

    # ---------- struct layout ----------

    def beat_struct(self):
        chip = chapter("2", "struct layout")
        chip.to_corner(UL, buff=0.4)
        self.play(FadeIn(chip, shift=RIGHT * 0.5))

        decl = ct("struct P { char a;  int b;  char c; };", size=24)
        decl.to_edge(UP, buff=1.0)
        self.play(Write(decl))

        n = 12
        cells = VGroup()
        for i in range(n):
            c = byte_cell("", None)
            cells.add(c)
        cells.arrange(RIGHT, buff=0.08).move_to([0, 1.0, 0])
        offs = VGroup(*[ct(str(i), size=13, color=GREY_A) for i in (0, 4, 8)])
        for i, o in zip((0, 4, 8), offs):
            o.set_x(cells[i].get_x())
        offs.next_to(cells, DOWN, buff=0.15)
        self.play(LaggedStart(*[FadeIn(c, scale=0.7) for c in cells], lag_ratio=0.05), run_time=2)

        a_block = byte_cell("a", TEAL, w=0.62, h=0.62)
        a_block.move_to(cells[0]).shift(UP * 2.2)
        self.play(FadeIn(a_block, shift=DOWN * 0.5))
        self.play(a_block.animate.move_to(cells[0]))
        t1 = ct("char a  @ 0", size=17, color=TEAL).next_to(a_block, UP, buff=0.2)
        self.play(FadeIn(t1))

        b_cells = VGroup(*[byte_cell(x, BLUE, w=0.62, h=0.62) for x in "bcde"])
        b_cells.arrange(RIGHT, buff=0.08).move_to(cells[1], aligned_edge=LEFT).shift(UP * 2.2)
        b_label = ct("int b", size=17, color=BLUE).next_to(b_cells, UP, buff=0.2)
        b_group = VGroup(b_cells, b_label)
        self.play(FadeIn(b_group, shift=DOWN * 0.5))
        # int must align to 4: slide from offset 1 to offset 4
        pad1 = VGroup(*[byte_cell("pad", GREY_D, tsize=12) for _ in range(3)])
        for i, p in zip((1, 2, 3), pad1):
            p.move_to(cells[i])
        self.play(
            b_group.animate.move_to(cells[4], aligned_edge=LEFT),
            LaggedStart(*[FadeIn(p) for p in pad1], lag_ratio=0.1),
        )
        t2 = ct("int b  @ 4  (alignment: multiple of 4)", size=17, color=BLUE)
        t2.next_to(b_cells, UP, buff=0.2)
        self.play(FadeIn(t2))

        c_block = byte_cell("c", TEAL, w=0.62, h=0.62).move_to(cells[8]).shift(UP * 2.2)
        self.play(FadeIn(c_block, shift=DOWN * 0.5))
        self.play(c_block.animate.move_to(cells[8]))
        pad2 = VGroup(*[byte_cell("pad", GREY_D, tsize=12) for _ in range(3)])
        for i, p in zip((9, 10, 11), pad2):
            p.move_to(cells[i])
        t3 = ct("char c  @ 8", size=17, color=TEAL).next_to(c_block, UP, buff=0.2)
        self.play(FadeIn(t3), LaggedStart(*[FadeIn(p) for p in pad2], lag_ratio=0.1))

        brace = Brace(VGroup(cells[0], cells[11]), DOWN, color=YELLOW)
        sz = ct("sizeof(struct P) = 12", size=22, color=YELLOW).next_to(brace, DOWN, buff=0.2)
        self.play(FadeIn(offs), Create(brace), FadeIn(sz))
        cap = caption(
            "struct_layout(): round each offset up to the member's alignment,\n"
            "then round the total up too. this is c's memory model, exactly.",
            21,
        )
        cap.to_edge(DOWN, buff=0.35)
        self.play(FadeIn(cap))
        self.wait(3.5)

        fade = [chip, decl, cells, a_block, t1, b_group, t2, pad1, c_block, t3, pad2, offs, brace, sz, cap]
        self.play(*[FadeOut(m) for m in fade])

    # ---------- lvalue / rvalue / indexing ----------

    def beat_lvalue(self):
        chip = chapter("3", "lvalues & indexing")
        chip.to_corner(UL, buff=0.4)
        self.play(FadeIn(chip, shift=RIGHT * 0.5))

        lv = tbox("gen_lvalue(e)", "-> ADDRESS in r0", color=YELLOW, w=4.4, h=0.8, tsize=21)
        rv = tbox("gen_expr(e)", "-> VALUE in r0", color=TEAL, w=4.4, h=0.8, tsize=21)
        pair = VGroup(lv, rv).arrange(RIGHT, buff=0.6).move_to([0, 2.7, 0])
        self.play(FadeIn(lv, shift=UP * 0.3), FadeIn(rv, shift=UP * 0.3))

        src = ct("a[1] = g + a[0];     (a is int[3])", size=22)
        src.next_to(pair, DOWN, buff=0.6)
        self.play(Write(src))

        asm = code_block(INDEX_ASM, size=19)
        asm.to_edge(LEFT, buff=0.8).move_to([asm.get_x(), -1.3, 0])
        self.play(Write(asm))

        mem, cells = mem_row(
            ["a[0]", "a[1]", "a[2]"], color=BLUE, sublabels=["fp-12", "fp-8", "fp-4"]
        )
        mem.move_to([3.6, -1.2, 0])
        self.play(FadeIn(mem, shift=UP * 0.3))

        arrow = CurvedArrow(
            asm[4].get_right() + RIGHT * 0.15,
            cells[1].get_top() + UP * 0.05,
            color=YELLOW, stroke_width=3, angle=-TAU / 7,
        )
        self.play(Create(arrow), Indicate(cells[1], color=YELLOW))

        cap = caption(
            "a[i] is sugar for *(a + i): the index is scaled by type_size(elem)  (scale_of).\n"
            "arrays decay to pointers; &buf means &buf[0].  p + 1 advances a whole element.",
            21,
        )
        cap.to_edge(DOWN, buff=0.35)
        self.play(FadeIn(cap))
        self.wait(3.5)

        fade = [chip, lv, rv, src, asm, mem, arrow, cap]
        self.play(*[FadeOut(m) for m in fade])

    # ---------- strings, calls, div ----------

    def beat_strings_calls(self):
        chip = chapter("4", "strings & calls")
        chip.to_corner(UL, buff=0.4)
        self.play(FadeIn(chip, shift=RIGHT * 0.5))

        data = code_block(DATA_ASM, size=20)
        data.to_edge(LEFT, buff=0.8).move_to([data.get_x(), 1.6, 0])
        self.play(Write(data))
        cap1 = ct(
            'collect_strings(): every "literal" is lifted into .data once.\n'
            "using one gives you its address.  char msg[6] = \"hello\" lives here too.",
            size=19,
        )
        cap1.next_to(data, DOWN, buff=0.5).align_to(data, LEFT)
        self.play(FadeIn(cap1))
        self.wait(2.5)

        call = code_block(CALL_ASM, size=19)
        call.to_edge(RIGHT, buff=0.8).move_to([call.get_x(), 1.9, 0])
        self.play(Write(call))
        cap2 = ct(
            "gen_call(): args left to right, pushed,\n"
            "popped into r0..r3, then bl.  return value: r0.\n"
            "more than 4 args? not implemented (stack args).",
            size=19,
        )
        cap2.next_to(call, DOWN, buff=0.4).align_to(call, RIGHT)
        self.play(FadeIn(cap2))
        self.wait(2.5)

        div = code_block(DIV_ASM, size=19)
        div.move_to([0.5, -2.2, 0])
        self.play(Write(div))
        cap3 = ct(
            "the CPU has no divide instruction: / and % become calls to\n"
            "__divsi3 / __modsi3, hand-written assembly in the runtime (build.py).",
            size=19,
        )
        cap3.next_to(div, DOWN, buff=0.3)
        self.play(FadeIn(cap3), Indicate(div[2], color=YELLOW))
        self.wait(3)

        fade = [chip, data, cap1, call, cap2, div, cap3]
        self.play(*[FadeOut(m) for m in fade])

    # ---------- assembler pass 1 ----------

    def beat_assembler_pass1(self):
        chip = chapter("5", "assembler.py: pass 1")
        chip.to_corner(UL, buff=0.4)
        self.play(FadeIn(chip, shift=RIGHT * 0.5))

        src = code_block(PASS1_S, size=20)
        src.to_edge(LEFT, buff=0.8).move_to([src.get_x(), 1.2, 0])
        self.play(Write(src))

        sym = tbox("symbols", None, color=BLUE, w=4.6, h=0.6, tsize=22)
        sym.move_to([3.9, 2.6, 0])
        self.play(FadeIn(sym, scale=0.8))
        entries = VGroup()
        for name, val in SYMTAB:
            nm = ct(name, size=19, color=TEAL)
            v = ct(val, size=19)
            entries.add(VGroup(nm, v).arrange(RIGHT, buff=0.5))
        entries.arrange(DOWN, aligned_edge=LEFT, buff=0.22).next_to(sym, DOWN, buff=0.4)
        self.play(LaggedStart(*[FadeIn(e, shift=RIGHT * 0.2) for e in entries], lag_ratio=0.15))

        cap = caption(
            "pass 1 encodes nothing: lay out .text/.data, turn every label into a\n"
            "(section, offset) pair, and remember each  ldr rd, =X  as a literal-pool promise.",
            21,
        )
        cap.to_edge(DOWN, buff=0.35)
        self.play(FadeIn(cap))
        self.wait(3.5)

        fade = [chip, src, sym, entries, cap]
        self.play(*[FadeOut(m) for m in fade])

    # ---------- pass 2: encoding ----------

    def beat_encode(self):
        chip = chapter("6", "pass 2: 32-bit encoding")
        chip.to_corner(UL, buff=0.4)
        self.play(FadeIn(chip, shift=RIGHT * 0.5))

        src = ct("mov r0, #5", size=26)
        src.to_edge(UP, buff=1.0)
        self.play(Write(src))

        cells = VGroup()
        group_cells = {}
        for label, bits, note, color in BITS:
            g = VGroup()
            for chch in bits:
                c = byte_cell(chch, color, w=0.42, h=0.5, tsize=16)
                cells.add(c)
                g.add(c)
            group_cells[label or bits] = g
        cells.arrange(RIGHT, buff=0.045).move_to([0, 0.9, 0])

        labels = VGroup()
        for (label, bits, note, color) in BITS:
            key = label or bits
            g = group_cells[key]
            txt = note or label
            if not txt:
                continue
            t = ct(txt, size=15, color=color)
            t.next_to(g, DOWN, buff=0.14)
            labels.add(t)
        self.play(
            LaggedStart(*[FadeIn(c, scale=0.6) for c in cells], lag_ratio=0.03), run_time=2.5
        )
        self.play(LaggedStart(*[FadeIn(t) for t in labels], lag_ratio=0.1))

        hexval = ct("= 0xE3A00005", size=26, color=YELLOW)
        hexval.next_to(cells, UP, buff=0.7)
        self.play(FadeIn(hexval, scale=0.7))
        cap = caption(
            "every ARM instruction is 32 bits: fields for condition, opcode,\n"
            "registers and operand. the assembler just packs them.",
            21,
        )
        cap.to_edge(DOWN, buff=0.35)
        self.play(FadeIn(cap))
        self.wait(3)

        fade = [chip, src, cells, labels, hexval, cap]
        self.play(*[FadeOut(m) for m in fade])

    # ---------- relocations + literal pool ----------

    def beat_relocs_pool(self):
        chip = chapter("7", "relocations & the literal pool")
        chip.to_corner(UL, buff=0.4)
        self.play(FadeIn(chip, shift=RIGHT * 0.5))

        # bl word with unknown imm24
        bl_hi = ct("1110 1011", size=26)
        bl_lo = ct("????????", size=26, color=GREY_B)
        bl_word = VGroup(bl_hi, bl_lo).arrange(RIGHT, buff=0.2).move_to([-3.6, 2.0, 0])
        bl_src = ct("bl putstr", size=22)
        bl_src.next_to(bl_word, UP, buff=0.4)
        box = hl(bl_lo, color=ORANGE, buff=0.1)
        rel_lbl = ct("R_ARM_CALL: patch my low 24 bits later", size=18, color=ORANGE)
        rel_lbl.next_to(bl_word, DOWN, buff=0.5)
        self.play(Write(bl_src), FadeIn(bl_word, shift=UP * 0.2))
        self.play(Create(box), FadeIn(rel_lbl))
        self.wait(2)

        # literal pool
        pool_src = ct("ldr r0, =.Lstr1", size=22).move_to([-3.6, -0.4, 0])
        pool_becomes = ct("ldr r0, [pc, #12]", size=22, color=TEAL).next_to(pool_src, DOWN, buff=0.4)
        pool = tbox(".text ... [ pool: &.Lstr1 ]", None, color=GREEN, w=5.0, h=0.7, tsize=19)
        pool.move_to([3.6, 0.6, 0])
        pool_cap = ct("pool of 4-byte constants, at the end of .text", size=16, color=GREY_A)
        pool_cap.next_to(pool, DOWN, buff=0.18)
        parrow = CurvedArrow(
            pool_becomes.get_right() + RIGHT * 0.15,
            pool.get_left() + LEFT * 0.1,
            color=GREEN, stroke_width=2.5, angle=-TAU / 7,
        )
        self.play(Write(pool_src), FadeIn(pool_becomes, shift=UP * 0.2))
        self.play(FadeIn(pool), FadeIn(pool_cap), Create(parrow))
        abs_lbl = ct("the pool word itself gets R_ARM_ABS32: word += symbol address", size=18, color=GREEN)
        abs_lbl.next_to(pool_cap, DOWN, buff=0.5)
        self.play(FadeIn(abs_lbl))
        self.wait(2.5)

        obj = tbox(
            "object()",
            ".text bytes | .data bytes | symbols | relocs",
            color=BLUE, w=7.6, h=0.8, tsize=22,
        )
        obj.to_edge(DOWN, buff=0.6)
        self.play(FadeIn(obj, scale=0.8))
        self.wait(2.5)

        fade = [chip, bl_src, bl_word, box, rel_lbl, pool_src, pool_becomes, pool, pool_cap, parrow, abs_lbl, obj]
        self.play(*[FadeOut(m) for m in fade])

    # ---------- linker ----------

    def beat_linker(self):
        chip = chapter("8", "linker.py: place & patch")
        chip.to_corner(UL, buff=0.4)
        self.play(FadeIn(chip, shift=RIGHT * 0.5))

        hdr = tbox("ELF hdr + phdr", "84 bytes", color=GREY_B, w=3.0, h=0.8, tsize=18)
        txt = tbox(".text", None, color=BLUE, w=3.2, h=0.8, tsize=22)
        dat = tbox(".data", None, color=GREEN, w=2.4, h=0.8, tsize=22)
        strip = VGroup(hdr, txt, dat).arrange(RIGHT, buff=0.25).move_to([0, 1.8, 0])
        addr0 = ct("0x10000", size=18, color=GREY_A).next_to(hdr, UP, buff=0.15)
        addr1 = ct("0x10000 + 84", size=18, color=GREY_A).next_to(txt, UP, buff=0.15)
        self.play(
            LaggedStart(FadeIn(hdr, shift=UP * 0.2), FadeIn(txt, shift=UP * 0.2), FadeIn(dat, shift=UP * 0.2), lag_ratio=0.15),
            FadeIn(addr0), FadeIn(addr1),
        )
        cap1 = ct(
            "sections get consecutive addresses; every symbol resolves to section.addr + offset",
            size=20,
        )
        cap1.next_to(strip, DOWN, buff=0.6)
        sym_chip = ct("main = 0x10000 + 84 + 0xa8", size=20, color=YELLOW).next_to(cap1, DOWN, buff=0.3)
        self.play(FadeIn(cap1), FadeIn(sym_chip))
        self.wait(2.5)

        # patch
        before = ct("0xEB??????", size=24, color=GREY_B).move_to([-3.2, -1.5, 0])
        formula = ct("imm24 = (S - P - 8) >> 2", size=20).move_to([0.2, -1.5, 0])
        after = ct("0xEB000643", size=24, color=YELLOW).move_to([3.6, -1.5, 0])
        parrow = Arrow(before.get_right(), after.get_left(), buff=0.15, stroke_width=2.5, color=TEAL)
        self.play(FadeIn(before), FadeIn(formula, shift=UP * 0.2))
        self.play(Create(parrow), Write(after))
        cap2 = ct("R_ARM_CALL: relative offset >> 2 into the low 24 bits.  R_ARM_ABS32: word += S", size=19)
        cap2.next_to(formula, DOWN, buff=0.5)
        self.play(FadeIn(cap2))
        self.wait(3)

        fade = [chip, hdr, txt, dat, addr0, addr1, cap1, sym_chip, before, formula, after, parrow, cap2]
        self.play(*[FadeOut(m) for m in fade])

    # ---------- ELF file ----------

    def beat_elf(self):
        chip = chapter("9", "a real ELF")
        chip.to_corner(UL, buff=0.4)
        self.play(FadeIn(chip, shift=RIGHT * 0.5))

        parts = [
            ("ELF hdr", "52", GREY_B),
            ("phdr", "32", GREY_B),
            (".text", "", BLUE),
            (".data", "", GREEN),
            (".shstrtab", "", GREY_B),
            (".symtab", "", TEAL),
            (".strtab", "", TEAL),
            ("shdrs", "40 each", GREY_B),
        ]
        boxes = VGroup()
        for name, sub, color in parts:
            boxes.add(tbox(name, sub if sub else None, color=color, w=1.62, h=0.8, tsize=15, ssize=13))
        boxes.arrange(RIGHT, buff=0.16).move_to([0, 1.4, 0])
        cap1 = ct("the file, in order:", size=20).next_to(boxes, UP, buff=0.5)
        self.play(FadeIn(cap1), LaggedStart(*[FadeIn(b, shift=UP * 0.2) for b in boxes], lag_ratio=0.08), run_time=2)

        entry_arrow = CurvedArrow(
            boxes[0].get_top() + UP * 0.05 + LEFT * 0.4,
            boxes[2].get_top() + UP * 0.05,
            color=YELLOW, stroke_width=2.5, angle=TAU / 5,
        )
        entry_lbl = ct("e_entry = _start", size=19, color=YELLOW).next_to(boxes[0], DOWN, buff=0.35)
        self.play(Create(entry_arrow), FadeIn(entry_lbl))

        cap2 = caption(
            "a genuine ELF32 executable - the same format an OS runs.\n"
            "one PT_LOAD segment maps hdr + .text + .data at 0x10000.",
            21,
        )
        cap2.to_edge(DOWN, buff=0.4)
        self.play(FadeIn(cap2))
        self.wait(3)

        fade = [chip, boxes, cap1, entry_arrow, entry_lbl, cap2]
        self.play(*[FadeOut(m) for m in fade])

    # ---------- runtime + run ----------

    def beat_run(self):
        chip = chapter("10", "runtime & qemu")
        chip.to_corner(UL, buff=0.4)
        self.play(FadeIn(chip, shift=RIGHT * 0.5))

        rt = code_block(RUNTIME_ASM, size=19)
        rt.to_edge(LEFT, buff=0.8).move_to([rt.get_x(), 1.3, 0])
        self.play(Write(rt))
        cap1 = ct(
            "_start: build a stack, call main, exit through semihosting\n"
            "(svc 0x123456: qemu plays the operating system).",
            size=19,
        )
        cap1.next_to(rt, DOWN, buff=0.5).align_to(rt, LEFT)
        self.play(FadeIn(cap1))
        self.wait(2.5)

        term = RoundedRectangle(corner_radius=0.12, width=8.0, height=2.6)
        term.set_stroke(GREY_B, 2).set_fill("#0d0d0d", 1.0).move_to([2.6, 1.4, 0])
        cmd = ct("$ qemu-system-arm -M versatilepb -nographic", size=19, color=GREEN)
        cmd2 = ct("        -semihosting -kernel output.elf", size=19, color=GREEN)
        out1 = ct("hi", size=20, color=WHITE)
        out2 = ct("hello", size=20, color=WHITE)
        term_text = VGroup(cmd, cmd2, out1, out2).arrange(DOWN, aligned_edge=LEFT, buff=0.28)
        term_text.move_to(term).shift(DOWN * 0.1)
        self.play(FadeIn(term, scale=0.9))
        self.play(Write(cmd), Write(cmd2), run_time=2)
        self.play(FadeIn(out1, shift=UP * 0.2), FadeIn(out2, shift=UP * 0.2))
        self.wait(2)

        cap2 = ct(
            "and the same ARM instructions run on the CPU built from transistors\n"
            "in section 1 - emulated in verilator. that's the whole point.",
            size=20,
        )
        cap2.to_edge(DOWN, buff=0.4)
        self.play(FadeIn(cap2))
        self.wait(3)

        fade = [chip, rt, cap1, term, term_text, cap2]
        self.play(*[FadeOut(m) for m in fade])

    # ---------- outro ----------

    def outro(self):
        b1 = tbox("compiler.py", "c -> asm: ladder, labels, frames", color=BLUE, w=5.4, h=0.9, tsize=22)
        b2 = tbox("assembler.py", "text -> 32-bit words + promises", color=TEAL, w=5.4, h=0.9, tsize=22)
        b3 = tbox("linker.py", "addresses + a real ELF", color=GREEN, w=5.4, h=0.9, tsize=22)
        b4 = tbox("qemu / your cpu", "it runs", color=ORANGE, w=5.4, h=0.9, tsize=22)
        col = VGroup(b1, b2, b3, b4).arrange(DOWN, buff=0.45).move_to([0, 0.8, 0])
        self.play(LaggedStart(*[FadeIn(b, shift=UP * 0.3) for b in col], lag_ratio=0.2), run_time=2.5)
        end = Text("from the transistor up.", font_size=36)
        end.to_edge(DOWN, buff=0.7)
        self.play(Write(end))
        self.wait(3.5)
        self.play(FadeOut(col), FadeOut(end))
