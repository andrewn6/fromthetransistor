"""video 2 (intermediate): the parser ladder, statement dispatch,
control-flow lowering, and function frames.

how Parser builds trees by precedence, and how CodeGen lowers
if/else, while, for and calls into labels + branches + fp-relative
stack slots. ~4.5 min.
"""
from manim import *
from viz import ct, code_block, hl, toks_row, node, edge, tbox, caption, chapter

LADDER = [
    ("parse_expr", "(entry: assignment)"),
    ("parse_assign", "=  +="),
    ("parse_logic_or", "||"),
    ("parse_logic_and", "&&"),
    ("parse_equality", "==  !="),
    ("parse_relational", "<  <=  >  >="),
    ("parse_additive", "+  -"),
    ("parse_term", "*  /  %"),
    ("parse_unary", "-  !  *  &"),
    ("parse_postfix", "( )  [ ]  ++  --"),
    ("parse_primary", "literals, names"),
]

IF_ASM = [
    "    ...                  @ r0 = (x < 0)",
    "    cmp r0, #0",
    "    beq .Lelse2          @ 0 means false",
    "    ...                  @ then: return -x",
    "    b .Lret1             @ jump over else",
    ".Lelse2:",
    "    ...                  @ else: return x",
    "    b .Lret1",
    ".Lret1:                 @ shared epilogue",
]

WHILE_ASM = [
    ".Lwhile2:               @ top",
    "    ...                 @ r0 = (i < n)",
    "    cmp r0, #0",
    "    beq .Lwend3          @ cond false -> out",
    "    ...                 @ s += i;  i += 1",
    "    b .Lwhile2           @ go re-test",
    ".Lwend3:",
]

FRAMES_ASM = [
    "sum:",
    "    push {fp, lr}",
    "    mov fp, sp",
    "    str r0, [fp, #-4]    @ spill arg n",
    "    sub sp, sp, #16      @ room for s, i",
]

EPILOGUE_ASM = [
    "    b .Lret1             @ any 'return'",
    ".Lret1:                 @ one per function",
    "    mov sp, fp",
    "    pop {fp, lr}",
    "    bx lr",
]

SUM_C = [
    "int sum(int n) {",
    "    int s = 0;",
    "    int i = 0;",
    "    while (i < n)",
    "        s += i;",
    "    return s;",
    "}",
]


