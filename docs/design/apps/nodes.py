# One frame a node: the rack drawn from a live graph (Easy Effects, fooyin, Hydrogen, Kadence,
# Soundcraft, a Bluetooth headset, the laptop's card, the MIDI bridges) with every node of a device
# under its roof and every port of a node in its one frame. Then what it takes to cover any graph:
# how many kinds of device, of jack and of frame.
#   python3 nodes.py nodes.svg, then render at full size (-z 1): nodes-rack.png is the rack, nodes-types.png the rest
import sys

F = 'font-family="Noto Sans"'
M = 'font-family="Noto Sans Mono"'
INK = "#f3efe6"
PRINT = "#c9cbcd"
DIM = "#8d9095"
ROLE = {  # face::ACCENTS
    "PLAYBACK": "#007c7d", "CAPTURE": "#206ea6", "INPUT": "#427935", "OUTPUT": "#7e5a42",
    "MONITOR": "#695ca6", "MIDI": "#934c7e", "VIDEO": "#876200", "OTHER": "#cba553",
}
TAG = {"in": "IN", "out": "OUT", "mon": "MON"}
# a device's paint, one for each kind (paints.py, variant D): base, print, and the dim print under frames
FIN = {
    "ALUMINIUM": ("#c3c6ca", "#1d1e20", "#4a4d52"),
    "BLACK": ("#232427", "#d5d7d9", "#8d9095"),
    "OLIVE": ("#66683f", "#f1f1e6", "#cfd0b8"),
}
ROOF_FIN = {"HARDWARE": "ALUMINIUM", "PROGRAM": "BLACK", "NODE": "OLIVE"}
ink = {"print": PRINT, "dim": DIM}  # the paint's print, for the device being drawn


def finish(d):
    """Hardware is aluminium, a program black, a node olive: the paint says whose it is, nothing else."""
    return ROOF_FIN[d["roof"]]


JW = 30          # one jack's pitch
FH = 102         # a frame's height
ROW = 136        # a row of frames on a unit
METER = 44       # a stereo zone's right edge: its meter, and SPLIT under it
VOL_MAX = 1.53   # the VOLUME knob's end (graph::VOL_MAX)
GAP = 30         # between frames: room for a short cord
PLATE = 210      # the unit's plate: its screen
W = 2100
RX, RW = 90, W - 180

o, top = [], []  # what is drawn; cords go on top
jacks = {}       # (frame, zone, port, i) -> (x, y)
clip = [0]


def text(x, y, t, size=10, fill=PRINT, bold=False, anchor="start", extra="", font=F):
    w = ' font-weight="bold"' if bold else ""
    t = str(t).replace("&", "&amp;").replace("<", "&lt;")
    o.append(f'<text x="{x:.1f}" y="{y:.1f}" {font} font-size="{size}"{w} fill="{fill}" text-anchor="{anchor}" {extra}>{t}</text>')


def tw(t, size):
    """About how wide a bold caps text is."""
    return len(t) * size * .72


def tag(x, y, t, fill):
    w = 8 + 6.2 * len(t)
    o.append(f'<rect x="{x}" y="{y - 8}" width="{w}" height="11" rx="2" fill="none" stroke="{fill}" stroke-width="1.2"/>')
    text(x + w / 2, y + .5, t, 7.5, fill, True, "middle", 'letter-spacing=".8"')


def jack(cx, cy, ring, r=12):
    o.append(f'<circle cx="{cx:.1f}" cy="{cy:.1f}" r="{r}" fill="#2b2c2f" stroke="#111" stroke-width="2"/>')
    o.append(f'<circle cx="{cx:.1f}" cy="{cy:.1f}" r="{r * .74:.1f}" fill="none" stroke="{ring}" stroke-width="2.6"/>')
    o.append(f'<circle cx="{cx:.1f}" cy="{cy:.1f}" r="{r * .4:.1f}" fill="#0c0c0d"/>')


# --- the data: what PipeWire says, node by node ----------------------------------------------

def P(key, label, n=1, kind="audio"):
    """A port (or a joined stereo pair) and its mult: n jacks, one of them always free."""
    return {"key": key, "label": label, "n": n, "kind": kind}


def Z(d, role, *ports, lvl=0.0, split=False):
    """A zone: its ports, how loud they are now (0..1 of the meter), and whether SPLIT is on
    (then its stereo ports are L over R)."""
    return {"d": d, "role": role, "ports": list(ports), "lvl": lvl, "split": split}


def N(fid, name, cls, *zones, mix=None):
    """A node: one frame. Its zones are IN, OUT and MON, in that order, those it has ports for.
    mix: the node's own volume, if PipeWire gives it one (Props): (VOLUME or GAIN, stereo,
    its channelVolumes, mute)."""
    return {"id": fid, "name": name, "cls": cls, "zones": list(zones), "mix": mix}


def VOL(v=1.0, stereo=True, mute=False):
    return ("VOLUME", stereo, v, mute)


def GAIN(v=1.0, stereo=True, mute=False):
    return ("GAIN", stereo, v, mute)


