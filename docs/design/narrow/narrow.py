# Narrow windows: what breaks now, and ways for the top panel, ENGINE's strip and the rest.
#   python3 narrow.py narrow.svg && rsvg-convert -z .5 narrow.svg -o narrow.png
# Pictures are the app's own, at a screen scale of 2.3 (parts/ are cut from a wide window: the
# panel's groups at 1:1). 1 logical unit = 2.3 px here.
import sys

S = 2.3
F = 'font-family="Noto Sans"'
M = 'font-family="Noto Sans Mono"'
o = []

# the parts' widths in px (parts/*.png, 150 high)
PW = {"logo": 470, "edit": 470, "layout": 760, "cables": 245, "view": 505, "cue": 235, "lcd": 1225, "metal": 22}
PH = 150
W1, W2 = 2070, 1242


def t(x, y, s, size=26, fill="#c9cbcd", bold=False, ls=0, anchor="start", mono=False):
    o.append(f'<text x="{x:.1f}" y="{y:.1f}" {M if mono else F} font-size="{size}" fill="{fill}"{" font-weight=\"bold\"" if bold else ""} text-anchor="{anchor}"{f" letter-spacing=\"{ls}\"" if ls else ""} xml:space="preserve">{s}</text>')


def title(x, y, s, note=""):
    t(x, y, s, 34, "#e58f2c", True, ls=3)
    if note:
        for k, line in enumerate(note.split("\n")):
            t(x, y + 40 + 34 * k, line, 24, "#9a9da1")


def img(href, x, y, w, h, crop=None):
    if crop:
        cx, cy, cw, ch = crop
        o.append(f'<svg x="{x}" y="{y}" width="{w}" height="{h}" viewBox="{cx} {cy} {cw} {ch}" preserveAspectRatio="none"><image href="{href}" width="{1250}" height="{1450}"/></svg>')
    else:
        o.append(f'<image href="{href}" x="{x}" y="{y}" width="{w}" height="{h}" preserveAspectRatio="none"/>')


def mark(n, x, y):
    o.append(f'<circle cx="{x}" cy="{y}" r="22" fill="#ff4a32" stroke="#111214" stroke-width="3"/>')
    t(x, y + 9, str(n), 26, "#fff", True, anchor="middle")


def metal(x, y, w, h=PH):
    img("parts/metal.png", x, y, w, h)


def part(name, x, y, w=None):
    img(f"parts/{name}.png", x, y, w or PW[name], PH)
    return x + (w or PW[name])


def key(x, y, w, label, led=False):
    """A dark key as on the panel, `label` on it."""
    o.append(f'<rect x="{x-4}" y="{y-4}" width="{w+8}" height="94" rx="12" fill="#050506" stroke="#55585b" stroke-width="2"/>')
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="86" rx="10" fill="#1f2022" stroke="#070708" stroke-width="2"/>')
    o.append(f'<rect x="{x+5}" y="{y+5}" width="{w-10}" height="30" rx="7" fill="#fff" fill-opacity=".06"/>')
    if led:
        o.append(f'<circle cx="{x+20}" cy="{y+20}" r="8" fill="#3a1512"/>')
    t(x + w / 2, y + 54, label, 28, "#e6e7e8", True, anchor="middle", ls=1)


def badge(x, y):
    """The logo, short: PC-1 over PATCHBAY."""
    t(x, y + 76, "PC-1", 64, "#ffffff", True, ls=2)
    t(x - 2, y + 74, "PC-1", 64, "#2a2b2e", True, ls=2)
    t(x, y + 112, "PATCHBAY", 20, "#2a2b2e", True, ls=3)


def frame(x, y, w, h, label):
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="none" stroke="#5a5c60" stroke-width="3" stroke-dasharray="14 10"/>')
    t(x, y - 12, label, 22, "#7d8085", True, ls=2)


Y = 60
title(40, Y, "NARROW WINDOWS · WHAT BREAKS NOW", "A window 1250 px wide at a screen scale of 2.3 (about 540 units: a rack's width), as on your screenshots.\nThe numbers are listed under the pictures.")
Y += 100
DECIDED = [
    "DECIDED · top panel: B, folding into MORE (logo first, then grey keys, then groups from the right); ALIGN is off in RACK",
    "DECIDED · ENGINE's strip: A, what matters first, the rest behind i; SAVE REPORT lives on DIAGNOSTICS' tab row; its key aligned",
    "DECIDED · the line about the open table: in the status bar while over the tabs · the status bar: the hints go first, then the left text ends in …",
]
for k, line in enumerate(DECIDED):
    t(40, Y + 34 * k, line, 24, "#5fd4a0", True)
