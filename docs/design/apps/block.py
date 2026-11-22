# One node of a chained program (an effect of Easy Effects: SPECTRUM, IN and OUT) as one block on
# the face: what it is now (two frames in one outline) and blocks with one shared header, split into
# its input and its output. Then a unit of Easy Effects' chain in each, to see the room it saves.
#   python3 block.py block.svg && rsvg-convert -z .5 block.svg -o block.png
import sys

F = 'font-family="Noto Sans"'
M = 'font-family="Noto Sans Mono"'
IN_C, OUT_C = "#a8862c", "#b45a2e"  # an app's input and output bands (face::ACCENTS)
INK = "#f3efe6"
PRINT = "#c9cbcd"
o = []


def text(x, y, t, size=10, fill=PRINT, bold=False, anchor="start", extra=""):
    w = ' font-weight="bold"' if bold else ""
    o.append(f'<text x="{x}" y="{y}" {F} font-size="{size}"{w} fill="{fill}" text-anchor="{anchor}" {extra}>{t}</text>')


def tag(x, y, t, fill=INK):
    """IN / OUT on a band: a small outlined tag."""
    w = 8 + 6.2 * len(t)
    o.append(f'<rect x="{x}" y="{y - 8}" width="{w}" height="11" rx="2" fill="none" stroke="{fill}" stroke-width="1"/>')
    text(x + w / 2, y + 0.5, t, 7.5, fill, True, "middle", 'letter-spacing=".8"')
    return w


def jack(cx, cy, ring):
    o.append(f'<circle cx="{cx}" cy="{cy}" r="15" fill="#2b2c2f" stroke="#111" stroke-width="2"/>')
    o.append(f'<circle cx="{cx}" cy="{cy}" r="11" fill="none" stroke="{ring}" stroke-width="3"/>')
    o.append(f'<circle cx="{cx}" cy="{cy}" r="6" fill="#0c0c0d"/>')


def meter(x, y, lit=6):
    for i in range(12):
        c = "#3fae5a" if i < 8 else ("#e9c53a" if i < 10 else "#d23b32")
        on = i < lit
        for k in (0, 5):
            o.append(f'<rect x="{x + k}" y="{y + 60 - i * 5}" width="4" height="3.4" fill="{c if on else "#202123"}"/>')


def split_key(x, y):
    o.append(f'<rect x="{x}" y="{y}" width="34" height="11" rx="2" fill="#151617" stroke="rgba(255,255,255,.15)"/>')
    text(x + 17, y + 8.2, "SPLIT", 6.5, "#d8d9db", True, "middle", 'letter-spacing=".6"')


def frame(x, y, w, name, d):
    """A frame as the face draws it now: its band, its name and IN or OUT, SIG, a stereo jack, its
    meter, SPLIT."""
    c = IN_C if d == "in" else OUT_C
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="92" rx="6" fill="rgba(0,0,0,.15)" stroke="{c}" stroke-width="1.5"/>')
    o.append(f'<path d="M{x},{y + 6} a6,6 0 0 1 6,-6 h{w - 12} a6,6 0 0 1 6,6 v12 h-{w} z" fill="{c}"/>')
    text(x + 8, y + 13, name, 9, INK, True, extra='letter-spacing="1.3"')
    tw = 7.2 * len(name) + 16
    tag(x + tw, y + 12, "IN" if d == "in" else "OUT")
    text(x + w - 8, y + 13, "SIG", 7.5, INK, True, "end")
    jack(x + (w - 24) / 2, y + 48, c)
    text(x + (w - 24) / 2, y + 78, "STEREO", 8, PRINT, True, "middle")
    meter(x + w - 20, y + 22)
    split_key(x + w - 42, y + 79)


def now(x, y, name):
    """Two frames in one outline."""
    w = max(150, 7.2 * len(name) + 90)
    o.append(f'<rect x="{x - 4}" y="{y - 4}" width="{2 * w + 18}" height="100" rx="9" fill="rgba(0,0,0,.12)" stroke="{PRINT}" stroke-opacity=".45" stroke-width="1.5"/>')
    frame(x, y, w, name, "in")
    frame(x + w + 10, y, w, name, "out")
    return 2 * w + 18


def half(x, y, w, d, top):
    """One side of a shared block: its jack, meter and SPLIT."""
    c = IN_C if d == "in" else OUT_C
    jack(x + (w - 22) / 2, y + top + 26, c)
    text(x + (w - 22) / 2, y + top + 56, "STEREO", 8, PRINT, True, "middle")
    meter(x + w - 18, y + top - 0)
    split_key(x + w - 40, y + 79)


def block_a(x, y, name):
    """A: one band, its left half the input's colour, its right half the output's; the name once,
    over both; IN and OUT tags at each half's start. A dotted rule between the halves."""
    hw = 108
    w = 2 * hw
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="92" rx="6" fill="rgba(0,0,0,.15)" stroke="#8a7d6a" stroke-width="1.5"/>')
    o.append(f'<path d="M{x},{y + 6} a6,6 0 0 1 6,-6 h{hw - 6} v18 h-{hw} z" fill="{IN_C}"/>')
    o.append(f'<path d="M{x + hw},{y} h{hw - 6} a6,6 0 0 1 6,6 v12 h-{hw} z" fill="{OUT_C}"/>')
    text(x + w / 2, y + 13, name, 9, INK, True, "middle", 'letter-spacing="1.3"')
    tag(x + 6, y + 12, "IN")
    tag(x + w - 6 - 8 - 6.2 * 3, y + 12, "OUT")
    o.append(f'<path d="M{x + hw},{y + 24} V{y + 88}" stroke="{PRINT}" stroke-opacity=".35" stroke-dasharray="2 3"/>')
    half(x, y, hw, "in", 22)
    half(x + hw, y, hw, "out", 22)
    return w


