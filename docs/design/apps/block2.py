# Variants B and C of block.py up close: one node (an effect: its input and its output) as one
# block, with its stereo jacks joined (one STEREO jack a side) and split (L over R, as the face
# splits them now). Drawn at twice the face's size.
#   python3 block2.py block2.svg && rsvg-convert -z .5 block2.svg -o block2.png
import sys

from block import o, text, tag, IN_C, OUT_C, INK, PRINT, F

S = 2.0  # drawn at twice the face's size


def jack(cx, cy, ring, r=15):
    o.append(f'<circle cx="{cx}" cy="{cy}" r="{r}" fill="#2b2c2f" stroke="#111" stroke-width="2"/>')
    o.append(f'<circle cx="{cx}" cy="{cy}" r="{r * .73}" fill="none" stroke="{ring}" stroke-width="3"/>')
    o.append(f'<circle cx="{cx}" cy="{cy}" r="{r * .4}" fill="#0c0c0d"/>')


def meter(x, y, cols=2, n=12, lit=7, step=5):
    for i in range(n):
        c = "#3fae5a" if i < n * .66 else ("#e9c53a" if i < n * .85 else "#d23b32")
        for k in range(cols):
            o.append(f'<rect x="{x + k * 5}" y="{y - i * step}" width="4" height="{step * .68}" fill="{c if i < lit - k else "#202123"}"/>')


def split_key(x, y, split, w=34):
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="11" rx="2" fill="#151617" stroke="rgba(255,255,255,.15)"/>')
    text(x + w / 2, y + 8.2, "JOIN" if split else "SPLIT", 6.5, "#d8d9db", True, "middle", 'letter-spacing=".6"')


def side(cx, y0, ring, split, label=True):
    """A side's jacks: one STEREO jack, or L over R."""
    if split:
        jack(cx, y0 + 18, ring, 12)
        jack(cx, y0 + 46, ring, 12)
        if label:
            text(cx + 19, y0 + 21, "L", 8, PRINT, True, "middle")
            text(cx + 19, y0 + 49, "R", 8, PRINT, True, "middle")
    else:
        jack(cx, y0 + 26, ring)
        if label:
            text(cx, y0 + 56, "STEREO", 8, PRINT, True, "middle")


def box(x, y, w, name, right=""):
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="92" rx="6" fill="rgba(0,0,0,.15)" stroke="#8a7d6a" stroke-width="1.5"/>')
    o.append(f'<path d="M{x},{y + 6} a6,6 0 0 1 6,-6 h{w - 12} a6,6 0 0 1 6,6 v12 h-{w} z" fill="#7a6a52"/>')


def block_b(x, y, name, split_in, split_out):
    """B: the name once on a plain band, IN ▸ OUT at its right; each half its tag, jacks, meter
    and SPLIT; a dotted rule between them."""
    hw = 104
    w = 2 * hw
    box(x, y, w, name)
    text(x + 8, y + 13, name, 9, INK, True, extra='letter-spacing="1.3"')
    text(x + w - 8, y + 13, "IN ▸ OUT", 7.5, INK, True, "end", 'letter-spacing=".8"')
    for i, (d, sp) in enumerate((("in", split_in), ("out", split_out))):
        hx = x + i * hw
        c = IN_C if d == "in" else OUT_C
        tag(hx + 7, y + 31, d.upper(), c)
        side(hx + 50, y + 20, c, sp)
        meter(hx + hw - 20, y + 74, 4 if sp else 2, lit=7)
        split_key(hx + hw - 42, y + 79, sp)
    o.append(f'<path d="M{x + hw},{y + 24} V{y + 88}" stroke="{PRINT}" stroke-opacity=".35" stroke-dasharray="2 3"/>')


def block_c(x, y, name, split, two_keys=False):
    """C: as a pedal: IN at the left edge, OUT at the right, an arrow between; a small meter on
    each side of the arrow; one SPLIT for both, or (C2) one under each side."""
    w = 176
    box(x, y, w, name)
    text(x + w / 2, y + 13, name, 9, INK, True, "middle", 'letter-spacing="1.3"')
    lx, rx = x + 30, x + w - 30
    side(lx, y + 20, IN_C, split[0], label=False)
    side(rx, y + 20, OUT_C, split[1], label=False)
    if split[0]:
        text(lx - 20, y + 41, "L", 8, PRINT, True, "middle")
        text(lx - 20, y + 69, "R", 8, PRINT, True, "middle")
    if split[1]:
        text(rx + 20, y + 41, "L", 8, PRINT, True, "middle")
        text(rx + 20, y + 69, "R", 8, PRINT, True, "middle")
    text(lx, y + 87, "IN", 8, IN_C, True, "middle")
    text(rx, y + 87, "OUT", 8, OUT_C, True, "middle")
    ay = y + 46
    o.append(f'<path d="M{lx + 22},{ay} H{rx - 24}" stroke="{PRINT}" stroke-opacity=".5" stroke-width="1.5"/><path d="M{rx - 30},{ay - 5} l7,5 l-7,5" fill="none" stroke="{PRINT}" stroke-opacity=".5" stroke-width="1.5"/>')
    # meters along the arrow: L over R, short
    for k, mx in enumerate((lx + 24, rx - 66)):
        for row in range(2):
            for i in range(8):
                c = "#3fae5a" if i < 5 else ("#e9c53a" if i < 7 else "#d23b32")
                o.append(f'<rect x="{mx + i * 5}" y="{ay + 8 + row * 6}" width="3.4" height="4" fill="{c if i < 4 + k - row else "#202123"}"/>')
    if two_keys:
        split_key(x + w / 2 - 33, y + 79, split[0], 30)
        split_key(x + w / 2 + 3, y + 79, split[1], 30)
    else:
        split_key(x + (w - 34) / 2, y + 79, split[0])