Y += 34 * len(DECIDED) + 50
for k, (f, label) in enumerate([("desk", "DESK"), ("rack", "RACK"), ("engine", "ENGINE, a node chosen, the tables open")]):
    x = 40 + k * 1300
    t(x, Y - 10, label, 24, "#7d8085", True, ls=2)
    img(f"now-{f}.png", x, Y, 1250, 1450)
mark(1, 1210, Y + 70)
mark(1, 2510, Y + 70)
mark(1, 3810, Y + 70)
mark(2, 2600 + 900, Y + 200)
mark(3, 2600 + 650, Y + 285)
mark(4, 2600 + 1050, Y + 340)
mark(5, 2600 + 500, Y + 930)
mark(6, 2600 + 700, Y + 1420)
mark(7, 2600 + 1190, Y + 1000)
Y += 1500
ISSUES = [
    "1  The top panel is one row: past LAYOUT its keys are cut off. LOCK, ALIGN, CABLES, VIEW, CUE · LEVEL and the LCD (defaults, preset) can't be reached.",
    "2  ENGINE's strip drops the cells that don't fit, from the end: CLOCK, QUANTUM, DRIVERS, DSP LOAD, XRUNS go; SERVER and SESSION, the least needed, stay.",
    "3  ALSO SHOW's chips run on under the details: NOT RUNNING cut, INTERNAL gone.",
    "4  The details' tabs and rows are cut at the right: TIMING, and long values wrap into many lines.",
    "5  The tables' tabs and the line about the open table are drawn over each other.",
    "6  The status bar's two texts are drawn over each other.",
    "7  The tables' columns run off the right edge (W/Q, B/Q, ERR, FORMAT, NAME); the graph is left a sliver, drawn tiny.",
]
for k, s in enumerate(ISSUES):
    t(40, Y + 34 * k, s, 24, "#c9cbcd")
Y += 34 * len(ISSUES) + 90

# ---------------- the top panel: B, folding into MORE ----------------
PW.update({"layout3": 530, "lock": 116, "align": 116, "fit": 120})
title(40, Y, "THE TOP PANEL · B: WHAT DOESN'T FIT FOLDS INTO MORE", "When the panel is too wide for the window, in this order: 1. the logo goes, all of it;  2. grey keys go (what this screen can't use:\nLOCK and ALIGN in ENGINE, ALIGN in RACK, where it is off now; in ENGINE CABLES, CUE · LEVEL and VIEW but FIT too);  3. groups fold into MORE ▾ from the right:\nLCD, CUE · LEVEL, VIEW, CABLES, LOCK · ALIGN, EDIT. DESK · RACK · ENGINE never fold. MORE drops the folded groups below the panel, as they look.")
Y += 180


def panel_row(x, y, w):
    metal(x, y, w)
    o.append(f'<path d="M{x},{y+PH} H{x+w}" stroke="#000" stroke-width="2"/>')


def more(x, y):
    key(x, y + 26, 190, "MORE ▾")
    return x + 214


def lone(name, x, y):
    """A key on its own, its group's label left out."""
    img(f"parts/{name}.png", x, y, PW[name], 108 if name == "fit" else PH)
    return x + PW[name]


def lcd_half(x, y, half, w=640):
    """One half of the LCD: the defaults (0) or the preset and clock (1)."""
    cx = 0 if half == 0 else 600
    o.append(f'<svg x="{x}" y="{y}" width="{w}" height="{PH}" viewBox="{cx} 0 {625} {PH}" preserveAspectRatio="xMinYMid slice"><image href="parts/lcd.png" width="1225" height="{PH}"/></svg>')
    return x + w


SCREENS = [
    ("DESK", "desk", [("edit",), ("layout3", "desk"), ("lock",), ("align",), ("cables",), ("view",), ("cue",), ("lcd",)]),
    ("RACK", "rack", [("edit",), ("layout3", "rack"), ("lock",), ("cables",), ("view",), ("cue",), ("lcd",)]),
    ("ENGINE", "engine", [("edit",), ("layout3", "engine"), ("fit",), ("lcd",)]),
]


