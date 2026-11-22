# One program, one roof: what Easy Effects puts in the rack today (seven devices, cords between
# them) and ways to keep a program's things together. Easy Effects as it runs here (pw-dump,
# 2026-11-22): its sink and source, a spectrum and a level meter on each side, a test signal; all
# from one client (easyeffects, one process, application.id com.github.wwmm.easyeffects).
#   python3 roof.py roof.svg && rsvg-convert -z .5 roof.svg -o roof.png
import sys

F = 'font-family="Noto Sans"'
M = 'font-family="Noto Sans Mono"'
DEV_H = 128.0
U = DEV_H * 1.75 / 1.719
GAP = U - DEV_H
NUMS = 20.0
EAR, PAD, IDW = 20.4, 16.0, 190.0
FGAP = 10.0
FW = 150.0
PW = 1100.0  # a face's width here
TEAL, BLUE, GOLD, ORANGE = "#2e8580", "#3a6596", "#a8862a", "#b45a2e"
CREAM, BLACK = "#d9d3be", "#3a3b3e"
CORD = {"out": "#d1543a", "in": "#3fa7d6", "mic": "#7fbf4a", "app": "#e0a020"}

o = []


def text(x, y, t, size=16, fill="#c9cbcd", bold=False, anchor="start", font=F, extra=""):
    o.append(f'<text x="{x}" y="{y}" {font} font-size="{size}" fill="{fill}" text-anchor="{anchor}"{" font-weight=\"bold\"" if bold else ""} {extra}>{t.replace("&", "&amp;")}</text>')


def fw(label, n=2):
    """A frame's width: wide enough for its label and its jacks."""
    return max(FW, 7.4 * (len(label) + 5) + 16, 50 * n + 30)


def frame(x, y, label, d, w=None, jacks=None):
    """A frame; its left jack's centre. jacks: names under mono jacks (else one stereo pair)."""
    w = w or fw(label, len(jacks) if jacks else 2)
    c = (TEAL if d == "in" else BLUE)
    o.append(f'<rect x="{x}" y="{y + 18}" width="{w}" height="92" rx="6" fill="rgba(0,0,0,.18)" stroke="{c}" stroke-width="1.5"/>')
    o.append(f'<path d="M{x},{y + 24} a6,6 0 0 1 6,-6 h{w - 12} a6,6 0 0 1 6,6 v10 h-{w} z" fill="{c}"/>')
    text(x + 8, y + 31, f'{label}  {"IN" if d == "in" else "OUT"}', 9.5, "#fff", True, extra='letter-spacing="1"')
    if jacks:
        jx = [x + w / 2 + 50 * (i - (len(jacks) - 1) / 2) for i in range(len(jacks))]
    else:
        jx = [x + w / 2 - 28, x + w / 2 + 28]
    for i, cx in enumerate(jx):
        o.append(f'<circle cx="{cx}" cy="{y + 66}" r="13" fill="#2a2b2e" stroke="#8a8d91" stroke-width="3"/>')
        if jacks:
            text(cx, y + 100, jacks[i], 10, "#77797d", anchor="middle", font=M)
    return (jx[0], y + 66) if jacks else (jx[0], y + 70)


def screws(x, y, w, h):
    for sx in (x + 17.45, x + w - 17.45):
        for sy in (y + 17.5, y + h - 17.5):
            o.append(f'<circle cx="{sx}" cy="{sy}" r="7.2" fill="#c9ccd0" stroke="#333"/><path d="M{sx - 4},{sy} h8 M{sx},{sy - 4} v8" stroke="#333" stroke-width="1.6"/>')


