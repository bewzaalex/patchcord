# Draws the legend window into specimen.svg and, given a second path, alone for the app:
#   python3 docs/design/engine/legend.py docs/design/engine/specimen.svg src/ui/legend.svg
import re, sys
path = sys.argv[1]
s = open(path).read()
k = s.find('<text x="30" y="2846"')
if k >= 0:
    s = s[:k] + '</svg>'
X0, Y0, W = 30, 2860, 1180
F = 'font-family="Noto Sans"'; M = 'font-family="Noto Sans Mono"'
o = []
def t(x, y, txt, size=11, fill="#c9cbcd", mono=False, bold=False, anchor="start", ls=0):
    o.append(f'<text x="{x:.1f}" y="{y}" {M if mono else F} font-size="{size}" fill="{fill}"{" font-weight=\"bold\"" if bold else ""} text-anchor="{anchor}"{f" letter-spacing=\"{ls}\"" if ls else ""} xml:space="preserve">{txt}</text>')
def head(x, y, txt):
    t(x, y, txt, 10.5, "#e58f2c", True, True, ls=1.2)
    o.append(f'<path d="M{x},{y+7} H{x+546}" stroke="#2c2d31"/>')
def desc(x, y, txt): t(x, y, txt, 11, "#9a9da1")
def mark(n, x, y):
    o.append(f'<circle cx="{x:.1f}" cy="{y}" r="8" fill="#e58f2c"/><text x="{x:.1f}" y="{y+3.5}" {F} font-size="10" font-weight="bold" fill="#17181a" text-anchor="middle">{n}</text>')
def call(n, mx, my, px, py):
    o.append(f'<path d="M{mx},{my} L{px},{py}" stroke="#e58f2c" stroke-width="1" opacity=".7"/>'); mark(n, mx, my)

o.append('FRAME')
o.append(f'<rect width="{W}" height="50" rx="10" fill="#1f2023"/><rect y="40" width="{W}" height="10" fill="#1f2023"/><path d="M0,50.5 H{W}" stroke="#000"/>')
t(20, 30, "HOW TO READ THE GRAPH", 14, "#e6e7e8", bold=True, ls=1)
t(250, 30, "what ENGINE shows: PipeWire's real nodes, ports and links, as they run", 11, "#7d8085")
o.append(f'<path d="M{W-26},19 l9,9 M{W-17},19 l-9,9" stroke="#7d8085" stroke-width="1.8" stroke-linecap="round"/>')
L, R = 24, 610

# ---- left column
y = 82; head(L, y, "A BLOCK IS ONE NODE")
bx, by = L + 26, y + 34
o.append(f'<g transform="translate({bx},{by})"><path d="M0,6 A6,6 0 0 1 6,0 H238 A6,6 0 0 1 244,6 V12 H0 Z" fill="#9b6bd6"/><rect x="0.5" y="4" width="243" height="73.5" rx="5.5" fill="#1d1e21"/><rect width="244" height="78" rx="6" fill="none" stroke="#9b6bd6" stroke-width="1.5"/><circle cx="12" cy="17" r="4" fill="#5cff7a"/><text x="22" y="21" {F} font-size="12" font-weight="bold" fill="#e6e7e8">fooyin</text><text x="12" y="34" {M} font-size="10" fill="#7d8085">65 · Stream/Output/Audio</text><text x="234" y="21" text-anchor="end" {M} font-size="10" fill="#5cff7a">44µs</text><text x="234" y="34" text-anchor="end" {F} font-size="8.5" font-weight="bold" letter-spacing=".5" fill="#ff6a52">44100→48000</text><circle cy="51.5" r="4" fill="#9b6bd6" stroke="#0f1011" stroke-width="1.5"/><text x="10" y="55" {M} font-size="10" fill="#a8abae">input_FL</text><circle cx="244" cy="51.5" r="4" fill="#9b6bd6" stroke="#0f1011" stroke-width="1.5"/><text x="234" y="55" text-anchor="end" {M} font-size="10" fill="#a8abae">output_FL</text><circle cx="244" cy="66.5" r="4" fill="#9b6bd6" stroke="#0f1011" stroke-width="1.5"/><text x="234" y="70" text-anchor="end" {M} font-size="10" fill="#a8abae">monitor_FL</text></g>')
call(1, bx+150, by-16, bx+150, by+1); call(3, bx+45, by-16, bx+45, by+12)
call(2, bx-20, by+13, bx+8, by+17); call(4, bx-20, by+32, bx+10, by+31); call(7, bx-20, by+51.5, bx-5, by+51.5)
call(5, bx+268, by+13, bx+237, by+17); call(6, bx+268, by+32, bx+237, by+30); call(8, bx+268, by+59, bx+249, by+59)
ex, ey = bx + 300, by - 8
for n, txt in [(1, "colour: what it is (see COLOURS)"), (2, "light: what it does (see LIGHTS)"), (3, "its name, as apps and the desk call it"), (4, "its id in PipeWire, and its media class"), (5, "time it works each cycle, on average"), (6, "tags: what is special about it"), (7, "inputs, at its left"), (8, "outputs, at its right")]:
    mark(n, ex, ey); desc(ex+14, ey+4, txt); ey += 19