def draw_panel(x, y, w, parts):
    """Lays the parts out in order; what is past `w` (leaving room for MORE) folds. Returns the folded."""
    panel_row(x, y, w)
    widths = [PW[p[0]] for p in parts]
    keep = len(parts)
    while sum(widths[:keep]) + (214 if keep < len(parts) else 0) > w - 20:
        keep -= 1
    cx = x + 10
    for p in parts[:keep]:
        name = p[0] if len(p) == 1 else f"{p[0]}-{p[1]}"
        if p[0] in ("lock", "align", "fit"):
            if p[0] == "fit":
                metal(cx, y, PW["fit"])
            cx = lone(p[0], cx, y) if p[0] != "fit" else lone("fit", cx, y)
        else:
            img(f"parts/{name}.png", cx, y, PW[p[0]], PH)
            cx += PW[p[0]]
    if keep < len(parts):
        more(x + w - 214, y)
    return parts[keep:]


for screen, mode, parts in SCREENS:
    t(40, Y, screen, 26, "#e6e7e8", True, ls=2)
    Y += 24
    for k, (w, label) in enumerate([(2070, "900 UNITS"), (1242, "540 UNITS")]):
        x = 40 if k == 0 else 40 + 2070 + 120
        frame(x, Y + 20, w, PH, label)
        folded = draw_panel(x, Y + 20, w, parts)
        names = {"edit": "EDIT", "lock": "LOCK", "align": "ALIGN", "cables": "CABLES", "view": "VIEW", "cue": "CUE · LEVEL", "lcd": "LCD", "fit": "FIT"}
        t(x, Y + PH + 56, "in MORE: " + (" · ".join(names[p[0]] for p in reversed(folded)) or "nothing"), 20, "#9a9da1")
    Y += PH + 110

# MORE, open, on DESK at 540
t(40, Y, "MORE, OPEN · DESK AT 540 UNITS: THE FOLDED GROUPS DROP BELOW THE PANEL, IN THEIR ORDER, AS MANY ROWS AS THEY NEED", 24, "#e6e7e8", True, ls=1)
Y += 30
x0, w = 40, 1242
draw_panel(x0, Y, w, SCREENS[0][2])
o.append(f'<rect x="{x0 + 6}" y="{Y + PH + 6}" width="{w - 12}" height="{3 * PH + 24}" rx="14" fill="#0b0b0c" fill-opacity=".92"/>')
ty = Y + PH + 18
panel_row(x0 + 18, ty, w - 36)
cx = lone("lock", x0 + 30, ty)
cx = lone("align", cx, ty)
cx = part("cables", cx + 10, ty)
part("view", cx + 10, ty)
panel_row(x0 + 18, ty + PH, w - 36)
cx = part("cue", x0 + 30, ty + PH)
lcd_half(cx + 10, ty + PH, 0, 900)
panel_row(x0 + 18, ty + 2 * PH, w - 36)
lcd_half(x0 + 30, ty + 2 * PH, 1, 900)
t(x0 + 1290, Y + 200, "the LCD's two halves, each in its own row", 20, "#7d8085")
t(x0 + 1290, Y + 230, "when a row can't hold it whole", 20, "#7d8085")
Y += 4 * PH + 110

# ---------------- ENGINE's strip: A ----------------
title(40, Y, "ENGINE'S STRIP · A: WHAT MATTERS FIRST", "CLOCK, QUANTUM, DSP LOAD, XRUNS, HEALTH; SERVER, SESSION, ALLOWED, DRIVERS and OBJECTS behind the i cell (its card drops under it).\nSAVE REPORT is gone from here, for good: it is a key on DIAGNOSTICS' tab row.")
Y += 130


def cell(x, y, k, v, w, wide_v=False):
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="84" rx="8" fill="#e8a14a"/>')
    t(x + 18, y + 30, k, 20, "#3a2408", True, ls=2)
    t(x + 18, y + 70, v, 28, "#3a2408", True, mono=True)
    return x + w + 16


