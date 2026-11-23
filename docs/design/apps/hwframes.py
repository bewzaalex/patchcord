# Hardware first: a frame on a device in the colour of its block in ENGINE, and the groups of
# ports in a block (its playback, its monitor, its capture) each in a colour of their own, the
# same in ENGINE and in the RACK. Drawn from this machine's Ryzen card and Bluetooth headset, the
# frames as the face draws them now (band, IN/OUT, VOLUME · BAL, SIG, hex jacks).
#   python3 hwframes.py hwframes.svg && rsvg-convert hwframes.svg -o hwframes.png
import math
import sys

import nodes as n
from nodes import F, M, ROLE, text, o

INK = "#f3efe6"
ALU = n.FIN["ALUMINIUM"]  # base, print, dim

# A block's colour: its kind (media.class). A hardware node is one of these.
BLOCKS = [
    ("SINK", "Audio/Sink", ROLE["PLAYBACK"], "sound goes in to be heard: speakers, headphones, HDMI"),
    ("SOURCE", "Audio/Source", ROLE["CAPTURE"], "sound comes out of it: a mic, a line in"),
    ("VIDEO SOURCE", "Video/Source", ROLE["VIDEO"], "a camera"),
]
# A group of ports' colour: what its ports are for.
GROUPS = [
    ("PLAYBACK", "a sink's inputs"),
    ("MONITOR", "a sink's monitor outputs (port.monitor)"),
    ("CAPTURE", "a source's outputs"),
    ("VIDEO", "a camera's output"),
]


# --- ENGINE's block ---------------------------------------------------------------------------

def eport(x, y, name, colour, right):
    """A port in ENGINE: a pill tinted in its group's colour, its dot on the block's edge."""
    w = 7 * len(name) + 18
    px = x - w - 4 if right else x + 4
    o.append(f'<rect x="{px}" y="{y - 9}" width="{w}" height="18" rx="9" fill="{colour}" fill-opacity=".28"/>')
    text(px + (w - 9 if right else 9), y + 4, name, 11, "#c9cbcd", anchor="end" if right else "start", font=M)
    o.append(f'<circle cx="{x}" cy="{y}" r="4.5" fill="{colour}" stroke="#0f1011" stroke-width="1.5"/>')


def eblock(x, y, name, nid, cls, colour, ins, outs, w=330):
    """A block as ENGINE draws it: its outline and cap in the block's colour; each port in its group's."""
    rows = max(len(ins), len(outs))
    h = 50 + rows * 22 + 6
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="8" fill="#1d1e21" stroke="{colour}" stroke-width="2.2"/>')
    o.append(f'<path d="M{x},{y + 8} a8,8 0 0 1 8,-8 h{w - 16} a8,8 0 0 1 8,8 v-2 h-{w} z" fill="{colour}"/>')
    o.append(f'<circle cx="{x + 16}" cy="{y + 22}" r="4.5" fill="#4a4c50"/>')
    text(x + 28, y + 27, name, 14, "#e6e7e8", True)
    text(x + 16, y + 44, f"{nid} · {cls}", 11.5, "#7d8085", font=M)
    for k, (p, c) in enumerate(ins):
        eport(x, y + 62 + k * 22, p, ROLE[c], False)
    for k, (p, c) in enumerate(outs):
        eport(x + w, y + 62 + k * 22, p, ROLE[c], True)
    return h


# --- the face's frame, as it is drawn now ------------------------------------------------------

FH, BAND = 150, 26
JACK_W, MIX_W, METER_W = 92, 176, 40


def pt(x, y, a, r):
    a = math.radians(a)
    return f"{x + r * math.sin(a):.1f},{y - r * math.cos(a):.1f}"


def hexjack(cx, cy, ring):
    """A 6.3 mm jack: its hex nut, its coloured ring, the hole."""
    pts = " ".join(pt(cx, cy, 30 + 60 * k, 22) for k in range(6))
    o.append(f'<polygon points="{pts}" fill="url(#nut)" stroke="#3a3b3e" stroke-width="1.2"/>')
    o.append(f'<circle cx="{cx}" cy="{cy}" r="14.5" fill="#26272a"/><circle cx="{cx}" cy="{cy}" r="12.5" fill="none" stroke="{ring}" stroke-width="3.6"/>')
    o.append(f'<circle cx="{cx}" cy="{cy}" r="8" fill="#0b0b0c"/><circle cx="{cx - 4}" cy="{cy - 4}" r="2" fill="#fff" opacity=".15"/>')


def knob(cx, cy, r, a):
    o.append(f'<circle cx="{cx}" cy="{cy + 2}" r="{r + 1}" fill="#000" opacity=".35"/><circle cx="{cx}" cy="{cy}" r="{r}" fill="url(#knob)" stroke="#0c0c0d"/>')
    o.append(f'<path d="M{pt(cx, cy, a, r * .3)} L{pt(cx, cy, a, r * .85)}" stroke="#ffb02e" stroke-width="{3 if r > 20 else 2.4}" stroke-linecap="round"/>')


