"""video 1 (simple): c source -> tokens -> ast -> arm assembly

follows compiler.py on `return 2 + 3 * 4` through tokenize(),
Parser and CodeGen, with a live r0-accumulator + push/pop stack
model. ~2.5 min.
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
# machine actions: ("set", reg, val) / ("push", val) / ("pop",)
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


class SimplePipeline(Scene):
    def construct(self):
        self.intro()
        self.pipeline_map()
        self.beat_tokenize()
        self.beat_parse()
        self.beat_codegen()
        self.outro()

    # ---------- intro ----------

    def intro(self):
        title = Text("how C becomes ARM assembly", font_size=42)
        sub = caption("a ~1500 line python compiler, built from scratch", 24)
        sub.next_to(title, DOWN, buff=0.4)
        self.play(Write(title), run_time=1.5)
        self.play(FadeIn(sub, shift=UP * 0.3))
        self.wait(1.5)
        self.play(FadeOut(title), FadeOut(sub))

    def pipeline_map(self):
        c = tbox("compiler.py", "C -> .s", color=BLUE, w=2.6, h=0.85)
        a = tbox("assembler.py", ".s -> object", color=GREY_B, w=2.6, h=0.85)
        l = tbox("linker.py", "object -> ELF", color=GREY_B, w=2.6, h=0.85)
        q = tbox("qemu", "run it", color=GREY_B, w=2.0, h=0.85)
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
        cap = caption("the whole toolchain. this video: just the compiler.", 24)
        cap.next_to(strip, DOWN, buff=0.9)
        self.play(
            LaggedStart(*[FadeIn(m, shift=UP * 0.3) for m in strip], lag_ratio=0.25)
        )
        self.play(*[FadeIn(ar) for ar in arrows], FadeIn(cap))
        self.play(Indicate(c, color=YELLOW))
        self.wait(1.5)
        self.play(FadeOut(strip), FadeOut(arrows), FadeOut(cap))

    # ---------- tokenize ----------

    def beat_tokenize(self):
        chip = chapter("1", "tokenize(src)")
        chip.to_corner(UL, buff=0.4)
        src = code_block(SRC_LINES, size=30)
        src.next_to(chip, DOWN, buff=0.8).to_edge(LEFT, buff=0.9)
        row, chips = toks_row(TOKENS)
        row.next_to(src, DOWN, buff=1.0)

        self.play(FadeIn(chip, shift=RIGHT * 0.5), Write(src))
        self.wait(0.6)
        self.play(
            LaggedStart(*[FadeIn(ch, scale=0.6) for ch in chips], lag_ratio=0.08),
            run_time=2.5,
        )

        legend = VGroup(
            VGroup(ct("int", TEAL, 18), caption("keyword", 18)),
            VGroup(ct("main", BLUE, 18), caption("name", 18)),
            VGroup(ct("2", YELLOW, 18), caption("number", 18)),
            VGroup(ct("+", ORANGE, 18), caption("operator", 18)),
            VGroup(ct(";", GREY_B, 18), caption("punctuation", 18)),
        ).arrange(DOWN, aligned_edge=LEFT, buff=0.25)
        legend.next_to(src, RIGHT, buff=1.6)
        self.play(LaggedStart(*[FadeIn(r) for r in legend], lag_ratio=0.12))

        cap = caption(
            "characters -> words with a kind and a value.\n"
            "whitespace and comments are dropped on the floor.",
            22,
        )
        cap.next_to(row, DOWN, buff=0.5)
        self.play(FadeIn(cap))
        self.wait(2.5)

        self.play(FadeOut(src), FadeOut(legend), FadeOut(cap), FadeOut(chip))
        self.tokens_row = row

    # ---------- parse ----------

    def beat_parse(self):
        chip = chapter("2", "Parser.parse_program()")
        chip.to_corner(UL, buff=0.4)
        row = self.tokens_row
        self.play(
            FadeIn(chip, shift=RIGHT * 0.5),
            row.animate.scale(0.75).to_edge(UP, buff=1.0),
        )

        main_n = node("main()", TEAL).move_to([0, 2.1, 0])
        ret_n = node("return", TEAL).move_to([0, 1.1, 0])
        add_n = node("+", ORANGE).move_to([0, -0.1, 0])
        two_n = node("2", YELLOW).move_to([-2.4, -1.3, 0])
        mul_n = node("*", ORANGE).move_to([1.4, -1.3, 0])
        three_n = node("3", YELLOW).move_to([0.5, -2.6, 0])
        four_n = node("4", YELLOW).move_to([2.4, -2.6, 0])

        pairs = [
            (main_n, ret_n),
            (ret_n, add_n),
            (add_n, two_n),
            (add_n, mul_n),
            (mul_n, three_n),
            (mul_n, four_n),
        ]
        edges = [edge(a, b) for a, b in pairs]

        self.play(
            LaggedStart(
                FadeIn(main_n, scale=0.7),
                Create(edges[0]),
                FadeIn(ret_n, scale=0.7),
                Create(edges[1]),
                FadeIn(add_n, scale=0.7),
                Create(edges[2]),
                FadeIn(two_n, scale=0.7),
                Create(edges[3]),
                FadeIn(mul_n, scale=0.7),
                Create(edges[4]),
                FadeIn(three_n, scale=0.7),
                Create(edges[5]),
                FadeIn(four_n, scale=0.7),
                lag_ratio=0.25,
            ),
            run_time=5,
        )

        cap = caption("the tree IS the precedence: * nests deeper than +", 24)
        cap.to_edge(DOWN, buff=0.4)
        self.play(
            FadeIn(cap),
            Indicate(
                VGroup(mul_n, three_n, four_n, edges[3], edges[4], edges[5]),
                color=YELLOW,
            ),
        )
        self.wait(1.5)

        snippet = ct(
            "BinOp(add, IntLit(2), BinOp(mul, IntLit(3), IntLit(4)))",
            size=20,
            color=GREY_A,
        )
        snippet.next_to(cap, UP, buff=0.3)
        self.play(FadeIn(snippet))
        self.wait(2)

        self.tree_nodes = dict(
            main=main_n, ret=ret_n, add=add_n, two=two_n,
            mul=mul_n, three=three_n, four=four_n,
        )
        self.tree = VGroup(*edges, main_n, ret_n, add_n, two_n, mul_n, three_n, four_n)
        self.play(FadeOut(row), FadeOut(cap), FadeOut(snippet), FadeOut(chip))
        self.play(self.tree.animate.scale(0.65).move_to([-5.2, -0.5, 0]))

    # ---------- codegen ----------

    def beat_codegen(self):
        chip = chapter("3", "CodeGen.gen_expr()")
        chip.to_corner(UL, buff=0.4)
        self.play(FadeIn(chip, shift=RIGHT * 0.5))

        rule1 = caption("rule: every expression computes its result into r0", 24)
        rule2 = caption("rule: temporaries ride the stack (push / pop)", 24)
        rules = VGroup(rule1, rule2).arrange(DOWN, aligned_edge=LEFT, buff=0.15)
        rules.to_edge(DOWN, buff=0.35)
        self.play(FadeIn(rule1), FadeIn(rule2))

        asm = code_block(["    " + l for l in ASM], size=24)
        asm.to_edge(LEFT, buff=3.1).move_to([asm.get_x(), -0.2, 0])

        r0g = reg_box("r0", color=TEAL, w=1.3, h=0.6)
        r1g = reg_box("r1", color=TEAL, w=1.3, h=0.6)
        VGroup(r0g, r1g).arrange(DOWN, aligned_edge=LEFT, buff=0.25).move_to(
            [4.7, 2.7, 0]
        )
        stack_title = ct("stack", size=20, color=GREY_A).move_to([4.7, 1.2, 0])
        self.play(FadeIn(r0g), FadeIn(r1g), FadeIn(stack_title))

        slot_ys = [0.55, -0.2, -0.95]
        stack_slots = []

        def make_slot(v):
            t = ct(str(v), color=YELLOW, size=22)
            b = RoundedRectangle(corner_radius=0.06, width=0.9, height=0.5)
            b.set_stroke(GREY_B, 1.5).set_fill(GREY_B, 0.15)
            return VGroup(b, t.move_to(b))

        def set_val(rg, val):
            return Transform(rg.val, ct(str(val), color=YELLOW, size=26).move_to(rg.val))

        regs = {"r0": r0g, "r1": r1g}
        link = Arrow(
            asm[0].get_right() + RIGHT * 0.12,
            r0g.get_left() + LEFT * 0.05,
            buff=0.04,
            stroke_width=2.5,
            color=GREY_A,
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
                anims += [FadeIn(line, shift=LEFT * 0.4), Create(line_hl), Create(cursor), Create(link)]
                first = False
            else:
                anims += [Write(line), Transform(line_hl, hl(line)), Transform(cursor, hl(nd))]
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
                        buff=0.04,
                        stroke_width=2.5,
                        color=GREY_A,
                        max_tip_length_to_length_ratio=0.3,
                    ),
                )
            )
            self.play(*anims)
            self.wait(0.35)

        note = VGroup(
            ct("mov immediate only encodes 0..255", size=20, color=GREY_A),
            ct("bigger constants: ldr r0, =value (literal pool, video 3)", size=20, color=GREY_A),
        ).arrange(DOWN, aligned_edge=LEFT, buff=0.12)
        note.next_to(asm, DOWN, buff=0.6)
        self.play(FadeIn(note), Indicate(r0g, color=YELLOW))
        self.wait(2.5)

        self.play(
            FadeOut(chip), FadeOut(rule1), FadeOut(rule2), FadeOut(note),
            FadeOut(self.tree), FadeOut(asm), FadeOut(r0g), FadeOut(r1g),
            FadeOut(stack_title), FadeOut(line_hl), FadeOut(cursor), FadeOut(link),
        )

    # ---------- outro ----------

    def outro(self):
        t1 = tbox("tokenize()", "characters -> tokens", color=TEAL, w=3.4, h=0.8, tsize=22)
        t2 = tbox("Parser", "tokens -> AST", color=BLUE, w=3.4, h=0.8, tsize=22)
        t3 = tbox("CodeGen", "AST -> ARM asm", color=ORANGE, w=3.4, h=0.8, tsize=22)
        row = VGroup(t1, t2, t3).arrange(RIGHT, buff=0.8).move_to([0, 1.2, 0])
        cap1 = ct("compile_text(src) = these three, in order", size=24)
        cap1.next_to(row, DOWN, buff=0.9)
        self.play(
            LaggedStart(*[FadeIn(m, shift=UP * 0.3) for m in row], lag_ratio=0.3),
            FadeIn(cap1),
        )
        nexts = VGroup(
            caption("video 2: if / while / for, functions, and stack frames", 22),
            caption("video 3: types, pointers, the assembler, the linker, ELF", 22),
        ).arrange(DOWN, aligned_edge=LEFT, buff=0.2)
        nexts.next_to(cap1, DOWN, buff=0.6)
        self.play(FadeIn(nexts))
        self.wait(3)
        self.play(FadeOut(row), FadeOut(cap1), FadeOut(nexts))