def ident(x, y, name, kind, state, line2, line3, base, keys=(), line4=""):
    dark = base < "#80"
    ink = "#e6e7e8" if dark else "#25272a"
    text(x, y + 30, name, 15, ink, extra='font-weight="900"')
    o.append(f'<rect x="{x - 1.5}" y="{y + 40}" width="{IDW + 3}" height="63" rx="4" fill="#111214"/><rect x="{x + 1.5}" y="{y + 43}" width="{IDW - 3}" height="57" rx="2" fill="#f2a443"/>')
    text(x + 8, y + 56, kind, 10.5, "#3a2408", True, font=M)
    text(x + IDW - 8, y + 56, state, 10.5, "#3a2408", True, "end", M)
    text(x + 8, y + 69.5, line2, 10.5, "#3a2408", font=M)
    text(x + 8, y + 83, line3, 10.5, "#3a2408", font=M)
    text(x + 8, y + 96.5, line4, 10.5, "#3a2408", font=M)
    # the keys' row: HIDE, then the rest
    o.append(f'<rect x="{x + 4}" y="{y + 107}" width="22" height="14" rx="2.5" fill="#1b1c1e"/>')
    text(x + 15, y + 117, "⊘", 10, "#e6e7e8", anchor="middle")
    kx = x + 32
    for k in keys:
        kw = (IDW - 8 - 28 - 6 * (len(keys) - 1)) / len(keys)
        o.append(f'<rect x="{kx}" y="{y + 107}" width="{kw}" height="14" rx="2.5" fill="#1b1c1e"/>')
        text(kx + kw / 2, y + 117.5, k, 7.5, "#e6e7e8", True, "middle", extra='letter-spacing="1.5"')
        kx += kw + 6


def vents(x0, x1, y):
    if x1 - x0 < 40:
        return
    for i in range(9):
        o.append(f'<rect x="{x0}" y="{y + 24 + i * 9}" width="{x1 - x0}" height="4.5" rx="2" fill="#0c0d0e" opacity=".85"/>')


def face(x, y, units, base, name, kind, line2, line3, rows, keys=(), state="RUNNING", line4=""):
    """A face of `units` units; rows: per unit, a list of (label, dir, key) frames, the first
    beside the name. Returns {key: jack centre}."""
    h = units * U - GAP
    o.append(f'<rect x="{x}" y="{y}" width="{PW}" height="{h}" rx="3" fill="{base}" stroke="#000" stroke-opacity=".6"/>')
    screws(x, y, PW, h)
    left, right = x + EAR + PAD, x + PW - EAR - PAD
    ident(left, y, name, kind, state, line2, line3, base, keys, line4)
    jacks = {}
    for k, row in enumerate(rows):
        ry = y + k * U
        if k:
            o.append(f'<path d="M{left},{ry - GAP / 2} H{right}" stroke="#000" stroke-opacity=".35"/>')
        cx = left + IDW + 14 if k == 0 else left
        for f in row:
            if f is None:
                cx += 18
                continue
            label, d, key = f[:3]
            js = f[3] if len(f) > 3 else None
            jacks[key] = frame(cx, ry, label, d, jacks=js)
            cx += fw(label, len(js) if js else 2) + FGAP
        vents(cx + 4, right, ry)
    return jacks, h


def cord(a, b, col, sag=60):
    (x1, y1), (x2, y2) = a, b
    o.append(f'<path d="M{x1},{y1} C{x1},{y1 + sag} {x2},{y2 + sag} {x2},{y2}" fill="none" stroke="#000" stroke-opacity=".35" stroke-width="9" stroke-linecap="round"/>')
    o.append(f'<path d="M{x1},{y1} C{x1},{y1 + sag} {x2},{y2 + sag} {x2},{y2}" fill="none" stroke="{col}" stroke-width="6" stroke-linecap="round"/>')
    for x, y in (a, b):
        o.append(f'<circle cx="{x}" cy="{y}" r="8" fill="{col}" stroke="#111" stroke-width="2"/>')


def away(p, label, col, side):
    """A cord off to a device outside the picture."""
    x, y = p
    ex = x + (160 if side > 0 else -160)
    o.append(f'<path d="M{x},{y} C{x},{y + 50} {ex},{y + 10} {ex},{y + 30}" fill="none" stroke="{col}" stroke-width="6" stroke-linecap="round" stroke-dasharray="2 10" opacity=".9"/>')
    o.append(f'<circle cx="{x}" cy="{y}" r="8" fill="{col}" stroke="#111" stroke-width="2"/>')
    o.append(f'<rect x="{ex - 70}" y="{y + 32}" width="140" height="26" rx="13" fill="#111214" stroke="{col}" stroke-width="2"/>')
    text(ex, y + 50, label, 13, col, True, "middle")


