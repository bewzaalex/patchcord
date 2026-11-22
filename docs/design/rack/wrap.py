# RACK: how a face that needs more than one unit spreads its frames. The same devices in each
# variant, at two widths (700: half the user's screen; 1000). Frames in order: what takes sound in
# (IN, left), then what sends it out (OUT, right). Sizes are the app's (name and screen 190, frames
# as measured on the screenshots, 10 between).
#   python3 wrap.py wrap.svg && rsvg-convert -z .5 wrap.svg -o wrap.png
import sys

F = 'font-family="Noto Sans"'
DEV_H = 128.0
U = DEV_H * 1.75 / 1.719
GAP = U - DEV_H
NUMS = 20.0
EAR, PAD, IDW = 20.4, 16.0, 190.0
FGAP = 10.0
IN, OUT = "in", "out"
TEAL, BLUE, GOLD, ORANGE = "#2e8580", "#3a6596", "#a8862a", "#b45a2e"

# name, kind, frames (label, width, in/out, colour)
DEVICES = [
    ("RYZEN HD AUDIO", "ALSA · PCI", "#a9acb0", [("SPEAKERS", 247, IN, TEAL), ("BUILT-IN MIC", 241, OUT, BLUE)]),
    ("JLAB GO WORK 2", "BLUETOOTH HEADSET", "#a9acb0", [("HEADPHONES", 247, IN, TEAL), ("MIC", 150, OUT, BLUE)]),
    ("MEETING", "APP · RECORD &amp; PLAY", "#3a3b3e", [("INPUT", 150, IN, GOLD), ("OUTPUT 1", 150, OUT, ORANGE), ("OUTPUT 2", 150, OUT, ORANGE), ("OUTPUT 3", 150, OUT, ORANGE)]),
    ("USB AUDIO 18i8", "ALSA · USB", "#a9acb0", [("PLAYBACK 1-2", 247, IN, TEAL), ("PLAYBACK 3-4", 247, IN, TEAL), ("CAPTURE 1-2", 190, OUT, BLUE), ("CAPTURE 3-4", 190, OUT, BLUE), ("CAPTURE 5-6", 190, OUT, BLUE), ("CAPTURE 7-8", 190, OUT, BLUE)]),
]

o = []


def rooms(pw):
    """The room beside the name and screen, and a whole unit's."""
    whole = pw - 2 * (EAR + PAD)
    return whole - IDW - 14, whole


def fits(row, room):
    return sum(f[1] for f in row) + FGAP * max(0, len(row) - 1) <= room


# ---- the variants: each gives the units, top first, as lists of frames (in order) ----

def now(frames, pw):
    """Today: from the top, each unit as full as it goes (the first beside the name)."""
    first, whole = rooms(pw)
    rows = [[]]
    for f in frames:
        room = first if len(rows) == 1 else whole
        if rows[-1] and not fits(rows[-1] + [f], room):
            rows.append([])
        elif not rows[-1] and len(rows) == 1 and f[1] > room:
            rows.append([])
        rows[-1].append(f)
    return rows


def bottom_first(frames, pw):
    """From the bottom: the lower units (the whole width) fill first, from the last frame back;
    the unit beside the name takes only what is left over. As many units as today, or one fewer."""
    first, whole = rooms(pw)
    for n in range(1, 10):
        rows = [[] for _ in range(n)]
        k = n - 1
        ok = True
        for f in reversed(frames):
            while k >= 0 and not fits([f] + rows[k], first if k == 0 else whole):
                k -= 1
            if k < 0:
                ok = False
                break
            rows[k].insert(0, f)
        if ok:
            return rows
    return [frames]


def column(frames, pw):
    """A column of frames right of the name: every unit's frames start at the same x, after the
    name and screen, flowing on as text does; under the name: a blank plate."""
    first, _ = rooms(pw)
    rows = [[]]
    for f in frames:
        if rows[-1] and not fits(rows[-1] + [f], first):
            rows.append([])
        rows[-1].append(f)
    return rows


def name_below(frames, pw):
    """Bottom-first upside down: the frames take the top units (the whole width) and the name and
    screen sit in the last unit, with what is left over beside them."""
    return list(reversed(bottom_first(list(frames), pw)))


VARIANTS = [
    ("A · NOW", "From the top, each unit as full as it goes: the frames stair-step.", now, "pack"),
    ("B · BOTTOM FIRST", "The lower units fill first (whole width); beside the name only what is left.", bottom_first, "pack"),
    ("C · ONE COLUMN", "Every unit's frames start after the name, one under another; a plate under the name.", column, "column"),
    ("D · NAME BELOW", "B upside down: frames on top across the whole width, the name in the last unit.", name_below, "below"),
    ("E · B + C", "B's units (the lower ones first, the whole width), every frame from the left; vents only at the right.", bottom_first, "left"),
]


# ---- drawing ----

def vents(x0, x1, y):
    if x1 - x0 < 40:
        return
    for i in range(9):
        o.append(f'<rect x="{x0}" y="{y + 24 + i * 9}" width="{x1 - x0}" height="4.5" rx="2" fill="#0c0d0e" opacity=".85"/>')


