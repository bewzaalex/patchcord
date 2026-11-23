# The other kinds of device: programs and PipeWire's own nodes. Each block in ENGINE and its frame
# on the rack, side by side, in one table of colours: a block's colour by its kind (media.class),
# each group of ports by what its ports are for. Drawn from this machine: Teams, Easy Effects,
# the MIDI bridge; Bitwig as it was seen before.
#   python3 progframes.py progframes.svg && rsvg-convert progframes.svg -o progframes.png
import sys

import nodes as n
from hwframes import DEFS, INK, hexjack, knob, key, pt
from nodes import F, M, ROLE, text, o

PAINT = {"PROGRAM": n.FIN["BLACK"], "NODE": n.FIN["OLIVE"]}
DUPLEX = "#73348d"   # kinds.py, variant 4: a deep magenta
NOCLASS = "#cba553"  # and a light sand, lettered dark
DARK_INK = "#1d1e20"


def ink(c):
    """Lettering on a band: light, but dark on the light sand."""
    return DARK_INK if c == NOCLASS else INK

# Every kind of block, and its colour: hardware's from hwframes.py, and the rest.
KINDS = [
    ("SINK", "Audio/Sink", ROLE["PLAYBACK"], "a card's output; a program's virtual sink"),
    ("SOURCE", "Audio/Source, …/Virtual", ROLE["CAPTURE"], "a mic; a program's virtual source"),
    ("PLAYBACK STREAM", "Stream/Output/Audio", ROLE["OUTPUT"], "a program playing"),
    ("RECORDING STREAM", "Stream/Input/Audio", ROLE["INPUT"], "a program recording"),
    ("MIDI", "Midi/Bridge, Midi/*", ROLE["MIDI"], "the ALSA MIDI bridge, BLE MIDI"),
    ("VIDEO", "Video/*, Stream/*/Video", ROLE["VIDEO"], "a camera, a screen cast"),
    ("DUPLEX", "Audio/Duplex", DUPLEX, "in and out alike: Bitwig, a JACK app"),
    ("NO CLASS", "(none)", NOCLASS, "Easy Effects' effects, its test signal"),
]


def eport(x, y, name, colour, right):
    w = 7 * len(name) + 18
    px = x - w - 4 if right else x + 4
    o.append(f'<rect x="{px}" y="{y - 9}" width="{w}" height="18" rx="9" fill="{colour}" fill-opacity=".28"/>')
    text(px + (w - 9 if right else 9), y + 4, name, 11, "#c9cbcd", anchor="end" if right else "start", font=M)
    o.append(f'<circle cx="{x}" cy="{y}" r="4.5" fill="{colour}" stroke="#0f1011" stroke-width="1.5"/>')


def eblock(x, y, name, nid, cls, caps, ins, outs, w=330):
    """ENGINE's block: its cap and outline in its colour (or its groups' colours, half and half);
    each port in its group's colour. Dashed: PipeWire gives it no class."""
    rows = max(len(ins), len(outs))
    h = 50 + rows * 22 + 6
    line = caps[0]
    dash = ""
    o.append(f'<clipPath id="e{nid}{len(o)}"><rect x="{x}" y="{y}" width="{w}" height="{h}" rx="8"/></clipPath>')
    cid = f"e{nid}{len(o) - 1}"
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="8" fill="#1d1e21"/>')
    for k, c in enumerate(caps):
        cw = w / len(caps)
        o.append(f'<rect x="{x + k * cw}" y="{y}" width="{cw}" height="6" fill="{c}" clip-path="url(#{cid})"/>')
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="8" fill="none" stroke="{line}" stroke-width="2.2"{dash}/>')
    o.append(f'<circle cx="{x + 16}" cy="{y + 22}" r="4.5" fill="#5cff7a"/>')
    text(x + 28, y + 27, name, 14, "#e6e7e8", True)
    text(x + 16, y + 44, f"{nid} · {cls}", 11.5, "#7d8085", font=M)
    for k, (p, c) in enumerate(ins):
        eport(x, y + 62 + k * 22, p, c, False)
    for k, (p, c) in enumerate(outs):
        eport(x + w, y + 62 + k * 22, p, c, True)
    return h


FH, BAND, JW, MIX_W, MW = 150, 26, 88, 176, 34