for k, (w, label) in enumerate([(2070, "900 UNITS: ALL BUT THE i CELL'S FIT; DSP LOAD'S CURVE TOO"), (1242, "540 UNITS")]):
    frame(40, Y + 20, w, 120, label)
    o.append(f'<rect x="40" y="{Y + 20}" width="{w}" height="120" fill="#1a1b1d"/>')
    x, y = 60, Y + 38
    x = cell(x, y, "CLOCK", "48000 Hz", 190)
    x = cell(x, y, "QUANTUM", "1024 · 32–2048", 270)
    if w > 1500:
        x = cell(x, y, "DSP LOAD", "1.5%", 420)
        o.append(f'<path d="M{x-300},{y+62} l20,-14 l14,10 l18,-22 l16,18 l20,-8 l22,12 l18,-16 l20,10 l26,-4 l30,8 l40,-6 l40,4" fill="none" stroke="#3a2408" stroke-width="3"/>')
        x = cell(x, y, "XRUNS", "0", 120)
        x = cell(x, y, "DRIVERS", "1 running", 210)
        x = cell(x, y, "SERVER", "PipeWire 1.6.9", 290)
    else:
        x = cell(x, y, "DSP LOAD", "1.5%", 170)
        x = cell(x, y, "XRUNS", "0", 120)
    o.append(f'<rect x="{x}" y="{y}" width="70" height="84" rx="8" fill="#e8a14a"/>')
    t(x + 35, y + 58, "i", 40, "#3a2408", True, anchor="middle")
    x += 86
    hx = 40 + w - 20 - 300
    o.append(f'<rect x="{hx}" y="{y}" width="300" height="84" rx="8" fill="#e8a14a"/>')
    t(hx + 18, y + 30, "HEALTH", 20, "#3a2408", True, ls=2)
    for j, (c, n) in enumerate([("#ff4a32", "0"), ("#ffb02e", "0"), ("#5fd4ff", "3")]):
        o.append(f'<circle cx="{hx + 30 + j * 86}" cy="{y + 62}" r="10" fill="{c}" stroke="#3a2408" stroke-opacity=".5" stroke-width="2"/>')
        t(hx + 46 + j * 86, y + 71, n, 28, "#3a2408", True, mono=True)
    Y += 200

# the DIAGNOSTICS key, aligned; SAVE REPORT on the tab row
t(40, Y, "DIAGNOSTICS · ITS KEY, ALIGNED; SAVE REPORT ON ITS TAB ROW", 24, "#e6e7e8", True, ls=1)
Y += 30
t(40, Y + 50, "now:", 22, "#7d8085")
img("now-key.png", 130, Y + 14, 420, 62)
t(600, Y + 50, "the chevron sits low and its text high: both on the key's middle, the same gap to the edges", 22, "#9a9da1")
Y += 120


def diag_key(x, y, label):
    w = len(label) * 14.5 + 76
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="56" rx="9" fill="#2a2b2e" stroke="#0b0b0c" stroke-width="2"/>')
    o.append(f'<path d="M{x+22},{y+33} l11,-11 l11,11" fill="none" stroke="#9a9da1" stroke-width="4" stroke-linecap="round" stroke-linejoin="round"/>')
    t(x + 58, y + 37, label, 23, "#e6e7e8", True, ls=1)
    return w


t(40, Y + 36, "fixed:", 22, "#7d8085")
diag_key(130, Y, "DIAGNOSTICS · 4 ISSUES")
Y += 100
o.append(f'<rect x="40" y="{Y}" width="{W2}" height="70" fill="#1e1f22"/>')
x = 54
for k, name in enumerate(["TOP", "ISSUES 4", "OBJECTS", "METADATA", "EVENTS"]):
    w = len(name) * 15 + 34
    if k == 0:
        o.append(f'<path d="M{x},{Y+6} H{x+w}" stroke="#e58f2c" stroke-width="4"/>')
    o.append(f'<rect x="{x}" y="{Y+10}" width="{w}" height="50" rx="6" fill="{"#2c2d31" if k == 0 else "#25262a"}"/>')
    t(x + w / 2, Y + 43, name, 21, "#e6e7e8" if k == 0 else "#9a9da1", True, anchor="middle", ls=1)
    x += w + 8
