# MIXER for a node of more than two channels (issue #14, ADR 0025 left MIXER's way open). On the
# rack such a node is frames by its channels' positions: MASTER (the node's volume and mute),
# FRONT, REAR (pairs: VOLUME and BAL), CENTER, LFE (mono). MIXER shows it as one strip today: the
# node's volume and mute only. The specimen: the Radeon HDMI card in Digital Surround 5.1, as
# MIXER draws a card's output in its faders (the measures, type and paints of
# docs/design/mixer/index.html, which is `ui/mixer.rs` exactly).
#   NOW  one strip: MASTER's fader, MUTE, CUE; the channels can't be set
#   A    SPLIT (as on a stereo strip, which splits into L and R): a fader a channel, FL…LFE, beside
#        MASTER's; the strip as wide as it needs while split
#   B    a strip a part, always, as the rack's frames: MASTER, FRONT, REAR, CENTER, LFE, each its
#        band (`… · FRONT`); a pair's SPLIT into L and R as any stereo strip's
#   C    B folded: the MASTER strip alone, and SPLIT on it opens the parts' strips beside it
#        (chosen 2026-11-28: closed, its block is VOLUME as any strip's; open, MASTER; the
#        strips' dividers start under SETUP, which is the whole module's)
#   D    one strip, two wide: MASTER's fader and a fader a part, a pair's BAL over its fader
#   python3 surround.py   (writes now.png, a.png … d.png next to it; needs rsvg-convert, Pillow)
import math
import os
import subprocess

from PIL import ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
ASSETS = os.path.join(HERE, "..", "..", "..", "assets")
FONT = ImageFont.truetype(os.path.join(ASSETS, "fonts", "NotoSans-Bold.ttf"), 100)

# the code's measures (docs/design/mixer/index.html)
STRIP_W, BAND, GRAPH_Y, ROUTE_Y, TRAVEL, LEDS, LED_W = 112, 38, 20, 45, 160, 12, 4
ROW0, KEY_H, RADIUS, VOL_MAX = 17, 17, 8, 1.5
MAIN_Y = ROUTE_Y + ROW0                      # MAIN and ALL HERE
PROFILE_Y = MAIN_Y + KEY_H + 6 + 9           # the card's PROFILE, under them
KEYS_Y = PROFILE_Y + 16 + 27                 # the block's keys (CUE, MUTE, SPLIT)
TOP_F = KEYS_Y + 37                          # the faders' top
MOD_H = KEYS_Y + 228
SECTION, SMALL = (9, 1.2), (7.5, 0.8)
SINK = "#007c7d"
NAVY, PRINT = "#2b3950", "#dde5f0"           # a card's paint (the rack's hardware)
DEVICE, NODE, PROFILE = "Radeon High Definition Audio Controller", 54, "Digital Surround 5.1 (HDMI) Output"
CH = ["FL", "FR", "RL", "RR", "FC", "LFE"]
TRIM = [1.0, 1.0, 0.8, 0.8, 1.1, 0.7]        # each channel's volume, for the picture
LEV = [0.8, 0.75, 0.55, 0.5, 0.85, 0.6]       # its meter's level
PARTS = [("FRONT", [0, 1]), ("REAR", [2, 3]), ("CENTER", [4]), ("LFE", [5])]
MASTER = 0.93


def width(st, t):
    return FONT.getlength(t) * st[0] / 100 + st[1] * len(t)


def fit(t, st, w):
    if width(st, t) <= w:
        return t
    while t and width(st, t + "…") > w:
        t = t[:-1]
    return t.rstrip() + "…"


def T(st, x, y, fill, anchor, t):
    shift = st[1] / 2 if anchor == "middle" else st[1] if anchor == "end" else 0
    return f'<text x="{x + shift:.1f}" y="{y:.1f}" font-family="Noto Sans" font-weight="bold" font-size="{st[0]}" letter-spacing="{st[1]}" fill="{fill}" text-anchor="{anchor}">{t}</text>'


def ruled(x0, x1, y, text, print_=PRINT):
    cx, half, ly = (x0 + x1) / 2, width(SMALL, text) / 2 + 6, y - 2.8
    s = T(SMALL, cx, y, print_, "middle", text)
    for a, b in ((x0, cx - half), (cx + half, x1)):
        if b - a > 2:
            s += f'<path d="M{a:.1f},{ly:.1f} H{b:.1f}" stroke="{print_}" stroke-opacity=".5"/>'
    return s