def jack_w(j):
    """A port's room: its jacks (a mult has one a cord, and a free one) or its label, the wider."""
    m = j[2] if len(j) > 2 else 1
    # (a MIDI client's name goes in its tag, after IN · or OUT ·)
    named = j[0] not in ("STEREO", "MONO", "VIDEO", "MIDI", "MONITOR", "L", "R")
    return max(JW, m * 50 + 38, (len(j[0]) + (6 if named else 0)) * 8.6 + 18)


def frame(x, y, fr, paint):
    """A frame as the face draws it, for a block of one or more groups. One group: the name and
    its IN or OUT on the band, SIG over the meter, SPLIT beside MUTE. Groups both ways: the name
    across the band, IN at its left end, OUT at its right; each group its meter and its SPLIT."""
    base, prn, dim = paint
    zones = fr["zones"]
    mix = fr.get("mix")
    single = len(zones) == 1
    # one group with a volume: its meter at the frame's right edge, past VOLUME (as the face)
    tail = single and mix and zones[0].get("cols")
    for z in zones:
        z["w"] = sum(jack_w(j) for j in z["jacks"]) + (MW if z.get("cols") and not tail else 0)
    head = len(fr["name"]) * 11.2 + 40 + 2 * (40 + 8 * len(str(fr.get("nid") or "")))
    short = head - sum(z["w"] for z in zones)
    if short > 0:
        for z in zones:
            z["w"] += short / len(zones)
    w = 16 + sum(z["w"] for z in zones) + (MIX_W if mix else 0) + (MW if tail else 0) + 10
    bands = fr["bands"]
    line = bands[0]
    dash = ""
    tink = ink(bands[0])
    cid = f"f{len(o)}"
    o.append(f'<clipPath id="{cid}"><rect x="{x}" y="{y}" width="{w}" height="{FH}" rx="9"/></clipPath>')
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="{FH}" rx="9" fill="#000" fill-opacity=".05"/>')
    if len(bands) == 1:
        o.append(f'<rect x="{x}" y="{y}" width="{w}" height="{BAND}" fill="{bands[0]}" clip-path="url(#{cid})"/>')
    else:  # one colour a group, each over its own jacks
        bx = x
        for k, (z, c) in enumerate(zip(zones, bands)):
            bw = z["w"] + (16 if k == 0 else 0) + (10 + (MIX_W if mix else 0) + (MW if tail else 0) if k == len(zones) - 1 else 0)
            o.append(f'<rect x="{bx}" y="{y}" width="{bw}" height="{BAND}" fill="{c}" clip-path="url(#{cid})"/>')
            bx += bw
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="{FH}" rx="9" fill="none" stroke="{line}" stroke-width="2.4"{dash}/>')

    def tag(tx, t, c):
        tw = 14 + 9 * len(t)
        if t == "IN":
            o.append(f'<rect x="{tx}" y="{y + 5}" width="{tw}" height="17" rx="3" fill="none" stroke="{tink}" stroke-width="1.6"/>')
            text(tx + tw / 2, y + 18, t, 11.5, tink, True, "middle", 'letter-spacing="1"')
        else:
            o.append(f'<rect x="{tx}" y="{y + 5}" width="{tw}" height="17" rx="3" fill="{tink}"/>')
            text(tx + tw / 2, y + 18, t, 11.5, c, True, "middle", 'letter-spacing="1"')
        return tw

    # GRAPH, on the band before the name: this frame's block in ENGINE (chosen there, in the LCD's
    # amber), the node's id on it as ENGINE's block shows it
    gx, gy = x + 9, y + 6
    gw = 12 + 8 * len(f"#{fr['nid']}") if fr.get("nid") else 0
    if gw:
        o.append(f'<rect x="{gx}" y="{gy}" width="{gw}" height="15" rx="2" fill="#1b1c1e" stroke="rgba(0,0,0,.5)"/>')
        text(gx + gw / 2, gy + 11, f"#{fr['nid']}", 9, "#e6e7e8", True, "middle", font=M)
    # then the name only: what each port is goes under its jacks
    if single:
        text(gx + gw + 9, y + 18.5, fr["name"], 13.5, tink, True, extra='letter-spacing="1.6"')
    else:
        jw_all = sum(z["w"] for z in zones)
        text(x + 16 + jw_all / 2, y + 18.5, fr["name"], 13.5, tink, True, "middle", 'letter-spacing="1.6"')

    def ptag(cx, ty, t):
        """What a port is, under its jacks: IN, OUT, MONITOR alike, outlined in the paint's print."""
        tw = 14 + 8.4 * len(t)
        o.append(f'<rect x="{cx - tw / 2}" y="{ty - 12}" width="{tw}" height="17" rx="3" fill="none" stroke="{prn}" stroke-width="1.6"/>')
        text(cx, ty + 1, t, 11, prn, True, "middle", 'letter-spacing="1"')
    zx = x + 16
    for k, z in enumerate(zones):
        if k:
            o.append(f'<path d="M{zx},{y + BAND + 10} V{y + FH - 10}" stroke="{prn}" stroke-opacity=".45" stroke-dasharray="2 3"/>')
        area = z["w"] - (MW if z.get("cols") and not tail else 0)
        widths = [jack_w(j) for j in z["jacks"]]
        spare = (area - sum(widths)) / len(widths)
        jx = zx
        for j, jw in zip(z["jacks"], widths):
            label, ring, m, ref = (tuple(j) + (1, None))[:4]
            jw += spare
            cx = jx + jw / 2
            jx += jw
            # a port is a mult: a jack for each cord, and one free
            if m > 1:
                o.append(f'<path d="M{cx - (m - 1) * 25},{y + 94} H{cx + (m - 1) * 25}" stroke="{prn}" stroke-opacity=".5" stroke-width="2"/>')
            for i in range(m):
                jcx = cx + (i - (m - 1) / 2) * 50
                hexjack(jcx, y + 66, ring)
                if ref:
                    n.jacks[ref + (i,)] = (jcx, y + 66)
            if label == "MONITOR":
                ptag(cx, y + 110, "MONITOR")
            elif label in ("STEREO", "MONO", "VIDEO", "MIDI"):
                ptag(cx, y + 110, z["tag"])
            elif label in ("L", "R"):
                ptag(cx, y + 110, f'{z["tag"]} {label}')
            else:  # a bridge's client: in its tag (IN · HYDROGEN)
                ptag(cx, y + 110, f'{z["tag"]} · {label}')
        if z.get("cols"):
            mx = x + w - 10 - MW + 4 if tail else zx + area + 4
            if single:
                text(x + w - 10 - MW / 2 - 6, y + 18.5, "SIG", 12, tink, True, "middle", 'letter-spacing="1"')
            lit = round(z.get("lvl", 0) * 10)
            for i in range(10):
                col = "#3fae5a" if i < 7 else ("#e9c53a" if i < 9 else "#d23b32")
                for c in range(z["cols"]):
                    on = i < lit - c
                    o.append(f'<rect x="{mx + c * 12}" y="{y + FH - 22 - i * 10.5}" width="9" height="7" rx="1.2" fill="{col if on else "#3a3b3e"}" stroke="#000" stroke-opacity=".35"/>')
        # a stereo group without a volume: a little switch under its jacks, at the right (the
        # face's); with one, SPLIT is a key beside MUTE
        if z.get("split") and not (single and mix):
            sx, sy = zx + area - 44, y + FH - 22
            o.append(f'<rect x="{sx}" y="{sy}" width="38" height="14" rx="2" fill="#1b1c1e" stroke="rgba(255,255,255,.18)"/>')
            text(sx + 19, sy + 10.3, "JOIN" if z.get("on") else "SPLIT", 8.5, "#e6e7e8", True, "middle", 'letter-spacing=".6"')
        zx += z["w"]
    if mix:
        name, st, v, mute = mix
        mx = zx
        o.append(f'<path d="M{mx + 4},{y + BAND + 8} V{y + FH - 10}" stroke="#000" stroke-opacity=".35"/><path d="M{mx + 5},{y + BAND + 8} V{y + FH - 10}" stroke="#fff" stroke-opacity=".3"/>')
        cx0 = mx + 4
        text(cx0 + MIX_W / 2, y + 18.5, f"{name} · BAL" if st else name, 13.5, tink, True, "middle", 'letter-spacing="1.4"')
        kx, ky = cx0 + (58 if st else MIX_W / 2), y + 68
        va = lambda u: -135 + 270 * min(u, 1.53) / 1.53
        o.append(f'<path d="M{pt(kx, ky, va(1), 32)} A32,32 0 0 1 {pt(kx, ky, va(1.53), 32)}" fill="none" stroke="#d2402f" stroke-width="3"/>')
        for i in range(13):
            a = va(i / 12 * 1.53)
            o.append(f'<path d="M{pt(kx, ky, a, 29)} L{pt(kx, ky, a, 33 if i % 4 == 0 else 31)}" stroke="{prn}" stroke-width="1.2" opacity=".7"/>')
        knob(kx, ky, 23, va(v))
        if st:
            bx, by = cx0 + 136, y + 62
            for i in range(-4, 5):
                o.append(f'<path d="M{pt(bx, by, i * 33.75, 18)} L{pt(bx, by, i * 33.75, 24 if i == 0 else 21)}" stroke="{prn}" stroke-width="{2 if i == 0 else 1.1}" opacity="{1 if i == 0 else .7}"/>')
            knob(bx, by, 14, 0)
            text(bx - 18, by + 30, "L", 11, prn, True, "middle")
            text(bx + 18, by + 30, "R", 11, prn, True, "middle")
        if single and zones[0].get("split"):
            key(cx0 + 12, y + FH - 34, 70, "SPLIT")
            key(cx0 + 90, y + FH - 34, 74, "MUTE", led=mute)
        else:
            key(cx0 + (MIX_W - 74) / 2, y + FH - 34, 74, "MUTE", led=mute)
    return w


