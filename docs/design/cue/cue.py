# Listening (cue): how a jack looks heard on DESK and RACK, and listening in ENGINE.
#   python3 cue.py cue.svg && rsvg-convert cue.svg -o cue.png
# The jacks are the app's own (face.rs: the hex nut, its shadow, washer, chrome), the blocks as
# engine.rs draws them; sizes in world units, the jacks shown 3× and 1×.
import math
import sys

F = 'font-family="Noto Sans"'
M = 'font-family="Noto Sans Mono"'
CYAN = "#5fd4ff"
RED = "#ff503c"
o = []


def t(x, y, s, size=13, fill="#c9cbcd", bold=False, ls=0, anchor="start", mono=False, op=1):
    o.append(f'<text x="{x:.1f}" y="{y:.1f}" {M if mono else F} font-size="{size}" fill="{fill}"{" font-weight=\"bold\"" if bold else ""} text-anchor="{anchor}"{f" letter-spacing=\"{ls}\"" if ls else ""}{f" opacity=\"{op}\"" if op < 1 else ""} xml:space="preserve">{s}</text>')


def title(x, y, s, note=""):
    t(x, y, s, 17, "#e58f2c", True, ls=1.5)
    for k, line in enumerate(note.split("\n") if note else []):
        t(x, y + 22 + 18 * k, line, 13, "#9a9da1")


HEX = " ".join(f"{15 * math.cos(math.radians(k * 60 + 30)):.2f},{15 * math.sin(math.radians(k * 60 + 30)):.2f}" for k in range(6))
DEFS = f'''<defs>
<pattern id="pt" width="80" height="80" patternUnits="userSpaceOnUse"><image href="../../../assets/paint.png" width="80" height="80"/></pattern>
<linearGradient id="gNut" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#f6f7f8"/><stop offset=".45" stop-color="#b6b9bd"/><stop offset="1" stop-color="#55585d"/></linearGradient>
<radialGradient id="gWasher" cx=".4" cy=".35" r=".75"><stop offset="0" stop-color="#5e6166"/><stop offset="1" stop-color="#141517"/></radialGradient>
<radialGradient id="gChrome" cx=".35" cy=".3" r=".85"><stop offset="0" stop-color="#fff"/><stop offset=".4" stop-color="#b9bcc1"/><stop offset=".75" stop-color="#65686d"/><stop offset="1" stop-color="#a6a9ad"/></radialGradient>
<radialGradient id="gHole" cx=".5" cy=".62" r=".6"><stop offset="0" stop-color="#000"/><stop offset=".7" stop-color="#0a0a0b"/><stop offset="1" stop-color="#3c3d40"/></radialGradient>
<radialGradient id="halo" r=".5"><stop offset=".48" stop-color="{CYAN}" stop-opacity="0"/><stop offset=".62" stop-color="{CYAN}" stop-opacity=".45"/><stop offset="1" stop-color="{CYAN}" stop-opacity="0"/></radialGradient>
<filter id="nutShadow" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="1.2"/></filter>
<polygon id="hexShape" points="{HEX}"/>
<g id="hex"><use href="#hexShape" fill="#000" opacity=".4" transform="translate(1.2,2.4)" filter="url(#nutShadow)"/><use href="#hexShape" fill="url(#gNut)" stroke="#43454a" stroke-width=".8"/></g>
<g id="jack"><use href="#hex"/><circle r="11.4" fill="url(#gWasher)"/><circle r="8.7" fill="url(#gChrome)" stroke="#2a2b2e" stroke-width=".8"/><circle r="5.5" fill="url(#gHole)"/></g>
<g id="jack-stereo"><use href="#jack"/><circle r="10.1" fill="none" stroke="#c9a24a" stroke-width="1.3"/></g>
<g id="phones"><circle r="8.5" fill="#121315" stroke="currentColor" stroke-width="1.4"/><g transform="scale(.4583) translate(-12,-12.5)" fill="none" stroke="currentColor" stroke-width="2.4"><path d="M4 15v-3a8 8 0 0 1 16 0v3"/><rect x="3" y="14" width="4.5" height="7" stroke="none" fill="currentColor"/><rect x="16.5" y="14" width="4.5" height="7" stroke="none" fill="currentColor"/></g></g>
</defs>'''


