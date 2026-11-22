# RACK at 100%: no zoom. A unit is always 128 high; a face is as wide as the rack (the window
# between the rails): the room its frames don't need is vents, from none to the whole face (a card
# whose profile is off). Frames that don't fit go on to a second unit (2U, 3U…). A window wider
# than a 19" rack gets two racks side by side, the devices flowing down the first and on into the
# second. Schematic: blocks for the faces, sized as the app's (ID block 240, frames as measured on
# the screenshot, 10 between).
#   python3 fixed.py fixed.svg && rsvg-convert -z .5 fixed.svg -o fixed.png
import sys

F = 'font-family="Noto Sans"'
M = 'font-family="Noto Sans Mono"'
DEV_H = 128.0
U = DEV_H * 1.75 / 1.719
GAP = U - DEV_H
NUMS, RAIL_W = 20.0, 60.2
ID_W, RIGHT, FGAP = 240.0, 37.0, 10.0
PANEL_H, STATUS_H = 62.0, 22.0
NINETEEN = 1390.0  # a 19" panel at this scale (1.75" = U)

FIN = {"metal": ("#a9acb0", "#8c8f93", "#25272a"), "black": ("#3a3b3e", "#2a2b2d", "#c9cbcd"), "navy": ("#3d5275", "#2c3c58", "#dfe6f0")}
OUT, IN = "out", "in"
# name, finish, frames (label, width, out/in, colour)
DEVICES = [
    ("RADEON HIGH DEFINITION", "metal", []),
    ("RYZEN HD AUDIO", "metal", [("BUILT-IN MIC", 240, OUT, "#3a6596"), ("SPEAKERS", 246, IN, "#2e8580")]),
    ("JLAB GO WORK 2", "metal", [("MIC", 149, OUT, "#3a6596"), ("HEADPHONES", 342, IN, "#2e8580")]),
    ("USB AUDIO 18i8", "metal", [("IN 1-2", 240, OUT, "#3a6596"), ("IN 3-4", 240, OUT, "#3a6596"), ("IN 5-6", 240, OUT, "#3a6596"), ("IN 7-8", 240, OUT, "#3a6596"), ("OUT 1-2", 246, IN, "#2e8580"), ("OUT 3-4", 246, IN, "#2e8580")]),
    ("VIVALDI", "black", [("OUTPUT", 226, OUT, "#b45a2e")]),
    ("MIDI-BRIDGE", "navy", [("MIDI", 74, OUT, "#8f3f63"), ("MIDI", 74, IN, "#8f3f63")]),
    ("FOOYIN", "black", [("OUTPUT", 226, OUT, "#b45a2e")]),
]

o = []


def rows_of(frames, room):
    """Frames into rows of at most `room`: in order (outputs, then inputs), a new row when full."""
    rows, cur, w = [], [], 0.0
    for f in frames:
        need = f[1] + (FGAP if cur else 0)
        if cur and w + need > room:
            rows.append(cur)
            cur, w = [], 0.0
            need = f[1]
        cur.append(f)
        w += need
    if cur or not rows:
        rows.append(cur)
    return rows