def place(x, y, fr, roof):
    """A frame on a bit of its device's paint, with its meter at the right edge."""
    paint = PAINT[roof]
    at = len(o)
    o.append("")
    w = frame(x + 20, y + 16, fr, paint)
    o[at] = (f'<rect x="{x}" y="{y}" width="{w + 70}" height="{FH + 32}" rx="10" fill="{paint[0]}" stroke="#000" stroke-opacity=".6"/>'
             + "".join(f'<circle cx="{x + w + 52}" cy="{sy}" r="7" fill="#8b8e93" stroke="#222"/>' for sy in (y + 16, y + FH + 16)))
    return w + 70


R = ROLE
CASES = [
    ("PROGRAM", "TEAMS FOR LINUX · ITS RECORDING STREAM", "A recording stream: green. Its groups: the input (green) and its monitor (violet), like a sink's.",
     [(("Teams for Linux", 119, "Stream/Input/Audio", [R["INPUT"]], [("input_FL", R["INPUT"]), ("input_FR", R["INPUT"])], [("monitor_FL", R["MONITOR"]), ("monitor_FR", R["MONITOR"])]),
       {"name": "INPUT", "bands": [R["INPUT"]], "zones": [{"tag": "IN", "jacks": [("STEREO", R["INPUT"]), ("MONITOR", R["MONITOR"])], "cols": 2, "lvl": .35, "split": True}],
        "mix": ("GAIN", True, 1.0, False)})]),
    ("PROGRAM", "TEAMS FOR LINUX · ONE OF ITS FIVE PLAYBACK STREAMS", "A playback stream: rust, its one group too. Five of them, numbered (OUTPUT 1…5) as now.",
     [(("Teams for Linux", 70, "Stream/Output/Audio", [R["OUTPUT"]], [], [("output_FL", R["OUTPUT"]), ("output_FR", R["OUTPUT"])]),
       {"name": "OUTPUT 1", "bands": [R["OUTPUT"]], "zones": [{"tag": "OUT", "jacks": [("STEREO", R["OUTPUT"])], "cols": 2, "lvl": .5, "split": True}],
        "mix": ("VOLUME", True, 1.0, False)})]),
    ("PROGRAM", "EASY EFFECTS SINK", "A program's virtual sink is a sink: teal, like a card's. The paint (black) says whose it is.",
     [(("Easy Effects Sink", 141, "Audio/Sink", [R["PLAYBACK"]], [("playback_FL", R["PLAYBACK"]), ("playback_FR", R["PLAYBACK"])], [("monitor_FL", R["MONITOR"]), ("monitor_FR", R["MONITOR"])]),
       {"name": "EASY EFFECTS SINK", "bands": [R["PLAYBACK"]], "zones": [{"tag": "IN", "jacks": [("STEREO", R["PLAYBACK"]), ("MONITOR", R["MONITOR"])], "cols": 2, "lvl": .8, "split": True}],
        "mix": ("VOLUME", True, 1.0, False)})]),
    ("PROGRAM", "EASY EFFECTS SOURCE", "A virtual source is a source: blue. Its groups: what is fed in (green), and what apps record (blue; PipeWire flags these port.monitor).",
     [(("Easy Effects Source", 88, "Audio/Source/Virtual", [R["CAPTURE"]], [("input_FL", R["INPUT"]), ("input_FR", R["INPUT"])], [("capture_FL", R["CAPTURE"]), ("capture_FR", R["CAPTURE"])]),
       {"name": "EASY EFFECTS SOURCE", "bands": [R["CAPTURE"]], "zones": [{"tag": "IN", "jacks": [("STEREO", R["INPUT"])], "cols": 2, "lvl": .3, "split": True},
                                                                    {"tag": "OUT", "jacks": [("STEREO", R["CAPTURE"])], "cols": 2, "lvl": .3, "split": True}],
        "mix": ("GAIN", True, 1.0, False)})]),
    ("PROGRAM", "EASY EFFECTS · SPECTRUM (NO media.class)", "PipeWire gives it no class: sand, lettered dark. Its groups keep their colours: the input green, the output rust.",
     [(("Spectrum", 196, "no media.class", [NOCLASS], [("input_FL", R["INPUT"]), ("input_FR", R["INPUT"])], [("output_FL", R["OUTPUT"]), ("output_FR", R["OUTPUT"])]),
       {"name": "SPECTRUM", "bands": [NOCLASS], "zones": [{"tag": "IN", "jacks": [("STEREO", R["INPUT"])], "cols": 2, "lvl": .8, "split": True},
                                                         {"tag": "OUT", "jacks": [("STEREO", R["OUTPUT"])], "cols": 2, "lvl": .8, "split": True}]})]),
    ("PROGRAM", "BITWIG STUDIO (AUDIO/DUPLEX, SEEN BEFORE)", "A duplex block: magenta. Its input green, its output rust.",
     [(("Bitwig Studio", 77, "Audio/Duplex", [DUPLEX], [("Audio In 1", R["INPUT"]), ("Audio In 2", R["INPUT"])], [("Audio Out 1", R["OUTPUT"]), ("Audio Out 2", R["OUTPUT"])]),
       {"name": "BITWIG STUDIO", "bands": [DUPLEX], "zones": [{"tag": "IN", "jacks": [("STEREO", R["INPUT"])], "cols": 2, "lvl": .2, "split": True},
                                                             {"tag": "OUT", "jacks": [("STEREO", R["OUTPUT"])], "cols": 2, "lvl": .6, "split": True}]})]),
    ("NODE", "MIDI-BRIDGE (PIPEWIRE'S OWN, OLIVE)", "One block, two groups each way, a group a client (MIDI Through, Hydrogen): the same kind of port, the same colour; their names tell them apart.",
     [(("Midi-Bridge", 42, "Midi/Bridge", [R["MIDI"]], [("Midi Through", R["MIDI"]), ("Hydrogen Midi-In", R["MIDI"])],
        [("Midi Through", R["MIDI"]), ("Hydrogen Midi-Out", R["MIDI"])]),
       {"name": "MIDI", "bands": [R["MIDI"]], "zones": [{"tag": "IN", "jacks": [("MIDI THROUGH", R["MIDI"]), ("HYDROGEN", R["MIDI"])]},
                                                       {"tag": "OUT", "jacks": [("MIDI THROUGH", R["MIDI"]), ("HYDROGEN", R["MIDI"])]}]})]),
]