def face(x, y, w, h, finish):
    base = {"metal": "#8b8d90", "black": "#26272a"}[finish]
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="6" fill="{base}"/>')
    if finish == "metal":
        o.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="6" fill="url(#pt)" opacity=".9"/>')


def jack(x, y, k, state, stereo=False, label="", print_="#1d1e20"):
    """A jack at (x, y), k times its size. state: plain, now-hover, now-pinned, hover, pinned, bad."""
    g = [f'<g transform="translate({x},{y}) scale({k})">']
    c = RED if state == "bad" else CYAN
    if state == "now-hover":
        g.append(f'<circle r="26" fill="none" stroke="{CYAN}" stroke-opacity=".16" stroke-width="8"/>')
    if state == "now-pinned":
        g.append('<circle r="34" fill="url(#halo)"/>')
    if state == "pinned":
        # the fill from the ring to the jack, under the jack: its nut's shadow falls on it
        g.append(f'<circle r="21.1" fill="{c}"/><circle r="21.1" fill="none" stroke="#000" stroke-opacity=".35" stroke-width=".8"/>')
    g.append(f'<use href="#{"jack-stereo" if stereo else "jack"}"/>')
    if state in ("now-hover", "now-pinned", "hover", "bad"):
        w = 3.2 if state == "now-pinned" else 2.2
        g.append(f'<circle r="20" fill="none" stroke="{c}" stroke-width="{w}"/>')
    if state != "plain":
        g.append(f'<use href="#phones" x="17" y="-17" style="color:{c}"/>')
    g.append("</g>")
    o.append("".join(g))
    if label:
        t(x, y + 30 * k, label, min(9 * k, 15), print_, True, ls=1.2, anchor="middle")


W = 1500

# ---------------- 1. DESK and RACK ----------------
y0 = 50
title(40, y0, "1 · A JACK HEARD ON DESK AND RACK",
      "Hovered with Alt or in CUE mode: a ring, nothing else (no glow, nothing pulses). Clicked in CUE mode, heard until clicked again:\n"
      "the ring fills in up to the jack; the jack lies on it with its nut's shadow. Red: the output you listen on (it would feed back).")

cols = [("NOW · HOVERED", "now-hover", "a ring in a pulsing glow"), ("NOW · KEPT ON", "now-pinned", "a soft glow"),
        ("HOVERED · ALT OR CUE", "hover", "a ring"), ("KEPT ON · CLICKED IN CUE", "pinned", "filled to the jack"),
        ("WOULD FEED BACK", "bad", "a red ring, can't be kept")]
cx0, cw = 40, 284
for row, (finish, print_) in enumerate([("metal", "#1d1e20"), ("black", "#d5d7d9")]):
    ry = y0 + 96 + row * 250
    face(cx0, ry, cw * 5 - 14, 228, finish)
    for i, (name, st, note) in enumerate(cols):
        x = cx0 + i * cw + cw / 2 - 7
        if row == 0:
            t(x, ry - 12, name, 11.5, "#8a8d91" if st.startswith("now") else "#e6e7e8", True, ls=1.2, anchor="middle")
        # 3× and 1×
        jack(x - 40, ry + 100, 3, st, stereo=(i % 2 == 1), label="OUTPUT" if row == 0 else "PLAYBACK", print_=print_)
        jack(x + 92, ry + 100, 1, st, stereo=(i % 2 == 1), label="1×", print_=print_)
        if row == 1:
            t(x, ry + 250, note, 12, "#7d8085" if st.startswith("now") else "#c9cbcd", anchor="middle")
    if row == 0:
        # the old two greyed: what goes
        o.append(f'<path d="M{cx0 + 2 * cw - 7},{ry - 4} V{ry + 480}" stroke="#e58f2c" stroke-width="1.5" stroke-dasharray="6 6"/>')
        t(cx0 + cw - 7, ry - 30, 'NOW: GOES', 11.5, '#e58f2c', True, ls=1.2, anchor='middle')
        t(cx0 + 3.5 * cw - 7, ry - 30, 'NEW', 11.5, '#e58f2c', True, ls=1.2, anchor='middle')