def key(x, y, w, label, led=None):
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="24" rx="3" fill="#1b1c1e" stroke="rgba(255,255,255,.18)"/>')
    if led is None:
        text(x + w / 2, y + 16.5, label, 11, "#e6e7e8", True, "middle", 'letter-spacing=".8"')
    else:
        o.append(f'<circle cx="{x + 13}" cy="{y + 12}" r="5" fill="{"#ff3b2f" if led else "#5a1a14"}" stroke="#000" stroke-opacity=".6"/>')
        text(x + w / 2 + 8, y + 16.5, label, 11, "#e6e7e8", True, "middle", 'letter-spacing=".8"')


def face_frame(x, y, fr):
    """A frame as the face draws it: outline and band in the block's colour, the name and IN or
    OUT on the band, its jacks (each ring in its group's colour), VOLUME · BAL, SPLIT and MUTE, SIG."""
    c = fr["colour"]
    mix = fr.get("mix")
    has_meter = fr.get("meter", True)
    # the jacks' part is wide enough for the name and its tag on the band above it
    zone = max(len(fr["jacks"]) * JACK_W, len(fr["name"]) * 11.2 + 14 + 14 + 9 * len(fr["tag"]) + 10)
    jw = zone / len(fr["jacks"])
    w = 16 + zone + (MIX_W if mix else 0) + (METER_W if has_meter else 0) + 10
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="{FH}" rx="9" fill="#000" fill-opacity=".04" stroke="{c}" stroke-width="2.4"/>')
    o.append(f'<path d="M{x},{y + 9} a9,9 0 0 1 9,-9 h{w - 18} a9,9 0 0 1 9,9 v{BAND - 9} h-{w} z" fill="{c}"/>')
    text(x + 16, y + 18.5, fr["name"], 13.5, INK, True, extra='letter-spacing="1.6"')
    tx = x + 16 + len(fr["name"]) * 11.2 + 14
    tag = fr["tag"]
    tw = 14 + 9 * len(tag)
    if tag == "IN":
        o.append(f'<rect x="{tx}" y="{y + 5}" width="{tw}" height="17" rx="3" fill="none" stroke="{INK}" stroke-width="1.6"/>')
        text(tx + tw / 2, y + 18, tag, 11.5, INK, True, "middle", 'letter-spacing="1"')
    else:
        o.append(f'<rect x="{tx}" y="{y + 5}" width="{tw}" height="17" rx="3" fill="{INK}"/>')
        text(tx + tw / 2, y + 18, tag, 11.5, c, True, "middle", 'letter-spacing="1"')
    jx = x + 16
    for k, (label, ring) in enumerate(fr["jacks"]):
        if k:
            o.append(f'<path d="M{jx},{y + BAND + 10} V{y + FH - 12}" stroke="{ALU[1]}" stroke-opacity=".4" stroke-dasharray="2 3"/>')
        hexjack(jx + jw / 2, y + 78, ROLE[ring])
        text(jx + jw / 2, y + 124, label, 12, ALU[1], True, "middle", 'letter-spacing=".5"')
        jx += jw
    if mix:
        name, st, v, mute = mix
        mx = jx + 4
        o.append(f'<path d="M{mx},{y + BAND + 8} V{y + FH - 10}" stroke="#000" stroke-opacity=".35"/><path d="M{mx + 1},{y + BAND + 8} V{y + FH - 10}" stroke="#fff" stroke-opacity=".35"/>')
        text(mx + MIX_W / 2, y + 18.5, f"{name} · BAL" if st else name, 13.5, INK, True, "middle", 'letter-spacing="1.4"')
        kx, ky = mx + (58 if st else MIX_W / 2), y + 70
        va = lambda u: -135 + 270 * min(u, 1.53) / 1.53
        o.append(f'<path d="M{pt(kx, ky, va(1), 32)} A32,32 0 0 1 {pt(kx, ky, va(1.53), 32)}" fill="none" stroke="#d2402f" stroke-width="3"/>')
        for i in range(13):
            a = va(i / 12 * 1.53)
            o.append(f'<path d="M{pt(kx, ky, a, 29)} L{pt(kx, ky, a, 33 if i % 4 == 0 else 31)}" stroke="{ALU[1]}" stroke-width="1.2" opacity=".7"/>')
        o.append(f'<path d="M{pt(kx, ky, va(1), 27)} L{pt(kx, ky, va(1), 37)}" stroke="{ALU[1]}" stroke-width="2.2"/>')
        knob(kx, ky, 23, va(v))
        if st:
            bx, by = mx + 136, y + 64
            for i in range(-4, 5):
                o.append(f'<path d="M{pt(bx, by, i * 33.75, 18)} L{pt(bx, by, i * 33.75, 24 if i == 0 else 21)}" stroke="{ALU[1]}" stroke-width="{2 if i == 0 else 1.1}" opacity="{1 if i == 0 else .7}"/>')
            knob(bx, by, 14, 0)
            text(bx - 18, by + 30, "L", 11, ALU[1], True, "middle")
            text(bx + 18, by + 30, "R", 11, ALU[1], True, "middle")
        if fr.get("split", True):
            key(mx + 12, y + FH - 36, 70, "SPLIT")
            key(mx + 90, y + FH - 36, 74, "MUTE", led=mute)
        else:
            key(mx + (MIX_W - 74) / 2, y + FH - 36, 74, "MUTE", led=mute)
        jx = mx + MIX_W
    if has_meter:
        text(jx + METER_W / 2, y + 18.5, "SIG", 12, INK, True, "middle", 'letter-spacing="1"')
        lit = round(fr.get("lvl", 0) * 10)
        for i in range(10):
            col = "#3fae5a" if i < 7 else ("#e9c53a" if i < 9 else "#d23b32")
            for k in range(fr.get("cols", 2)):
                on = i < lit - k
                o.append(f'<rect x="{jx + 8 + k * 13}" y="{y + FH - 22 - i * 10.5}" width="10" height="7" rx="1.2" fill="{col if on else "#3a3b3e"}" stroke="#000" stroke-opacity=".35"/>')
    return w