if __name__ == "__main__":
    W, y = 1720, 150
    text(60, y, "KINDS OF BLOCK: A COLOUR EACH", 20, "#e58f2c", True, extra='letter-spacing="1.5"')
    text(60, y + 24, "By media.class, whoever owns the block: a program's virtual sink is a sink. Who owns it is the device's paint (aluminium, black, olive).", 14, "#9a9da1")
    for k, (name, cls, c, what) in enumerate(KINDS):
        x = 60 + (k % 4) * 410
        yy = y + 46 + (k // 4) * 78
        o.append(f'<rect x="{x}" y="{yy}" width="150" height="24" rx="4" fill="{c}"/>')
        text(x + 75, yy + 16.5, name, 10.5, ink(c), True, "middle", 'letter-spacing="1"')
        text(x + 160, yy + 17, cls, 12, "#e6e7e8", font=M)
        text(x, yy + 44, what, 12.5, "#9a9da1")
    y += 46 + 3 * 78 + 30
    text(60, y, "GROUPS OF PORTS: SEVEN, AND OTHER", 20, "#e58f2c", True, extra='letter-spacing="1.5"')
    text(60, y + 24, "PLAYBACK, CAPTURE, INPUT, OUTPUT, MONITOR, MIDI, VIDEO; OTHER for a format not known (NO CLASS's sand). A group: the ports of one direction and one kind (monitor or not, audio, MIDI, video); on a MIDI bridge, of one client.", 14, "#9a9da1")
    text(60, y + 46, "Not port.group: Easy Effects puts each channel in a group of its own (stream.0 for FL, stream.1 for FR).", 14, "#9a9da1")
    for k, g in enumerate(("PLAYBACK", "CAPTURE", "INPUT", "OUTPUT", "MONITOR", "MIDI", "VIDEO", "OTHER")):
        x = 60 + k * 205
        n.jack(x + 14, y + 82, ROLE[g], 13)
        text(x + 36, y + 80, g, 12.5, "#e6e7e8", True, extra='letter-spacing="1"')
        text(x + 36, y + 97, ROLE[g], 11.5, "#9a9da1", font=M)
    y += 140
    text(60, y, "ENGINE", 16, "#e6e7e8", True, extra='letter-spacing="2"')
    text(450, y, "RACK", 16, "#e6e7e8", True, extra='letter-spacing="2"')
    y += 24
    for roof, title, sub, items in CASES:
        text(60, y + 14, title, 15, "#e58f2c", True, extra='letter-spacing="1"')
        text(60, y + 34, sub, 13.5, "#9a9da1")
        y += 54
        for item in items:
            (en, nid, cls, caps, ins, outs), fr = item
            fr["nid"] = nid
            h = eblock(60, y, en if len(en) < 28 else en[:26] + "…", nid, cls, caps, ins, outs)
            place(450, y, fr, roof)
            y += max(h, FH + 32) + 26
        y += 24
    notes = [
        "WHAT IT TAKES",
        "· Kinds of block: eight, a colour each: SINK, SOURCE, PLAYBACK STREAM, RECORDING STREAM, MIDI, VIDEO, and DUPLEX (magenta #73348d),",
        "  NO CLASS (sand #cba553, lettered dark: kinds.py, variant 4). One outline, never dashed.",
        "· Groups of ports: the seven colours we have cover every port. Two groups of one kind in one block (a bridge's clients) share their colour; their names tell them apart.",
        "· The kind is by media.class, never by owner: a program's sink is teal like a card's; the paint tells them apart (black, aluminium).",
        "· ENGINE: blocks by kind and ports by group, as the rack. Its SYSTEM colour goes: PipeWire's own nodes are told by their paint on the rack.",
        "· GRAPH, on the band before the name, with its node's id: ENGINE opens with that block chosen (in the LCD's amber) and in view. No animation.",
    ]
    for k, t in enumerate(notes):
        text(60, y + k * 26, t, 20 if k == 0 else 15, "#e58f2c" if k == 0 else "#c9cbcd", k == 0)
    y += len(notes) * 26 + 40
    head = f'{DEFS}<rect width="{W}" height="{y}" fill="#1f2023"/>'
    head += f'<text x="60" y="70" {F} font-weight="bold" font-size="40" letter-spacing="3" fill="#e58f2c">PROGRAMS AND NODES: A FRAME IS ITS BLOCK</text>'
    head += f'<text x="60" y="108" {F} font-size="20" fill="#9a9da1">The same table of colours as hardware\'s: a block\'s by its kind, its ports\' by their group, in ENGINE and on the rack.</text>'
    open(sys.argv[1] if len(sys.argv) > 1 else "progframes.svg", "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{y}" viewBox="0 0 {W} {y}">{head}{"".join(o)}</svg>\n')