ny = y0 + 96 + 2 * 250 + 30
for k, line in enumerate([
    "Drawing it: two pictures made once (the ring, the filled disc), laid as sprites on the GPU like the jacks: the filled disc under the jack's",
    "picture, the ring and the headphones over it. Nothing pulses, so nothing is drawn again while listening: the cords' layer no longer redraws for the cue.",
]):
    t(40, ny + k * 18, line, 13, "#9a9da1")

# ---------------- 2. ENGINE ----------------
NW, NH0, PR = 244.0, 40.0, 15.0
C = {"device": "#4f86d9", "app": "#9b6bd6", "patchcord": "#5fa36a", "cue": CYAN}


def ear_w(s):
    return len(s) * 6.02 + 20


def node_h(n):
    return NH0 + max(len(n["ins"]), len(n["outs"]), 1) * PR + 8


def port_xy(n, out, i):
    return (n["x"] + (NW if out else 0), n["y"] + NH0 + i * PR + PR / 2 + 2)


def block(n):
    x, y, c = n["x"], n["y"], n["c"]
    nh = node_h(n)
    s = [f'<g transform="translate({x},{y})">',
         f'<path d="M0,6 A6,6 0 0 1 6,0 H{NW-6} A6,6 0 0 1 {NW},6 V12 H0 Z" fill="{c}"/><rect x=".75" y="4" width="{NW-1.5}" height="{nh-4.75}" rx="5.25" fill="#1d1e21"/>',
         f'<rect width="{NW}" height="{nh}" rx="6" fill="none" stroke="{c}" stroke-width="1.5"/><circle cx="12" cy="17" r="4" fill="#5cff7a"/>',
         f'<text x="22" y="21" {F} font-size="12" font-weight="bold" fill="#e6e7e8">{n["desc"]}</text>',
         f'<text x="12" y="34" {M} font-size="10" fill="#7d8085">{n["id"]} · {n["cls"]}</text>']
    if n.get("tag"):
        s.append(f'<text x="{NW-10}" y="34" text-anchor="end" {F} font-size="8.5" font-weight="bold" letter-spacing=".5" fill="#e58f2c">{n["tag"]}</text>')
    if n.get("busy"):
        s.append(f'<text x="{NW-10}" y="21" text-anchor="end" {M} font-size="10" fill="#5cff7a">{n["busy"]}</text>')
    for out, ports in ((False, n["ins"]), (True, n["outs"])):
        for i, p in enumerate(ports):
            name, st = p if isinstance(p, tuple) else (p, "")
            py = NH0 + i * PR + PR / 2 + 2
            w = ear_w(name)
            if out:
                px, tx, anc = NW, NW - 10, ' text-anchor="end"'
                d = f"M{NW-.75},{py-6.5} H{NW-w+6.5} A6.5,6.5 0 0 0 {NW-w+6.5},{py+6.5} H{NW-.75} Z"
            else:
                px, tx, anc = 0, 10, ""
                d = f"M.75,{py-6.5} H{w-6.5} A6.5,6.5 0 0 1 {w-6.5},{py+6.5} H.75 Z"
            if st == "pinned":
                # the whole port in the cue's cyan, its name dark on it (as a chosen one on amber)
                s.append(f'<path d="{d}" fill="{CYAN}"/><circle cx="{px}" cy="{py}" r="4" fill="{CYAN}" stroke="#0f1011" stroke-width="1.5"/><text x="{tx}" y="{py+3.5}"{anc} {M} font-size="10" font-weight="bold" fill="#062533">{name}</text>')
            elif st == "hover":
                # only a frame round the port
                s.append(f'<path d="{d}" fill="{c}" fill-opacity=".1" stroke="{CYAN}" stroke-width="1.6"/><circle cx="{px}" cy="{py}" r="4" fill="{c}" stroke="#0f1011" stroke-width="1.5"/><circle cx="{px}" cy="{py}" r="5.6" fill="none" stroke="{CYAN}" stroke-width="1.4"/><text x="{tx}" y="{py+3.5}"{anc} {M} font-size="10" fill="#e6e7e8">{name}</text>')
            else:
                s.append(f'<path d="{d}" fill="{c}" fill-opacity=".1"/><circle cx="{px}" cy="{py}" r="4" fill="{c}" stroke="#0f1011" stroke-width="1.5"/><text x="{tx}" y="{py+3.5}"{anc} {M} font-size="10" fill="#a8abae">{name}</text>')
    s.append("</g>")
    o.append("".join(s))