o.append(f'<rect x="{40 + W2 - 290}" y="{Y + 10}" width="200" height="50" rx="6" fill="#050506" stroke="#55585b" stroke-width="2"/>')
t(40 + W2 - 190, Y + 43, "SAVE REPORT", 19, "#e6e7e8", True, anchor="middle", ls=1)
o.append(f'<path d="M{40 + W2 - 62},{Y+28} l11,11 l11,-11" fill="none" stroke="#9a9da1" stroke-width="4" stroke-linecap="round" stroke-linejoin="round"/>')
t(40 + W2 + 20, Y + 44, "the tab row at 540 units: the tabs, SAVE REPORT, the fold key", 20, "#7d8085")
Y += 150

# ---------------- text that doesn't fit ----------------
title(40, Y, "TEXT THAT DOESN'T FIT", "Two lines ran over things: the line about the open table, and the status bar's two texts. At 540 units.")
Y += 130
HINT = "as pw-top: each driver and the nodes it clocks; the last half second's cycles, averaged, with the peak"


def tabrow(x, y, w, hint=None, info=False):
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="70" fill="#1e1f22"/>')
    cx = x + 14
    for k, name in enumerate(["TOP", "ISSUES 4", "OBJECTS", "METADATA", "EVENTS"]):
        bw = len(name) * 15 + 34
        o.append(f'<rect x="{cx}" y="{y+10}" width="{bw}" height="50" rx="6" fill="{"#2c2d31" if k == 0 else "#25262a"}"/>')
        t(cx + bw / 2, y + 43, name, 21, "#e6e7e8" if k == 0 else "#9a9da1", True, anchor="middle", ls=1)
        cx += bw + 8
    if info:
        o.append(f'<circle cx="{cx + 30}" cy="{y + 35}" r="17" fill="none" stroke="#9a9da1" stroke-width="3"/>')
        t(cx + 30, y + 44, "i", 24, "#e6e7e8", True, anchor="middle")
    return cx


t(40, Y, "THE LINE ABOUT THE OPEN TABLE", 24, "#e6e7e8", True, ls=1)
Y += 24
# 3: in the status bar
t(40, Y + 30, "CHOSEN · IN THE STATUS BAR, WHILE THE POINTER IS OVER THE TABS (AS FOR ANY KEY); THE TAB ROW HOLDS ONLY ITS TABS AND KEYS", 21, "#c9cbcd", True)
tabrow(40, Y + 44, W2)
o.append(f'<rect x="40" y="{Y + 130}" width="{W2}" height="44" fill="#141517"/>')
t(56, Y + 160, "TOP: as pw-top: each driver and the nodes it clocks; the last half…", 21, "#9a9da1", mono=True)
Y += 220

t(40, Y, "THE STATUS BAR", 24, "#e6e7e8", True, ls=1)
Y += 24
LEFT = "ENGINE · what PipeWire really has · 15 nodes · 37 ports · 11 links · read-only"
RIGHT = "click: inspect · drag: pan · wheel: zoom · F: fit · ?: legend · E: back to the desk"


def bar(y, left, right=""):
    o.append(f'<rect x="40" y="{y}" width="{W2}" height="44" fill="#141517"/>')
    t(56, y + 30, left, 21, "#9a9da1", mono=True)
    if right:
        t(40 + W2 - 16, y + 30, right, 21, "#7d8085", mono=True, anchor="end")


t(40, Y + 30, "CHOSEN · THE HINTS ON THE RIGHT GO FIRST, THEN THE LEFT TEXT ENDS IN …", 21, "#c9cbcd", True)
bar(Y + 44, "ENGINE · what PipeWire really has · 15 nodes · 37 ports…")
Y += 110
Y += 140

# ---------------- the rest ----------------
title(40, Y, "THE REST · ONE WAY EACH", "These are faults whichever way the panel goes.")
Y += 110
FIXES = [
    "3  ALSO SHOW's chips wrap onto a second row inside the graph's part.",
    "4  The details panel never takes more than half the window; in a window under 700 units it opens over the graph (graph full width under it).",
    "7  The tables keep the columns that say most (S, ID, BUSY, ERR, NAME) and drop the others from the right when narrow; the rest scroll sideways.",
]
for k, s_ in enumerate(FIXES):
    t(40, Y + 36 * k, s_, 24, "#c9cbcd")
Y += 36 * len(FIXES) + 60

W = 3900
open(sys.argv[1], "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{W}" height="{Y}" viewBox="0 0 {W} {Y}"><rect width="{W}" height="{Y}" fill="#111214"/>{"".join(o)}</svg>\n')
