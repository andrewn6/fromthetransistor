"""shared helpers for the toolchain manim videos"""
from manim import *

CODE_FONT = "Menlo"

TOK_COLOR = {
    "kw": TEAL,
    "id": BLUE,
    "intl": YELLOW,
    "strl": YELLOW,
    "chrl": YELLOW,
    "p": GREY_B,
    "o": ORANGE,
}


def ct(s, color=WHITE, size=28):
    # monospace text
    return Text(s, font=CODE_FONT, font_size=size, color=color)


def code_block(lines, size=26, color=WHITE, buff=0.14):
    # column of monospace lines, left aligned
    rows = [ct(l, color=color, size=size) for l in lines]
    return VGroup(*rows).arrange(DOWN, aligned_edge=LEFT, buff=buff)


def hl(m, color=YELLOW, buff=0.08):
    # highlight rectangle around a mobject
    return SurroundingRectangle(m, color=color, buff=buff, corner_radius=0.08)


def tok(val, kind, size=22):
    # one token chip
    color = TOK_COLOR[kind]
    t = Text(str(val), font=CODE_FONT, font_size=size, color=color)
    box = RoundedRectangle(
        corner_radius=0.09,
        width=max(t.width + 0.22, 0.4),
        height=t.height + 0.16,
    ).set_stroke(color, 2).set_fill(color, 0.18)
    return VGroup(box, t.move_to(box))


def toks_row(pairs, buff=0.1, size=22):
    # row of token chips from (value, kind) pairs
    chips = [tok(v, k, size=size) for v, k in pairs]
    row = VGroup(*chips).arrange(RIGHT, buff=buff)
    return row, chips


def node(label, color=BLUE, size=20):
    # ast node box
    t = Text(label, font=CODE_FONT, font_size=size, color=color)
    box = RoundedRectangle(
        corner_radius=0.1,
        width=max(t.width + 0.3, 0.62),
        height=t.height + 0.2,
    ).set_stroke(color, 2).set_fill(color, 0.15)
    return VGroup(box, t.move_to(box))


def edge(a, b):
    # line from node a's bottom to node b's top
    return Line(
        a.get_bottom() + DOWN * 0.03,
        b.get_top() + UP * 0.03,
        stroke_width=2.5,
        color=GREY_A,
    ).set_z_index(-1)


def tbox(title, sub=None, color=BLUE, w=3.0, h=0.9, tsize=26, ssize=17):
    # titled rounded box with optional small subtitle below
    b = RoundedRectangle(corner_radius=0.13, width=w, height=h)
    b.set_stroke(color, 2.5).set_fill(color, 0.14)
    t = Text(title, font=CODE_FONT, font_size=tsize, color=color)
    g = VGroup(b, t.move_to(b))
    if sub is not None:
        g.add(Text(sub, font_size=ssize, color=GREY_A).next_to(b, DOWN, buff=0.12))
    return g


def caption(s, size=22, color=GREY_A):
    return Text(s, font_size=size, color=color)


def chapter(num, name, color=YELLOW):
    # section chip like "2. Parser"
    t = ct(f"{num}. {name}", color=color, size=30)
    r = RoundedRectangle(
        corner_radius=0.12, width=t.width + 0.5, height=t.height + 0.24
    )
    r.set_stroke(color, 2).set_fill(color, 0.12)
    return VGroup(r, t.move_to(r))


def reg_box(name, color=TEAL, w=1.5, h=0.7):
    # register display: [r0] [ value ]
    lbl = ct(name, color=color, size=24)
    val = ct("?", color=WHITE, size=26)
    val_box = RoundedRectangle(corner_radius=0.08, width=w, height=h)
    val_box.set_stroke(color, 2).set_fill(BLACK, 0.6)
    val.move_to(val_box)
    inner = VGroup(val_box, val)
    g = VGroup(lbl, inner).arrange(RIGHT, buff=0.18)
    g.val = val
    return g
