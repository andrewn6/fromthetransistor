"""the toolchain video (1 of 3): C source -> compiler -> assembler ->
linker -> a running ELF.

one video, under 2 minutes. real code from compiler.py, assembler.py
and linker.py; every asm snippet is authentic compiler output.

series: 1) the toolchain  2) the CPU  3) bootloader + OS
"""
from manim import *
from viz import (
    ct,
    code_block,
    hl,
    toks_row,
    node,
    edge,
    tbox,
    caption,
    chapter,
    reg_box,
)

SRC_LINES = ["int main() {", "    return 2 + 3 * 4;", "}"]

TOKENS = [
    ("int", "kw"), ("main", "id"), ("(", "p"), (")", "p"), ("{", "p"),
    ("return", "kw"), ("2", "intl"), ("+", "o"), ("3", "intl"), ("*", "o"),
    ("4", "intl"), (";", "p"), ("}", "p"),
]

ASM = [
    "mov r0, #2",
    "push {r0}",
    "mov r0, #3",
    "push {r0}",
    "mov r0, #4",
    "pop {r1}",
    "mul r0, r1, r0",
    "pop {r1}",
    "add r0, r1, r0",
]

# (asm line index, machine actions, ast node key)
STEPS = [
    (0, [("set", "r0", 2)], "two"),
    (1, [("push", 2)], "two"),
    (2, [("set", "r0", 3)], "three"),
    (3, [("push", 3)], "three"),
    (4, [("set", "r0", 4)], "four"),
    (5, [("pop",), ("set", "r1", 3)], "mul"),
    (6, [("set", "r0", 12)], "mul"),
    (7, [("pop",), ("set", "r1", 2)], "add"),
    (8, [("set", "r0", 14)], "add"),
]

EXTRA_CARDS = [
    ("if / while / for", "cmp r0, #0  beq .Lelse", "control flow = labels + branches"),
    ("functions", "push {fp, lr}  str r0, [fp, #-4]", "one frame per call, fp-relative slots"),
    ("a[i]", "*(a + i), scaled by sizeof(elem)", "pointers step whole elements"),
    ('"hi\\n"', '.Lstr1:  .asciz "hi\\n"', "string literals live in .data"),
]

PASS1_S = [
    ".text",
    "main:",
    "    push {fp, lr}",
    ".Lret1:",
    "    mov sp, fp",
    ".data",
    'msg:  .asciz "hello"',
]

