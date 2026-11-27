# MIXER's SETUP block. Now a card's PROFILE sits in its first strip as if it were that strip's,
# a strip's PORT lands where the next strip has PROFILE, strips have 0, 1 or 2 LCDs so their rows
# don't meet, and three kinds of control (an amber lit key, a black key, amber LCDs with their
# names over them) crowd a small block. Five rows, the same four modules in each (a laptop card
# with two strips, a headset with a profile, a card that is off, a card with a profile and a port
# on each strip):
#   now  as the code draws it
#   A    PROFILE the device's: one LCD across the module under its bands; in a strip MAIN | ALL
#        HERE, then PORT, its place kept (an empty recess) where a strip has none
#   B    A in one language: each LCD's name inside it, no line over it; MAIN and ALL HERE keys of
#        one kind, MAIN lit by a green lamp, not amber (amber is the LCDs')
#   C    B with MAIN a lamp on the strip's band (a click there makes it main): SETUP is ALL HERE
#        and PORT
#   D    PROFILE as in B, PORT on the band's second line; SETUP only MAIN | ALL HERE
#   python3 setup.py   (writes setup.png next to it; needs rsvg-convert)
import os
import subprocess

HERE = os.path.dirname(os.path.abspath(__file__))
NAVY, PRINT = "#2b3950", "#dde5f0"
SINK, SOURCE, OFF = "#007c7d", "#206ea6", "#6b6e72"
SW = 125.0          # a strip
BAND = 34.0         # its band
KEY_H, LCD_H = 16.0, 16.0
MONO = "Noto Sans Mono"

# (name, profile, strips: (band colour, strip name, main, port))
MODULES = [
    ("LAPTOP CARD", None, [(SINK, "SPEAKERS", True, None), (SOURCE, "MIC", False, "Internal Microphone")]),
    ("JLAB GO WORK 2", "High Fidelity Playback (A2DP Sink)", [(SINK, "HEADPHONES", True, None)]),
    ("RADEON HDMI", "Off", []),
    ("RYZEN HD AUDIO", "Analog Stereo Duplex", [(SINK, "SPEAKERS", False, "Speakers"), (SOURCE, "BUILT-IN MIC", False, "Internal Microphone")]),
]


def t(x, y, s, size=7.0, fill=PRINT, anchor="start", weight="bold", family="Noto Sans", ls=1.0):
    return f'<text x="{x:.1f}" y="{y:.1f}" font-family="{family}" font-weight="{weight}" font-size="{size}" fill="{fill}" text-anchor="{anchor}" letter-spacing="{ls}" xml:space="preserve">{s}</text>'


def ruled(x0, x1, y, title):
    m = (x0 + x1) / 2
    half = len(title) * 3.0 + 6
    return (f'<path d="M{x0 + 6},{y} H{m - half} M{m + half},{y} H{x1 - 6}" stroke="{PRINT}" stroke-opacity=".5" stroke-width=".7"/>'
            + t(m, y + 2.5, title, anchor="middle"))


def hole(x, y, w, h):
    return f'<rect x="{x - 1.5}" y="{y - 1.5}" width="{w + 3}" height="{h + 3}" rx="2" fill="#0f1114"/><path d="M{x - 1},{y + h + 2} h{w + 2}" stroke="#fff" stroke-opacity=".12"/>'


def lit(x, y, w, label, on, colour="#ffb02e"):
    """Now: a flush lit key, amber when on, frosted ivory when off."""
    fill = colour if on else "#e9e4d6"
    return hole(x, y, w, KEY_H) + f'<rect x="{x}" y="{y}" width="{w}" height="{KEY_H}" rx="1" fill="{fill}"/>' + t(x + w / 2, y + 10.8, label, fill="#1d1408", anchor="middle")


def black(x, y, w, label):
    """Now: ALL HERE, a raised black key."""
    return (f'<rect x="{x}" y="{y}" width="{w}" height="{KEY_H}" rx="3" fill="#18191b" stroke="#000"/>'
            f'<rect x="{x + 1}" y="{y + 1}" width="{w - 2}" height="{KEY_H - 2}" rx="2.5" fill="none" stroke="#fff" stroke-opacity=".12"/>'
            + t(x + w / 2, y + 10.8, label, fill="#e6e7e8", anchor="middle"))