def face(x, y, pw, dev):
    name, fin, frames = dev
    base, dark, ink = FIN[fin]
    rows = rows_of(frames, pw - ID_W - RIGHT)
    h = len(rows) * U - GAP
    o.append(f'<rect x="{x}" y="{y}" width="{pw}" height="{h}" rx="3" fill="{base}" stroke="#000" stroke-opacity=".6"/>')
    # screws, through the face into the rails
    for sx in (x + 17.45, x + pw - 17.45):
        for sy in (y + 17.5, y + h - 17.5):
            o.append(f'<circle cx="{sx}" cy="{sy}" r="7.2" fill="#c9ccd0" stroke="#333"/><path d="M{sx - 4},{sy} h8 M{sx},{sy - 4} v8" stroke="#333" stroke-width="1.6"/>')
    # the ID block: name, LCD
    o.append(f'<text x="{x + 36}" y="{y + 30}" {F} font-weight="900" font-size="15" fill="{ink}">{name}</text>')
    o.append(f'<rect x="{x + 35}" y="{y + 52}" width="193" height="58" rx="4" fill="#111214"/><rect x="{x + 38}" y="{y + 54}" width="187" height="54" rx="2" fill="#f2a443"/>')
    for k, row in enumerate(rows):
        ry = y + k * U
        x0, x1 = x + ID_W, x + pw - RIGHT
        outs = [f for f in row if f[2] == OUT]
        ins = [f for f in row if f[2] == IN]
        cx = x0
        for f in outs:
            frame(cx, ry, f)
            cx += f[1] + FGAP
        ex = x1
        for f in reversed(ins):
            ex -= f[1]
            frame(ex, ry, f)
            ex -= FGAP
        # what is left between them: vents
        vx0, vx1 = (cx if outs else x0) + 4, ex - 4 + (FGAP if ins else 0)
        if not ins:
            vx1 = x1
        if vx1 - vx0 > 24:
            for i in range(9):
                o.append(f'<rect x="{vx0}" y="{ry + 24 + i * 9}" width="{vx1 - vx0}" height="4.5" rx="2" fill="#0c0d0e" opacity=".85"/>')
        if k > 0:
            # under the ID block, in a second unit: vents too
            for i in range(9):
                o.append(f'<rect x="{x + 36}" y="{ry + 24 + i * 9}" width="{ID_W - 46}" height="4.5" rx="2" fill="#0c0d0e" opacity=".85"/>')
    return h, len(rows)


