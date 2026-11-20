# Listening, part two: choosing what to hear in ENGINE (its ports), and patching on DESK and RACK
# while CUE mode is on (the hole patches, the ring listens).
#   python3 cue2.py cue2.svg && rsvg-convert cue2.svg -o cue2.png
# The jacks, blocks and links are cue.py's (the app's own drawing); this file only lays them out.
import os
import sys

# cue.py's pieces: run it (into nothing), then draw afresh with its functions
HERE = os.path.dirname(os.path.abspath(__file__))
ns = {"__name__": "cue"}
argv, sys.argv = sys.argv, [sys.argv[0], os.devnull]
exec(open(os.path.join(HERE, "cue.py")).read(), ns)
sys.argv = argv
o = ns["o"]
o.clear()
t, title, jack, face, block, link, port_xy, num = (ns[k] for k in ("t", "title", "jack", "face", "block", "link", "port_xy", "num"))
F, M, CYAN, RED, C, NW, NH0, PR = (ns[k] for k in ("F", "M", "CYAN", "RED", "C", "NW", "NH0", "PR"))
GREEN, GOLD = "#78eb96", "#ffc85a"
W = 1500


def arrow(x, y, phones=None):
    """The pointer; `phones`: the headphones badge beside it, in that colour."""
    o.append(f'<path d="M{x},{y} l0,16 l4,-4 l3,7 l3,-1.5 l-3,-7 l5.5,0 z" fill="#f2f2f2" stroke="#000" stroke-width="1"/>')
    if phones:
        o.append(f'<use href="#phones" x="{x+22}" y="{y+22}" style="color:{phones}"/>')


def cross(x, y, k=1.0):
    """The crosshair pointer, centred on (x, y)."""
    a, b = 9 * k, 3 * k
    for sw, col in ((3.2 * k, "#000"), (1.4 * k, "#f2f2f2")):
        o.append(f'<path d="M{x-a},{y} H{x-b} M{x+b},{y} H{x+a} M{x},{y-a} V{y-b} M{x},{y+b} V{y+a}" stroke="{col}" stroke-width="{sw}" stroke-linecap="round"/>')


def status(x, y, w, left, right=""):
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="22" fill="#0e0f10"/>')
    t(x + 14, y + 15, left, 11.5, "#a8abae", mono=True)
    if right:
        t(x + w - 14, y + 15, right, 11.5, "#7d8085", mono=True, anchor="end")


def caption(x, y, lines, size=12.5, fill="#c9cbcd"):
    for k, s in enumerate(lines):
        t(x, y + k * 18, s, size, fill)


# ---------------- 1. ENGINE ----------------
y0 = 50
title(40, y0, "1 · CHOOSING WHAT TO HEAR IN ENGINE: ITS PORTS",
      "In CUE mode a port is heard while the pointer is on it and kept on with a click, as a jack on the desk; with Alt it is heard while Alt is held.\n"
      "One port is one channel: FL in the left ear, FR in the right, anything else (MONO, AUX…) in both. An input port: what comes into it.")

