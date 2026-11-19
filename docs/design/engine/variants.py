# ENGINE selection and port variants, a sheet for choosing: python3 variants.py variants.svg
# Blocks are drawn as ENGINE draws them (src/ui/engine.rs: 244 wide, ports every 15 from 49.5).
import sys

NW, Y0 = 244, 49.5
APP, DEV, MET = "#9b6bd6", "#4f86d9", "#5fa36a"
F = 'font-family="Noto Sans"'
M = 'font-family="Noto Sans Mono"'
out = []


def shade(c, f):
    r, g, b = int(c[1:3], 16), int(c[3:5], 16), int(c[5:7], 16)
    m = (lambda v: v + (255 - v) * f) if f > 0 else (lambda v: v * (1 + f))
    return "#%02x%02x%02x" % tuple(round(m(v)) for v in (r, g, b))


def tw(t):
    return len(t) * 6.02


def text(x, y, t, size=11, fill="#9a9da1", mono=False, bold=False, anchor="start", ls=0):
    out.append(f'<text x="{x:.1f}" y="{y:.1f}" {M if mono else F} font-size="{size}" fill="{fill}"'
               f'{" font-weight=\"bold\"" if bold else ""} text-anchor="{anchor}"{f" letter-spacing=\"{ls}\"" if ls else ""}'
               f' xml:space="preserve">{t}</text>')


def py(i):
    return Y0 + 15 * i


def block(x, y, c, name, sub, ins, outs, busy, tag="", fade=1.0, ports=None):
    """A block at (x, y); `ports(side, i, name, px, py)` draws each port (side "in"/"out")."""
    n = max(len(ins), len(outs), 1)
    h = 40 + 15 * n + 8
    o = [f'<g transform="translate({x},{y})" opacity="{fade}">',
         f'<path d="M0,6 A6,6 0 0 1 6,0 H{NW-6} A6,6 0 0 1 {NW},6 V12 H0 Z" fill="{c}"/>',
         f'<rect x=".75" y="4" width="{NW-1.5}" height="{h-4.75}" rx="5.25" fill="#1d1e21"/>',
         f'<rect width="{NW}" height="{h}" rx="6" fill="none" stroke="{c}" stroke-width="1.5"/>',
         '<circle cx="12" cy="17" r="4" fill="#5cff7a"/>',
         f'<text x="22" y="21" {F} font-size="12" font-weight="bold" fill="#e6e7e8">{name}</text>',
         f'<text x="12" y="34" {M} font-size="10" fill="#7d8085">{sub}</text>',
         f'<text x="{NW-10}" y="21" text-anchor="end" {M} font-size="10" fill="#5cff7a">{busy}</text>']
    if tag:
        o.append(f'<text x="{NW-10}" y="34" text-anchor="end" {F} font-size="8.5" font-weight="bold" letter-spacing=".5" fill="#e58f2c">{tag}</text>')
    out.append("".join(o))
    for i, p in enumerate(ins):
        (ports or plain)(c, "in", i, p)
    for i, p in enumerate(outs):
        (ports or plain)(c, "out", i, p)
    out.append("</g>")
    return h


def plain(c, side, i, name, ring=None, fill=None, label=None, bold=False):
    x = 0 if side == "in" else NW
    lx, anchor = (10, "start") if side == "in" else (NW - 10, "end")
    if ring:
        out.append(f'<circle cx="{x}" cy="{py(i)}" r="7" fill="none" stroke="{ring}" stroke-width="2"/>')
    out.append(f'<circle cx="{x}" cy="{py(i)}" r="4" fill="{fill or c}" stroke="#0f1011" stroke-width="1.5"/>')
    out.append(f'<text x="{lx}" y="{py(i)+3.5}" text-anchor="{anchor}" {M} font-size="10" fill="{label or "#a8abae"}"{" font-weight=\"bold\"" if bold else ""}>{name}</text>')


def link(x1, y1, x2, y2, styles):
    """A link as ENGINE curves it, drawn once per style (stroke attributes)."""
    mx = (x1 + x2) / 2
    d = f"M{x1},{y1} C{mx},{y1} {mx},{y2} {x2},{y2}"
    for st in styles:
        out.append(f'<path d="{d}" fill="none" {st}/>')