def block_b(x, y, name):
    """B: one band in the program's input colour for its name, IN ▸ OUT at its right; under it the
    two halves, each with a small IN or OUT printed over its jack."""
    hw = 104
    w = 2 * hw
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="92" rx="6" fill="rgba(0,0,0,.15)" stroke="#8a7d6a" stroke-width="1.5"/>')
    o.append(f'<path d="M{x},{y + 6} a6,6 0 0 1 6,-6 h{w - 12} a6,6 0 0 1 6,6 v12 h-{w} z" fill="#7a6a52"/>')
    text(x + 8, y + 13, name, 9, INK, True, extra='letter-spacing="1.3"')
    text(x + w - 8, y + 13, "IN ▸ OUT", 7.5, INK, True, "end", 'letter-spacing=".8"')
    for i, d in enumerate(("in", "out")):
        hx = x + i * hw
        tag(hx + 7, y + 52, d.upper(), IN_C if d == "in" else OUT_C)
        half(hx + 14, y, hw - 14, d, 22)
    o.append(f'<path d="M{x + hw},{y + 24} V{y + 88}" stroke="{PRINT}" stroke-opacity=".35" stroke-dasharray="2 3"/>')
    return w


def block_c(x, y, name):
    """C: as a pedal: the name on its band, the input jack at the left edge, the output at the
    right, an arrow between them; one meter for each side, low; SPLIT for both (they split alike)."""
    w = 176
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="92" rx="6" fill="rgba(0,0,0,.15)" stroke="#8a7d6a" stroke-width="1.5"/>')
    o.append(f'<path d="M{x},{y + 6} a6,6 0 0 1 6,-6 h{w - 12} a6,6 0 0 1 6,6 v12 h-{w} z" fill="#7a6a52"/>')
    text(x + w / 2, y + 13, name, 9, INK, True, "middle", 'letter-spacing="1.3"')
    jack(x + 30, y + 46, IN_C)
    jack(x + w - 30, y + 46, OUT_C)
    text(x + 30, y + 76, "IN", 8, IN_C, True, "middle")
    text(x + w - 30, y + 76, "OUT", 8, OUT_C, True, "middle")
    o.append(f'<path d="M{x + 52},{y + 46} H{x + w - 54}" stroke="{PRINT}" stroke-opacity=".5" stroke-width="1.5"/><path d="M{x + w - 60},{y + 41} l7,5 l-7,5" fill="none" stroke="{PRINT}" stroke-opacity=".5" stroke-width="1.5"/>')
    for k, mx in enumerate((x + 70, x + w - 82)):
        for i in range(8):
            c = "#3fae5a" if i < 6 else "#e9c53a"
            o.append(f'<rect x="{mx + i * 5}" y="{y + 60}" width="3.4" height="4" fill="{c if i < 4 + k else "#202123"}"/>')
    split_key(x + (w - 34) / 2, y + 79)
    return w


def section(y, title, sub):
    text(60, y, title, 26, "#e58f2c", True, extra='letter-spacing="2"')
    text(60, y + 30, sub, 16, "#9a9da1")


def unit(x, y, w):
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="124" rx="3" fill="#2a2b2e" stroke="#000" stroke-opacity=".6"/>')


NAMES = ["EQUALIZER", "SPECTRUM", "OUTPUT LEVEL METER"]
VARIANTS = [
    ("NOW", "Two frames in one outline: the name twice, two bands, two IN/OUT tags.", now),
    ("A · ONE BAND, TWO COLOURS", "The name once, across a band half the input's colour, half the output's; IN and OUT at the band's ends.", block_a),
    ("B · ONE BAND, IN AND OUT BELOW", "The name once on a plain band (IN ▸ OUT at its right); IN and OUT printed over each half.", block_b),
    ("C · AS A PEDAL", "Input jack at the left edge, output at the right, an arrow between; small meters; one SPLIT for both.", block_c),
]

if __name__ == "__main__":
    W, y = 1700, 150
    for title, sub, fn in VARIANTS:
        section(y, title, sub)
        y += 50
        # one block alone, and a unit of the chain
        unit(60, y, W - 120)
        x = 80
        widths = []
        for n in NAMES:
            if x > W - 380:
                break
            wd = fn(x, y + 16, n)
            widths.append(wd)
            x += wd + 18
        used = x - 80
        text(W - 80, y + 70, f"{len(widths)} blocks · {int(used)} px", 14, "#e58f2c", True, "end")
        y += 190
    head = f'<rect width="{W}" height="{y}" fill="#1f2023"/>'
    head += f'<text x="60" y="70" {F} font-weight="bold" font-size="40" letter-spacing="3" fill="#e58f2c">ONE NODE, ONE BLOCK</text>'
    head += f'<text x="60" y="108" {F} font-size="20" fill="#9a9da1">A node of a chained program (an effect: its input and its output) as one block with one header, to save room on the face.</text>'
    open(sys.argv[1] if len(sys.argv) > 1 else "block.svg", "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{y}" viewBox="0 0 {W} {y}">{head}{"".join(o)}</svg>\n')
