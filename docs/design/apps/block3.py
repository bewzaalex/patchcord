# One node of a chained program as one block: variants A, B and C (and C2) of block.py, each with
# its jacks joined and split, in a new palette for the frames' bands, set beside the one in use.
#   python3 block3.py block3.svg && rsvg-convert -z .6 block3.svg -o block3.png
import math
import sys

F = 'font-family="Noto Sans"'
M = 'font-family="Noto Sans Mono"'
INK = "#f3efe6"
PRINT = "#c9cbcd"
o = []


def oklch(L, C, h):
    """OKLCH to sRGB hex: one lightness and chroma for all, so no colour shouts over another."""
    a, b = C * math.cos(math.radians(h)), C * math.sin(math.radians(h))
    l_, m_, s_ = L + .3963377774 * a + .2158037573 * b, L - .1055613458 * a - .0638541728 * b, L - .0894841775 * a - 1.2914855480 * b
    l, m, s = l_ ** 3, m_ ** 3, s_ ** 3
    rgb = (4.0767416621 * l - 3.3077115913 * m + .2309699292 * s, -1.2684380046 * l + 2.6097574011 * m - .3413193965 * s, -.0041960863 * l - .7034186147 * m + 1.7076147010 * s)
    f = lambda x: 12.92 * x if x <= .0031308 else 1.055 * x ** (1 / 2.4) - .055
    return "#" + "".join(f"{round(max(0, min(1, f(v))) * 255):02x}" for v in rgb)


def contrast(a, b):
    def lum(h):
        f = lambda x: x / 12.92 if x <= .04045 else ((x + .055) / 1.055) ** 2.4
        r, g, bl = (f(int(h[i:i + 2], 16) / 255) for i in (1, 3, 5))
        return .2126 * r + .7152 * g + .0722 * bl
    x, y = sorted((lum(a), lum(b)))
    return (y + .05) / (x + .05)


# (role, what it is, hue): one lightness, one chroma, the hues apart round the circle
ROLES = [
    ("OUTPUT", "an app's / a program's output", 40),
    ("VIDEO", "a camera, a screen", 85),
    ("INPUT", "an app's / a program's input", 140),
    ("PLAYBACK", "a device's output: speakers, headphones", 195),
    ("CAPTURE", "a device's input: a microphone", 245),
    ("MONITOR", "a copy of what an output plays", 290),
    ("MIDI", "MIDI ports", 340),
]
NEW = {r: oklch(.52, .115, h) for r, _, h in ROLES}
NOW = {"MIDI": "#8f3f63", "MONITOR": "#5d5390", "PLAYBACK": "#2e8580", "CAPTURE": "#3a6596", "OUTPUT": "#b45a2e", "INPUT": "#a8862c", "VIDEO": "#a8862c"}
IN_C, OUT_C = NEW["INPUT"], NEW["OUTPUT"]
HEAD = "#5f5a52"  # a block's shared band (B, C): a warm grey, the face's own


def text(x, y, t, size=10, fill=PRINT, bold=False, anchor="start", extra="", font=F):
    w = ' font-weight="bold"' if bold else ""
    o.append(f'<text x="{x}" y="{y}" {font} font-size="{size}"{w} fill="{fill}" text-anchor="{anchor}" {extra}>{t}</text>')


def tag(x, y, t, fill=INK):
    w = 8 + 6.2 * len(t)
    o.append(f'<rect x="{x}" y="{y - 8}" width="{w}" height="11" rx="2" fill="none" stroke="{fill}" stroke-width="1"/>')
    text(x + w / 2, y + .5, t, 7.5, fill, True, "middle", 'letter-spacing=".8"')
    return w


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


def side(cx, y0, ring, split, lr=1):
    """A side's jacks: one STEREO jack, or L over R (L/R printed on side `lr`: 1 right, -1 left)."""
    if split:
        jack(cx, y0 + 18, ring, 12)
        jack(cx, y0 + 46, ring, 12)
        text(cx + 19 * lr, y0 + 21, "L", 8, PRINT, True, "middle")
        text(cx + 19 * lr, y0 + 49, "R", 8, PRINT, True, "middle")
    else:
        jack(cx, y0 + 26, ring)


def body(x, y, w, stroke):
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="92" rx="6" fill="rgba(0,0,0,.15)" stroke="{stroke}" stroke-width="1.5"/>')