SYMTAB = [
    ("main", "(.text, +0xa8)"),
    (".Lret1", "(.text, +0xd0)"),
    ("msg", "(.data, +0x4)"),
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


def byte_cell(text, color=None, w=0.42, h=0.5, tsize=15):
    b = RoundedRectangle(corner_radius=0.05, width=w, height=h)
    b.set_stroke(GREY_B, 1.5)
    if color:
        b.set_fill(color, 0.22)
    t = ct(text, size=tsize, color=color or GREY_A)
    return VGroup(b, t.move_to(b))


class Toolchain(Scene):
    def construct(self):
        self.intro()
        self.pipeline_map()
        self.compiler_front()
        self.compiler_backend()
        self.compiler_extras()
        self.assembler()
        self.linker()
        self.run()
        self.cpu_run()
        self.outro()

    # ---------- intro ----------

    def intro(self):
        title = Text("the C toolchain", font_size=44)
        sub = caption("compiler -> assembler -> linker -> a running ELF", 24)
        sub.next_to(title, DOWN, buff=0.4)
        self.play(Write(title), run_time=1.2)
        self.play(FadeIn(sub, shift=UP * 0.3))
        self.wait(1.2)
        self.play(FadeOut(title), FadeOut(sub))

    def pipeline_map(self):
        c = tbox("compiler.py", "C -> .s", color=BLUE, w=2.6, h=0.85)
        a = tbox("assembler.py", ".s -> object", color=TEAL, w=2.6, h=0.85)
        l = tbox("linker.py", "object -> ELF", color=GREEN, w=2.6, h=0.85)
        q = tbox("run", "qemu / the CPU", color=ORANGE, w=2.4, h=0.85)
        strip = VGroup(c, a, l, q).arrange(RIGHT, buff=0.8).move_to(ORIGIN)
        arrows = VGroup(
            *[
                Arrow(
                    x.get_right() + RIGHT * 0.08,
                    y.get_left() + LEFT * 0.08,
                    buff=0.03,
                    stroke_width=3,
                    max_tip_length_to_length_ratio=0.25,
                )
                for x, y in zip(strip[:3], strip[1:])
            ]
        )
        cap = caption("all python, all in this repo. build.py drives it end to end.", 24)
        cap.next_to(strip, DOWN, buff=0.9)
        self.play(
            LaggedStart(*[FadeIn(m, shift=UP * 0.3) for m in strip], lag_ratio=0.2),
            run_time=1.8,
        )
        self.play(*[FadeIn(ar) for ar in arrows], FadeIn(cap), run_time=0.8)
        self.wait(1.8)
        self.play(FadeOut(strip), FadeOut(arrows), FadeOut(cap), run_time=0.6)

    # ---------- compiler: front end ----------

    def compiler_front(self):
        chip = chapter("1", "compiler.py: tokenize + parse")
        chip.to_corner(UL, buff=0.4)
        src = code_block(SRC_LINES, size=26)
        src.next_to(chip, DOWN, buff=0.7).to_edge(LEFT, buff=0.9)
        row, chips = toks_row(TOKENS)
        row.next_to(src, DOWN, buff=0.7)

        self.play(FadeIn(chip, shift=RIGHT * 0.5), Write(src), run_time=1.4)
        self.play(
            LaggedStart(*[FadeIn(ch, scale=0.6) for ch in chips], lag_ratio=0.06),
            run_time=2.2,
        )

        # small ast on the right
        main_n = node("main()", TEAL).move_to([3.6, 2.2, 0])
        ret_n = node("return", TEAL).move_to([3.6, 1.3, 0])
        add_n = node("+", ORANGE).move_to([3.2, 0.2, 0])
        two_n = node("2", YELLOW).move_to([2.2, -0.9, 0])
        mul_n = node("*", ORANGE).move_to([4.4, -0.9, 0])
        three_n = node("3", YELLOW).move_to([3.7, -2.0, 0])
        four_n = node("4", YELLOW).move_to([5.2, -2.0, 0])
        nodes = [main_n, ret_n, add_n, two_n, mul_n, three_n, four_n]
        pairs = [
            (main_n, ret_n), (ret_n, add_n), (add_n, two_n),
            (add_n, mul_n), (mul_n, three_n), (mul_n, four_n),
        ]
        edges = [edge(p, ch) for p, ch in pairs]
        seq = []
        for i, (p, ch) in enumerate(pairs):
            seq += [Create(edges[i]), FadeIn(nodes[i + 1], scale=0.7)]
        self.play(
            LaggedStart(FadeIn(main_n, scale=0.7), *seq, lag_ratio=0.25),
            run_time=3,
        )
        cap = caption(
            "tokenize(): characters -> tokens.  Parser: tokens -> a tree.\n"
            "the tree IS the precedence: * nests deeper than +.",
            21,
        )
        cap.to_edge(DOWN, buff=0.35)
        self.play(FadeIn(cap), run_time=0.7)
        self.wait(2)

        self.tree_nodes = dict(
            two=two_n, three=three_n, four=four_n, mul=mul_n, add=add_n,
        )
        self.tree = VGroup(*edges, *nodes)
        self.play(
            FadeOut(chip), FadeOut(src), FadeOut(row), FadeOut(cap), run_time=0.6
        )
        self.play(self.tree.animate.scale(0.62).move_to([-4.9, -0.4, 0]), run_time=0.8)

    # ---------- compiler: codegen ----------

    def compiler_backend(self):
        chip = chapter("2", "CodeGen: tree -> ARM asm")
        chip.to_corner(UL, buff=0.4)
        self.play(FadeIn(chip, shift=RIGHT * 0.5), run_time=0.6)

        rule = caption("every expression computes into r0; temporaries ride the stack", 22)
        rule.to_edge(DOWN, buff=0.35)

        asm = code_block(["    " + l for l in ASM], size=22)
        asm.to_edge(LEFT, buff=3.0).move_to([asm.get_x(), -0.2, 0])

        r0g = reg_box("r0", color=TEAL, w=1.3, h=0.6)
        r1g = reg_box("r1", color=TEAL, w=1.3, h=0.6)
        VGroup(r0g, r1g).arrange(DOWN, aligned_edge=LEFT, buff=0.25).move_to(
            [4.7, 2.7, 0]
        )
        stack_title = ct("stack", size=20, color=GREY_A).move_to([4.7, 1.2, 0])
        self.play(
            FadeIn(rule), FadeIn(r0g), FadeIn(r1g), FadeIn(stack_title), run_time=0.8
        )

        slot_ys = [0.55, -0.2, -0.95]
        stack_slots = []

        def make_slot(v):
            t = ct(str(v), color=YELLOW, size=22)
            b = RoundedRectangle(corner_radius=0.06, width=0.9, height=0.5)
            b.set_stroke(GREY_B, 1.5).set_fill(GREY_B, 0.15)
            return VGroup(b, t.move_to(b))

        def set_val(rg, val):
            return Transform(
                rg.val, ct(str(val), color=YELLOW, size=26).move_to(rg.val)
            )

        regs = {"r0": r0g, "r1": r1g}
        link = Arrow(
            asm[0].get_right() + RIGHT * 0.12,
            r0g.get_left() + LEFT * 0.05,
            buff=0.04, stroke_width=2.5, color=GREY_A,
            max_tip_length_to_length_ratio=0.3,
        )
        line_hl = hl(asm[0])
        cursor = hl(self.tree_nodes["two"])
        first = True
        for idx, actions, nkey in STEPS:
            line = asm[idx]
            nd = self.tree_nodes[nkey]
            anims = []
            if first:
                anims += [
                    FadeIn(line, shift=LEFT * 0.4), Create(line_hl),
                    Create(cursor), Create(link),
                ]
                first = False
            else:
                anims += [
                    Write(line),
                    Transform(line_hl, hl(line)),
                    Transform(cursor, hl(nd)),
                ]
            target = r0g
            for act in actions:
                if act[0] == "set":
                    _, which, val = act
                    rg = regs[which]
                    anims.append(set_val(rg, val))
                    target = rg
                elif act[0] == "push":
                    _, v = act
                    s = make_slot(v).move_to([4.7, slot_ys[len(stack_slots)], 0])
                    stack_slots.append(s)
                    anims.append(FadeIn(s, scale=0.7))
                    target = s
                elif act[0] == "pop":
                    s = stack_slots.pop()
                    anims.append(FadeOut(s))
                    target = s
            anims.append(
                Transform(
                    link,
                    Arrow(
                        line.get_right() + RIGHT * 0.12,
                        target.get_left() + LEFT * 0.05,
                        buff=0.04, stroke_width=2.5, color=GREY_A,
                        max_tip_length_to_length_ratio=0.3,
                    ),
                )
            )
            self.play(*anims, run_time=0.8)
            self.wait(0.2)

        note = ct(
            "2 + 3*4 = 14, in r0.  (mov only encodes 0..255; bigger constants\n"
            "go through a literal pool:  ldr r0, =value)",
            size=19, color=GREY_A,
        )
        note.next_to(asm, DOWN, buff=0.5)
        self.play(FadeIn(note), Indicate(r0g, color=YELLOW), run_time=0.8)
        self.wait(1.8)

        fade = [
            chip, rule, self.tree, asm, r0g, r1g, stack_title,
            line_hl, cursor, link, note,
        ]
        self.play(*[FadeOut(m) for m in fade], run_time=0.6)

    # ---------- compiler: the rest, quickly ----------

    def compiler_extras(self):
        chip = chapter("3", "what else it lowers")
        chip.to_corner(UL, buff=0.4)
        self.play(FadeIn(chip, shift=RIGHT * 0.5), run_time=0.6)

        cards = VGroup()
        for title, code, note in EXTRA_CARDS:
            t = ct(title, size=21, color=YELLOW)
            c1 = ct(code, size=17, color=WHITE)
            n1 = caption(note, 16)
            card = VGroup(t, c1, n1).arrange(DOWN, aligned_edge=LEFT, buff=0.16)
            bg = RoundedRectangle(
                corner_radius=0.12,
                width=max(card.width + 0.5, 3.2),
                height=card.height + 0.4,
            )
            bg.set_stroke(GREY_B, 1.5).set_fill(GREY_B, 0.08)
            cards.add(VGroup(bg, card.move_to(bg)))
        cards.arrange_in_grid(rows=2, cols=2, buff=0.45).move_to([0, 0.1, 0])
        self.play(
            LaggedStart(*[FadeIn(cd, shift=UP * 0.25) for cd in cards], lag_ratio=0.15),
            run_time=2.2,
        )
        cap = caption(
            "types drive all of it: type_size() and align_of() size every load,\n"
            "store, frame slot and struct field.",
            21,
        )
        cap.to_edge(DOWN, buff=0.35)
        self.play(FadeIn(cap), run_time=0.7)
        self.wait(3)
        self.play(FadeOut(chip), FadeOut(cards), FadeOut(cap), run_time=0.6)

    # ---------- assembler ----------

    def assembler(self):
        chip = chapter("4", "assembler.py: two passes")
        chip.to_corner(UL, buff=0.4)
        self.play(FadeIn(chip, shift=RIGHT * 0.5), run_time=0.6)

        # pass 1
        src = code_block(PASS1_S, size=19)
        src.to_edge(LEFT, buff=0.8).move_to([src.get_x(), 1.2, 0])
        sym = tbox("symbols", None, color=BLUE, w=4.0, h=0.55, tsize=21)
        sym.move_to([3.9, 2.4, 0])
        entries = VGroup()
        for name, val in SYMTAB:
            nm = ct(name, size=18, color=TEAL)
            v = ct(val, size=18)
            entries.add(VGroup(nm, v).arrange(RIGHT, buff=0.5))
        entries.arrange(DOWN, aligned_edge=LEFT, buff=0.2).next_to(sym, DOWN, buff=0.35)
        cap1 = ct(
            "pass 1: labels become (section, offset) pairs. no encoding yet.",
            size=19,
        )
        cap1.to_edge(DOWN, buff=0.5)
        self.play(Write(src), run_time=1.2)
        self.play(
            FadeIn(sym, scale=0.8),
            LaggedStart(*[FadeIn(e, shift=RIGHT * 0.2) for e in entries], lag_ratio=0.15),
            FadeIn(cap1),
            run_time=1.8,
        )
        self.wait(1.6)

        # pass 2: encoding
        self.play(FadeOut(src), FadeOut(sym), FadeOut(entries), FadeOut(cap1), run_time=0.6)
        mne = ct("mov r0, #5", size=26).to_edge(UP, buff=1.0)
        cells = VGroup()
        group_cells = {}
        for label, bits, note, color in BITS:
            g = VGroup()
            for chch in bits:
                c = byte_cell(chch, color)
                cells.add(c)
                g.add(c)
            group_cells[label or bits] = g
        cells.arrange(RIGHT, buff=0.045).move_to([0, 0.9, 0])
        labels = VGroup()
        for (label, bits, note, color) in BITS:
            g = group_cells[label or bits]
            txt = note or label
            if not txt:
                continue
            t = ct(txt, size=14, color=color).next_to(g, DOWN, buff=0.13)
            labels.add(t)
        hexval = ct("= 0xE3A00005", size=25, color=YELLOW).next_to(cells, UP, buff=0.7)
        cap2 = ct(
            "pass 2: every instruction packs into 32 bits of fields.",
            size=19,
        )
        cap2.to_edge(DOWN, buff=0.5)
        self.play(Write(mne), run_time=0.8)
        self.play(
            LaggedStart(*[FadeIn(c, scale=0.6) for c in cells], lag_ratio=0.03),
            run_time=2,
        )
        self.play(
            LaggedStart(*[FadeIn(t) for t in labels], lag_ratio=0.08),
            FadeIn(hexval, scale=0.7), FadeIn(cap2),
            run_time=1.2,
        )
        self.wait(2)

        fade = [chip, mne, cells, labels, hexval, cap2]
        self.play(*[FadeOut(m) for m in fade], run_time=0.6)

    # ---------- linker ----------

    def linker(self):
        chip = chapter("5", "linker.py: place & patch")
        chip.to_corner(UL, buff=0.4)
        self.play(FadeIn(chip, shift=RIGHT * 0.5), run_time=0.6)

        # relocation: the promise left by the assembler
        bl_hi = ct("1110 1011", size=24)
        bl_lo = ct("????????", size=24, color=GREY_B)
        bl_word = VGroup(bl_hi, bl_lo).arrange(RIGHT, buff=0.2).move_to([-3.4, 2.1, 0])
        bl_src = ct("bl putstr  ->", size=21).next_to(bl_word, LEFT, buff=0.4)
        box = hl(bl_lo, color=ORANGE, buff=0.1)
        rel_lbl = ct("R_ARM_CALL: patch my low 24 bits", size=17, color=ORANGE)
        rel_lbl.next_to(bl_word, DOWN, buff=0.4)
        self.play(
            Write(bl_src), FadeIn(bl_word, shift=UP * 0.2), Create(box), FadeIn(rel_lbl),
            run_time=1.4,
        )

        # memory map
        txt = tbox(".text", None, color=BLUE, w=2.8, h=0.75, tsize=21)
        dat = tbox(".data", None, color=GREEN, w=2.2, h=0.75, tsize=21)
        strip = VGroup(txt, dat).arrange(RIGHT, buff=0.25).move_to([2.9, 2.1, 0])
        addr = ct("0x10000", size=17, color=GREY_A).next_to(strip, UP, buff=0.15)
        self.play(FadeIn(strip, shift=UP * 0.2), FadeIn(addr), run_time=0.8)
        cap1 = ct(
            "sections get consecutive addresses; symbols resolve to addr + offset",
            size=19,
        )
        cap1.next_to(strip, DOWN, buff=0.5)
        self.play(FadeIn(cap1), run_time=0.6)
        self.wait(1.2)

        # patch
        before = ct("0xEB??????", size=23, color=GREY_B).move_to([-3.4, -0.1, 0])
        formula = ct("imm24 = (S - P - 8) >> 2", size=19).move_to([0.4, -0.1, 0])
        after = ct("0xEB000643", size=23, color=YELLOW).move_to([4.1, -0.1, 0])
        parrow = Arrow(
            before.get_right(), after.get_left(), buff=0.15, stroke_width=2.5, color=TEAL
        )
        self.play(FadeIn(before), FadeIn(formula, shift=UP * 0.2), run_time=0.8)
        self.play(Create(parrow), Write(after), run_time=1.2)
        self.wait(0.8)

        # ELF file layout
        parts = [
            ("ELF hdr", GREY_B), ("phdr", GREY_B), (".text", BLUE), (".data", GREEN),
            (".symtab", TEAL), (".strtab", TEAL), ("shdrs", GREY_B),
        ]
        boxes = VGroup()
        for name, color in parts:
            boxes.add(tbox(name, None, color=color, w=1.66, h=0.75, tsize=16))
        boxes.arrange(RIGHT, buff=0.16).move_to([0, -2.0, 0])
        entry_lbl = ct("e_entry = _start", size=18, color=YELLOW).next_to(boxes[0], DOWN, buff=0.3)
        cap2 = ct("out comes a genuine ELF32 executable - the same format an OS runs.", size=19)
        cap2.to_edge(DOWN, buff=0.35)
        self.play(
            FadeOut(bl_src), FadeOut(bl_word), FadeOut(box), FadeOut(rel_lbl),
            FadeOut(strip), FadeOut(addr), FadeOut(cap1),
            FadeOut(before), FadeOut(formula), FadeOut(after), FadeOut(parrow),
            run_time=0.6,
        )
        self.play(
            LaggedStart(*[FadeIn(b, shift=UP * 0.2) for b in boxes], lag_ratio=0.09),
            FadeIn(entry_lbl), FadeIn(cap2),
            run_time=1.8,
        )
        self.wait(2.2)

        self.play(FadeOut(chip), FadeOut(boxes), FadeOut(entry_lbl), FadeOut(cap2), run_time=0.6)

    # ---------- run ----------

    def run(self):
        chip = chapter("6", "run it")
        chip.to_corner(UL, buff=0.4)
        self.play(FadeIn(chip, shift=RIGHT * 0.5), run_time=0.6)

        rt = code_block(
            [
                "_start:",
                "    ldr sp, =0x00800000   @ build the stack",
                "    bl main",
                "    svc 0x123456          @ semihosting exit",
            ],
            size=19,
        )
        rt.to_edge(LEFT, buff=0.8).move_to([rt.get_x(), 1.5, 0])
        cap1 = ct("the runtime (build.py) calls main and talks to qemu", size=19)
        cap1.next_to(rt, DOWN, buff=0.5).align_to(rt, LEFT)
        self.play(Write(rt), FadeIn(cap1), run_time=1.6)

        term = RoundedRectangle(corner_radius=0.12, width=7.4, height=2.3)
        term.set_stroke(GREY_B, 2).set_fill("#0d0d0d", 1.0).move_to([3.0, 1.3, 0])
        cmd = ct("$ qemu-system-arm -M versatilepb -nographic", size=18, color=GREEN)
        cmd2 = ct("    -semihosting -kernel output.elf", size=18, color=GREEN)
        out = ct("hi", size=20, color=WHITE)
        term_text = VGroup(cmd, cmd2, out).arrange(DOWN, aligned_edge=LEFT, buff=0.3)
        term_text.move_to(term).shift(DOWN * 0.08)
        self.play(FadeIn(term, scale=0.92), run_time=0.7)
        self.play(Write(cmd), Write(cmd2), run_time=1.6)
        self.play(FadeIn(out, shift=UP * 0.2), run_time=0.6)

        cap2 = ct(
            "qemu is only the reference model.\nthe same instructions, on our own silicon:",
            size=20,
        )
        cap2.to_edge(DOWN, buff=0.35)
        self.play(FadeIn(cap2), run_time=0.7)
        self.wait(2)

        fade = [chip, rt, cap1, term, term_text, cap2]
        self.play(*[FadeOut(m) for m in fade], run_time=0.6)

    # ---------- our own cpu ----------

    def cpu_run(self):
        chip = chapter("7", "the same binary, on our own cpu")
        chip.to_corner(UL, buff=0.4)
        self.play(FadeIn(chip, shift=RIGHT * 0.5), run_time=0.6)

        # the flat image: linker sections concatenated
        txt = tbox(".text", None, color=BLUE, w=1.5, h=0.55, tsize=17)
        dat = tbox(".data", None, color=GREEN, w=1.5, h=0.55, tsize=17)
        plus = ct("+", size=20, color=GREY_A)
        img = VGroup(txt, plus, dat).arrange(RIGHT, buff=0.2).move_to([-4.8, 2.3, 0])
        binb = tbox("cputest.bin", "flat image, 1832 bytes", color=YELLOW, w=3.1, h=0.7, tsize=19)
        binb.move_to([-4.3, 1.0, 0])
        ar1 = Arrow(
            img.get_bottom(), binb.get_top(), buff=0.1,
            stroke_width=2.5, color=GREY_A, max_tip_length_to_length_ratio=0.25,
        )
        note = ct("one branch word at 0x10000 jumps to the entry", size=15, color=GREY_A)
        note.next_to(binb, DOWN, buff=0.3)
        self.play(FadeIn(img, shift=UP * 0.2), run_time=0.8)
        self.play(Create(ar1), FadeIn(binb, scale=0.85), FadeIn(note), run_time=1.0)

        # the core and its memory
        cpu = tbox("arm7_cpu.v", "fetch -> exec -> block", color=ORANGE, w=3.7, h=0.9, tsize=21)
        cpu.move_to([2.7, 2.3, 0])
        sram = tbox("sram 1MB", "image + data + stack", color=BLUE, w=3.2, h=0.7, tsize=17)
        sram.move_to([2.7, 0.9, 0])
        bus = DoubleArrow(
            cpu.get_bottom(), sram.get_top(), buff=0.08,
            stroke_width=2.5, color=GREY_A, max_tip_length_to_length_ratio=0.2,
        )
        cap1 = ct("one memory access per clock edge, hand-written verilog", size=18)
        cap1.next_to(sram, DOWN, buff=0.4)
        self.play(FadeIn(cpu, shift=UP * 0.2), run_time=0.8)
        self.play(Create(bus), FadeIn(sram, shift=UP * 0.2), FadeIn(cap1), run_time=1.0)

        # the run
        term = RoundedRectangle(corner_radius=0.12, width=10.6, height=3.3)
        term.set_stroke(GREY_B, 2).set_fill("#0d0d0d", 1.0).move_to([0.3, -1.7, 0])
        lines = [
            ct("$ cpu_sim cputest.bin 0x10054", size=17, color=GREEN),
            ct("loaded 1832 bytes at 0x00010054, entry 0x00010054", size=17),
            ct("halted on svc after 20258 cycles", size=17),
            ct("check 0: got 55 want 55 ok      check 4: got 3000 want 3000 ok", size=15, color=GREY_A),
            ct("check 6: got 104105 want 104105 ok", size=15, color=GREY_A),
            ct("ALL 9 CHECKS PASS", size=22, color=GREEN),
        ]
        ttext = VGroup(*lines).arrange(DOWN, aligned_edge=LEFT, buff=0.24)
        ttext.move_to(term).shift(UP * 0.05)
        self.play(FadeIn(term, scale=0.95), run_time=0.6)
        self.play(Write(lines[0]), run_time=1.0)
        self.play(FadeIn(lines[1], shift=UP * 0.2), run_time=0.5)

        # clock edges tick by in the core while it runs
        cyc = ct("20,258 clock edges", size=18, color=TEAL)
        cyc.next_to(cpu, RIGHT, buff=0.5)
        self.play(
            FadeIn(cyc, scale=0.7), Indicate(cpu, color=ORANGE),
            FadeIn(lines[2], shift=UP * 0.2), run_time=1.0,
        )
        self.play(
            FadeIn(lines[3], shift=UP * 0.2), FadeIn(lines[4], shift=UP * 0.2),
            run_time=0.7,
        )
        self.play(Write(lines[5]), Flash(lines[5], color=GREEN), run_time=1.2)

        cap2 = ct(
            "no qemu, no interpreter: a c program we compiled,\n"
            "executed by a cpu we built, cycle by cycle.",
            size=20,
        )
        cap2.to_edge(DOWN, buff=0.3)
        self.play(FadeIn(cap2), run_time=0.7)
        self.wait(2.6)

        fade = [chip, img, ar1, binb, note, cpu, sram, bus, cap1, term, ttext, cyc, cap2]
        self.play(*[FadeOut(m) for m in fade], run_time=0.6)

    # ---------- outro ----------

    def outro(self):
        b1 = tbox("compiler.py", "c -> asm: tokens, tree, walks", color=BLUE, w=5.6, h=0.85, tsize=21)
        b2 = tbox("assembler.py", "text -> 32-bit words + promises", color=TEAL, w=5.6, h=0.85, tsize=21)
        b3 = tbox("linker.py", "addresses + a real ELF", color=GREEN, w=5.6, h=0.85, tsize=21)
        b4 = tbox("arm7_cpu.v", "executes it, one clock at a time", color=ORANGE, w=5.6, h=0.85, tsize=21)
        col = VGroup(b1, b2, b3, b4).arrange(DOWN, buff=0.32).move_to([0, 0.8, 0])
        self.play(
            LaggedStart(*[FadeIn(b, shift=UP * 0.3) for b in col], lag_ratio=0.2),
            run_time=1.8,
        )
        nexts = VGroup(
            caption("video 2: inside the cpu - decode, flags, the block phase", 22),
            caption("video 3: the bootloader + a tiny OS", 22),
        ).arrange(DOWN, aligned_edge=LEFT, buff=0.2)
        nexts.to_edge(DOWN, buff=0.7)
        self.play(FadeIn(nexts), run_time=0.8)
        self.wait(2.8)
        self.play(FadeOut(col), FadeOut(nexts), run_time=0.6)