def ivory(x, y, w, label, lamp=None):
    """B: one kind of key, ivory, flush; a green lamp at its left when `lamp` is on."""
    s = hole(x, y, w, KEY_H) + f'<rect x="{x}" y="{y}" width="{w}" height="{KEY_H}" rx="1" fill="#e9e4d6"/>'
    if lamp is not None:
        c = "#46d96a" if lamp else "#3a4a3e"
        s += f'<circle cx="{x + 8}" cy="{y + KEY_H / 2}" r="2.6" fill="{c}"/>'
        if lamp:
            s += f'<circle cx="{x + 8}" cy="{y + KEY_H / 2}" r="2.6" fill="none" stroke="#1d5a2c" stroke-width=".6"/>'
        return s + t(x + w / 2 + 4, y + 10.8, label, fill="#1d1408", anchor="middle")
    return s + t(x + w / 2, y + 10.8, label, fill="#1d1408", anchor="middle")


def fit(s, w):
    n = int((w - 20) / 5.75)
    return s if len(s) <= n else s[:n - 1] + "…"


def lcd(x, y, w, value, name=None, over=False):
    """An LCD with ▾. `over`: its name above it (now); else inside it, small and faint, before the
    value (B)."""
    s = hole(x, y, w, LCD_H) + f'<rect x="{x}" y="{y}" width="{w}" height="{LCD_H}" rx="2.5" fill="url(#lcd)"/>'
    if name and over:
        s += t(x + 2, y - 4, name)
    vx, vw = x + 6, w
    if name and not over:
        s += t(x + 6, y + 10.6, name, size=6.0, fill="#3a2408", ls=.6) + ""
        off = len(name) * 5.0 + 14
        vx, vw = x + off, w - off + 6
        s += f'<path d="M{x + off - 4},{y + 3} v{LCD_H - 6}" stroke="#3a2408" stroke-opacity=".35"/>'
    s += t(vx, y + LCD_H / 2 + 3.4, fit(value, vw), size=9.5, fill="#3a2408", weight="400", family=MONO, ls=0)
    s += f'<path d="M{x + w - 11},{y + LCD_H / 2 - 1.5} l3,3.5 l3,-3.5 z" fill="#3a2408"/>'
    return s


def empty(x, y, w):
    return f'<rect x="{x - 1.5}" y="{y - 1.5}" width="{w + 3}" height="{LCD_H + 3}" rx="2" fill="#000" fill-opacity=".14"/>'