def unit(x, y, w, h):
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="10" fill="{ALU[0]}" stroke="#000" stroke-opacity=".6"/>')
    for sy in (y + 16, y + h - 16):
        sx = x + w - 18
        o.append(f'<circle cx="{sx}" cy="{sy}" r="7" fill="#8b8e93" stroke="#222"/><path d="M{sx - 4},{sy - 4} l8,8 M{sx + 4},{sy - 4} l-8,8" stroke="#333" stroke-width="1.6"/>')


CASES = [
    ("RYZEN HD AUDIO CONTROLLER · ITS SINK", "One block, two groups: playback in, monitor out. Block and frame in the sink's teal; the monitor's ports violet.",
     ("Ryzen HD Audio Controller", 50, "Audio/Sink", ROLE["PLAYBACK"],
      [("playback_FL", "PLAYBACK"), ("playback_FR", "PLAYBACK")], [("monitor_FL", "MONITOR"), ("monitor_FR", "MONITOR")]),
     {"name": "SPEAKERS", "tag": "IN", "colour": ROLE["PLAYBACK"], "jacks": [("STEREO", "PLAYBACK"), ("MONITOR", "MONITOR")],
      "mix": ("VOLUME", True, 0.0, False)}),
    ("RYZEN HD AUDIO CONTROLLER · ITS SOURCE", "One block, one group: capture out. Blue, block and ports alike.",
     ("Ryzen HD Audio Controller", 51, "Audio/Source", ROLE["CAPTURE"], [], [("capture_FL", "CAPTURE"), ("capture_FR", "CAPTURE")]),
     {"name": "INTERNAL MICROPHONE", "tag": "OUT", "colour": ROLE["CAPTURE"], "jacks": [("STEREO", "CAPTURE")],
      "mix": ("GAIN", True, .21, False)}),
    ("JLAB GO WORK 2 · ITS SINK", "The same kind of block as the card's, so the same colours: hardware is hardware, by PCI, USB or Bluetooth.",
     ("JLab GO Work 2", 247, "Audio/Sink", ROLE["PLAYBACK"],
      [("playback_FL", "PLAYBACK"), ("playback_FR", "PLAYBACK")], [("monitor_FL", "MONITOR"), ("monitor_FR", "MONITOR")]),
     {"name": "HEADPHONES", "tag": "IN", "colour": ROLE["PLAYBACK"], "jacks": [("STEREO", "PLAYBACK"), ("MONITOR", "MONITOR")],
      "mix": ("VOLUME", True, .55, False), "lvl": .6}),
    ("JLAB GO WORK 2 · ITS MIC", "A mono source: one meter column, no SPLIT, no BAL.",
     ("JLab GO Work 2", 95, "Audio/Source", ROLE["CAPTURE"], [], [("capture_MONO", "CAPTURE")]),
     {"name": "HANDSFREE", "tag": "OUT", "colour": ROLE["CAPTURE"], "jacks": [("MONO", "CAPTURE")],
      "mix": ("GAIN", False, 1.0, False), "split": False, "cols": 1, "lvl": .3}),
    ("A CAMERA (NOT ON THIS MACHINE)", "A Video/Source: ochre, its one port too. No meter, no SPLIT, no volume: PipeWire has none for video.",
     ("Integrated Camera", 60, "Video/Source", ROLE["VIDEO"], [], [("capture_1", "VIDEO")]),
     {"name": "CAMERA", "tag": "OUT", "colour": ROLE["VIDEO"], "jacks": [("VIDEO", "VIDEO")], "meter": False}),
]