def link(a, b, colour, kind="active"):
    (x1, y1), (x2, y2) = a, b
    dx = max(60, (x2 - x1) * 0.5)
    st = {"active": 'stroke-width="2"', "passive": 'stroke-width="1.2" stroke-dasharray="3 4"'}[kind]
    o.append(f'<path d="M{x1},{y1} C{x1+dx},{y1} {x2-dx},{y2} {x2},{y2}" fill="none" stroke="{colour}" {st}/>')


ey = ny + 80
title(40, ey, "2 · LISTENING IN ENGINE: THE GRAPH AS IT IS",
      "The cue is drawn as the nodes it really is, in cyan: Patchcord's loopback (what is heard goes in) and its other side (out into the default output).\n"
      "A port hovered (Alt, or in CUE mode): a cyan frame round it. A port kept on (clicked in CUE mode): the whole port cyan. One port is one channel, heard in its place.")

# a window: panel strip (just a hint), graph, status bar
gx, gy, gw, gh = 40, ey + 66, W - 80, 620
o.append(f'<rect x="{gx}" y="{gy}" width="{gw}" height="{gh}" fill="#1b1c1e" stroke="#3a3c40"/>')
# the driver's band
bx, by, bw, bh = gx + 20, gy + 20, gw - 40, gh - 70
o.append(f'<rect x="{bx}" y="{by}" width="{bw}" height="{bh}" rx="10" fill="#fff" fill-opacity=".025" stroke="#4f86d9" stroke-opacity=".35" stroke-width="1.2"/>')
o.append(f'<text x="{bx+26}" y="{by+22}" {M} font-weight="bold" font-size="12" fill="#c9cbcd" xml:space="preserve"><tspan fill="#e58f2c" letter-spacing="1">DRIVER</tspan>  JLab GO Work 2  <tspan fill="#7d8085" font-weight="normal">sink #282 · 48000 Hz · QUANT 1024 · 21.3 ms · DSP 0.4%</tspan></text>')

c0, c1, c2 = bx + 40, bx + 40 + 364 + 40, bx + 40 + 2 * (364 + 40)
viv = dict(x=c0, y=by + 70, c=C["app"], desc="Vivaldi", id=211, cls="Stream/Output/Audio", busy="38µs", ins=[], outs=[("output_FL", "pinned"), "output_FR"])
mic = dict(x=c0, y=by + 250, c=C["device"], desc="Ryzen HD Audio Controller", id=64, cls="Audio/Source", busy="21µs", ins=[], outs=[("capture_FL", "hover"), "capture_FR"])
cue = dict(x=c1, y=by + 190, c=C["cue"], desc="Patchcord cue", id=318, cls="Stream/Input/Audio", tag="PATCHCORD", busy="9µs", ins=["input_FL", "input_FR"], outs=[])
cout = dict(x=c1, y=by + 330, c=C["cue"], desc="Patchcord cue", id=319, cls="Stream/Output/Audio", tag="PATCHCORD", busy="12µs", ins=[], outs=["output_FL", "output_FR"])
jl = dict(x=c2, y=by + 70, c=C["device"], desc="JLab GO Work 2", id=282, cls="Audio/Sink", tag="DRIVER", busy="54µs", ins=["playback_FL", "playback_FR"], outs=["monitor_FL", "monitor_FR"])
met = dict(x=c2 + 0, y=by + 200, c=C["patchcord"], desc="Patchcord signal meter", id=72, cls="Stream/Input/Audio", tag="PATCHCORD", busy="6µs", ins=["input_FL", "input_FR"], outs=[])