gx, gy, gw, gh = 40, y0 + 66, W - 80, 560
o.append(f'<rect x="{gx}" y="{gy}" width="{gw}" height="{gh}" fill="#1b1c1e" stroke="#3a3c40"/>')
bx, by, bw, bh = gx + 20, gy + 20, gw - 40, 290
o.append(f'<rect x="{bx}" y="{by}" width="{bw}" height="{bh}" rx="10" fill="#fff" fill-opacity=".025" stroke="#4f86d9" stroke-opacity=".35" stroke-width="1.2"/>')
o.append(f'<text x="{bx+26}" y="{by+22}" {M} font-weight="bold" font-size="12" fill="#c9cbcd" xml:space="preserve"><tspan fill="#e58f2c" letter-spacing="1">DRIVER</tspan>  JLab GO Work 2  <tspan fill="#7d8085" font-weight="normal">Audio/Sink #183 · 48000 Hz · QUANT 256 · 5.3 ms</tspan></text>')
c0, c1, c2 = bx + 26, bx + 26 + 364, bx + 26 + 2 * 364
teams = dict(x=c0, y=by + 50, c=C["app"], desc="Teams for Linux", id=163, cls="Stream/Output/Audio", busy="9µs", ins=[], outs=[("output_MONO", "pinned")])
viv = dict(x=c0, y=by + 130, c=C["app"], desc="Vivaldi", id=211, cls="Stream/Output/Audio", busy="38µs", ins=[], outs=["output_FL", ("output_FR", "hover")])
synth = dict(x=c0, y=by + 225, c="#e08a3c", desc="BLE MIDI 1", id=47, cls="Midi/Bridge", ins=[], outs=["out"])
jl = dict(x=c1, y=by + 50, c=C["device"], desc="JLab GO Work 2", id=183, cls="Audio/Sink", tag="DRIVER", busy="31µs", ins=["playback_FL", "playback_FR"], outs=[("monitor_FL", "bad"), "monitor_FR"])
rec = dict(x=c1, y=by + 150, c=C["app"], desc="Recorder", id=190, cls="Stream/Input/Audio", busy="12µs", ins=[("input_FL", "pinned"), ("input_FR", "pinned")], outs=[])
met = dict(x=c2, y=by + 50, c=C["patchcord"], desc="Patchcord signal meter", id=74, cls="Stream/Input/Audio", tag="PATCHCORD", busy="8µs", ins=["input_FL", "input_FR"], outs=[])

link(port_xy(teams, True, 0), port_xy(jl, False, 0), C["app"])
link(port_xy(teams, True, 0), port_xy(jl, False, 1), C["app"])
link(port_xy(viv, True, 0), port_xy(jl, False, 0), C["app"])
link(port_xy(viv, True, 1), port_xy(jl, False, 1), C["app"])
link(port_xy(viv, True, 0), port_xy(rec, False, 0), C["app"])
link(port_xy(viv, True, 1), port_xy(rec, False, 1), C["app"])
link(port_xy(jl, True, 0), port_xy(met, False, 0), C["device"], "passive")
link(port_xy(jl, True, 1), port_xy(met, False, 1), C["device"], "passive")


def red_frame(n, out, i):
    """A port that would feed back: a red frame, as a red ring on the desk."""
    x, y = port_xy(n, out, i)
    name = n["outs" if out else "ins"][i]
    name = name[0] if isinstance(name, tuple) else name
    w = len(name) * 6.02 + 20
    # the port's own shape (square at the block's edge, round inside), as the cyan frame
    if out:
        d = f"M{x-.75},{y-6.5} H{x-w+6.5} A6.5,6.5 0 0 0 {x-w+6.5},{y+6.5} H{x-.75} Z"
    else:
        d = f"M{x+.75},{y-6.5} H{x+w-6.5} A6.5,6.5 0 0 1 {x+w-6.5},{y+6.5} H{x+.75} Z"
    o.append(f'<path d="{d}" fill="none" stroke="{RED}" stroke-width="1.6"/><circle cx="{x}" cy="{y}" r="5.6" fill="none" stroke="{RED}" stroke-width="1.4"/>')


for n in (teams, viv, synth, jl, rec, met):
    # (cue.py's block knows "pinned" and "hover"; the red one is drawn over a plain port)
    if n is jl:
        n = dict(n, outs=["monitor_FL", "monitor_FR"])
    block(n)
red_frame(jl, True, 0)


def phones_on(n, out, i):
    """The headphones on a port kept on (as the app draws it now)."""
    x, y = port_xy(n, out, i)
    name = n["outs" if out else "ins"][i]
    name = name[0] if isinstance(name, tuple) else name
    w = len(name) * 6.02 + 20
    hx = x - w - 9 if out else x + w + 9
    o.append(f'<g transform="translate({hx},{y}) scale(.6)"><use href="#phones" style="color:{CYAN}"/></g>')