ST = "STEREO"
DEVICES = [
    {"name": "FOOYIN", "roof": "PROGRAM", "kind": "APP · PLAYBACK", "res": "1 NODE  2 PORTS", "c": "#D4232A",
     "blocks": [[N("fooyin", "PLAYBACK", "Stream/Output/Audio", Z("out", "OUTPUT", P("st", ST, 2), lvl=.62), mix=VOL())]]},
    {"name": "HYDROGEN", "roof": "PROGRAM", "kind": "APP · PLAYBACK", "res": "1 NODE  2 PORTS", "c": "#EC7A1C",
     "blocks": [[N("hydrogen", "OUTPUT", "Stream/Output/Audio", Z("out", "OUTPUT", P("st", ST, 2), lvl=.72), mix=VOL())]]},
    {"name": "KADENCE", "roof": "PROGRAM", "kind": "APP · PLAYBACK", "res": "1 NODE  2 PORTS", "c": "#F0CC1C",
     "blocks": [[N("kadence", "ALSA PLAYBACK", "Stream/Output/Audio", Z("out", "OUTPUT", P("st", ST, 2), lvl=.5), mix=VOL())]]},
    {"name": "SOUNDCRAFT", "roof": "PROGRAM", "kind": "APP · PLAYBACK", "res": "1 NODE  2 PORTS", "c": "#14965A",
     "blocks": [[N("soundcraft", "ALSA PLAYBACK", "Stream/Output/Audio", Z("out", "OUTPUT", P("st", ST, 2), lvl=.45), mix=VOL())]]},
    {"name": "EASY EFFECTS", "roof": "PROGRAM", "kind": "APP · RECORD & PLAY", "res": "7 NODES  26 PORTS", "c": "#12A5B8",
     "blocks": [
         # node.link-group ee_sink_group, along its links
         [N("ees", "EASY EFFECTS SINK", "Audio/Sink", Z("in", "PLAYBACK", P("st", ST, 5), lvl=.8), Z("mon", "MONITOR", P("st", ST, 2), lvl=.8), mix=VOL()),
          N("soe_sp", "SPECTRUM", "no media.class", Z("in", "INPUT", P("st", ST, 2), lvl=.8), Z("out", "OUTPUT", P("st", ST, 2), lvl=.8)),
          N("soe_lv", "OUTPUT LEVEL METER", "no media.class", Z("in", "INPUT", P("st", ST, 2), lvl=.8), Z("out", "OUTPUT", P("st", ST, 2), lvl=.8))],
         # no link group
         [N("test", "EASY EFFECTS FILTER", "no media.class", Z("out", "OUTPUT", P("st", ST, 1)))],
         # node.link-group ee_source_group
         [N("sie_sp", "SPECTRUM", "no media.class", Z("in", "INPUT", P("l", "L", 2), P("r", "R", 2), lvl=.3, split=True), Z("out", "OUTPUT", P("st", ST, 2), lvl=.3)),
          N("sie_lv", "OUTPUT LEVEL METER", "no media.class", Z("in", "INPUT", P("st", ST, 2), lvl=.3), Z("out", "OUTPUT", P("st", ST, 2), lvl=.3)),
          N("ees_src", "EASY EFFECTS SOURCE", "Audio/Source/Virtual", Z("in", "INPUT", P("st", ST, 2), lvl=.3), Z("out", "CAPTURE", P("st", ST, 1), lvl=.3), mix=GAIN())],
     ]},
    {"name": "JLAB GO WORK 2", "roof": "HARDWARE", "kind": "BLUETOOTH · HEADSET", "res": "2 NODES  5 PORTS", "c": "#1752C8",
     "blocks": [
         [N("jl_sink", "PLAYBACK", "Audio/Sink", Z("in", "PLAYBACK", P("st", ST, 2), lvl=.6), Z("mon", "MONITOR", P("st", ST, 1), lvl=.6), mix=VOL(.166378 ** (1 / 3)))],
         # its mic: the output of WirePlumber's loopback; the loopback's internal capture stream is
         # kept off the rack (SYSTEM), as now
         [N("jl_src", "CAPTURE", "Audio/Source", Z("out", "CAPTURE", P("mono", "MONO", 3), lvl=.3), mix=GAIN(stereo=False))],
     ]},
    {"name": "RYZEN HD AUDIO CONTROLLER", "roof": "HARDWARE", "kind": "CARD · ANALOG STEREO DUPLEX", "res": "2 NODES  6 PORTS", "c": "#7B4FB8",
     "blocks": [[N("ry_sink", "PLAYBACK", "Audio/Sink", Z("in", "PLAYBACK", P("st", ST, 1)), Z("mon", "MONITOR", P("st", ST, 1)), mix=VOL(0.0))],
                [N("ry_src", "CAPTURE", "Audio/Source", Z("out", "CAPTURE", P("st", ST, 1)), mix=GAIN(.01))]]},
    {"name": "RADEON HD AUDIO CONTROLLER", "roof": "HARDWARE", "kind": "CARD · OFF", "res": "NO NODES", "c": "#D63C8C", "blocks": []},
    {"name": "MIDI-BRIDGE", "roof": "NODE", "kind": "NODE · MIDI", "res": "1 NODE  4 PORTS", "c": "#EC7A1C",
     "blocks": [[N("mb", "MIDI", "Midi/Bridge",
                   Z("in", "MIDI", P("thru", "MIDI THROUGH", 1, "midi"), P("hyd", "HYDROGEN", 1, "midi")),
                   Z("out", "MIDI", P("thru", "MIDI THROUGH", 1, "midi"), P("hyd", "HYDROGEN", 1, "midi")))]]},
    {"name": "BLE MIDI 1", "roof": "NODE", "kind": "NODE · MIDI", "res": "1 NODE  2 PORTS", "c": "#12A5B8",
     "blocks": [[N("ble", "MIDI", "Midi/Bridge", Z("in", "MIDI", P("in", "MIDI", 1, "midi")), Z("out", "MIDI", P("out", "MIDI", 1, "midi")))]]},
]

# (frame, zone, port, jack) -> (frame, zone, port, jack): the links, each a cord
CORDS = [
    (("fooyin", "out", "st", 0), ("ees", "in", "st", 0)),
    (("hydrogen", "out", "st", 0), ("ees", "in", "st", 1)),
    (("kadence", "out", "st", 0), ("ees", "in", "st", 2)),
    (("soundcraft", "out", "st", 0), ("ees", "in", "st", 3)),
    (("ees", "mon", "st", 0), ("soe_sp", "in", "st", 0)),
    (("soe_sp", "out", "st", 0), ("soe_lv", "in", "st", 0)),
    (("soe_lv", "out", "st", 0), ("jl_sink", "in", "st", 0)),
    (("jl_src", "out", "mono", 0), ("sie_sp", "in", "l", 0)),
    (("jl_src", "out", "mono", 1), ("sie_sp", "in", "r", 0)),
    (("sie_sp", "out", "st", 0), ("sie_lv", "in", "st", 0)),
    (("sie_lv", "out", "st", 0), ("ees_src", "in", "st", 0)),
]