def rails(x, y, h):
    o.append(f'<rect x="{x}" y="{y}" width="{NUMS}" height="{h}" fill="#303134"/><rect x="{x + PW + NUMS}" y="{y}" width="{NUMS}" height="{h}" fill="#303134"/>')


def title(x, y, t, sub):
    text(x, y, t, 30, "#e58f2c", True, extra='letter-spacing="2"')
    for i, s in enumerate(sub):
        text(x, y + 36 + i * 26, s, 19, "#c9cbcd")


# ---- the variants ----

def now(x, y):
    """Today: seven devices, in the order of their keys, cords between them."""
    rails(x, y, 7 * U + GAP)
    fx = x + NUMS
    yy = y + GAP
    j = {}
    devs = [
        ("EASY EFFECTS SINK", "VIRTUAL SINK", CREAM, [("PLAYBACK", "in", "sink_in"), ("MONITOR", "out", "sink_out")]),
        ("EASY EFFECTS SOURCE", "VIRTUAL SOURCE", CREAM, [("INPUT", "in", "src_in"), ("OUTPUT", "out", "src_out")]),
        ("OUTPUT LEVEL METER", "VIRTUAL DEVICE", BLACK, [("INPUT", "in", "lvl_i_in"), ("OUTPUT", "out", "lvl_i_out")]),
        ("SPECTRUM", "VIRTUAL DEVICE", BLACK, [("INPUT", "in", "spc_i_in"), ("OUTPUT", "out", "spc_i_out")]),
        ("OUTPUT LEVEL METER", "VIRTUAL DEVICE", BLACK, [("INPUT", "in", "lvl_o_in"), ("OUTPUT", "out", "lvl_o_out")]),
        ("SPECTRUM", "VIRTUAL DEVICE", BLACK, [("INPUT", "in", "spc_o_in"), ("OUTPUT", "out", "spc_o_out")]),
        ("EASY EFFECTS FILTER", "VIRTUAL DEVICE", BLACK, [("OUTPUT", "out", "test_out")]),
    ]
    for name, kind, base, frames in devs:
        jj, h = face(fx, yy, 1, base, name, kind, "1 LINK", "STEREO", [frames])
        j.update(jj)
        yy += h + GAP
    cord(j["sink_out"], j["spc_o_in"], CORD["out"], 90)
    cord(j["spc_o_out"], j["lvl_o_in"], CORD["out"], 70)
    cord(j["spc_i_out"], j["lvl_i_in"], CORD["mic"], 70)
    cord(j["lvl_i_out"], j["src_in"], CORD["mic"], 90)
    away(j["sink_in"], "← VIVALDI, KADENCE", CORD["app"], 1)
    away(j["lvl_o_out"], "→ JLAB HEADPHONES", CORD["out"], 1)
    away(j["spc_i_in"], "← JLAB MIC", CORD["mic"], 1)
    away(j["src_out"], "→ APPS THAT RECORD", CORD["in"], 1)
    return 7 * U + GAP


def every(x, y):
    """A: one face, every node a pair of frames, in the order the sound goes: the output chain,
    then the input chain, the test signal last."""
    rails(x, y, 4 * U + GAP)
    rows = [
        [("PLAYBACK", "in", "sink_in"), ("MONITOR", "out", "sink_out"), ("SPECTRUM", "in", "spc_o_in")],
        [("SPECTRUM", "out", "spc_o_out"), ("LEVEL", "in", "lvl_o_in"), ("LEVEL", "out", "lvl_o_out"), None, ("SPECTRUM", "in", "spc_i_in")],
        [("SPECTRUM", "out", "spc_i_out"), ("LEVEL", "in", "lvl_i_in"), ("LEVEL", "out", "lvl_i_out"), ("SOURCE", "in", "src_in")],
        [("SOURCE", "out", "src_out"), None, ("TEST SIGNAL", "out", "test_out")],
    ]
    j, h = face(x + NUMS, y + GAP, 4, BLACK, "EASY EFFECTS", "EFFECTS", "7 NODES  4 LINKS OUT", "2 CHAINS", rows)
    cord(j["sink_out"], j["spc_o_in"], CORD["out"], 40)
    cord(j["spc_o_out"], j["lvl_o_in"], CORD["out"], 40)
    cord(j["spc_i_out"], j["lvl_i_in"], CORD["mic"], 40)
    cord(j["lvl_i_out"], j["src_in"], CORD["mic"], 40)
    away(j["sink_in"], "← VIVALDI, KADENCE", CORD["app"], 1)
    away(j["lvl_o_out"], "→ JLAB HEADPHONES", CORD["out"], 1)
    away(j["spc_i_in"], "← JLAB MIC", CORD["mic"], -1)
    away(j["src_out"], "→ APPS THAT RECORD", CORD["in"], 1)
    return 4 * U + GAP