phones_on(teams, True, 0)
phones_on(rec, False, 0)
phones_on(rec, False, 1)

# the pointer on Vivaldi's output_FR, in CUE mode
px, py = port_xy(viv, True, 1)
arrow(px + 6, py + 2, CYAN)
num(1, c0 + NW + 22, teams["y"] + 49 - 18)
num(2, px + 54, py + 30)
num(3, c1 + NW + 22, rec["y"] + 49 - 18)
num(4, c1 + NW - 70, jl["y"] + 49 + 34)
num(5, c0 + NW + 22, synth["y"] + 49)
num(6, c2 + NW + 22, met["y"] + 49 - 18)
caption(bx + 26, by + bh + 30, [
    "1  kept on (clicked in CUE mode): the whole port cyan, as now. MONO: heard in both ears.",
    "2  under the pointer in CUE mode (or with Alt held): a cyan frame, the headphones by the pointer; heard at once. Vivaldi's output_FR: the right ear only.",
    "3  an input port kept on: what comes into it (here Vivaldi's output_FL, left ear; its FR on input_FR, right ear): a stereo pair, two clicks.",
    "4  the output listened on (or its monitor) would feed back: a red frame, can't be kept, as the red ring on the desk.",
    "5  nothing to hear: MIDI, video, Patchcord's own nodes (6). No frame, the pointer as it is; the status bar says why.",
    "Not in CUE mode, a click selects for the details, as now. In CUE mode a click on a node or a link (not a port) still selects it.",
])
status(gx, gy + gh - 22, gw, f'<tspan fill="{CYAN}">CUE ▸</tspan> Vivaldi · output_FR (right)   + kept: Teams for Linux · output_MONO, Recorder · input_FL + input_FR',
       "click: keep it on  ·  CUE or Esc stops all")

# ---------------- 2. DESK and RACK ----------------
y1 = gy + gh + 70
title(40, y1, "2 · PATCHING IN CUE MODE ON DESK AND RACK: THE HOLE PATCHES, THE RING LISTENS",
      "In CUE mode the middle of a jack, its hole (where the plug goes), does what it does outside CUE mode: pull a cord out, move it, plug a new one.\n"
      "The rest of the jack, round the hole, listens as now. Alt stays the quick listen: while it is held the whole jack listens.")
fy = y1 + 92
face(40, fy, W - 80, 300, "metal")


def cord_into(x, y, k, dx=-80, dy=150, colour="#c8463c"):
    """A cord's end straight in the jack, hanging out of it down to one side."""
    o.append(f'<path d="M{x},{y} C{x},{y+40*k} {x+dx*k*.4},{y+dy*k*.8} {x+dx*k},{y+dy*k}" fill="none" stroke="#000" stroke-opacity=".35" stroke-width="{11*k}" transform="translate({3*k},{6*k})" stroke-linecap="round"/>')
    o.append(f'<path d="M{x},{y} C{x},{y+40*k} {x+dx*k*.4},{y+dy*k*.8} {x+dx*k},{y+dy*k}" fill="none" stroke="{colour}" stroke-width="{9*k}" stroke-linecap="round"/>')
    o.append(f'<circle cx="{x}" cy="{y}" r="{5.2*k}" fill="#2a2b2e"/>')