def band(x, y, w, fill, left=True, right=True):
    """A band across the top, rounded at the block's corners."""
    l = "a6,6 0 0 1 6,-6" if left else ""
    r = "a6,6 0 0 1 6,6" if right else ""
    o.append(f'<path d="M{x},{y + (6 if left else 0)} {l} h{w - (6 if left else 0) - (6 if right else 0)} {r} v{12 if right else 18} h-{w} z" fill="{fill}"/>')


def block_a(x, y, name, split):
    """A: the band half the input's colour, half the output's; the name once across it; IN and OUT
    tags at its ends; each half its jacks, meter and SPLIT."""
    hw, w = 108, 216
    body(x, y, w, "#77706a")
    band(x, y, hw, IN_C, right=False)
    band(x + hw, y, hw, OUT_C, left=False)
    text(x + w / 2, y + 13, name, 9, INK, True, "middle", 'letter-spacing="1.3"')
    tag(x + 6, y + 12, "IN")
    tag(x + w - 6 - 8 - 6.2 * 3, y + 12, "OUT")
    o.append(f'<path d="M{x + hw},{y + 24} V{y + 88}" stroke="{PRINT}" stroke-opacity=".35" stroke-dasharray="2 3"/>')
    for i, (c, sp) in enumerate(((IN_C, split[0]), (OUT_C, split[1]))):
        hx = x + i * hw
        side(hx + 40, y + 20, c, sp)
        if not sp:
            text(hx + 40, y + 76, "STEREO", 8, PRINT, True, "middle")
        meter(hx + hw - 20, y + 74, 4 if sp else 2)
        split_key(hx + hw - 42, y + 79, sp)


def block_b(x, y, name, split):
    """B: one warm grey band with the name, IN ▸ OUT at its right; IN and OUT tags by the jacks."""
    hw, w = 104, 208
    body(x, y, w, "#77706a")
    band(x, y, w, HEAD)
    text(x + 8, y + 13, name, 9, INK, True, extra='letter-spacing="1.3"')
    text(x + w - 8, y + 13, "IN ▸ OUT", 7.5, INK, True, "end", 'letter-spacing=".8"')
    for i, (d, c, sp) in enumerate((("IN", IN_C, split[0]), ("OUT", OUT_C, split[1]))):
        hx = x + i * hw
        tag(hx + 7, y + 31, d, c)
        side(hx + 50, y + 20, c, sp)
        if not sp:
            text(hx + 50, y + 76, "STEREO", 8, PRINT, True, "middle")
        meter(hx + hw - 20, y + 74, 4 if sp else 2)
        split_key(hx + hw - 42, y + 79, sp)
    o.append(f'<path d="M{x + hw},{y + 24} V{y + 88}" stroke="{PRINT}" stroke-opacity=".35" stroke-dasharray="2 3"/>')


def block_c(x, y, name, split, two=False):
    """C: as a pedal, IN at the left edge, OUT at the right, an arrow between; small meters along
    the arrow; one SPLIT for both (C), or one each (C2)."""
    w = 176
    body(x, y, w, "#77706a")
    band(x, y, w, HEAD)
    text(x + w / 2, y + 13, name, 9, INK, True, "middle", 'letter-spacing="1.3"')
    lx, rx = x + 30, x + w - 30
    side(lx, y + 20, IN_C, split[0], -1)
    side(rx, y + 20, OUT_C, split[1], 1)
    text(lx, y + 87, "IN", 8, IN_C, True, "middle")
    text(rx, y + 87, "OUT", 8, OUT_C, True, "middle")
    ay = y + 46
    o.append(f'<path d="M{lx + 22},{ay} H{rx - 24}" stroke="{PRINT}" stroke-opacity=".5" stroke-width="1.5"/><path d="M{rx - 30},{ay - 5} l7,5 l-7,5" fill="none" stroke="{PRINT}" stroke-opacity=".5" stroke-width="1.5"/>')
    for k, mx in enumerate((lx + 24, rx - 66)):
        for row in range(2):
            for i in range(8):
                c = "#3fae5a" if i < 5 else ("#e9c53a" if i < 7 else "#d23b32")
                o.append(f'<rect x="{mx + i * 5}" y="{ay + 8 + row * 6}" width="3.4" height="4" fill="{c if i < 4 + k - row else "#202123"}"/>')
    if two:
        split_key(x + w / 2 - 33, y + 79, split[0], 30)
        split_key(x + w / 2 + 3, y + 79, split[1], 30)
    else:
        split_key(x + (w - 34) / 2, y + 79, split[0])