def frame(x, y, w, h, title, note):
    out.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="10" fill="#fff" fill-opacity=".025" stroke="#2c2e33"/>')
    text(x + 16, y + 24, title, 12, "#e58f2c", True, True, ls=1)
    for k, line in enumerate(note.split("\n")):
        text(x + 16, y + 42 + 15 * k, line, 11, "#9a9da1")


# ---------------- ports: an ear from the edge (chosen) ----------------
Y = "#f7bb62"      # the LCD's amber: what is chosen
YT = "#3a2408"     # text on it, as on the LCD


def ear(c, side, i, name, state=""):
    """A port: an ear from the block's edge, square there, round inside. `state`: "" as it is,
    "hover" under the pointer, "on" chosen (filled amber), "end" an end of the chosen link
    (an amber edge)."""
    w = tw(name) + 20
    if side == "in":
        d = f"M0.75,{py(i)-6.5} H{w-6.5} A6.5,6.5 0 0 1 {w-6.5},{py(i)+6.5} H0.75 Z"
        lx, anchor, x = 10, "start", 0
    else:
        d = f"M{NW-0.75},{py(i)-6.5} H{NW-w+6.5} A6.5,6.5 0 0 0 {NW-w+6.5},{py(i)+6.5} H{NW-0.75} Z"
        lx, anchor, x = NW - 10, "end", NW
    if state == "on":
        out.append(f'<path d="{d}" fill="{Y}"/>')
    elif state == "end":
        out.append(f'<path d="{d}" fill="{c}" fill-opacity=".22" stroke="{Y}" stroke-width="1.5"/>')
    else:
        out.append(f'<path d="{d}" fill="{c}" fill-opacity="{.24 if state == "hover" else .10}"/>')
    out.append(f'<circle cx="{x}" cy="{py(i)}" r="4" fill="{Y if state in ("on", "end") else c}" stroke="#0f1011" stroke-width="1.5"/>')
    lab = {"on": YT, "end": "#f2f3f4", "hover": "#e6e7e8"}.get(state, "#a8abae")
    out.append(f'<text x="{lx}" y="{py(i)+3.5}" text-anchor="{anchor}" {M} font-size="10" fill="{lab}"{" font-weight=\"bold\"" if state in ("on", "end") else ""}>{name}</text>')


def ports_with(states):
    return lambda c, side, i, name: ear(c, side, i, name, states.get((side, i), ""))


DIM = 'stroke-width="2" opacity=".2"'

# ---------------- 2. a port chosen; a link chosen ----------------
W = 1420
text(30, 36, "2. CHOOSING A PORT, CHOOSING A LINK: TELLING THEM APART", 15, "#e58f2c", True, True, ls=1.5)
text(30, 56, "A port chosen: the port is what lights, filled amber; its links stay as they are (the rest fades). A link chosen: the link is what lights;", 12, "#9a9da1")
text(30, 72, "its two ports only get an amber edge. Amber is the LCD's. Ports are ears from the block's edge (3 · C).", 12, "#9a9da1")

y = 92
frame(30, y, W - 60, 300, "A PORT CHOSEN · THE SAME WITH EVERY WAY OF SHOWING A LINK", "Left: an output chosen (output_FL), its one link lit. Right: an input chosen (playback_FL),\nboth links into it lit; the ports at their other ends stay as they are.")
for k in range(2):
    ox, oy = 46 + k * 690, y + 96
    ax, bx = ox, ox + 380
    fy, ry = oy + py(0), oy + py(1)
    vf, ff = oy + 96 + py(0), oy + 96 + py(1)
    if k == 0:
        link(ax + NW, fy, bx, fy, [f'stroke="{APP}" stroke-width="2"'])
        link(ax + NW, ry, bx, ry, [f'stroke="{APP}" {DIM}'])
        link(ax + NW, vf, bx, fy, [f'stroke="{APP}" {DIM}'])
        link(ax + NW, ff, bx, ry, [f'stroke="{APP}" {DIM}'])
        sa, sb, fa, fb = {("out", 0): "on"}, {}, 1, .35
    else:
        link(ax + NW, fy, bx, fy, [f'stroke="{APP}" stroke-width="2"'])
        link(ax + NW, ry, bx, ry, [f'stroke="{APP}" {DIM}'])
        link(ax + NW, vf, bx, fy, [f'stroke="{APP}" stroke-width="2"'])
        link(ax + NW, ff, bx, ry, [f'stroke="{APP}" {DIM}'])
        sa, sb, fa, fb = {}, {("in", 0): "on"}, 1, 1
    block(ax, oy, APP, "Vivaldi", "186 · Stream/Output/Audio", [], ["output_FL", "output_FR"], "26µs", ports=ports_with(sa))
    block(ax, oy + 96, APP, "Firefox", "60 · Stream/Output/Audio", [], ["output_FL", "output_FR"], "31µs", fade=fb, ports=ports_with({}))
    block(bx, oy, DEV, "JLab GO Work 2", "202 · Audio/Sink", ["playback_FL", "playback_FR"], ["monitor_FL", "monitor_FR"], "214µs", "DRIVER", ports=ports_with(sb))