# the links, under the blocks: as always in the colour they come from
link(port_xy(viv, True, 0), port_xy(jl, False, 0), C["app"])
link(port_xy(viv, True, 1), port_xy(jl, False, 1), C["app"])
link(port_xy(viv, True, 0), port_xy(cue, False, 0), C["app"], "passive")      # left, heard left
link(port_xy(mic, True, 0), port_xy(cue, False, 0), C["device"], "passive")   # left, heard left
link(port_xy(cout, True, 0), port_xy(jl, False, 0), CYAN)
link(port_xy(cout, True, 1), port_xy(jl, False, 1), CYAN)
for n in (viv, mic, cue, cout, jl):
    block(n)
# the badge by the pointer, as now with Alt or in CUE mode
px, py = port_xy(mic, True, 0)
px += 8
o.append(f'<path d="M{px},{py+4} l0,16 l4,-4 l3,7 l3,-1.5 l-3,-7 l5.5,0 z" fill="#f2f2f2" stroke="#000" stroke-width="1"/>')
o.append(f'<use href="#phones" x="{px+24}" y="{py+24}" style="color:{CYAN}"/>')

# numbered marks, explained under the graph
def num(n, x, y):
    o.append(f'<circle cx="{x}" cy="{y}" r="10" fill="#e58f2c" stroke="#111214" stroke-width="2"/>')
    t(x, y + 4.5, str(n), 12, "#111214", True, anchor="middle")

num(1, c0 + NW + 20, viv["y"] + 49 - 16)
num(2, c0 + NW - 100, mic["y"] + 49 + 22)
num(3, c1 + NW + 18, cue["y"] + 12)
num(4, c1 + NW + 18, cout["y"] + 12)
num(5, c1 - 40, cue["y"] - 22)
for k, s_ in enumerate([
    "1  kept on (clicked in CUE mode): the whole port cyan, its name dark on it. One port, one channel: heard in the left ear only.",
    "2  hovered (Alt, or in CUE mode): a cyan frame round the port; the headphones by the pointer, as now.",
    "3  the cue as it really is: Patchcord's loopback, all cyan. What is heard goes in. It listens passively: it wakes nothing up.",
    "4  its other side: what is heard comes out of it into the default output (here JLab GO Work 2): its links cyan, as it is.",
    "5  the links into the cue: as every link, in the colour they come from; passive, dashed. They come and go with what is heard.",
]):
    t(bx + 26, by + 440 + k * 18, s_, 12.5, "#c9cbcd")

# status bar
sy = gy + gh - 22
o.append(f'<rect x="{gx}" y="{sy}" width="{gw}" height="22" fill="#0e0f10"/>')
t(gx + 14, sy + 15, f'<tspan fill="{CYAN}">CUE ▸</tspan> Vivaldi · output_FL (left) + Ryzen HD Audio Controller · capture_FL (left) · in the default output at 70%', 11.5, "#a8abae", mono=True)
t(gx + gw - 14, sy + 15, "click: keep it on  ·  CUE or Esc stops all", 11.5, "#7d8085", mono=True, anchor="end")

H = sy + 22 + 40
open(sys.argv[1], "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{W}" height="{H}" viewBox="0 0 {W} {H}">{DEFS}<rect width="{W}" height="{H}" fill="#111214"/>{"".join(o)}</svg>\n')