def mixc(a, b, t):
    p = lambda c, i: int(c[1 + 2 * i:3 + 2 * i], 16)
    return "#" + "".join(f"{round(p(a, i) + (p(b, i) - p(a, i)) * t):02x}" for i in range(3))


GLASS = {"#5fd4ff": "gC", "#ff4a32": "gM", "#ffb02e": "gS"}


def defs():
    g = "".join(
        f'<radialGradient id="{i}On" cx=".5" cy=".5" r=".75"><stop offset="0" stop-color="{mixc(c, "#ffffff", .3)}"/><stop offset=".7" stop-color="{mixc(c, "#ffffff", .08)}"/><stop offset="1" stop-color="{c}"/></radialGradient>'
        f'<linearGradient id="{i}Body" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{mixc(c, "#000000", .08)}"/><stop offset="1" stop-color="{mixc(c, "#000000", .2)}"/></linearGradient>'
        for c, i in GLASS.items())
    return ('<defs>' + g +
            '<linearGradient id="gFrost" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#d9dcdc" stop-opacity=".9"/><stop offset="1" stop-color="#cdd0d1" stop-opacity=".88"/></linearGradient>'
            '<radialGradient id="gFrostIn" cx=".5" cy=".5" r=".62"><stop offset="0" stop-color="#5a5f63" stop-opacity=".16"/><stop offset=".75" stop-color="#5a5f63" stop-opacity=".04"/><stop offset="1" stop-color="#fff" stop-opacity=".22"/></radialGradient>'
            '<linearGradient id="gFrostSide" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#b9bdbf" stop-opacity=".85"/><stop offset="1" stop-color="#a3a7aa" stop-opacity=".85"/></linearGradient>'
            '<linearGradient id="gLcd" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f7bb62"/><stop offset="1" stop-color="#e58f2c"/></linearGradient>'
            f'<pattern id="noise" width="128" height="128" patternUnits="userSpaceOnUse"><image href="{os.path.join(ASSETS, "noise.png")}" width="128" height="128"/></pattern>'
            '<linearGradient id="sheen" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity=".16"/><stop offset=".35" stop-color="#fff" stop-opacity="0"/><stop offset="1" stop-color="#000" stop-opacity=".24"/></linearGradient>'
            '<linearGradient id="hole" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#050506"/><stop offset="1" stop-color="#232427"/></linearGradient>'
            '<linearGradient id="edgeHole" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#2a2c2e"/><stop offset="0.5" stop-color="#55585b"/><stop offset="1" stop-color="#a7aaad"/></linearGradient>'
            '<linearGradient id="btnGrad" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#303134"/><stop offset="0.6" stop-color="#1d1e20"/><stop offset="1" stop-color="#131314"/></linearGradient>'
            '<linearGradient id="capEdge" x1="0" y1="0" x2="0.3" y2="1"><stop offset="0" stop-color="#fff" stop-opacity="0.28"/><stop offset="0.45" stop-color="#fff" stop-opacity="0.04"/><stop offset="1" stop-color="#000" stop-opacity="0.5"/></linearGradient>'
            '<linearGradient id="capTop" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity="0.13"/><stop offset="1" stop-color="#fff" stop-opacity="0"/></linearGradient>'
            '<radialGradient id="gVol" cx=".38" cy=".3" r=".8"><stop offset="0" stop-color="#5a5b5f"/><stop offset="1" stop-color="#151618"/></radialGradient>'
            '<radialGradient id="gHole" cx=".5" cy=".62" r=".6"><stop offset="0" stop-color="#000"/><stop offset=".7" stop-color="#0a0a0b"/><stop offset="1" stop-color="#3c3d40"/></radialGradient>'
            '<linearGradient id="cap" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#5a5b5f"/><stop offset=".45" stop-color="#2b2c2f"/><stop offset=".55" stop-color="#1b1c1e"/><stop offset="1" stop-color="#3c3d41"/></linearGradient>'
            '</defs>')