# --- layout and drawing -----------------------------------------------------------------------

def port_w(p):
    return max(p["n"] * JW, tw(p["label"], 7.5) + 6)


def audio(z):
    return any(p["kind"] == "audio" for p in z["ports"])


def stereo(z):
    return z["split"] or any(p["label"] == ST for p in z["ports"])


def edge(z):
    """A zone's right edge: a meter for audio (and SPLIT under it for stereo), none for MIDI."""
    return METER if stereo(z) else (22 if audio(z) else 0)


def jacks_w(z):
    if z["split"]:  # L over R: one row a port, its label at its left
        return max(p["n"] for p in z["ports"]) * JW + 20
    return sum(port_w(p) for p in z["ports"]) + 8 * (len(z["ports"]) - 1)


def zone_w(z):
    return max(jacks_w(z) + edge(z), 44 + edge(z)) + 16


def mix_w(f):
    return 0 if not f["mix"] else (100 if f["mix"][1] else 64)


def frame_w(f, cap=330):
    need = sum(zone_w(z) for z in f["zones"]) + mix_w(f)
    return max(need, min(tw(f["name"], 9.5) + 30, cap))


def fit(name, w, size=9.5):
    if tw(name, size) <= w:
        return name
    while name and tw(name + "…", size) > w:
        name = name[:-1]
    return name.rstrip() + "…"


def key(x, y, w, label, led=None):
    o.append(f'<rect x="{x:.1f}" y="{y:.1f}" width="{w}" height="12" rx="2.5" fill="#1b1c1e" stroke="rgba(255,255,255,.18)"/>')
    if led is None:
        text(x + w / 2, y + 8.8, label, 6.8, "#e6e7e8", True, "middle", 'letter-spacing=".6"')
    else:
        o.append(f'<circle cx="{x + 8:.1f}" cy="{y + 6:.1f}" r="2.6" fill="{"#ff3b2f" if led else "#3a1411"}" stroke="rgba(0,0,0,.6)" stroke-width=".6"/>')
        text(x + w / 2 + 4, y + 8.8, label, 6.8, "#e6e7e8", True, "middle", 'letter-spacing=".6"')


def meter(x, y0, cols, lvl):
    """LEDs from the bottom (y0) up: a column a channel, lit as loud as it plays."""
    n = 12
    for i in range(n):
        c = "#3fae5a" if i < 8 else ("#e9c53a" if i < 10 else "#d23b32")
        for k in range(cols):
            on = i < round(lvl * n) - k
            o.append(f'<rect x="{x + k * 6:.1f}" y="{y0 - i * 4.4:.1f}" width="4.4" height="3" fill="{c if on else "#2a2b2e"}"/>')


def polar(x, y, a, r):
    import math
    a = math.radians(a)
    return f"{x + r * math.sin(a):.2f},{y - r * math.cos(a):.2f}"


def knob(x, y, r, a):
    o.append(f'<circle cx="{x}" cy="{y + 1.5}" r="{r + .5}" fill="#111" opacity=".5"/><circle cx="{x}" cy="{y}" r="{r}" fill="#3a3b3f" stroke="#0c0c0d"/>'
             f'<circle cx="{x}" cy="{y}" r="{r * .75:.1f}" fill="none" stroke="rgba(255,255,255,.08)"/>'
             f'<path d="M{polar(x, y, a, r * .29)} L{polar(x, y, a, r * .86)}" stroke="#ffb02e" stroke-width="2.2" stroke-linecap="round"/>')


def mix(x, y, w, m):
    """The node's own VOLUME (GAIN on a capture), BAL for a stereo one, MUTE: from its Props."""
    name, st, v, mute = m
    o.append(f'<path d="M{x:.1f},{y + 22} V{y + FH - 4}" stroke="rgba(0,0,0,.6)"/><path d="M{x + 1:.1f},{y + 22} V{y + FH - 4}" stroke="rgba(255,255,255,.1)"/>')
    text(x + w / 2, y + 32, f"{name} · BAL" if st else name, 7.5, PRINT, True, "middle", 'letter-spacing=".8"')
    cx, cy = x + (30 if st else w / 2), y + 57
    va = lambda u: -135 + 270 * min(u, VOL_MAX) / VOL_MAX
    o.append(f'<path d="M{polar(cx, cy, va(1), 19.5)} A19.5,19.5 0 0 1 {polar(cx, cy, va(VOL_MAX), 19.5)}" fill="none" stroke="#d2402f" stroke-width="2.2" opacity=".85"/>')
    for i in range(13):
        a = va(i / 12 * VOL_MAX)
        o.append(f'<path d="M{polar(cx, cy, a, 17.5)} L{polar(cx, cy, a, 21 if i % 4 == 0 else 19.5)}" stroke="{PRINT}" stroke-width=".9" opacity=".6"/>')
    o.append(f'<path d="M{polar(cx, cy, va(1), 16.5)} L{polar(cx, cy, va(1), 23.5)}" stroke="{PRINT}" stroke-width="1.6"/>')
    knob(cx, cy, 14, va(v))
    if st:
        bx, by = x + 76, y + 54
        for i in range(-4, 5):
            a = i * 33.75
            o.append(f'<path d="M{polar(bx, by, a, 11.5)} L{polar(bx, by, a, 16 if i == 0 else 13.5)}" stroke="{PRINT}" stroke-width="{1.5 if i == 0 else .9}" opacity="{1 if i == 0 else .6}"/>')
        knob(bx, by, 9, 0)
        text(bx - 12, by + 19, "L", 7, PRINT, True, "middle")
        text(bx + 12, by + 19, "R", 7, PRINT, True, "middle")
    key(x + 8, y + 83, w - 16, "MUTE", led=mute)