y = 290; head(L, y, "COLOURS: WHAT IT IS"); yy = y + 26
for c, n, d in [("#4f86d9", "device", "a sound card, a headset, a mic: something with hardware"), ("#9b6bd6", "app", "a program that plays or records"), ("#2fb0a6", "system", "WirePlumber's plumbing and PipeWire's own clocks"), ("#e08a3c", "MIDI", "notes and controls, not sound"), ("#d9578f", "video", "a camera, a screen being shared"), ("#5fa36a", "Patchcord", "Patchcord's own signal meters and cue")]:
    o.append(f'<rect x="{L+10}" y="{yy-7}" width="30" height="5" rx="2.5" fill="{c}"/>'); t(L+52, yy, n, 11, "#e6e7e8", True); desc(L+140, yy, d); yy += 21
y = yy + 14; head(L, y, "LIGHTS: WHAT IT DOES"); yy = y + 26
for c, n, d in [("#5cff7a", "running", "a driver clocks it: it works every cycle"), ("#ffb02e", "idle", "ready, but nothing makes it run"), ("#4a4c50", "suspended", "asleep: the device is closed, no CPU"), ("#ff4a32", "xruns", "it missed a deadline lately: you may hear a crackle")]:
    o.append(f'<circle cx="{L+25}" cy="{yy-4}" r="5" fill="{c}"/>'); t(L+52, yy, n, 11, "#e6e7e8", True); desc(L+140, yy, d); yy += 21
y = yy + 14; head(L, y, "TAGS"); yy = y + 26
for tag, c, d in [("DRIVER", "#e58f2c", "it gives the clock to its group"), ("SYSTEM", "#e58f2c", "WirePlumber's own, left off the desk"), ("PATCHCORD", "#e58f2c", "made by Patchcord: a meter, the cue"), ("HIDDEN ON DESK", "#e58f2c", "you hid its device on the desk"), ("44100→48000", "#ff6a52", "its rate is not the group's: it is resampled")]:
    t(L+10, yy, tag, 8.5, c, False, True, ls=.5); desc(L+140, yy, d); yy += 21
y = yy + 14; head(L, y, "MOUSE AND KEYS"); yy = y + 26
for kk, d in [("click", "a node, a port or a link: its details"), ("drag", "move the graph"), ("wheel", "zoom (over the tables: scroll)"), ("F", "fit the graph to the window"), ("Esc", "close the legend, then the details"), ("?", "open or close this legend"), ("E", "back to the desk")]:
    o.append(f'<rect x="{L+10}" y="{yy-13}" width="{max(22, len(kk)*7.5+12)}" height="18" rx="3" fill="#2a2b2e" stroke="#0b0b0c"/>'); t(L+16, yy, kk, 10.5, "#e6e7e8", True, True); desc(L+140, yy, d); yy += 21
left_end = yy