def edges(x, y):
    """B: one face with only what reaches outside the program; its insides behind a key."""
    rails(x, y, 3 * U + GAP)
    rows = [[("FROM APPS", "in", "sink_in"), ("MIC", "in", "spc_i_in"), ("TO OUTPUT", "out", "lvl_o_out"), ("TO APPS", "out", "src_out")]]
    j, h = face(x + NUMS, y + GAP, 1, BLACK, "EASY EFFECTS", "EFFECTS", "7 NODES  4 INSIDE", "", rows, keys=("INSIDE ▾",))
    away(j["sink_in"], "← VIVALDI, KADENCE", CORD["app"], 1)
    away(j["spc_i_in"], "← JLAB MIC", CORD["mic"], 1)
    away(j["lvl_o_out"], "→ JLAB HEADPHONES", CORD["out"], 1)
    away(j["src_out"], "→ APPS THAT RECORD", CORD["in"], 1)
    # INSIDE open: a drawer under the face, the program's own wiring, read-only
    dy = y + GAP + DEV_H + 80
    dx = x + NUMS + 30
    dw = PW - 60
    o.append(f'<rect x="{dx}" y="{dy}" width="{dw}" height="200" rx="6" fill="#141517" stroke="#3a3c40" stroke-width="2"/>')
    text(dx + 16, dy + 28, "INSIDE ▴  (what Easy Effects wired itself; 7 nodes, nothing hidden)", 14, "#9a9da1", True)

    def node(cx, cy, t, col="#4a4c50"):
        o.append(f'<rect x="{cx - 62}" y="{cy - 17}" width="124" height="34" rx="5" fill="#26282b" stroke="{col}" stroke-width="2"/>')
        text(cx, cy + 5, t, 12, "#e6e7e8", True, "middle")

    def arrow(x1, y1, x2, col):
        o.append(f'<path d="M{x1},{y1} H{x2 - 8}" stroke="{col}" stroke-width="3"/><path d="M{x2},{y1} l-10,-6 v12 z" fill="{col}"/>')

    xs = [dx + 110 + i * 150 for i in range(5)]
    r1, r2, r3 = dy + 65, dy + 120, dy + 172
    for (cy, chain, col) in ((r1, ["FROM APPS", "SINK", "SPECTRUM", "LEVEL", "TO OUTPUT"], CORD["out"]), (r2, ["MIC", "SPECTRUM", "LEVEL", "SOURCE", "TO APPS"], CORD["mic"])):
        for i, t in enumerate(chain):
            edge = i in (0, 4)
            node(xs[i], cy, t, col if edge else "#5a5c60")
            if i:
                arrow(xs[i - 1] + 62, cy, xs[i] - 62, col)
    node(xs[0], r3, "TEST SIGNAL")
    text(xs[0] + 80, r3 + 5, "not linked", 13, "#9a9da1")
    return 3 * U + GAP + 120