def module(x0, y0, mod, variant):
    name, profile, strips = mod
    n = max(1, len(strips))
    w = n * SW
    o = [f'<g transform="translate({x0},{y0})">', t(4, -8, name, size=9)]
    H = 196.0
    o.append(f'<rect width="{w}" height="{H}" rx="5" fill="{NAVY}"/>')
    # bands
    bands = strips or [(OFF, "OFF", False, None)]
    for i, (c, sname, main, port) in enumerate(bands):
        x = i * SW
        r = "5" if True else "0"
        o.append(f'<rect x="{x + .6}" y=".6" width="{SW - 1.2}" height="{H - 1.2}" rx="{r}" fill="none" stroke="{c}" stroke-width="1.2"/>')
        o.append(f'<path d="M{x},{BAND} V5 a5,5 0 0 1 5,-5 H{x + SW - 5} a5,5 0 0 1 5,5 V{BAND} Z" fill="{c}"/>')
        o.append(t(x + 10, 14, sname, size=8, fill="#f3efe6", ls=1.4))
        if variant == "D" and port:
            o.append(f'<rect x="{x + 8}" y="19" width="{SW - 16}" height="11" rx="2" fill="#000" fill-opacity=".22"/>')
            o.append(t(x + 12, 27.3, fit(port, SW - 10), size=7.5, fill="#f3efe6", weight="400", family=MONO, ls=0))
            o.append(f'<path d="M{x + SW - 17},{23} l3,3.5 l3,-3.5 z" fill="#f3efe6"/>')
        if variant == "C" and strips:
            on = main
            c2 = "#46d96a" if on else "#1d2a20"
            o.append(f'<circle cx="{x + SW - 14}" cy="11" r="3.2" fill="{c2}" stroke="#000" stroke-opacity=".4" stroke-width=".6"/>')
            o.append(t(x + SW - 21, 14, "MAIN", size=6.5, fill="#f3efe6", anchor="end", ls=.8))
        if i > 0:
            o.append(f'<path d="M{x},{BAND} V{H - 8}" stroke="{PRINT}" stroke-opacity=".35" stroke-dasharray="2 3"/>')
    y = BAND + 12
    top_profile = variant in "ABCD" and profile is not None
    if top_profile:
        # across the module, under the bands
        if variant == "A":
            o.append(t(10, y + 6, "PROFILE"))
            o.append(lcd(10, y + 11, w - 20, profile))
            y += 44
        else:
            o.append(lcd(10, y, w - 20, profile, "PROFILE"))
            y += 32
    elif variant in "ABCD" and any(m[1] for m in MODULES):
        # (modules without a profile keep its room, so strips' rows meet across the desk)
        y += 44 if variant == "A" else 32
    for i, (c, sname, main, port) in enumerate(strips):
        x = i * SW
        kx, kw = x + 8, SW - 16
        hw = (kw - 5) / 2
        yy = y
        if variant == "now":
            o.append(ruled(x, x + SW, yy, "SETUP"))
            yy += 10
            o.append(lit(kx, yy, hw, "MAIN", main))
            o.append(black(kx + hw + 5, yy, hw, "ALL HERE"))
            yy += KEY_H + 21
            if i == 0 and profile:
                o.append(lcd(kx, yy, kw, profile, "PROFILE", over=True))
                yy += 30
            if port:
                o.append(lcd(kx, yy, kw, port, "PORT", over=True))
        elif variant == "A":
            o.append(ruled(x, x + SW, yy, "SETUP"))
            yy += 10
            o.append(lit(kx, yy, hw, "MAIN", main))
            o.append(black(kx + hw + 5, yy, hw, "ALL HERE"))
            yy += KEY_H + 21
            o.append(t(kx + 2, yy - 4, "PORT", fill=PRINT if port else "#7d879a"))
            o.append(lcd(kx, yy, kw, port) if port else empty(kx, yy, kw))
        elif variant == "B":
            o.append(ruled(x, x + SW, yy, "SETUP"))
            yy += 10
            o.append(ivory(kx, yy, hw, "MAIN", lamp=main))
            o.append(ivory(kx + hw + 5, yy, hw, "ALL HERE"))
            yy += KEY_H + 8
            o.append(lcd(kx, yy, kw, port, "PORT") if port else empty(kx, yy, kw))
        elif variant == "C":
            o.append(ruled(x, x + SW, yy, "SETUP"))
            yy += 10
            o.append(ivory(kx, yy, kw, "ALL HERE"))
            yy += KEY_H + 8
            o.append(lcd(kx, yy, kw, port, "PORT") if port else empty(kx, yy, kw))
        elif variant == "D":
            o.append(ruled(x, x + SW, yy, "SETUP"))
            yy += 10
            o.append(ivory(kx, yy, hw, "MAIN", lamp=main))
            o.append(ivory(kx + hw + 5, yy, hw, "ALL HERE"))
    # the next block's title, where it starts in each variant
    nxt = {"now": BAND + 12 + 10 + KEY_H + 21 + 30 + LCD_H + 14, "A": y + 10 + KEY_H + 21 + LCD_H + 14,
           "B": y + 10 + KEY_H + 8 + LCD_H + 14, "C": y + 10 + KEY_H + 8 + LCD_H + 14, "D": y + 10 + KEY_H + 14}[variant]
    for i, (c, sname, main, port) in enumerate(strips):
        o.append(ruled(i * SW, (i + 1) * SW, nxt, "VOLUME" if c == SINK else "GAIN"))
    o.append("</g>")
    return "".join(o), w


ROWS = [("now", "now: PROFILE in the first strip, PORT under it or in its place, rows that don't meet"),
        ("A", "A: PROFILE the device's, across the module; PORT's place kept in every strip"),
        ("B", "B: A in one language: names inside the LCDs, MAIN and ALL HERE one kind of key, MAIN a green lamp"),
        ("C", "C: B with MAIN a lamp on the band; SETUP is ALL HERE and PORT"),
        ("D", "D: PROFILE as in B, PORT on the band's second line; SETUP only MAIN | ALL HERE")]
body, y = [], 20.0
W = 0
for v, title in ROWS:
    body.append(t(20, y + 10, title, size=10, fill="#cfd2d6", ls=0))
    y += 36
    x = 20.0
    for m in MODULES:
        g, w = module(x, y, m, v)
        body.append(g)
        x += w + 18
    W = max(W, x)
    y += 196 + 26
svg = (f'<svg xmlns="http://www.w3.org/2000/svg" width="{W:.0f}" height="{y:.0f}">'
       '<defs><linearGradient id="lcd" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f7b552"/><stop offset="1" stop-color="#e4952e"/></linearGradient></defs>'
       '<rect width="100%" height="100%" fill="#16171a"/>' + "".join(body) + "</svg>")
p = os.path.join(HERE, "setup.svg")
with open(p, "w") as f:
    f.write(svg)
subprocess.run(["rsvg-convert", "-z", "2", "-o", os.path.join(HERE, "setup.png"), p], check=True)
os.remove(p)