def cap_key(x, y, w, h):
    return (f'<rect x="{x - 3}" y="{y - 3}" width="{w + 6}" height="{h + 6}" rx="6" fill="url(#hole)" stroke="url(#edgeHole)" stroke-width="1.2"/>'
            f'<rect x="{x + .2}" y="{y + 1.4}" width="{w + 2}" height="{h + 2}" rx="5" fill="#000" opacity=".34"/>'
            f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="4" fill="url(#btnGrad)" stroke="#070708"/>'
            f'<rect x="{x + .8}" y="{y + .8}" width="{w - 1.6}" height="{h - 1.6}" rx="3.4" fill="none" stroke="url(#capEdge)" stroke-width="1.4"/>'
            f'<rect x="{x + 3}" y="{y + 1.6}" width="{w - 6}" height="{h * .4:.1f}" rx="2.5" fill="url(#capTop)"/>')


def action_key(x, y, w, label):
    return cap_key(x, y, w, KEY_H) + T(SMALL, x + w / 2, y + KEY_H / 2 + 3.1, "#e6e7e8", "middle", label)


def lit_key(x, y, w, label, color, on):
    i, h = GLASS[color], KEY_H
    top, th = y, h - 3
    s = f'<rect x="{x - .5}" y="{y + 1.5}" width="{w + 1}" height="{h + 1.5}" rx="3" fill="#000" opacity=".5"/>'
    if on:
        s += f'<rect x="{x - 2}" y="{y - 1.5}" width="{w + 4}" height="{h + 5}" rx="4" fill="{color}" opacity=".28"/>'
    s += f'<rect x="{x}" y="{top}" width="{w}" height="{h}" rx="2.5" fill="{f"url(#{i}Body)" if on else "#25282a"}"/>'
    if not on:
        s += f'<rect x="{x}" y="{top}" width="{w}" height="{h}" rx="2.5" fill="url(#gFrostSide)"/>'
    s += f'<rect x="{x}" y="{top}" width="{w}" height="{th}" rx="2.5" fill="{f"url(#{i}On)" if on else "url(#gFrost)"}"/>'
    if not on:
        s += f'<rect x="{x}" y="{top}" width="{w}" height="{th}" rx="2.5" fill="url(#gFrostIn)"/>'
    s += f'<rect x="{x + .5}" y="{top + .5}" width="{w - 1}" height="{th - 1}" rx="2" fill="none" stroke="{mixc(color, "#ffffff", .55) if on else "#fff"}" stroke-opacity="{.7 if on else .4}"/>'
    s += f'<rect x="{x}" y="{top}" width="{w}" height="{h}" rx="2.5" fill="none" stroke="#000" stroke-opacity=".35" stroke-width=".8"/>'
    return s + T(SMALL, x + w / 2, top + th / 2 + 2.9, "#000", "middle", label)