def frame(x, y, f, caption=True):
    w = frame_w(f)
    zs = f["zones"]
    mw = mix_w(f)
    extra = (w - mw - sum(zone_w(z) for z in zs)) / len(zs)
    clip[0] += 1
    cid = f"c{clip[0]}"
    o.append(f'<clipPath id="{cid}"><rect x="{x}" y="{y}" width="{w}" height="{FH}" rx="6"/></clipPath>')
    block = f.get("block")  # hwframes.py: the frame in its block's colour, as ENGINE draws the block
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="{FH}" rx="6" fill="#1d1e21" fill-opacity=".92" stroke="{block or "#77736b"}" stroke-width="{2 if block else 1.3}"/>')
    if block:
        o.append(f'<rect x="{x}" y="{y}" width="{w}" height="18" fill="{block}" clip-path="url(#{cid})"/>')
    zx = x
    spans = []
    for z in zs:
        zw = zone_w(z) + extra
        spans.append((zx, zw, z))
        if block:  # each group of ports: a stripe of its colour under the band
            o.append(f'<rect x="{zx + 4:.1f}" y="{y + 20}" width="{zw - 8:.1f}" height="3" rx="1.5" fill="{ROLE[z["role"]]}"/>')
        else:
            o.append(f'<rect x="{zx:.1f}" y="{y}" width="{zw:.1f}" height="18" fill="{ROLE[z["role"]]}" clip-path="url(#{cid})"/>')
        zx += zw
    if mw and not block:  # the node's controls: a band of their own, dark
        o.append(f'<rect x="{zx:.1f}" y="{y}" width="{mw}" height="18" fill="#4a4b50" clip-path="url(#{cid})"/>')
    for zx, zw, z in spans[1:]:
        o.append(f'<path d="M{zx:.1f},{y + 22} V{y + FH - 4}" stroke="{PRINT}" stroke-opacity=".35" stroke-dasharray="2 3"/>')
    text(x + w / 2, y + 13, fit(f["name"], w - 16), 9.5, INK, True, "middle", 'letter-spacing="1.2"')
    for zx, zw, z in spans:
        c = ROLE[z["role"]]
        tag(zx + 7, y + 33, TAG[z["d"]], c)
        e = edge(z)
        if e:  # the zone's meter at its right edge, SPLIT (JOIN) under a stereo one
            cols = 2 if stereo(z) else 1
            meter(zx + zw - 8 - (cols * 6 - 1.6) - (10 if stereo(z) else 0), y + 72 if stereo(z) else y + 84, cols, z["lvl"])
            if stereo(z):
                key(zx + zw - 42, y + 83, 34, "JOIN" if z["split"] else "SPLIT")
        area = zw - e
        if z["split"]:
            n = max(p["n"] for p in z["ports"])
            j0 = zx + (area - n * JW - 20) / 2 + 20 + JW / 2
            for r, p in enumerate(z["ports"]):
                cy = y + 46 + r * 28
                text(j0 - 22, cy + 3, p["label"], 8, PRINT, True, "end")
                for i in range(p["n"]):
                    jack(j0 + i * JW, cy, c, 11)
                    jacks[(f["id"], z["d"], p["key"], i)] = (j0 + i * JW, cy)
            continue
        used = jacks_w(z)
        px = zx + (area - used) / 2
        for p in z["ports"]:
            pw = port_w(p)
            ring = ROLE["MIDI"] if p["kind"] == "midi" else c
            j0 = px + (pw - p["n"] * JW) / 2 + JW / 2
            if p["n"] > 1:
                o.append(f'<path d="M{j0:.1f},{y + 73} H{j0 + (p["n"] - 1) * JW:.1f}" stroke="{PRINT}" stroke-opacity=".45" stroke-width="1.5"/>')
            for i in range(p["n"]):
                jx = j0 + i * JW
                jack(jx, y + 58, ring)
                jacks[(f["id"], z["d"], p["key"], i)] = (jx, y + 58)
            text(px + pw / 2, y + 92, p["label"], 7.5, PRINT, True, "middle", 'letter-spacing=".5"')
            px += pw + 8
    if mw:
        mix(x + w - mw, y, mw, f["mix"])
    if caption:
        text(x + w / 2, y + FH + 13, f["cls"], 9, ink["dim"], False, "middle", font=M)
    return w


def unit(y, h, base):
    o.append(f'<rect x="{RX - 34}" y="{y}" width="{RW + 68}" height="{h}" rx="3" fill="{base}" stroke="#000" stroke-opacity=".6"/>')
    for sx in (RX - 20, RX + RW + 20):
        for sy in (y + 14, y + h - 14):
            o.append(f'<circle cx="{sx}" cy="{sy}" r="5" fill="#8b8e93" stroke="#222"/><path d="M{sx - 3},{sy} h6" stroke="#333" stroke-width="1.4"/>')
    o.append(f'<rect x="{RX}" y="{y + 4}" width="{RW}" height="{h - 8}" rx="2" fill="#000" fill-opacity=".06"/>')


def plate(x, y, d):
    o.append(f'<rect x="{x}" y="{y}" width="{PLATE - 24}" height="62" rx="3" fill="#15130c" stroke="#000"/>')
    for k, line in enumerate((d["name"], d["kind"], d["res"])):
        text(x + 8, y + 17 + k * 17, fit(line, PLATE - 40, 9), 9.5 if k == 0 else 8.5, "#f2b13c", k == 0, font=M)
    key(x, y + 70, 34, "HIDE")
    kx = x + 40
    if d["kind"].startswith("CARD") or d["kind"].startswith("BLUETOOTH"):
        key(kx, y + 70, 58, "PROFILE ▾")
        key(kx + 64, y + 70, 44, "PORT ▾")
        kx += 114
    o.append(f'<circle cx="{kx + 8}" cy="{y + 76}" r="5" fill="{d["c"]}" stroke="#111"/>')
    text(kx + 17, y + 79, "CORD", 7, ink["dim"], True)