S = 1.9


def scaled(fn, x, y, *a):
    at = len(o)
    fn(0, 0, *a)
    o.insert(at, f'<g transform="translate({x},{y}) scale({S})">')
    o.append("</g>")


def section(y, t, sub):
    text(60, y, t, 26, "#e58f2c", True, extra='letter-spacing="2"')
    text(60, y + 30, sub, 16, "#9a9da1")


def palette(y):
    section(y, "THE BANDS' COLOURS", "One lightness, one chroma, hues spread round the circle: none shouts, each is told apart, the cream lettering reads on all.")
    y += 70
    text(60, y, "NOW", 14, "#9a9da1", True, extra='letter-spacing="2"')
    text(60, y + 120, "NEW", 14, "#e58f2c", True, extra='letter-spacing="2"')
    for i, (role, what, hue) in enumerate(ROLES):
        x = 140 + i * 215
        for row, pal in ((0, NOW), (1, NEW)):
            c = pal[role]
            yy = y - 24 + row * 120
            o.append(f'<rect x="{x}" y="{yy}" width="200" height="34" rx="6" fill="{c}"/>')
            text(x + 10, yy + 22, f"{role}", 13, INK, True, extra='letter-spacing="1.5"')
            t = "IN" if role in ("INPUT", "PLAYBACK") else "OUT"
            tag(x + 200 - 10 - (8 + 6.2 * len(t)), yy + 21, t)
            text(x, yy + 54, c.upper(), 13, PRINT, font=M)
            text(x + 200, yy + 54, f"{contrast(c, INK):.1f}:1", 12, "#7d8085", anchor="end", font=M)
            if row == 1:
                text(x, yy + 74, what, 11, "#9a9da1")
                text(x, yy + 90, f"hue {hue}°", 11, "#6d7075", font=M)
    text(60, y + 205, "NOW: an app's INPUT (ochre) and OUTPUT (rust) are 42° apart and look alike; VIDEO shares INPUT's colour. NEW: INPUT green, OUTPUT rust, each its own hue.", 14, "#c9cbcd")
    return y + 240


if __name__ == "__main__":
    W, y = 1720, 150
    y = palette(y) + 40
    STATES = [("JOINED", (False, False)), ("THE INPUT SPLIT", (True, False)), ("BOTH SPLIT", (True, True))]
    for title, sub, fn, states, two in [
        ("A · ONE BAND, TWO COLOURS", "The band half the input's colour, half the output's; the name once across it.", block_a, STATES, False),
        ("B · ONE BAND, IN AND OUT BELOW", "A warm grey band with the name; IN and OUT tags by their jacks, in their colours.", block_b, STATES, False),
        ("C · AS A PEDAL, ONE SPLIT", "IN at the left edge, OUT at the right, an arrow between; one SPLIT splits both sides.", block_c, [("JOINED", (False, False)), ("SPLIT", (True, True))], False),
        ("C2 · AS A PEDAL, A SPLIT EACH SIDE", "C's shape, each side splitting on its own.", block_c, STATES, True),
    ]:
        section(y, title, sub)
        y += 66
        for i, (st, sp) in enumerate(states):
            x = 60 + i * 560
            text(x, y, st, 15, "#e6e7e8", True, extra='letter-spacing="1"')
            if two:
                scaled(fn, x, y + 16, "SPECTRUM", sp, True)
            else:
                scaled(fn, x, y + 16, "SPECTRUM", sp)
        y += 16 + 92 * S + 70
    head = f'<rect width="{W}" height="{y}" fill="#1f2023"/>'
    head += f'<text x="60" y="70" {F} font-weight="bold" font-size="40" letter-spacing="3" fill="#e58f2c">ONE BLOCK: A, B, C, JOINED AND SPLIT</text>'
    head += f'<text x="60" y="108" {F} font-size="20" fill="#9a9da1">Blocks at almost twice the face\'s size, in the new palette. SPLIT turns a STEREO jack into L over R; JOIN back.</text>'
    open(sys.argv[1] if len(sys.argv) > 1 else "block3.svg", "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{y}" viewBox="0 0 {W} {y}">{head}{"".join(o)}</svg>\n')