# ---- right column
y = 82; head(R, y, "PORTS"); yy = y + 26
o.append(f'<circle cx="{R+18}" cy="{yy-4}" r="4" fill="#9b6bd6" stroke="#0f1011" stroke-width="1.5"/><circle cx="{R+32}" cy="{yy-4}" r="4" fill="#4f86d9" stroke="#0f1011" stroke-width="1.5"/>'); desc(R+52, yy, "a port has its block's colour; one port is one channel: FL, FR, MONO…"); yy += 21
desc(R+10, yy+2, "Inputs are on a block's left, outputs on its right: sound flows left to right.")
y = yy + 30; head(R, y, "LINKS"); yy = y + 26
desc(R+10, yy, "A link takes the colour of the block it comes out of, so you see which way sound goes."); yy += 24
for st, n, d in [('stroke="#9b6bd6" stroke-width="2"', "active", "carries sound now"), ('stroke="#9b6bd6" stroke-width="1.6" stroke-opacity=".45"', "paused", "made, but nothing flows yet"), ('stroke="#9b6bd6" stroke-width="1.2" stroke-dasharray="3 4"', "passive", "listens only: never keeps a device awake"), ('stroke="#ff4a32" stroke-width="2"', "error", "PipeWire could not connect it (no common format)"), ('stroke="#ffd36a" stroke-width="10" opacity=".95" filter="url(#glow)"/><path d="M{R+10},{yy-4} H{R+40}" stroke="#9b6bd6" stroke-width="2"', "selected", "the chosen link: a yellow glow behind it")]:
    o.append(f'<path d="M{R+10},{yy-4} H{R+40}" {st}/>'.replace('{R+10}', str(R+10)).replace('{yy-4}', str(yy-4)).replace('{R+40}', str(R+40))); t(R+52, yy, n, 11, "#e6e7e8", True); desc(R+140, yy, d); yy += 21
desc(R+10, yy+2, "One link carries one channel: a stereo cord on the desk is two links here.")
y = yy + 30; head(R, y, "GROUPS"); gy = y + 20
o.append(f'<rect x="{R+10}" y="{gy}" width="546" height="44" rx="8" fill="#fff" fill-opacity=".025" stroke="#4f86d9" stroke-opacity=".35"/><path d="M{R+15},{gy+0.58} A8,8 0 0 0 {R+10},{gy+8} V{gy+36} A8,8 0 0 0 {R+15},{gy+43.42} Z" fill="#4f86d9"/>')
size = 8; cw = size * 0.68
parts = [("DRIVER", "#e58f2c", 1), ("  ", None, 0), ("Scarlett 2i2", "#c9cbcd", 0), ("  ", None, 0), ("Audio/Sink #50", "#7d8085", 0), (" · ", None, 0), ("48000 Hz", "#7d8085", 0), (" · ", None, 0), ("QUANT 1024", "#7d8085", 0), (" · ", None, 0), ("21.3 ms", "#7d8085", 0), (" · ", None, 0), ("DSP 6.0%", "#7d8085", 0), (" · ", None, 0), ("94% LEFT", "#7d8085", 0)]
x = R + 24; centres = []
for txt, c, ls in parts:
    w = len(txt) * (cw + ls)
    if c:
        t(x, gy+17, txt, size, c, True, True, ls=ls)
        if c != "#e58f2c": centres.append(x + w / 2)
    else:
        t(x, gy+17, txt.replace(" ", " "), size, "#5a5c60", True, True)
    x += w
for n, cx in enumerate(centres, 1):
    o.append(f'<path d="M{cx:.1f},{gy+22} V{gy+28}" stroke="#e58f2c" opacity=".7"/>'); mark(n, cx, gy+34)
yy = gy + 68
for n, d in [(1, "the driver: the device whose clock the group runs on"), (2, "its media class and id"), (3, "the rate everything in the group runs at"), (4, "the quantum: samples per cycle"), (5, "one cycle: the deadline for all its nodes"), (6, "DSP load: how much of the cycle the graph takes"), (7, "the worst cycle's time left: little left means xruns")]:
    mark(n, R+18, yy-4); desc(R+32, yy, d); yy += 19
yy += 10
for n, d in [("NOT RUNNING", "no driver clocks these now: suspended devices, idle streams"), ("INTERNAL DRIVERS", "PipeWire's own clocks, when nothing else runs")]:
    t(R+10, yy, n, 10, "#e58f2c", True, True, ls=1); desc(R+160, yy, d); yy += 20