def scaled(fn, x, y, *a):
    at = len(o)
    fn(0, 0, *a)
    o.insert(at, f'<g transform="translate({x},{y}) scale({S})">')
    o.append("</g>")


def label(x, y, t, sub=""):
    text(x, y, t, 15, "#e6e7e8", True)
    if sub:
        text(x, y + 20, sub, 13, "#9a9da1")


def section(y, t, sub):
    text(60, y, t, 26, "#e58f2c", True, extra='letter-spacing="2"')
    text(60, y + 30, sub, 16, "#9a9da1")


if __name__ == "__main__":
    W, y = 1700, 150
    section(y, "B · ONE BAND, IN AND OUT BELOW", "Each half keeps its own SPLIT, as each frame does now: the input and the output split apart.")
    y += 70
    for i, (t, sub, a) in enumerate([
        ("JOINED", "a STEREO jack a side", (False, False)),
        ("THE INPUT SPLIT", "L and R into the effect, one cord out", (True, False)),
        ("BOTH SPLIT", "four jacks, four mono cords", (True, True)),
    ]):
        x = 60 + i * 540
        label(x, y, t, sub)
        scaled(block_b, x, y + 40, "SPECTRUM", *a)
    y += 40 + 92 * S + 90
    section(y, "C · AS A PEDAL, ONE SPLIT", "One SPLIT for the block: the input and the output split together. The arrow says which way the sound goes.")
    y += 70
    for i, (t, sub, a) in enumerate([
        ("JOINED", "IN at the left edge, OUT at the right", (False, False)),
        ("SPLIT", "L over R on both sides at once", (True, True)),
    ]):
        x = 60 + i * 540
        label(x, y, t, sub)
        scaled(block_c, x, y + 40, "SPECTRUM", a)
    y += 40 + 92 * S + 90
    section(y, "C2 · AS A PEDAL, A SPLIT EACH SIDE", "As C, but each side splits on its own, as now (B's behaviour in C's shape).")
    y += 70
    for i, (t, sub, a) in enumerate([
        ("JOINED", "", (False, False)),
        ("THE INPUT SPLIT", "", (True, False)),
        ("BOTH SPLIT", "", (True, True)),
    ]):
        x = 60 + i * 540
        label(x, y, t, sub)
        scaled(block_c, x, y + 40, "SPECTRUM", a, True)
    y += 40 + 92 * S + 90
    # notes
    notes = [
        "THINGS TO WEIGH",
        "· B: as wide as two halves (208); every control where it is now, only the header is shared",
        "· C: 176 wide; one SPLIT means one switch flips four jacks: fewer keys, but the input and",
        "  the output of an effect can't be split apart (rarely wanted: an effect is stereo in, stereo out)",
        "· C2: C's shape, B's freedom; two small SPLIT keys under the jacks",
        "· split jacks stack L over R, as the face does now; the meter shows each jack's channel",
    ]
    for k, n in enumerate(notes):
        text(60, y + k * 26, n, 16 if k else 20, "#c9cbcd" if k else "#e58f2c", k == 0)
    y += len(notes) * 26 + 60
    head = f'<rect width="{W}" height="{y}" fill="#1f2023"/>'
    head += f'<text x="60" y="70" {F} font-weight="bold" font-size="40" letter-spacing="3" fill="#e58f2c">ONE BLOCK: B AND C, JOINED AND SPLIT</text>'
    head += f'<text x="60" y="108" {F} font-size="20" fill="#9a9da1">Drawn at twice the face\'s size. SPLIT turns a stereo jack into L over R; JOIN back.</text>'
    open(sys.argv[1] if len(sys.argv) > 1 else "block2.svg", "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{y}" viewBox="0 0 {W} {y}">{head}{"".join(o)}</svg>\n')