def frame(x, y, f):
    label, w, d, c = f
    o.append(f'<rect x="{x}" y="{y + 18}" width="{w}" height="92" rx="6" fill="rgba(0,0,0,.18)" stroke="{c}" stroke-width="1.5"/>')
    o.append(f'<path d="M{x},{y + 24} a6,6 0 0 1 6,-6 h{w - 12} a6,6 0 0 1 6,6 v10 h-{w} z" fill="{c}"/>')
    o.append(f'<text x="{x + 9}" y="{y + 31}" {F} font-weight="bold" font-size="10" fill="#fff" letter-spacing="1">{label}  {"OUT" if d == OUT else "IN"}</text>')
    n = max(1, int((w - 20) // 60))
    for i in range(n):
        o.append(f'<circle cx="{x + 30 + i * 60}" cy="{y + 70}" r="13" fill="#2a2b2e" stroke="#8a8d91" stroke-width="3"/>')


def rail(x0, right, y0, y1, units):
    w = RAIL_W
    xs = x0 - w if right else x0
    o.append(f'<rect x="{xs}" y="{y0}" width="{w}" height="{y1 - y0}" fill="#303134"/>')
    sgn = -1 if right else 1
    hx = x0 + sgn * (NUMS + 17.45)
    for u in range(units):
        top = y0 + GAP / 2 + u * U
        if top > y1:
            break
        for p in (18.6, 65.2, 111.7):
            o.append(f'<rect x="{hx - 14}" y="{top + p - 14}" width="28" height="28" rx="1.2" fill="#050506"/>')
        if u:
            o.append(f'<rect x="{min(x0 + sgn * 3, hx - sgn * 14)}" y="{top}" width="{abs(hx - sgn * 14 - x0 - sgn * 3)}" height="1.3" fill="#e6e6e6"/>')
        o.append(f'<text x="{x0 + sgn * NUMS / 2}" y="{top + U / 2 + 4}" {F} font-weight="bold" font-size="11" fill="#e8e8e8" text-anchor="middle">{u + 1}</text>')


def window(ox, oy, ww, wh, title, note):
    o.append(f'<text x="{ox}" y="{oy - 50}" {F} font-weight="bold" font-size="30" letter-spacing="2" fill="#e58f2c">{title}</text>')
    o.append(f'<text x="{ox}" y="{oy - 16}" {F} font-size="21" fill="#9a9da1">{note}</text>')
    o.append(f'<svg x="{ox}" y="{oy}" width="{ww}" height="{wh}" viewBox="0 0 {ww} {wh}">')
    o.append(f'<rect width="{ww}" height="{wh}" fill="#17181a"/>')
    top, bot = PANEL_H, wh - STATUS_H
    # how many racks: one per 19" (and its rails) the window has room for
    n = max(1, int(ww // (NINETEEN + 2 * NUMS)))
    rw = ww / n
    pw = rw - 2 * NUMS
    # the devices down the first rack, on into the next: split where the racks come out evenest
    hs = [face_h(pw, d) for d in DEVICES]
    cuts = split(hs, n)
    for r in range(n):
        x0 = r * rw
        rail(x0, False, top, bot, 12)
        rail(x0 + rw, True, top, bot, 12)
        y = top + GAP
        for i in cuts[r]:
            h, _ = face(x0 + NUMS, y, pw, DEVICES[i])
            y += h + GAP
    o.append(f'<rect width="{ww}" height="{PANEL_H}" fill="#8a8d91"/><text x="16" y="38" {F} font-weight="900" font-size="22" fill="#2a2b2e" letter-spacing="2">PATCHCORD</text>')
    o.append(f'<rect y="{bot}" width="{ww}" height="{STATUS_H}" fill="#0e0f10"/>')
    o.append('</svg>')
    o.append(f'<rect x="{ox - 2}" y="{oy - 2}" width="{ww + 4}" height="{wh + 4}" fill="none" stroke="#3a3c40" stroke-width="2"/>')


def face_h(pw, dev):
    return len(rows_of(dev[2], pw - ID_W - RIGHT)) * U - GAP


def split(hs, n):
    """The devices, in order, into n runs whose tallest is as short as can be."""
    if n == 1:
        return [list(range(len(hs)))]
    best = None
    for c in range(1, len(hs)):
        a, b = sum(hs[:c]), sum(hs[c:])
        if best is None or max(a, b) < best[0]:
            best = (max(a, b), c)
    c = best[1]
    return [list(range(c)), list(range(c, len(hs)))]


if __name__ == "__main__":
    PAD = 60
    WH = 1250
    wins = [
        (760, "NARROW · 760", "Faces 720 wide: the frames wrap, a card's two frames 2U, the 18i8 5U."),
        (1500, "MEDIUM · 1500", "One rack: frames on one unit where they fit, the 18i8 2U; the rest vents."),
        (2900, "WIDE · 2900", "Two 19″ racks side by side: devices down the first, on into the second."),
    ]
    W = PAD + sum(w + PAD for w, *_ in wins)
    H = 200 + WH + 300
    o.append(f'<rect width="{W}" height="{H}" fill="#1f2023"/>')
    o.append(f'<text x="{PAD}" y="62" {F} font-weight="bold" font-size="40" letter-spacing="3" fill="#e58f2c">RACK AT 100%</text>')
    o.append(f'<text x="{PAD}" y="104" {F} font-size="24" fill="#9a9da1">No zoom: a unit is always 128 high. The window\'s width is the rack\'s: vents take up what the frames don\'t need; frames that don\'t fit go on to another unit.</text>')
    x = PAD
    for ww, t, n in wins:
        window(x, 220, ww, WH, t, n)
        x += ww + PAD
    notes = [
        "Frames go in order, outputs left and inputs right in each unit; a unit full, the next ones go on to one more (under the ID block: vents).",
        "Two racks once the window has room for two 19\" ones (1390 each, at 1.75\" a unit); the devices split where the two come out evenest.",
        "Only the wheel moves it: up and down. The narrowest window: the ID block and the widest frame (about 520).",
    ]
    for i, n in enumerate(notes):
        o.append(f'<text x="{PAD}" y="{220 + WH + 70 + i * 40}" {F} font-size="24" fill="#c9cbcd">{n}</text>')
    open(sys.argv[1] if len(sys.argv) > 1 else "fixed.svg", "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">{"".join(o)}</svg>\n')