def rows_of(blocks, room):
    """Blocks (a link group, or a node alone) stay whole; they fill a row while they fit."""
    rows, cur, used = [], [], 0
    for b in blocks:
        bw = sum(frame_w(f) for f in b) + GAP * (len(b) - 1)
        if cur and used + GAP * 1.6 + bw > room:
            rows.append(cur)
            cur, used = [], 0
        used += (GAP * 1.6 if cur else 0) + bw
        cur.append(b)
    if cur:
        rows.append(cur)
    return rows


def device(y, d):
    room = RW - PLATE - 30
    rows = rows_of(d["blocks"], room)
    h = max(len(rows), 1) * ROW + 16
    base, ink["print"], ink["dim"] = FIN[finish(d)]
    unit(y, h, base)
    plate(RX + 14, y + 20, d)
    if not rows:
        text(RX + PLATE + 20, y + 72, "NO NODES: THE CARD'S PROFILE IS OFF. NOTHING TO PLUG.", 11, ink["dim"], True, extra='letter-spacing="1"')
    for r, row in enumerate(rows):
        x = RX + PLATE + 10
        fy = y + 14 + r * ROW
        for b, block in enumerate(row):
            if b:
                x += GAP * 1.6
            if len(block) > 1:  # a link group: a thin bracket under it
                bw = sum(frame_w(f) for f in block) + GAP * (len(block) - 1)
                o.append(f'<path d="M{x},{fy + FH + 20} v4 H{x + bw} v-4" fill="none" stroke="{ink["dim"]}" stroke-opacity=".7"/>')
            for i, f in enumerate(block):
                if i:
                    x += GAP
                x += frame(x, fy, f)
    return h


# --- the rack in the face's own frames (progframes.py): a frame in its kind's colour, its jacks
# in their groups' colours

DUPLEX, NOCLASS = "#73348d", "#cba553"  # kinds.py, variant 4
KIND_OF = {
    "Audio/Sink": ROLE["PLAYBACK"], "Audio/Source": ROLE["CAPTURE"], "Audio/Source/Virtual": ROLE["CAPTURE"],
    "Stream/Output/Audio": ROLE["OUTPUT"], "Stream/Input/Audio": ROLE["INPUT"], "Stream/Input/Audio/Internal": ROLE["INPUT"],
    "Midi/Bridge": ROLE["MIDI"], "Video/Source": ROLE["VIDEO"], "Audio/Duplex": DUPLEX, "no media.class": NOCLASS,
}
# each frame's node id (pw-dump): its GRAPH key says it
NID = {"fooyin": 180, "hydrogen": 133, "kadence": 121, "soundcraft": 295, "ees": 141, "soe_sp": 269, "soe_lv": 102, "test": 288,
       "sie_sp": 196, "sie_lv": 233, "ees_src": 88, "jl_sink": 247, "jl_src": 95, "ry_sink": 50, "ry_src": 51, "mb": 42, "ble": 47}
FH2 = 150         # the face's frame
ROW2 = FH2 + 46   # a row of them: the frame, its media.class under it, a link group's bracket


def real(f):
    """A node as the face's frame: IN and OUT groups; a monitor's jacks beside the input they copy."""
    zones = []
    for z in f["zones"]:
        js = [(p["label"], ROLE["MIDI"] if p["kind"] == "midi" else ROLE["VIDEO"] if p["kind"] == "video" else ROLE[z["role"]], p["n"], (f["id"], z["d"], p["key"]))
              for p in z["ports"]]
        if z["d"] == "mon" and zones and zones[-1]["tag"] == "IN":
            zones[-1]["jacks"] += [("MONITOR",) + j[1:] for j in js]
            continue
        zones.append({"tag": "IN" if z["d"] == "in" else "OUT", "jacks": js, "cols": (2 if stereo(z) else 1) if audio(z) else 0,
                      "lvl": z["lvl"], "split": stereo(z), "on": z["split"]})
    return {"name": f["name"], "bands": [KIND_OF[f["cls"]]], "zones": zones, "mix": f["mix"], "nid": NID.get(f["id"])}


def rw(f):
    """A face frame's width (drawn, measured, taken back)."""
    import progframes as pf
    at = len(o)
    w = pf.frame(0, 0, real(f), FIN["BLACK"])
    del o[at:]
    return w


def rows2(blocks, room):
    rows, cur, used = [], [], 0
    for b in blocks:
        bw = sum(rw(f) for f in b) + GAP * (len(b) - 1)
        if cur and used + GAP * 1.6 + bw > room:
            rows.append(cur)
            cur, used = [], 0
        used += (GAP * 1.6 if cur else 0) + bw
        cur.append(b)
    if cur:
        rows.append(cur)
    return rows


def device2(y, d):
    import progframes as pf
    rows = rows2(d["blocks"], RW - PLATE - 30)
    h = max(len(rows), 1) * ROW2 + 10
    paint = FIN[finish(d)]
    base, ink["print"], ink["dim"] = paint
    unit(y, h, base)
    plate(RX + 14, y + 20, d)
    if not rows:
        text(RX + PLATE + 20, y + 72, "NO NODES: THE CARD'S PROFILE IS OFF. NOTHING TO PLUG.", 11, ink["dim"], True, extra='letter-spacing="1"')
    for r, row in enumerate(rows):
        x = RX + PLATE + 10
        fy = y + 14 + r * ROW2
        for b, block in enumerate(row):
            if b:
                x += GAP * 1.6
            if len(block) > 1:  # a link group: a thin bracket under it
                bw = sum(rw(f) for f in block) + GAP * (len(block) - 1)
                o.append(f'<path d="M{x},{fy + FH2 + 22} v4 H{x + bw} v-4" fill="none" stroke="{ink["dim"]}" stroke-opacity=".7"/>')
            for i, f in enumerate(block):
                if i:
                    x += GAP
                w = pf.frame(x, fy, real(f), paint)
                text(x + w / 2, fy + FH2 + 15, f["cls"], 10, ink["dim"], False, "middle", font=M)
                x += w
    return h