def lcd(x, y, w, h, label, value):
    s = T(SMALL, x + 2, y - 4, PRINT, "start", label)
    s += (f'<rect x="{x - 2}" y="{y - 2}" width="{w + 4}" height="{h + 4}" rx="4" fill="#121214"/>'
          f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="2.5" fill="url(#gLcd)"/>')
    chars = int((w - 20) / 5.75)
    v = value if len(value) <= chars else value[:chars - 1] + "…"
    s += f'<text x="{x + 6}" y="{y + h / 2 + 3.4}" font-family="Noto Sans Mono" font-size="9.5" fill="#3a2408" xml:space="preserve">{v}</text>'
    return s + f'<path d="M{x + w - 11},{y + h / 2 - 1.5} l3,3.5 l3,-3.5 z" fill="#3a2408"/>'


def cap_y(v):
    return 6 + (1 - min(max(v / VOL_MAX, 0), 1)) * TRAVEL


def polar(x, y, a, r):
    a = math.radians(a)
    return f"{x + r * math.sin(a):.2f},{y - r * math.cos(a):.2f}"


def bal_knob(bx, by, b):
    """BAL, out (it turns): centre straight up, L and R at its ends."""
    s = ""
    for i in range(-4, 5):
        a = i * 33.75
        s += f'<path d="M{polar(bx, by, a, 11.5)} L{polar(bx, by, a, 16 if i == 0 else 13.5)}" stroke="{PRINT}" stroke-width="{1.5 if i == 0 else .9}" opacity="{1 if i == 0 else .6}"/>'
    s += f'<circle cx="{bx}" cy="{by}" r="10.8" fill="url(#gHole)" stroke="rgba(255,255,255,.12)" stroke-width=".7"/>'
    s += T(SMALL, bx - 12, by + 17, PRINT, "middle", "L") + T(SMALL, bx + 12, by + 17, PRINT, "middle", "R")
    a = b * 135
    s += f'<circle cx="{bx + 1.6}" cy="{by + 3.2}" r="10.4" fill="#000" opacity=".55"/><circle cx="{bx}" cy="{by + 2}" r="9.6" fill="#0d0e0f"/>'
    s += f'<circle cx="{bx}" cy="{by + 1.5}" r="10.1" fill="#111" opacity=".5"/><circle cx="{bx}" cy="{by}" r="9.6" fill="url(#gVol)" stroke="#0c0c0d"/>'
    return s + f'<path d="M{polar(bx, by, a, 2.8)} L{polar(bx, by, a, 8.3)}" stroke="#ffb02e" stroke-width="2" stroke-linecap="round"/>'


def fader(cx, top, v, label=None, scale=True, sx=None):
    """A fader at `cx`: its ticks, its slot (red past 100%), its cap with the number; the scale's
    numbers at `sx` (right-aligned) when `scale`."""
    s = ""
    if scale:
        for n in (150, 100, 75, 50, 25, 0):
            s += T(SMALL, sx, top + cap_y(n / 100) + 2.6, PRINT, "end", str(n))
    for k in range(13):
        vv = k * 12.5
        y = round(top + cap_y(vv / 100)) + .5
        s += f'<path d="M{cx - 12},{y} h{7 if vv in (0, 25, 50, 75, 100, 150) else 4}" stroke="{PRINT}" stroke-opacity=".6"/>'
    s += (f'<rect x="{cx - 2}" y="{top + 6}" width="4" height="{TRAVEL}" rx="2" fill="#0b0b0c"/>'
          f'<rect x="{cx - 2}" y="{top + cap_y(VOL_MAX):.1f}" width="4" height="{cap_y(1) - cap_y(VOL_MAX):.1f}" rx="2" fill="#b4483c"/>')
    if label:
        s += T(SMALL, cx, top - 4, PRINT, "middle", label)
    y = top + cap_y(v)
    n = round(v * 100)
    s += (f'<g transform="translate({cx - 17},{y - 11:.1f})"><rect x="1" y="2" width="34" height="22" rx="3" fill="#000" opacity=".5"/>'
          f'<rect x=".5" y=".5" width="33" height="21" rx="3" fill="url(#cap)" stroke="#0b0b0c"/>'
          f'<text x="17" y="14.5" font-family="Noto Sans Mono" font-weight="700" font-size="8.5" fill="{"#ff6a52" if n > 100 else "#e6e7e8"}" text-anchor="middle">{n}</text></g>')
    return s


def meter(mx, top, levels):
    """The meter: a column of LEDs a channel, lit to its level."""
    n, my, mh = len(levels), top + cap_y(VOL_MAX), TRAVEL
    step = (mh - 4) / LEDS
    s = f'<rect x="{mx}" y="{my}" width="{n * (LED_W + 2) + 4}" height="{mh}" rx="3" fill="#0b0b0c"/>'
    for c, lv in enumerate(levels):
        for i in range(LEDS):
            f = i / LEDS
            on = f < lv
            col = ("#ff4a32" if f >= .9 else "#ffb02e" if f >= .7 else "#5cff7a") if on else ("#351612" if f >= .9 else "#2f2810" if f >= .7 else "#1c2a1f")
            s += f'<rect x="{mx + 3 + c * (LED_W + 2)}" y="{my + mh - 1 - (i + 1) * step:.1f}" width="{LED_W}" height="{step - 2:.1f}" rx="1" fill="{col}"/>'
    return s


def plate(w):
    r = f'rx="{RADIUS}"'
    return (f'<rect width="{w}" height="{MOD_H}" {r} fill="{NAVY}"/><rect width="{w}" height="{MOD_H}" {r} fill="url(#noise)" opacity=".9"/>'
            f'<rect width="{w}" height="{MOD_H}" {r} fill="url(#sheen)"/>'
            f'<rect x=".5" y=".5" width="{w - 1}" height="{MOD_H - 1}" rx="{RADIUS}" fill="none" stroke="#000" stroke-opacity=".7"/>')


def frame(fx, fw, sub, node=NODE, color=SINK):
    """A strip's frame on the module's edge, its band: the device's name, then the strip's."""
    s = (f'<path d="M{fx},{BAND} V{RADIUS} a{RADIUS},{RADIUS} 0 0 1 {RADIUS},-{RADIUS} H{fx + fw - RADIUS} a{RADIUS},{RADIUS} 0 0 1 {RADIUS},{RADIUS} V{BAND} Z" fill="{color}"/>'
         f'<path d="M{fx},{BAND + .5} H{fx + fw}" stroke="rgba(0,0,0,.35)"/>'
         f'<rect x="{fx + .6}" y=".6" width="{fw - 1.2}" height="{MOD_H - 1.2}" rx="{RADIUS - .6}" fill="none" stroke="{color}" stroke-opacity=".9" stroke-width="1.2"/>')
    s += T(SECTION, fx + 10, 16, "#f3efe6", "start", fit(DEVICE.upper(), SECTION, fw - 20))
    gw = 10 + width(SMALL, f"#{node}")
    gx = fx + fw - 8 - gw
    s += f'<rect x="{gx:.1f}" y="{GRAPH_Y}" width="{gw:.1f}" height="13" rx="2.5" fill="#1b1c1e" stroke="rgba(0,0,0,.45)"/>' + T(SMALL, gx + gw / 2, GRAPH_Y + 9.4, "#e6e7e8", "middle", f"#{node}")
    return s + T(SMALL, fx + 10, 30, "#f3efe6", "start", fit(sub, SMALL, fw - 26 - gw))


def setup(w, x0, sw, main=True):
    """SETUP: MAIN and ALL HERE in the strip, the card's PROFILE across the module."""
    s = ruled(x0 + 6, x0 + sw - 6, ROUTE_Y + 8, "SETUP")
    if main:
        hw = (sw - 12 - 5) / 2
        s += lit_key(x0 + 6, MAIN_Y, hw, "MAIN", "#ffb02e", False) + action_key(x0 + 6 + hw + 5, MAIN_Y, hw, "ALL HERE")
    return s + lcd(12, PROFILE_Y, w - 24, 16, "PROFILE", PROFILE)


def keys(x0, sw, names, lit=()):
    """The block's keys in one row, each at its own place (CUE, MUTE, SPLIT)."""
    allk = ["CUE", "MUTE", "SPLIT"]
    kw = (sw - 12 - 2 * 5) / 3
    col = {"CUE": "#5fd4ff", "MUTE": "#ff4a32", "SPLIT": "#ffb02e"}
    return "".join(lit_key(x0 + 6 + allk.index(n) * (kw + 5), KEYS_Y, kw, n, col[n], n in lit) for n in names)


def strip(x0, levels, v, names, lit=(), heading="VOLUME"):
    """A strip's block: its name, its keys, one fader with its scale, its meter."""
    s = ruled(x0 + 6, x0 + STRIP_W - 6, KEYS_Y - 9, heading) + keys(x0, STRIP_W, names, lit)
    return s + fader(x0 + 50, TOP_F, v, sx=x0 + 30) + meter(x0 + 84, TOP_F, levels)


def module(body, w):
    return f'<g>{plate(w)}{body}</g>'


def part_volume(cs):
    return max(TRIM[c] for c in cs)


def now():
    w = STRIP_W + 12
    b = frame(0, w, "HDMI") + setup(w, 6, STRIP_W) + strip(6, [max(LEV)], MASTER, ["CUE", "MUTE"])
    return module(b, w), w


def a(split):
    if not split:
        w = STRIP_W + 12
        b = frame(0, w, "HDMI") + setup(w, 6, STRIP_W) + strip(6, [max(LEV)], MASTER, ["CUE", "MUTE", "SPLIT"])
        return module(b, w), w
    # split: MASTER's fader, then a fader a channel, each its own meter column beside it
    sw = 3 * STRIP_W
    w = sw + 12
    b = frame(0, w, "HDMI") + setup(w, 6, STRIP_W)
    b += ruled(12, w - 12, KEYS_Y - 9, "VOLUME") + keys(6, STRIP_W, ["CUE", "MUTE", "SPLIT"], ["SPLIT"])
    b += fader(56, TOP_F, MASTER, "MASTER", sx=36) + meter(78, TOP_F, [max(LEV)])
    for i, c in enumerate(CH):
        cx = 110 + i * 32
        b += fader(cx, TOP_F, TRIM[i], c, scale=False)
    b += meter(w - 12 - (len(CH) * (LED_W + 2) + 4), TOP_F, LEV)
    return module(b, w), w


def part_strips(x, parts_open=True, split_front=False):
    """The parts' strips from `x`: a band each, a pair's SPLIT, one fader, its meter."""
    b = ""
    for name, cs in PARTS:
        b += f'<path d="M{x},{KEYS_Y - 22} V{MOD_H - 8}" stroke="{PRINT}" stroke-opacity=".35" stroke-dasharray="2 3"/>'
        names = ["CUE", "SPLIT"] if len(cs) == 2 else ["CUE"]
        if split_front and name == "FRONT":
            b += ruled(x + 6, x + STRIP_W - 6, KEYS_Y - 9, name) + keys(x, STRIP_W, names, ["SPLIT"])
            for i, cx in enumerate((38, 74)):
                b += fader(x + cx, TOP_F, TRIM[cs[i]], "LR"[i], sx=x + 17, scale=i == 0)
            b += meter(x + 93, TOP_F, [LEV[c] for c in cs])
        else:
            b += strip(x, [LEV[c] for c in cs], part_volume(cs), names, heading=name)
        x += STRIP_W
    return b


def b_():
    w = 5 * STRIP_W + 12
    b = frame(0, w, "HDMI") + setup(w, 6, STRIP_W) + strip(6, [max(LEV)], MASTER, ["CUE", "MUTE"], heading="MASTER") + part_strips(6 + STRIP_W, split_front=True)
    return module(b, w), w


def c(open_):
    if not open_:
        w = STRIP_W + 12
        b = frame(0, w, "HDMI") + setup(w, 6, STRIP_W) + strip(6, [max(LEV)], MASTER, ["CUE", "MUTE", "SPLIT"])
        return module(b, w), w
    w = 5 * STRIP_W + 12
    b = frame(0, w, "HDMI") + setup(w, 6, STRIP_W) + strip(6, [max(LEV)], MASTER, ["CUE", "MUTE", "SPLIT"], ["SPLIT"], heading="MASTER") + part_strips(6 + STRIP_W)
    return module(b, w), w


def d():
    w = 312
    b = frame(0, w, "HDMI") + setup(w, 6, STRIP_W)
    b += ruled(12, 98, KEYS_Y - 9, "VOLUME") + keys(6, STRIP_W - 18, ["CUE", "MUTE"])
    b += fader(46, TOP_F, MASTER, "MASTER", sx=26)
    for i, (name, cs) in enumerate(PARTS):
        cx = 122 + i * 38
        b += fader(cx, TOP_F, part_volume(cs), name if name != "CENTER" else "CTR", scale=False)
        if len(cs) == 2:
            b += bal_knob(cx, KEYS_Y + 6, (TRIM[cs[1]] - TRIM[cs[0]]) / max(TRIM[cs[0]], TRIM[cs[1]]))
    b += meter(w - 12 - (len(CH) * (LED_W + 2) + 4), TOP_F, LEV)
    return module(b, w), w


def page(name, mods, note):
    """One picture: the modules side by side on MIXER's dark field, a line over them."""
    x, body = 14, ""
    for m, w in mods:
        body += f'<g transform="translate({x},34)">{m}</g>'
        x += w + 26
    W, H = x + 2, MOD_H + 34 + 14
    svg = (f'<svg xmlns="http://www.w3.org/2000/svg" width="{W * 2}" height="{H * 2}" viewBox="0 0 {W} {H}">{defs()}'
           f'<rect width="{W}" height="{H}" fill="#1c1d1f"/>' + T(SECTION, 14, 20, "#8d9095", "start", note) + body + '</svg>')
    path = os.path.join(HERE, name)
    with open(path + ".svg", "w") as f:
        f.write(svg)
    subprocess.run(["rsvg-convert", "-o", path + ".png", path + ".svg"], check=True)
    os.remove(path + ".svg")


def main():
    page("now", [now()], "NOW · ONE STRIP: THE NODE'S VOLUME AND MUTE")
    page("a", [a(False), a(True)], "A · SPLIT: A FADER A CHANNEL (JOINED · SPLIT)")
    page("b", [b_()], "B · A STRIP A PART, AS THE RACK'S FRAMES (FRONT SPLIT)")
    page("c", [c(False), c(True)], "C · SPLIT ON MASTER OPENS THE PARTS (CLOSED · OPEN)")
    page("d", [d()], "D · ONE STRIP: MASTER AND A FADER A PART, BAL ON A PAIR")


if __name__ == "__main__":
    main()