def frame(x, y, f):
    label, w, d, c = f
    o.append(f'<rect x="{x}" y="{y + 18}" width="{w}" height="92" rx="6" fill="rgba(0,0,0,.18)" stroke="{c}" stroke-width="1.5"/>')
    o.append(f'<path d="M{x},{y + 24} a6,6 0 0 1 6,-6 h{w - 12} a6,6 0 0 1 6,6 v10 h-{w} z" fill="{c}"/>')
    o.append(f'<text x="{x + 9}" y="{y + 31}" {F} font-weight="bold" font-size="10" fill="#fff" letter-spacing="1">{label}  {"IN" if d == IN else "OUT"}</text>')
    n = max(1, int((w - 20) // 60))
    for i in range(n):
        o.append(f'<circle cx="{x + 30 + i * 60}" cy="{y + 70}" r="13" fill="#2a2b2e" stroke="#8a8d91" stroke-width="3"/>')


def ident(x, y, dev, ink):
    name, kind, *_ = dev
    o.append(f'<text x="{x}" y="{y + 30}" {F} font-weight="900" font-size="15" fill="{ink}">{name}</text>')
    o.append(f'<text x="{x}" y="{y + 44}" {F} font-weight="bold" font-size="8" letter-spacing="1" fill="{ink}" opacity=".7">{kind}</text>')
    o.append(f'<rect x="{x - 1}" y="{y + 52}" width="193" height="58" rx="4" fill="#111214"/><rect x="{x + 2}" y="{y + 54}" width="187" height="54" rx="2" fill="#f2a443"/>')


def unit_frames(row, x0, x1, y, align):
    """A unit's frames between x0 and x1: IN from the left, OUT against the right (or all from
    the left, for the column), vents between."""
    if align in ("column", "left"):
        cx = x0
        for f in row:
            frame(cx, y, f)
            cx += f[1] + FGAP
        vents(cx + 4, x1, y)
        return
    ins = [f for f in row if f[2] == IN]
    outs = [f for f in row if f[2] == OUT]
    cx = x0
    for f in ins:
        frame(cx, y, f)
        cx += f[1] + FGAP
    ex = x1
    for f in reversed(outs):
        ex -= f[1]
        frame(ex, y, f)
        ex -= FGAP
    vents((cx if ins else x0) + 4 - (0 if ins else 4), (ex + FGAP if outs else x1) - 4 - (0 if outs else -4), y)


def face(x, y, pw, dev, variant):
    _, _, base, frames = dev
    fn, align = variant[2], variant[3]
    rows = fn(frames, pw)
    h = len(rows) * U - GAP
    dark = base < "#80"
    ink = "#e6e7e8" if dark else "#25272a"
    o.append(f'<rect x="{x}" y="{y}" width="{pw}" height="{h}" rx="3" fill="{base}" stroke="#000" stroke-opacity=".6"/>')
    for sx in (x + 17.45, x + pw - 17.45):
        for sy in (y + 17.5, y + h - 17.5):
            o.append(f'<circle cx="{sx}" cy="{sy}" r="7.2" fill="#c9ccd0" stroke="#333"/><path d="M{sx - 4},{sy} h8 M{sx},{sy - 4} v8" stroke="#333" stroke-width="1.6"/>')
    left = x + EAR + PAD
    right = x + pw - EAR - PAD
    id_unit = len(rows) - 1 if align == "below" else 0
    for k, row in enumerate(rows):
        ry = y + k * U
        if k:
            o.append(f'<path d="M{left},{ry - GAP / 2} H{right}" stroke="#000" stroke-opacity=".35"/><path d="M{left},{ry - GAP / 2 + 1} H{right}" stroke="#fff" stroke-opacity=".12"/>')
        if k == id_unit:
            ident(left, ry, dev, ink)
            unit_frames(row, left + IDW + 14, right, ry, align)
        elif align == "column":
            # under the name: a plain plate
            unit_frames(row, left + IDW + 14, right, ry, align)
        else:
            unit_frames(row, left, right, ry, align)
    return h


def rack(x, y, pw, variant):
    rw = pw + 2 * NUMS
    top = y
    yy = y + GAP
    for dev in DEVICES:
        yy += face(x + NUMS, yy, pw, dev, variant) + GAP
    # the rails' number strips
    o.append(f'<rect x="{x}" y="{top}" width="{NUMS}" height="{yy - top}" fill="#303134"/><rect x="{x + rw - NUMS}" y="{top}" width="{NUMS}" height="{yy - top}" fill="#303134"/>')
    return yy - top


if __name__ == "__main__":
    PAD_OUT = 60
    widths = [700, 1000]
    col_w = max(widths) + 2 * NUMS
    W = PAD_OUT + len(VARIANTS) * (col_w + PAD_OUT)
    body_y = 200
    y = body_y
    heights = []
    for pw in widths:
        o.append(f'<text x="{PAD_OUT}" y="{y + 30}" {F} font-weight="bold" font-size="26" fill="#9a9da1">A FACE {pw} WIDE</text>')
        y += 60
        tallest = 0
        for i, v in enumerate(VARIANTS):
            cx = PAD_OUT + i * (col_w + PAD_OUT)
            if pw == widths[0]:
                o.append(f'<text x="{cx}" y="{body_y - 60}" {F} font-weight="bold" font-size="30" letter-spacing="2" fill="#e58f2c">{v[0]}</text>')
                o.append(f'<text x="{cx}" y="{body_y - 24}" {F} font-size="18" fill="#c9cbcd">{v[1]}</text>')
            tallest = max(tallest, rack(cx, y, pw, v))
        y += tallest + 80
    H = y + 40
    head = f'<rect width="{W}" height="{H}" fill="#1f2023"/><text x="{PAD_OUT}" y="62" {F} font-weight="bold" font-size="40" letter-spacing="3" fill="#e58f2c">RACK · 2U AND 3U FACES</text>'
    head += f'<text x="{PAD_OUT}" y="104" {F} font-size="24" fill="#9a9da1">The same four devices in each variant. IN at the left, OUT at the right; vents fill the rest.</text>'
    open(sys.argv[1] if len(sys.argv) > 1 else "wrap.svg", "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">{head}{"".join(o)}</svg>\n')