y = yy + 20; head(R, y, "SELECTING"); yy = y + 26
o.append(f'<rect x="{R+12}" y="{yy-14}" width="34" height="18" rx="5" fill="#ffd36a" opacity=".95" filter="url(#glow)"/><path d="M{R+14},{yy-9} a3,3 0 0 1 3,-3 h24 a3,3 0 0 1 3,3 v-0 z" fill="#9b6bd6"/><rect x="{R+14}" y="{yy-12}" width="30" height="14" rx="3" fill="#1d1e21" stroke="#9b6bd6" stroke-width="1.5"/><rect x="{R+14}" y="{yy-12}" width="30" height="3" fill="#9b6bd6"/>'); desc(R+60, yy, "the chosen node: a yellow glow behind it; its colours stay"); yy += 21
o.append(f'<path d="M{R+10},{yy-4} H{R+44}" stroke="#ffd36a" stroke-width="10" opacity=".95" filter="url(#glow)"/><path d="M{R+10},{yy-4} H{R+44}" stroke="#4f86d9" stroke-width="2"/>'); desc(R+60, yy, "a chosen link (with its two ports) or port: glows the same way"); yy += 21
o.append(f'<rect x="{R+14}" y="{yy-12}" width="30" height="14" rx="3" fill="#1d1e21" stroke="#9b6bd6" stroke-width="1.5"/><rect x="{R+14}" y="{yy-12}" width="30" height="3" fill="#9b6bd6"/>'); desc(R+60, yy, "what it is linked to stays as it is"); yy += 21
o.append(f'<rect x="{R+14}" y="{yy-11}" width="30" height="12" rx="3" fill="#1d1e21" stroke="#5a5c60" opacity=".5"/>'); desc(R+60, yy, "everything else fades"); yy += 21

H = int(max(left_end, yy) + 24)
o[0] = f'<rect width="{W}" height="{H}" rx="10" fill="#17181a" stroke="#000" stroke-width="1.5"/>'
win = f'<text x="{X0}" y="{Y0-14}" {F} font-weight="bold" font-size="14" letter-spacing="1.5" fill="#e58f2c">THE LEGEND: OPENED BY ? LEGEND (OR THE ? KEY), OVER THE GRAPH</text><g transform="translate({X0},{Y0})">{"".join(o)}</g>'
m = re.search(r'width="(\d+(?:\.\d+)?)" height="(\d+(?:\.\d+)?)" viewBox="0 0 [\d.]+ [\d.]+"', s)
w0, h0 = m.group(1), m.group(2); h1 = Y0 + H + 40
s = s.replace(m.group(0), f'width="{w0}" height="{h1}" viewBox="0 0 {w0} {h1}"', 1)
s = s.replace(f'<rect width="{w0}" height="{h0}" fill="#1b1c1e"/>', f'<rect width="{w0}" height="{h1}" fill="#1b1c1e"/>', 1)
s = s[:s.rfind('</svg>')] + win + '</svg>'
open(path, 'w').write(s)
print(H, x - R)
# the window alone, for the app: python3 legend.py specimen.svg src/ui/legend.svg
if len(sys.argv) > 2:
    # each glow blurs only its own box: a blur over the whole window takes seconds
    body, defs = "".join(o), []
    def own(m):
        e = m.group(0)
        r = re.search(r'x="([\d.]+)" y="([\d.]+)" width="([\d.]+)" height="([\d.]+)"', e)
        if r:
            x, y, w, h = map(float, r.groups())
        else:
            p = re.search(r'd="M([\d.]+),([\d.]+) H([\d.]+)"', e)
            x, y, w, h = float(p.group(1)), float(p.group(2)), float(p.group(3)) - float(p.group(1)), 0.0
        k = len(defs)
        defs.append(f'<filter id="glow{k}" filterUnits="userSpaceOnUse" x="{x-24:g}" y="{y-24:g}" width="{w+48:g}" height="{h+48:g}"><feGaussianBlur stdDeviation="6"/></filter>')
        return e.replace('url(#glow)', f'url(#glow{k})')
    body = re.sub(r'<(?:rect|path) [^>]*filter="url\(#glow\)"[^>]*>', own, body)
    open(sys.argv[2], 'w').write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}"><defs>{"".join(defs)}</defs>{body}</svg>\n')