K = 3
cols = [("THE ZONES", "zones"), ("POINTER ON THE RING", "ring"), ("POINTER IN THE HOLE", "hole"), ("A PLUG IN THE HAND", "drag")]
cw = (W - 80) / 4
for i, (name, what) in enumerate(cols):
    cx = 40 + i * cw + cw / 2
    cy = fy + 120
    t(cx, fy - 12, name, 11.5, "#e6e7e8", True, ls=1.2, anchor="middle")
    if what == "zones":
        jack(cx, cy, K, "plain", label="OUTPUT")
        # the hole's zone (patch) and the ring's (listen)
        o.append(f'<circle cx="{cx}" cy="{cy}" r="{8*K}" fill="#fff" fill-opacity=".14" stroke="#f2f2f2" stroke-width="1.6" stroke-dasharray="4 3"/>')
        o.append(f'<circle cx="{cx}" cy="{cy}" r="{20*K}" fill="none" stroke="{CYAN}" stroke-width="1.6" stroke-dasharray="4 3"/>')
        t(cx + 20 * K + 10, cy - 40, "listen", 12, "#1d1e20", True)
        t(cx + 20 * K + 10, cy - 24, "the ring", 12, "#1d1e20")
        o.append(f'<path d="M{cx+20*K+8},{cy-36} L{cx+16*K},{cy-24}" stroke="#1d1e20" stroke-width="1.2"/>')
        t(cx - 20 * K - 10, cy + 60, "patch", 12, "#1d1e20", True, anchor="end")
        t(cx - 20 * K - 10, cy + 76, "the hole", 12, "#1d1e20", anchor="end")
        o.append(f'<path d="M{cx-20*K-8},{cy+62} L{cx-6*K},{cy+5*K}" stroke="#1d1e20" stroke-width="1.2"/>')
    elif what == "ring":
        jack(cx, cy, K, "hover", label="OUTPUT")
        arrow(cx + 13 * K, cy + 4 * K, CYAN)
    elif what == "hole":
        jack(cx, cy, K, "pinned", label="OUTPUT")
        cord_into(cx, cy, K * .55, dx=-40, dy=70)
        cross(cx + 2, cy + 3, 1.4)
    else:
        # a plug pulled out of the kept jack: it stays kept and heard as it was; the free jacks it fits ring green
        jack(cx - 70, cy, 2, "pinned", label="OUTPUT")
        jack(cx + 60, cy - 10, 2, "plain", label="INPUT")
        o.append(f'<circle cx="{cx+60}" cy="{cy-10}" r="{20*2}" fill="none" stroke="{GREEN}" stroke-width="3"/>')
        cord_into(cx - 70, cy, 1.1, dx=60, dy=40, colour="#c8463c")
        o.append(f'<path d="M{cx-70+66},{cy+44} C{cx+10},{cy+60} {cx+40},{cy+30} {cx+40},{cy+18}" fill="none" stroke="#c8463c" stroke-width="10" stroke-linecap="round"/>')
        o.append(f'<circle cx="{cx+40}" cy="{cy+16}" r="7" fill="#d9d9db" stroke="#333" stroke-width="1.2"/>')
        cross(cx + 40, cy + 16, 1.2)
caption(40, fy + 300 + 26, [
    "THE ZONES: the hole's, a little wider than the hole (8 units, and never under 8 pixels on the screen, however far out the view is), patches;",
    "the ring round it, out to the jack's edge, listens. The rest of the face and the cords' bends do what they do outside CUE mode.",
    "POINTER ON THE RING: heard at once, a click keeps it on, the headphones by the pointer: as now.",
    "POINTER IN THE HOLE: the crosshair; what is heard doesn't change. Drag: the plug comes out and moves; on a free jack, a new cord. A click alone does nothing.",
    "A PLUG IN THE HAND: nothing new is heard while it moves (no clicks of other jacks in the ears); the jacks it fits ring green, as always.",
    "Let go into a jack: the patch changes, and what is heard follows it (a kept input now hears what was plugged into it).",
], 12.5)
status(40, fy + 300 + 26 + 6 * 18 + 14, W - 80, f'<tspan fill="{CYAN}">CUE ▸</tspan> drag: pull this cord out and plug it elsewhere  ·  the ring around: listen',
       "CUE or Esc stops all")

H = fy + 300 + 26 + 6 * 18 + 14 + 22 + 40
open(sys.argv[1], "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{W}" height="{H}" viewBox="0 0 {W} {H}">{ns["DEFS"]}<rect width="{W}" height="{H}" fill="#111214"/>{"".join(o)}</svg>\n')