def case(x, y):
    """C: a case in the rack: a band with the program's name across the rails, its devices under
    it in the order the sound goes, a bar down their side. ▾ folds it to the band."""
    rails(x, y, 8 * U)
    bx = x + NUMS
    o.append(f'<rect x="{bx}" y="{y + GAP}" width="{PW}" height="40" rx="3" fill="#1b1c1e" stroke="#e58f2c" stroke-width="2"/>')
    text(bx + 20, y + GAP + 27, "▾  EASY EFFECTS", 17, "#e58f2c", True, extra='letter-spacing="2"')
    text(bx + PW - 20, y + GAP + 27, "7 DEVICES · ONE PROCESS", 13, "#9a9da1", True, "end", extra='letter-spacing="1"')
    yy = y + GAP + 40 + GAP
    j = {}
    devs = [
        ("EASY EFFECTS SINK", "VIRTUAL SINK", CREAM, [("PLAYBACK", "in", "sink_in"), ("MONITOR", "out", "sink_out")]),
        ("SPECTRUM", "FILTER", BLACK, [("INPUT", "in", "spc_o_in"), ("OUTPUT", "out", "spc_o_out")]),
        ("OUTPUT LEVEL METER", "FILTER", BLACK, [("INPUT", "in", "lvl_o_in"), ("OUTPUT", "out", "lvl_o_out")]),
        ("SPECTRUM", "FILTER", BLACK, [("INPUT", "in", "spc_i_in"), ("OUTPUT", "out", "spc_i_out")]),
        ("OUTPUT LEVEL METER", "FILTER", BLACK, [("INPUT", "in", "lvl_i_in"), ("OUTPUT", "out", "lvl_i_out")]),
        ("EASY EFFECTS SOURCE", "VIRTUAL SOURCE", CREAM, [("INPUT", "in", "src_in"), ("OUTPUT", "out", "src_out")]),
        ("EASY EFFECTS FILTER", "TEST SIGNAL", BLACK, [("OUTPUT", "out", "test_out")]),
    ]
    top = yy
    for name, kind, base, frames in devs:
        jj, h = face(bx, yy, 1, base, name, kind, "1 LINK", "STEREO", [frames])
        j.update(jj)
        yy += h + GAP
    o.append(f'<rect x="{bx - 7}" y="{top}" width="5" height="{yy - top - GAP}" fill="#e58f2c"/>')
    cord(j["sink_out"], j["spc_o_in"], CORD["out"], 50)
    cord(j["spc_o_out"], j["lvl_o_in"], CORD["out"], 50)
    cord(j["spc_i_out"], j["lvl_i_in"], CORD["mic"], 50)
    cord(j["lvl_i_out"], j["src_in"], CORD["mic"], 50)
    away(j["sink_in"], "← VIVALDI, KADENCE", CORD["app"], 1)
    away(j["lvl_o_out"], "→ JLAB HEADPHONES", CORD["out"], 1)
    away(j["spc_i_in"], "← JLAB MIC", CORD["mic"], 1)
    away(j["src_out"], "→ APPS THAT RECORD", CORD["in"], 1)
    # folded
    fy = yy + 40
    text(bx, fy + 10, "FOLDED:", 15, "#9a9da1", True, extra='letter-spacing="2"')
    o.append(f'<rect x="{bx}" y="{fy + 24}" width="{PW}" height="40" rx="3" fill="#1b1c1e" stroke="#e58f2c" stroke-width="2"/>')
    text(bx + 20, fy + 51, "▸  EASY EFFECTS", 17, "#e58f2c", True, extra='letter-spacing="2"')
    text(bx + PW - 20, fy + 51, "7 DEVICES · 4 CORDS OUT", 13, "#9a9da1", True, "end", extra='letter-spacing="1"')
    return fy + 64 - y