DEFS = ('<defs><radialGradient id="nut" cx=".35" cy=".3" r=".9"><stop offset="0" stop-color="#e4e6e8"/><stop offset="1" stop-color="#7d8084"/></radialGradient>'
        '<radialGradient id="knob" cx=".38" cy=".3" r=".8"><stop offset="0" stop-color="#5a5b5f"/><stop offset="1" stop-color="#151618"/></radialGradient></defs>')

if __name__ == "__main__":
    W, y = 1560, 150
    text(60, y, "BLOCKS: A COLOUR A KIND", 20, "#e58f2c", True, extra='letter-spacing="1.5"')
    text(60, y + 24, "The frame's outline and band on the rack, the block's outline and cap in ENGINE. Hardware's kinds:", 14, "#9a9da1")
    for k, (name, cls, c, what) in enumerate(BLOCKS):
        x = 60 + k * 470
        o.append(f'<rect x="{x}" y="{y + 44}" width="130" height="24" rx="4" fill="{c}"/>')
        text(x + 65, y + 60.5, name, 11, INK, True, "middle", 'letter-spacing="1.2"')
        text(x + 140, y + 61, f"{cls}  {c}", 12.5, "#e6e7e8", font=M)
        text(x, y + 88, what, 13, "#9a9da1")
    y += 130
    text(60, y, "GROUPS OF PORTS: A COLOUR A GROUP", 20, "#e58f2c", True, extra='letter-spacing="1.5"')
    text(60, y + 24, "The jack's ring on the rack; the port's dot and pill in ENGINE. A block's main group has the block's colour.", 14, "#9a9da1")
    for k, (g, what) in enumerate(GROUPS):
        x = 60 + k * 360
        n.jack(x + 14, y + 56, ROLE[g], 13)
        text(x + 36, y + 54, g, 13, "#e6e7e8", True, extra='letter-spacing="1"')
        text(x + 36, y + 72, ROLE[g], 12, "#9a9da1", font=M)
        text(x, y + 96, what, 13, "#9a9da1")
    y += 150
    text(60, y, "ENGINE", 16, "#e6e7e8", True, extra='letter-spacing="2"')
    text(450, y, "RACK", 16, "#e6e7e8", True, extra='letter-spacing="2"')
    y += 24
    for title, sub, (ename, nid, cls, bc, ins, outs), fr in CASES:
        text(60, y + 14, title, 15, "#e58f2c", True, extra='letter-spacing="1"')
        text(60, y + 34, sub, 13.5, "#9a9da1")
        y += 54
        h = eblock(60, y, ename, nid, cls, bc, ins, outs)
        at = len(o)
        fw = face_frame(470, y + 16, fr)
        o.insert(at, "")  # the unit goes under the frame
        unit_at = len(o)
        unit(450, y, fw + 70, FH + 32)
        o[at] = "".join(o[unit_at:])
        del o[unit_at:]
        y += max(h, FH + 32) + 44
    for k, t in enumerate([
        "WHAT IT TAKES",
        "· On the rack, hardware's frames already look like this: a sink's frame teal, a source's blue, the monitor's jack violet. Nothing to change there.",
        "· ENGINE changes to match: a block's colour by its kind (a sink teal, a source blue, a camera ochre), not one DEVICE blue for all of them;",
        "  and its ports by their group (playback teal, monitor violet, capture blue), not the block's colour.",
        "· One table of colours, used by both: a block's colour is its main group's, so one block reads one colour from afar, its other groups close up.",
        "· Next, the same for programs and PipeWire's own nodes: their kinds of block and their groups (INPUT, OUTPUT, MIDI…).",
    ]):
        text(60, y + k * 26, t, 20 if k == 0 else 15, "#e58f2c" if k == 0 else "#c9cbcd", k == 0)
    y += 6 * 26 + 40
    head = f'{DEFS}<rect width="{W}" height="{y}" fill="#1f2023"/>'
    head += f'<text x="60" y="70" {F} font-weight="bold" font-size="40" letter-spacing="3" fill="#e58f2c">HARDWARE: A FRAME IS ITS BLOCK</text>'
    head += f'<text x="60" y="108" {F} font-size="20" fill="#9a9da1">The same colours in ENGINE and on the rack: a block\'s by its kind, its ports\' by their group.</text>'
    open(sys.argv[1] if len(sys.argv) > 1 else "hwframes.svg", "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{y}" viewBox="0 0 {W} {y}">{head}{"".join(o)}</svg>\n')