y += 316

LINK_VARIANTS = [
    ("A · THICKER, WITH AN AMBER CORE", "The link keeps its colour, twice as thick, an amber line along its middle.",
     lambda c: [f'stroke="{c}" stroke-width="4.6"', f'stroke="{Y}" stroke-width="1.6"']),
    ("B · AMBER", "The chosen link turns amber. Plain; while it is chosen, the colour of where it comes from is gone.",
     lambda c: [f'stroke="{Y}" stroke-width="2.8"']),
    ("C · OUTLINED, AS A ROUTE ON A MAP", "Its colour inside an amber edge, on a dark casing: stands out over other links crossing it.",
     lambda c: ['stroke="#0b0b0c" stroke-width="8"', f'stroke="{Y}" stroke-width="5.4"', f'stroke="{c}" stroke-width="2.8"']),
    ("D · RUNNING DASHES", "Its colour with amber dashes running along it, from the output to the input (moving in the app; still here).",
     lambda c: [f'stroke="{c}" stroke-width="3"', f'stroke="{Y}" stroke-width="1.6" stroke-dasharray="6 6"']),
]
for n, (title, note, style) in enumerate(LINK_VARIANTS):
    col = n % 2
    if col == 0 and n:
        y += 176
    x0 = 30 + col * (W - 60 + 16) / 2
    fw = (W - 60 - 16) / 2
    frame(x0, y, fw, 160, "A LINK CHOSEN · " + title, note)
    ax, bx = x0 + 16, x0 + 16 + 380
    oy = y + 70
    fy, ry = oy + py(0), oy + py(1)
    link(ax + NW, fy, bx, fy, style(APP))
    link(ax + NW, ry, bx, ry, [f'stroke="{APP}" {DIM}'])
    block(ax, oy, APP, "Vivaldi", "186 · Stream/Output/Audio", [], ["output_FL", "output_FR"], "26µs", ports=ports_with({("out", 0): "end"}))
    block(bx, oy, DEV, "JLab GO Work 2", "202 · Audio/Sink", ["playback_FL", "playback_FR"], ["monitor_FL", "monitor_FR"], "214µs", "DRIVER", ports=ports_with({("in", 0): "end"}))
y += 176

# ---------------- 3. the ears, larger ----------------
y += 24
text(30, y, "3 · C. A PORT IS AN EAR FROM THE EDGE: ITS NAME CLICKS AS ITS JACK DOES", 15, "#e58f2c", True, True, ls=1.5)
y += 16
STATES = [("", "as it is"), ("hover", "under the pointer"), ("on", "chosen"), ("end", "an end of the chosen link")]
for k, (st, what) in enumerate(STATES):
    x0 = 30 + k * 340
    out.append(f'<g transform="translate({x0},{y}) scale(1.3)">')
    block(0, 0, DEV, "JLab GO Work 2", "202 · Audio/Sink", ["playback_FL", "playback_FR"], ["monitor_FL", "monitor_FR"], "214µs", "DRIVER", ports=ports_with({("in", 0): st}))
    out.append("</g>")
    text(x0, y + 128, f"playback_FL: {what}", 11, "#9a9da1")
y += 150

H = y + 20
open(sys.argv[1], "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}"><rect width="{W}" height="{H}" fill="#1b1c1e"/>{"".join(out)}</svg>\n')