class ParserAndCodegen(Scene):
    def construct(self):
        self.intro()
        self.beat_ladder()
        self.beat_dispatch()
        self.beat_if()
        self.beat_while()
        self.beat_for()
        self.beat_frames()
        self.outro()

    # ---------- intro ----------

    def intro(self):
        title = Text("inside the compiler", font_size=42)
        sub = caption(
            "how the parser builds trees, and how codegen lowers control flow", 24
        )
        sub.next_to(title, DOWN, buff=0.4)
        self.play(Write(title), run_time=1.2)
        self.play(FadeIn(sub, shift=UP * 0.3))
        self.wait(1.5)
        self.play(FadeOut(title), FadeOut(sub))

    # ---------- precedence ladder ----------

    def beat_ladder(self):
        chip = chapter("1", "the precedence ladder")
        chip.to_corner(UL, buff=0.4)

        rows = VGroup()
        row_lookup = {}
        for name, ops in LADDER:
            nm = ct(name, size=19)
            op = ct(ops, size=16, color=ORANGE)
            r = VGroup(nm, op).arrange(RIGHT, buff=0.35)
            row_lookup[name] = r
            rows.add(r)
        rows.arrange(DOWN, aligned_edge=LEFT, buff=0.17).move_to([-4.1, 0.3, 0])

        self.play(FadeIn(chip, shift=RIGHT * 0.5))
        self.play(LaggedStart(*[FadeIn(r, shift=RIGHT * 0.3) for r in rows], lag_ratio=0.07), run_time=3)

        cap = caption(
            "one generic helper: parse_binop(ops, lower)\n"
            "parse the next-tighter level, then fold your own operators, left to right",
            21,
        )
        cap.to_edge(DOWN, buff=0.35)
        self.play(FadeIn(cap))
        self.wait(2)

        # demo: 2 + 3 * 4 through additive -> term
        expr, chips = toks_row(
            [("2", "intl"), ("+", "o"), ("3", "intl"), ("*", "o"), ("4", "intl")],
            size=24,
        )
        expr.move_to([3.4, 3.0, 0])
        self.play(FadeIn(expr, shift=UP * 0.3))

        add_row, term_row = row_lookup["parse_additive"], row_lookup["parse_term"]
        h_add = hl(add_row, color=YELLOW)
        self.play(Create(h_add))
        self.wait(0.6)
        arrow1 = CurvedArrow(
            add_row.get_right() + RIGHT * 0.1,
            expr.get_left() + LEFT * 0.25,
            color=YELLOW, stroke_width=2.5, angle=-TAU / 8,
        )
        self.play(Create(arrow1))
        cap2 = ct("parse_additive: grab a term, see +, grab another term", size=20, color=YELLOW)
        cap2.next_to(expr, DOWN, buff=0.45).align_to(expr, LEFT)
        self.play(FadeIn(cap2))
        self.wait(1)

        add_n = node("+", ORANGE).move_to([3.4, 1.5, 0])
        two_n = node("2", YELLOW).move_to([1.9, 0.3, 0])
        todo = node("term?", GREY_B).move_to([4.9, 0.3, 0])
        e1, e2 = edge(add_n, two_n), edge(add_n, todo)
        self.play(
            FadeIn(add_n, scale=0.7), Create(e1), FadeIn(two_n, scale=0.7),
            Create(e2), FadeIn(todo, scale=0.7),
        )
        self.wait(0.8)

        h_term = hl(term_row, color=YELLOW)
        self.play(FadeOut(arrow1), FadeOut(h_add), Create(h_term))
        cap3 = ct("...and that term is 3 * 4: parsed one level down", size=20, color=YELLOW)
        cap3.next_to(cap2, DOWN, buff=0.25).align_to(cap2, LEFT)
        self.play(
            FadeIn(cap3),
            Indicate(VGroup(chips[2], chips[3], chips[4]), color=YELLOW),
        )
        mul_n = node("*", ORANGE).move_to([4.9, 0.3, 0])
        three_n = node("3", YELLOW).move_to([4.0, -1.0, 0])
        four_n = node("4", YELLOW).move_to([5.8, -1.0, 0])
        e3, e4 = edge(mul_n, three_n), edge(mul_n, four_n)
        self.play(
            ReplacementTransform(todo, mul_n),
            Create(e3), FadeIn(three_n, scale=0.7),
            Create(e4), FadeIn(four_n, scale=0.7),
        )
        self.wait(1.5)

        cap4 = caption(
            "precedence is nothing more than the order of the ladder", 22
        )
        cap4.to_edge(DOWN, buff=0.35)
        self.play(FadeOut(cap), FadeOut(cap2), FadeOut(cap3), FadeIn(cap4))
        self.wait(1.5)

        # left associativity: 10 - 4 - 2
        tree = VGroup(add_n, two_n, mul_n, three_n, four_n, e1, e2, e3, e4)
        self.play(
            FadeOut(cap4), FadeOut(expr),
            tree.animate.scale(0.62).move_to([4.6, 1.9, 0]),
        )

        expr2, _ = toks_row(
            [("10", "intl"), ("-", "o"), ("4", "intl"), ("-", "o"), ("2", "intl")],
            size=24,
        )
        expr2.move_to([1.2, 2.9, 0])
        self.play(FadeIn(expr2, shift=UP * 0.3))
        capA = ct("10 - 4 - 2 folds left to right", size=20, color=YELLOW)
        capA.next_to(expr2, DOWN, buff=0.4).align_to(expr2, LEFT)
        self.play(FadeIn(capA))

        # wrong tree (struck through)
        wsub = node("-", RED).move_to([1.0, 0.9, 0])
        w10 = node("10", YELLOW).move_to([-0.2, -0.3, 0])
        wsub2 = node("-", RED).move_to([2.3, -0.3, 0])
        w4 = node("4", YELLOW).move_to([1.6, -1.5, 0])
        w2 = node("2", YELLOW).move_to([3.0, -1.5, 0])
        wtree = VGroup(
            wsub, w10, wsub2, w4, w2,
            edge(wsub, w10), edge(wsub, wsub2), edge(wsub2, w4), edge(wsub2, w2),
        )
        wrong = VGroup(wtree, caption("10 - (4 - 2) = 8", 18).next_to(wtree, DOWN, buff=0.3))
        wrong.move_to([1.2, -0.6, 0])
        self.play(FadeIn(wrong, shift=UP * 0.2))
        strike = Line(wtree.get_corner(UL) + UL * 0.15, wtree.get_corner(DR) + DR * 0.15, color=RED, stroke_width=4)
        self.play(Create(strike))

        # correct tree
        csub = node("-", TEAL).move_to([4.9, 0.4, 0])
        csub2 = node("-", TEAL).move_to([3.9, -0.9, 0])
        c10 = node("10", YELLOW).move_to([5.9, -0.9, 0])
        c4 = node("4", YELLOW).move_to([3.3, -2.1, 0])
        c2 = node("2", YELLOW).move_to([4.6, -2.1, 0])
        ctree = VGroup(
            csub, csub2, c10, c4, c2,
            edge(csub, csub2), edge(csub, c10), edge(csub2, c4), edge(csub2, c2),
        )
        ok = VGroup(ctree, caption("(10 - 4) - 2 = 4  (correct)", 18).next_to(ctree, DOWN, buff=0.3))
        ok.move_to([4.6, -0.7, 0])
        self.play(FadeIn(ok, shift=UP * 0.2))
        self.wait(2.5)

        self.play(
            FadeOut(chip), FadeOut(rows), FadeOut(tree), FadeOut(expr2),
            FadeOut(capA), FadeOut(wrong), FadeOut(strike), FadeOut(ok),
            FadeOut(h_term),
        )

    # ---------- statement dispatch ----------

    def beat_dispatch(self):
        chip = chapter("2", "parse_stmt()")
        chip.to_corner(UL, buff=0.4)
        self.play(FadeIn(chip, shift=RIGHT * 0.5))

        top = tbox("parse_stmt", "look at the first token", color=BLUE, w=3.0, h=0.8, tsize=24)
        top.move_to([0, 2.6, 0])
        self.play(FadeIn(top, scale=0.8))

        entries = [
            ("{ }", "-> SBlock"),
            ("if / else", "-> SIf"),
            ("while", "-> SWhile"),
            ("for", "-> SFor"),
            ("return break continue", "-> S*"),
            ("int char void", "-> SDecl"),
            ("anything else", "-> SExpr"),
        ]
        boxes = VGroup()
        for t, s in entries:
            boxes.add(tbox(t, s, color=TEAL, w=3.1, h=0.7, tsize=18, ssize=16))
        boxes.arrange_in_grid(rows=2, cols=4, buff=0.5).move_to([0, -0.7, 0])
        arrows = VGroup(
            *[
                Arrow(
                    top.get_bottom() + DOWN * 0.05,
                    b.get_top() + UP * 0.05,
                    buff=0.05,
                    stroke_width=2,
                    color=GREY_A,
                    max_tip_length_to_length_ratio=0.18,
                )
                for b in boxes
            ]
        )
        self.play(
            LaggedStart(*[FadeIn(b, shift=UP * 0.2) for b in boxes], lag_ratio=0.1),
            run_time=2,
        )
        self.play(LaggedStart(*[Create(a) for a in arrows], lag_ratio=0.1))
        cap = caption(
            "statements: one dispatch.  expressions: the ladder.", 22
        )
        cap.to_edge(DOWN, buff=0.4)
        self.play(FadeIn(cap))
        self.wait(2.5)
        self.play(FadeOut(chip), FadeOut(top), FadeOut(boxes), FadeOut(arrows), FadeOut(cap))

    # ---------- if / else ----------

    def beat_if(self):
        chip = chapter("3", "if / else -> branches")
        chip.to_corner(UL, buff=0.4)
        self.play(FadeIn(chip, shift=RIGHT * 0.5))

        c_code = code_block(
            ["int abs(int x) {", "    if (x < 0)", "        return -x;", "    else", "        return x;", "}"],
            size=21,
        )
        c_code.to_edge(LEFT, buff=0.7).move_to([c_code.get_x(), 0.6, 0])
        self.play(Write(c_code))

        asm = code_block(IF_ASM, size=19)
        asm.to_edge(RIGHT, buff=0.7).move_to([asm.get_x(), 0.4, 0])
        self.play(Write(asm))

        beq_arrow = CurvedArrow(
            asm[2].get_right() + RIGHT * 0.1,
            asm[5].get_right() + RIGHT * 0.1,
            color=YELLOW, stroke_width=2.5, angle=-TAU / 5,
        )
        b1_arrow = CurvedArrow(
            asm[4].get_right() + RIGHT * 0.1,
            asm[8].get_right() + RIGHT * 0.1,
            color=TEAL, stroke_width=2.5, angle=-TAU / 6,
        )
        self.play(Create(beq_arrow))
        cap1 = ct("in C, 0 is false: beq = branch if zero -> run the else", size=20, color=YELLOW)
        cap1.to_edge(DOWN, buff=0.7)
        self.play(FadeIn(cap1))
        self.wait(1.5)
        self.play(Create(b1_arrow))
        cap2 = ct("the else-arm is just code the then-arm jumps over", size=20, color=TEAL)
        cap2.next_to(cap1, DOWN, buff=0.2)
        self.play(FadeIn(cap2))
        self.wait(2.5)

        self.play(
            FadeOut(chip), FadeOut(c_code), FadeOut(asm),
            FadeOut(beq_arrow), FadeOut(b1_arrow), FadeOut(cap1), FadeOut(cap2),
        )

    # ---------- while ----------

    def beat_while(self):
        chip = chapter("4", "while -> a backward branch")
        chip.to_corner(UL, buff=0.4)
        self.play(FadeIn(chip, shift=RIGHT * 0.5))

        c_code = code_block(
            ["while (i < n)", "    s += i;"],
            size=21,
        )
        c_code.to_edge(LEFT, buff=0.9).move_to([c_code.get_x(), 1.2, 0])
        self.play(Write(c_code))

        asm = code_block(WHILE_ASM, size=19)
        asm.to_edge(RIGHT, buff=0.9).move_to([asm.get_x(), 0.8, 0])
        self.play(Write(asm))

        exit_arrow = CurvedArrow(
            asm[3].get_right() + RIGHT * 0.1,
            asm[6].get_right() + RIGHT * 0.1,
            color=TEAL, stroke_width=2.5, angle=-TAU / 5,
        )
        back_arrow = CurvedArrow(
            asm[5].get_right() + RIGHT * 0.55,
            asm[0].get_right() + RIGHT * 0.3,
            color=YELLOW, stroke_width=2.5, angle=-TAU / 3.2,
        )
        self.play(Create(exit_arrow))
        cap1 = ct("cond false -> one branch out of the loop", size=20, color=TEAL)
        cap1.to_edge(DOWN, buff=0.75)
        self.play(FadeIn(cap1))
        self.wait(1)
        self.play(Create(back_arrow))
        cap2 = ct("end of body -> branch back to the top, re-test the cond", size=20, color=YELLOW)
        cap2.next_to(cap1, DOWN, buff=0.2)
        self.play(FadeIn(cap2))
        self.wait(2.5)

        self.play(
            FadeOut(chip), FadeOut(c_code), FadeOut(asm),
            FadeOut(exit_arrow), FadeOut(back_arrow), FadeOut(cap1), FadeOut(cap2),
        )

    # ---------- for ----------

    def beat_for(self):
        chip = chapter("5", "for -> while + a step label")
        chip.to_corner(UL, buff=0.4)
        self.play(FadeIn(chip, shift=RIGHT * 0.5))

        c_code = code_block(
            ["for (A; B; C)", "    body;"],
            size=21,
        )
        c_code.to_edge(LEFT, buff=0.9).move_to([c_code.get_x(), 2.4, 0])
        self.play(Write(c_code))

        init = tbox("A", "init", color=TEAL, w=1.4, h=0.7, tsize=22)
        cond = tbox("B?", "cond", color=BLUE, w=1.4, h=0.7, tsize=22)
        body = tbox("body", None, color=ORANGE, w=1.7, h=0.7, tsize=22)
        step = tbox("C", "step", color=TEAL, w=1.4, h=0.7, tsize=22)
        end = tbox("end", ".Lforend", color=GREY_B, w=1.5, h=0.7, tsize=22)

        init.move_to([-3.6, 2.2, 0])
        cond.move_to([-1.4, 2.2, 0])
        body.move_to([-1.4, 0.7, 0])
        step.move_to([0.9, 0.7, 0])
        end.move_to([3.3, 2.2, 0])

        a1 = Arrow(init.get_right(), cond.get_left(), buff=0.08, stroke_width=2.5, color=GREY_A)
        a2 = Arrow(cond.get_bottom(), body.get_top(), buff=0.08, stroke_width=2.5, color=GREY_A).set_color(GREEN)
        a2t = ct("true", size=16, color=GREEN).next_to(a2, RIGHT, buff=0.12)
        a3 = Arrow(cond.get_right(), end.get_left(), buff=0.08, stroke_width=2.5, color=TEAL)
        a3t = ct("false", size=16, color=TEAL).next_to(a3, UP, buff=0.12)
        a4 = Arrow(body.get_right(), step.get_left(), buff=0.08, stroke_width=2.5, color=GREY_A)
        a5 = CurvedArrow(
            step.get_top() + UP * 0.05, cond.get_bottom() + RIGHT * 0.5 + DOWN * 0.05,
            color=YELLOW, stroke_width=2.5, angle=-TAU / 5,
        )

        self.play(
            LaggedStart(
                FadeIn(init, scale=0.7), FadeIn(cond, scale=0.7), FadeIn(body, scale=0.7),
                FadeIn(step, scale=0.7), FadeIn(end, scale=0.7),
                lag_ratio=0.15,
            ),
            run_time=2,
        )
        self.play(
            LaggedStart(Create(a1), Create(a2), FadeIn(a2t), Create(a3), FadeIn(a3t), Create(a4), Create(a5), lag_ratio=0.12)
        )
        cap = ct(
            "continue jumps to C (the step), not to the top.\n"
            "self.loops is a stack of (break_target, continue_target).",
            size=20,
        )
        cap.to_edge(DOWN, buff=0.5)
        cont_arrow = CurvedArrow(
            body.get_left() + LEFT * 0.1, step.get_bottom() + DOWN * 0.15,
            color=PURPLE, stroke_width=2.5, angle=-TAU / 4,
        )
        cont_t = ct("continue", size=16, color=PURPLE).next_to(cont_arrow, DOWN, buff=0.1)
        self.play(FadeIn(cap), Create(cont_arrow), FadeIn(cont_t))
        self.wait(3)

        fade = [chip, c_code, init, cond, body, step, end, a1, a2, a2t, a3, a3t, a4, a5, cap, cont_arrow, cont_t]
        self.play(*[FadeOut(m) for m in fade])

    # ---------- function frames ----------

    def beat_frames(self):
        chip = chapter("6", "function frames")
        chip.to_corner(UL, buff=0.4)
        self.play(FadeIn(chip, shift=RIGHT * 0.5))

        c_code = code_block(SUM_C, size=19)
        c_code.to_edge(LEFT, buff=0.6).move_to([c_code.get_x(), 1.2, 0])
        self.play(Write(c_code))

        # stack diagram
        cells = VGroup()
        labels = ["caller ...", "saved fp", "saved lr", "n   (fp - 4)", "s   (fp - 8)", "i   (fp - 12)"]
        for lab in labels:
            t = ct(lab, size=17)
            b = RoundedRectangle(corner_radius=0.06, width=2.5, height=0.44)
            b.set_stroke(GREY_B, 1.5)
            color = {"saved fp": TEAL, "saved lr": TEAL, "n   (fp - 4)": BLUE, "s   (fp - 8)": BLUE, "i   (fp - 12)": BLUE}.get(lab)
            if color:
                b.set_fill(color, 0.16)
                t.set_color(color)
            cells.add(VGroup(b, t.move_to(b)))
        cells.arrange(DOWN, buff=0.1).move_to([0.4, 0.9, 0])
        fp_arrow = Arrow(LEFT * 1.1, cells[1].get_left() + LEFT * 0.05, buff=0.03, stroke_width=3, color=YELLOW)
        fp_lbl = ct("fp", size=20, color=YELLOW).next_to(fp_arrow, LEFT, buff=0.12)
        sp_arrow = Arrow(LEFT * 1.1, cells[5].get_bottom() + LEFT * 0.5 + DOWN * 0.05, buff=0.03, stroke_width=3, color=RED)
        sp_lbl = ct("sp", size=20, color=RED).next_to(sp_arrow, LEFT, buff=0.12)
        self.play(
            LaggedStart(*[FadeIn(c, scale=0.7) for c in cells], lag_ratio=0.1),
            run_time=2,
        )

        asm = code_block(FRAMES_ASM, size=19)
        asm.to_edge(RIGHT, buff=0.6).move_to([asm.get_x(), 1.5, 0])
        self.play(Write(asm))

        a_push = CurvedArrow(asm[1].get_left() + LEFT * 0.15, cells[1].get_right() + RIGHT * 0.15, color=TEAL, stroke_width=2.5, angle=TAU / 5)
        a_fp = CurvedArrow(asm[2].get_left() + LEFT * 0.15, fp_arrow.get_start() + RIGHT * 0.05, color=YELLOW, stroke_width=2.5, angle=TAU / 5)
        a_str = CurvedArrow(asm[3].get_left() + LEFT * 0.15, cells[3].get_right() + RIGHT * 0.15, color=BLUE, stroke_width=2.5, angle=TAU / 5)
        a_sub = CurvedArrow(asm[4].get_left() + LEFT * 0.15, sp_arrow.get_start() + RIGHT * 0.05, color=RED, stroke_width=2.5, angle=TAU / 5)
        self.play(
            FadeIn(fp_arrow, shift=RIGHT * 0.3), FadeIn(fp_lbl),
            FadeIn(sp_arrow, shift=RIGHT * 0.3), FadeIn(sp_lbl),
        )
        self.play(Create(a_push), Create(a_fp))
        cap1 = ct("prologue: save fp/lr, point fp at the frame, spill r0 (arg n)", size=19, color=GREY_A)
        cap1.to_edge(DOWN, buff=0.75)
        self.play(FadeIn(cap1))
        self.play(Create(a_str), Create(a_sub))
        cap2 = ct("collect_locals() walks the body first: frame size known up front", size=19, color=GREY_A)
        cap2.next_to(cap1, DOWN, buff=0.18)
        self.play(FadeIn(cap2))
        self.wait(2.5)

        epi = code_block(EPILOGUE_ASM, size=19)
        epi.to_edge(RIGHT, buff=0.6).move_to([epi.get_x(), -2.1, 0])
        ret_arrow = CurvedArrow(
            epi[0].get_left() + LEFT * 0.15, epi[1].get_left() + LEFT * 0.35,
            color=YELLOW, stroke_width=2.5, angle=TAU / 4,
        )
        self.play(Write(epi), Create(ret_arrow))
        cap3 = ct(
            "returns don't restore anything: they branch to one shared epilogue",
            size=19, color=GREY_A,
        )
        cap3.next_to(cap2, DOWN, buff=0.18)
        self.play(FadeIn(cap3))
        self.wait(3)

        fade = [
            chip, c_code, cells, fp_arrow, fp_lbl, sp_arrow, sp_lbl, asm,
            a_push, a_fp, a_str, a_sub, cap1, cap2, epi, ret_arrow, cap3,
        ]
        self.play(*[FadeOut(m) for m in fade])

    # ---------- outro ----------

    def outro(self):
        lines = VGroup(
            ct("parser: build the tree (the ladder does precedence)", size=24),
            ct("codegen: walk it, emit lines (labels + branches + fp slots)", size=24),
        ).arrange(DOWN, aligned_edge=LEFT, buff=0.3)
        lines.move_to([0, 1.0, 0])
        nxt = caption(
            "video 3: types, pointers, string literals, the assembler, the linker, ELF, and qemu",
            22,
        )
        nxt.next_to(lines, DOWN, buff=0.8)
        self.play(LaggedStart(*[FadeIn(l, shift=UP * 0.3) for l in lines], lag_ratio=0.3))
        self.play(FadeIn(nxt))
        self.wait(3)
        self.play(FadeOut(lines), FadeOut(nxt))