def cord(a, b, colour):
    (x1, y1), (x2, y2) = jacks[a], jacks[b]
    dx, dy = abs(x2 - x1), abs(y2 - y1)
    sag = 34 + .1 * dx + .12 * dy
    d = f'M{x1:.1f},{y1:.1f} C{x1 + 10:.1f},{y1 + sag:.1f} {x2 - 10:.1f},{y2 + sag:.1f} {x2:.1f},{y2:.1f}'
    top.append(f'<path d="{d}" fill="none" stroke="#000" stroke-opacity=".45" stroke-width="8" stroke-linecap="round"/>')
    top.append(f'<path d="{d}" fill="none" stroke="{colour}" stroke-width="5.5" stroke-linecap="round"/>')
    top.append(f'<path d="{d}" fill="none" stroke="#fff" stroke-opacity=".22" stroke-width="1.4" stroke-linecap="round" transform="translate(-1,-1)"/>')
    for x, y in ((x1, y1), (x2, y2)):
        top.append(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="8" fill="{colour}" stroke="#0b0b0b" stroke-width="2"/><circle cx="{x:.1f}" cy="{y:.1f}" r="3" fill="#000" fill-opacity=".45"/>')


def section(y, t, sub=""):
    text(60, y, t, 26, "#e58f2c", True, extra='letter-spacing="2"')
    if sub:
        text(60, y + 30, sub, 16, "#9a9da1")


def lines(x, y, rows, size=15, step=24):
    for k, (t, c, b) in enumerate(rows):
        text(x, y + k * step, t, size, c, b)
    return y + len(rows) * step


def main(argv):
    import progframes as pf
    y = 190
    section(y, "1 · THE RACK, ONE FRAME A NODE", "This machine now. A device: every node of one program or one piece of hardware. A frame: one node, all its ports. A cord: one link.")
    y += 64
    owner = {}
    for d in DEVICES:
        for blk in d["blocks"]:
            for f in blk:
                owner[f["id"]] = d["c"]
        y += device2(y, d) + 6
    for a, b in CORDS:
        cord(a, b, owner[a[0]])
    o.extend(top)
    top.clear()
    y += 20
    text(RX, y, "A frame in its kind's colour, its jacks in their groups' (section 4). Under a frame: its media.class. A bracket under frames: one chain of a program's.", 14, DIM)
    text(RX, y + 22, "Not on the rack: Patchcord's own meters (16 nodes), the Dummy and Freewheel drivers (no ports). Both are in ENGINE.", 14, DIM)
    y += 90

    # 2 · devices
    section(y, "2 · DEVICES: THREE ROOFS, EVERY NODE UNDER ONE", "Asked in this order, so each node gets exactly one answer.")
    y += 70
    kinds = [
        ("HARDWARE", "the node has a device.id", "a card, a Bluetooth headset, a USB interface, a camera",
         "kind from device.api / device.form-factor: CARD, BLUETOOTH, USB, CAMERA", "RYZEN, RADEON, JLAB GO WORK 2"),
        ("PROGRAM", "its client is a program: application.name + binary + pid", "everything a program made: streams, sinks, filters, nodes without a class",
         "kind APP; same names numbered by the client's object.serial", "EASY EFFECTS, FOOYIN, HYDROGEN, KADENCE, SOUNDCRAFT"),
        ("NODE", "neither: PipeWire or WirePlumber made it", "a MIDI bridge, a null sink, a loopback, a filter-chain",
         "one node, one device", "MIDI-BRIDGE, BLE MIDI 1"),
    ]
    for k, (name, rule, what, how, here) in enumerate(kinds):
        x = 60 + k * 560
        o.append(f'<rect x="{x}" y="{y - 26}" width="530" height="170" rx="6" fill="#26272a" stroke="#3a3c40"/>')
        text(x + 18, y + 4, f"{k + 1}  {name}", 20, "#e6e7e8", True, extra='letter-spacing="1.5"')
        p = ROOF_FIN[name]
        o.append(f'<rect x="{x + 512 - 96}" y="{y - 12}" width="96" height="22" rx="3" fill="{FIN[p][0]}" stroke="#000" stroke-opacity=".5"/>')
        text(x + 512 - 48, y + 3, p, 10, FIN[p][1], True, "middle", 'letter-spacing="1"')
        lines(x + 18, y + 34, [("if " + rule, "#e58f2c", True), (what, PRINT, False), (how, PRINT, False), ("here: " + here, DIM, False)], 13.5, 24)
    y += 170
    y = lines(60, y, [
        ("· Off the rack, in the HIDDEN list and ENGINE: Patchcord's own nodes (its meters, the cue: the tool, not the graph), and a Bluetooth headset's", PRINT, False),
        ("  internal capture stream (WirePlumber's plumbing behind its MIC: in A2DP its input waits for a mic that isn't a node yet).", PRINT, False),
    ]) + 50

    # 3 · jacks
    section(y, "3 · JACKS: WHAT A PORT CAN BE", "Four facts of a port: its direction, format.dsp, port.monitor and audio.channel. A jack shows them all.")
    y += 80
    legend = [
        (ROLE["INPUT"], "AUDIO IN", "direction in, 32 bit float mono audio", "STEREO when FL and FR go alike; L, R; MONO; FC, LFE, AUX0…"),
        (ROLE["OUTPUT"], "AUDIO OUT", "direction out, audio", "the same labels; a capture source rings blue, a playback sink teal"),
        (ROLE["MONITOR"], "MONITOR", "direction out, port.monitor = true", "a copy of what goes in: a sink's, and a recording stream's"),
        (ROLE["MIDI"], "MIDI IN / OUT", "8 bit raw midi, or 32 bit raw UMP", "named by its client when a bridge carries many (MIDI THROUGH, HYDROGEN)"),
        (ROLE["VIDEO"], "VIDEO IN / OUT", "a video format.dsp", "a camera, a screen cast"),
        (ROLE["OTHER"], "ANY OTHER", "a format.dsp Patchcord doesn't know, or none", "plugged as is, ringed in NO CLASS's sand: never hidden"),
    ]
    for k, (c, name, fact, label) in enumerate(legend):
        x = 60 + (k % 3) * 560
        yy = y + (k // 3) * 92
        jack(x + 20, yy, c, 15)
        text(x + 48, yy - 8, name, 16, "#e6e7e8", True, extra='letter-spacing="1"')
        text(x + 48, yy + 14, fact, 13.5, "#e58f2c")
        text(x + 48, yy + 34, label, 13, PRINT)
    y += 92 * 2 + 10
    y = lines(60, y, [
        ("· So: three media (audio, MIDI, video) and a fallback, in two directions, and the monitor flag on audio out. Which zone a jack sits in says", PRINT, False),
        ("  its direction; its ring says its media; its label says its channel or its client. A port is a mult: one jack per cord, one always free.", PRINT, False),
    ]) + 50

    # 4 · frames
    section(y, "4 · FRAMES: A FRAME IS ITS BLOCK", "One frame a node, in its kind's colour, as ENGINE's block. Its jacks in their groups' colours: IN at the left, OUT at the right, a monitor beside its input.")
    y += 76
    MON = lambda lb="STEREO": (lb, ROLE["MONITOR"])
    shapes = [
        ("PLAYBACK STREAM", "a player: OUT, its VOLUME", {"name": "PLAYBACK", "bands": [ROLE["OUTPUT"]], "zones": [{"tag": "OUT", "jacks": [("STEREO", ROLE["OUTPUT"])], "cols": 2, "split": True}], "mix": VOL()}),
        ("SINK", "a card's or a program's: IN and its MONITOR", {"name": "PLAYBACK", "bands": [ROLE["PLAYBACK"]], "zones": [{"tag": "IN", "jacks": [("STEREO", ROLE["PLAYBACK"]), ("MONITOR", ROLE["MONITOR"])], "cols": 2, "split": True}], "mix": VOL()}),
        ("RECORDING STREAM", "an app recording: IN and its MONITOR, its GAIN", {"name": "INPUT", "bands": [ROLE["INPUT"]], "zones": [{"tag": "IN", "jacks": [("STEREO", ROLE["INPUT"]), ("MONITOR", ROLE["MONITOR"])], "cols": 2, "split": True}], "mix": GAIN()}),
        ("SOURCE", "a mic: OUT, its GAIN", {"name": "CAPTURE", "bands": [ROLE["CAPTURE"]], "zones": [{"tag": "OUT", "jacks": [("MONO", ROLE["CAPTURE"])], "cols": 1}], "mix": GAIN(stereo=False)}),
        ("VIRTUAL SOURCE", "fed IN, recorded from OUT", {"name": "EASY EFFECTS SOURCE", "bands": [ROLE["CAPTURE"]], "zones": [{"tag": "IN", "jacks": [("STEREO", ROLE["INPUT"])], "cols": 2, "split": True}, {"tag": "OUT", "jacks": [("STEREO", ROLE["CAPTURE"])], "cols": 2, "split": True}], "mix": GAIN()}),
        ("NO CLASS", "an effect: IN, OUT, no volume", {"name": "SPECTRUM", "bands": [NOCLASS], "zones": [{"tag": "IN", "jacks": [("STEREO", ROLE["INPUT"])], "cols": 2, "split": True}, {"tag": "OUT", "jacks": [("STEREO", ROLE["OUTPUT"])], "cols": 2, "split": True}]}),
        ("DUPLEX", "Bitwig: IN, OUT", {"name": "BITWIG STUDIO", "bands": [DUPLEX], "zones": [{"tag": "IN", "jacks": [("STEREO", ROLE["INPUT"])], "cols": 2, "split": True}, {"tag": "OUT", "jacks": [("STEREO", ROLE["OUTPUT"])], "cols": 2, "split": True}]}),
        ("MIDI", "a bridge: a jack a client each way", {"name": "MIDI", "bands": [ROLE["MIDI"]], "zones": [{"tag": "IN", "jacks": [("MIDI THROUGH", ROLE["MIDI"]), ("HYDROGEN", ROLE["MIDI"])]}, {"tag": "OUT", "jacks": [("MIDI THROUGH", ROLE["MIDI"]), ("HYDROGEN", ROLE["MIDI"])]}]}),
        ("VIDEO", "a camera: OUT, no meter, no volume", {"name": "CAMERA", "bands": [ROLE["VIDEO"]], "zones": [{"tag": "OUT", "jacks": [("VIDEO", ROLE["VIDEO"])]}]}),
        ("MIXED MEDIA", "a synth: MIDI in, audio out", {"name": "SYNTH", "bands": [DUPLEX], "zones": [{"tag": "IN", "jacks": [("MIDI", ROLE["MIDI"])]}, {"tag": "OUT", "jacks": [("STEREO", ROLE["OUTPUT"])], "cols": 2, "split": True}]}),
    ]
    x, yy = 60, y
    for name, what, fr in shapes:
        at = len(o)
        w = pf.frame(0, 0, fr, FIN["BLACK"])
        del o[at:]
        if x + w > W - 60:
            x, yy = 60, yy + FH2 + 70
        text(x, yy, name, 15, "#e6e7e8", True, extra='letter-spacing="1"')
        text(x, yy + 20, what, 13, DIM)
        pf.frame(x, yy + 34, fr, FIN["BLACK"])
        x += w + 50
    y = yy + FH2 + 70
    text(60, y, "KINDS OF BLOCK", 18, "#e6e7e8", True, extra='letter-spacing="1.5"')
    text(60, y + 24, "The frame's outline and band on the rack; the block's outline and cap in ENGINE. By media.class, whoever owns it: the paint says that.", 14, DIM)
    for k, (name, cls, c, what) in enumerate(pf.KINDS):
        x = 60 + (k % 4) * 500
        yy = y + 50 + (k // 4) * 70
        o.append(f'<rect x="{x}" y="{yy}" width="160" height="24" rx="4" fill="{c}"/>')
        text(x + 80, yy + 16.5, name, 10.5, pf.ink(c), True, "middle", 'letter-spacing="1"')
        text(x + 170, yy + 12, cls, 12, "#e6e7e8", font=M)
        text(x + 170, yy + 30, c, 11.5, DIM, font=M)
        text(x, yy + 48, what, 12.5, DIM)
    y += 50 + 3 * 70 + 30
    text(60, y, "GROUPS OF PORTS", 18, "#e6e7e8", True, extra='letter-spacing="1.5"')
    text(60, y + 24, "The jack's ring on the rack; the port's dot and pill in ENGINE. A group: the ports of one direction and one kind; on a MIDI bridge, of one client (one colour).", 14, DIM)
    for k, (g, what) in enumerate([
        ("PLAYBACK", "a sink's inputs"), ("CAPTURE", "a source's outputs"), ("INPUT", "anything else's inputs"),
        ("OUTPUT", "anything else's outputs"), ("MONITOR", "port.monitor outputs"), ("MIDI", "MIDI ports"), ("VIDEO", "video ports"), ("OTHER", "a format not known"),
    ]):
        x = 60 + k * 250
        jack(x + 14, y + 62, ROLE[g], 13)
        text(x + 36, y + 60, g, 13, "#e6e7e8", True, extra='letter-spacing="1"')
        text(x + 36, y + 77, ROLE[g], 11.5, DIM, font=M)
        text(x, y + 100, what, 12.5, DIM)
    y += 130
    y = lines(60, y, [
        ("· One table of colours for ENGINE and the rack: a block's by its kind (eight), a port's by its group (seven). A block's colour is its main group's where it has one.", PRINT, False),
        ("· A frame never splits: a node's inputs, outputs and monitor are one frame on one device. Two groups of one kind (a bridge's clients) share a colour; names tell them apart.", PRINT, False),
        ("· Each audio group has its meter; a stereo one its SPLIT. A node with a volume (Props) has VOLUME (GAIN on a source or a recording), BAL if stereo, MUTE.", PRINT, False),
        ("· GRAPH, on each frame's band before its name, with its node's id: ENGINE opens with that block chosen (in the LCD's amber) and in view. A frame is a block.", PRINT, False),
    ]) + 50

    # 5 · the devices' paints
    section(y, "5 · DEVICE COLOURS: ONE PAINT A KIND", "Three kinds, three paints (paints.py, variant D). A paint says whose a device is; what its ports carry is on its frames.")
    y += 80
    paints = [
        ("HARDWARE", "the node has a device.id", "a card, a headset, a USB interface, a camera", "RYZEN, RADEON, JLAB GO WORK 2"),
        ("PROGRAM", "its client is a program", "every program, however many nodes", "EASY EFFECTS, FOOYIN, HYDROGEN, KADENCE, SOUNDCRAFT"),
        ("NODE", "neither: PipeWire or WirePlumber made it", "MIDI or audio, video, no class: all alike", "MIDI-BRIDGE, BLE MIDI 1"),
    ]
    for k, (kind, rule, what, here) in enumerate(paints):
        x = 60 + k * 560
        p = ROOF_FIN[kind]
        base, prn, dim = FIN[p]
        o.append(f'<rect x="{x}" y="{y - 20}" width="530" height="70" rx="4" fill="{base}" stroke="#000" stroke-opacity=".6"/>')
        text(x + 16, y + 8, f"{kind} · {p}", 18, prn, True, extra='letter-spacing="2"')
        text(x + 514, y + 8, base, 13, dim, anchor="end", font=M)
        text(x + 16, y + 34, "if " + rule, 13, prn)
        text(x, y + 76, what, 13.5, PRINT)
        text(x, y + 98, "here: " + here, 13, DIM)
    y += 140
    y = lines(60, y, [
        ("· Lightness first: aluminium light, black dark, olive in between, so no two pass for each other; only olive has a hue, muted against the bands.", PRINT, False),
        ("· No gap left: every node is one of the three kinds, so every device has one of the three paints. Navy and cream go.", PRINT, False),
    ]) + 50

    # open questions
    section(y, "SETTLED")
    y += 40
    y = lines(60, y, [
        ("· Every port is on the rack, a stream's monitor too: the rack can patch whatever PipeWire can link.", PRINT, False),
        ("· Hydrogen's MIDI stays on the MIDI bridge's frame, beside its other clients: as on the graph.", PRINT, False),
        ("· Passive links are cords like any other.", PRINT, False),
        ("· Not now: node.link-group as a roof (linkgroup.py); a device is still its hardware, its program, or a node of its own.", PRINT, False),
        ("· A headset's internal capture stream stays hidden (SYSTEM), as now: before the switch to HSP/HFP there is no mic to show it linked to.", PRINT, False),
    ]) + 40
    head = f'<rect width="{W}" height="{y}" fill="#1f2023"/>'
    head += f'<text x="60" y="70" {F} font-weight="bold" font-size="40" letter-spacing="3" fill="#e58f2c">ONE ROOF A DEVICE, ONE FRAME A NODE</text>'
    head += f'<text x="60" y="108" {F} font-size="20" fill="#9a9da1">The graph\'s blocks are the rack\'s frames, as many as PipeWire has; a device only gathers them under one roof.</text>'
    open(sys.argv[1] if len(sys.argv) > 1 else "nodes.svg", "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{y}" viewBox="0 0 {W} {y}">{head}{"".join(o)}</svg>\n')


if __name__ == "__main__":
    import nodes  # one module, so progframes draws into the same picture
    nodes.main(sys.argv)