def chains(x, y):
    """D: one face per chain the program keeps itself (PipeWire's node.group): its output chain,
    its input chain; what is in no chain (the test signal) a face of its own."""
    rails(x, y, 5 * U + GAP)
    rows1 = [[("PLAYBACK", "in", "sink_in"), ("MONITOR", "out", "sink_out"), ("SPECTRUM", "in", "spc_o_in")], [("SPECTRUM", "out", "spc_o_out"), ("LEVEL", "in", "lvl_o_in"), ("LEVEL", "out", "lvl_o_out")]]
    j, h = face(x + NUMS, y + GAP, 2, BLACK, "EASY EFFECTS · OUT", "EFFECTS", "3 NODES  2 LINKS OUT", "ee_sink_group", rows1)
    rows2 = [[("SPECTRUM", "in", "spc_i_in"), ("SPECTRUM", "out", "spc_i_out"), ("LEVEL", "in", "lvl_i_in")], [("LEVEL", "out", "lvl_i_out"), ("SOURCE", "in", "src_in"), ("SOURCE", "out", "src_out")]]
    j2, h2 = face(x + NUMS, y + GAP + 2 * U, 2, BLACK, "EASY EFFECTS · IN", "EFFECTS", "3 NODES  2 LINKS OUT", "ee_source_group", rows2)
    j.update(j2)
    j3, _ = face(x + NUMS, y + GAP + 4 * U, 1, BLACK, "EASY EFFECTS FILTER", "TEST SIGNAL", "NO LINKS", "STEREO", [[("OUTPUT", "out", "test_out")]])
    cord(j["sink_out"], j["spc_o_in"], CORD["out"], 40)
    cord(j["spc_o_out"], j["lvl_o_in"], CORD["out"], 40)
    cord(j["spc_i_out"], j["lvl_i_in"], CORD["mic"], 40)
    cord(j["lvl_i_out"], j["src_in"], CORD["mic"], 40)
    away(j["sink_in"], "← VIVALDI, KADENCE", CORD["app"], 1)
    away(j["lvl_o_out"], "→ JLAB HEADPHONES", CORD["out"], 1)
    away(j["spc_i_in"], "← JLAB MIC", CORD["mic"], 1)
    away(j["src_out"], "→ APPS THAT RECORD", CORD["in"], 1)
    return 5 * U + GAP


VARIANTS = [
    ("NOW", ["Seven devices, in the order of their keys;", "the program's own cords cross between them."], now),
    ("A · ONE FACE, EVERYTHING", ["One device per program: every node a pair of", "frames, in the order the sound goes; 4U."], every),
    ("B · ONE FACE, ITS EDGES", ["Only what reaches outside the program;", "INSIDE ▾ shows its own wiring, read-only."], edges),
    ("C · A CASE", ["The devices stay; a band with the program's", "name holds them together; ▾ folds it."], case),
    ("D · ONE FACE PER CHAIN", ["As A, split where the program splits itself", "(node.group); the rest a face each."], chains),
]

if __name__ == "__main__":
    PADO = 70
    col = PW + 2 * NUMS + 260
    W = PADO + len(VARIANTS) * col
    body = 320
    tallest = 0
    for i, (t, sub, fn) in enumerate(VARIANTS):
        x = PADO + i * col
        title(x, body - 90, t, sub)
        tallest = max(tallest, fn(x, body))
    ny = body + tallest + 150
    notes = [
        ("WHO IS ONE PROGRAM (the roof)", [
            "1. The same process: the client's pipewire.sec.pid (as apps' streams are joined today), with application.name. Easy Effects: client 199, all 7 nodes.",
            "2. application.id when the nodes carry it (com.github.wwmm.easyeffects on all 7).",
            "3. Modules PipeWire runs itself (filter-chain, loopback) have no program of their own: their node.link-group joins their two ends.",
            "Not: WirePlumber and PipeWire as programs: they make every card's and Bluetooth device's nodes; hardware stays by its device (device.id).",
        ]),
        ("A DAW", [
            "Ardour, Bitwig on JACK: one node with many ports, already one device today. Carla (a client per plugin): many nodes, one process: one roof.",
            "A program on PulseAudio with several streams (Teams, Audacity) is one device already: this extends that rule to everything a program makes.",
        ]),
    ]
    yy = ny
    for head, lines in notes:
        text(PADO, yy, head, 22, "#e58f2c", True, extra='letter-spacing="2"')
        yy += 36
        for ln in lines:
            text(PADO, yy, ln, 19, "#c9cbcd")
            yy += 30
        yy += 24
    H = yy + 30
    head = f'<rect width="{W}" height="{H}" fill="#1f2023"/>'
    head += f'<text x="{PADO}" y="70" {F} font-weight="bold" font-size="44" letter-spacing="3" fill="#e58f2c">ONE PROGRAM, ONE ROOF</text>'
    head += f'<text x="{PADO}" y="116" {F} font-size="24" fill="#9a9da1">Easy Effects as it runs here: its sink and source, a spectrum and a level meter on each side, a test signal. Dotted cords go to devices outside the picture.</text>'
    open(sys.argv[1] if len(sys.argv) > 1 else "roof.svg", "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">{head}{"".join(o)}</svg>\n')
