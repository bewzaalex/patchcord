# Three paints for three kinds of device (HARDWARE, PROGRAM, NODE), in variants. Each kind its own
# lightness first (one light, one dark, one in between), so no two can be taken for each other,
# then a hue for the one in between. Each variant drawn as three units of the rack with a frame on.
#   python3 paints.py paints.svg && rsvg-convert paints.svg -o paints.png
import sys

import nodes as n
from nodes import F, M, N, Z, P, ST, text, o

# kind -> (name, base, print, dim print)
VARIANTS = [
    ("A · ALUMINIUM, BLACK, HAMMERTONE GREEN", "As test gear: the green of an old oscilloscope's case.", [
        ("ALUMINIUM", "#c3c6ca", "#1d1e20", "#4a4d52"),
        ("BLACK", "#232427", "#d5d7d9", "#8d9095"),
        ("HAMMERTONE GREEN", "#56705f", "#eef2ee", "#c2cec5")]),
    ("B · ALUMINIUM, BLACK, STEEL BLUE", "Today's navy, lighter and bluer, so it no longer passes for black.", [
        ("ALUMINIUM", "#c3c6ca", "#1d1e20", "#4a4d52"),
        ("BLACK", "#232427", "#d5d7d9", "#8d9095"),
        ("STEEL BLUE", "#4b6584", "#edf1f6", "#c0cbd8")]),
    ("C · ALUMINIUM, BLACK, OXBLOOD", "A warm red-brown, as some studio desks and amps.", [
        ("ALUMINIUM", "#c3c6ca", "#1d1e20", "#4a4d52"),
        ("BLACK", "#232427", "#d5d7d9", "#8d9095"),
        ("OXBLOOD", "#6e3a3a", "#f4ebe8", "#d4bcb7")]),
    ("D · ALUMINIUM, BLACK, OLIVE", "Field gear: olive drab, warm and quiet.", [
        ("ALUMINIUM", "#c3c6ca", "#1d1e20", "#4a4d52"),
        ("BLACK", "#232427", "#d5d7d9", "#8d9095"),
        ("OLIVE", "#66683f", "#f1f1e6", "#cfd0b8")]),
    ("E · GUNMETAL, BLACK, IVORY", "The other way round: hardware in between, PipeWire's own nodes the light ones.", [
        ("GUNMETAL", "#5d6168", "#eef0f2", "#c3c7cc"),
        ("BLACK", "#232427", "#d5d7d9", "#8d9095"),
        ("IVORY", "#e4dfd2", "#2a2926", "#6a665c")]),
    ("NOW · METAL, BLACK, CREAM (+ NAVY)", "For comparison: the metal and the cream are both light, and navy passes for black.", [
        ("METAL", "#8b8d90", "#1d1e20", "#34363a"),
        ("BLACK", "#26272a", "#d5d7d9", "#8d9095"),
        ("CREAM", "#d9d3be", "#2a2926", "#5d5a52")]),
]

KINDS = [
    ("HARDWARE", "RYZEN HD AUDIO CONTROLLER", "CARD · ANALOG STEREO", "2 NODES  6 PORTS",
     [N("h1", "PLAYBACK", "Audio/Sink", Z("in", "PLAYBACK", P("st", ST)), Z("mon", "MONITOR", P("st", ST))),
      N("h2", "CAPTURE", "Audio/Source", Z("out", "CAPTURE", P("st", ST)))]),
    ("PROGRAM", "EASY EFFECTS", "APP · RECORD & PLAY", "7 NODES  26 PORTS",
     [N("p1", "SPECTRUM", "no media.class", Z("in", "INPUT", P("st", ST)), Z("out", "OUTPUT", P("st", ST))),
      N("p2", "PLAYBACK", "Stream/Output/Audio", Z("out", "OUTPUT", P("st", ST)))]),
    ("NODE", "MIDI-BRIDGE", "NODE · MIDI", "1 NODE  4 PORTS",
     [N("m1", "MIDI", "Midi/Bridge", Z("in", "MIDI", P("a", "MIDI THROUGH", 1, "midi"), P("b", "HYDROGEN", 1, "midi")),
        Z("out", "MIDI", P("a", "MIDI THROUGH", 1, "midi"), P("b", "HYDROGEN", 1, "midi")))]),
]

UW, UH = 790, 122  # a unit


def unit(x, y, paint, kind):
    name, base, prn, dim = paint
    kname, dev, what, res, frames = kind
    o.append(f'<rect x="{x}" y="{y}" width="{UW}" height="{UH}" rx="3" fill="{base}" stroke="#000" stroke-opacity=".6"/>')
    o.append(f'<rect x="{x}" y="{y}" width="{UW}" height="2" fill="#fff" fill-opacity=".12"/>')
    for sx in (x + 13, x + UW - 13):
        for sy in (y + 13, y + UH - 13):
            o.append(f'<circle cx="{sx}" cy="{sy}" r="5" fill="#8b8e93" stroke="#222"/><path d="M{sx - 3},{sy} h6" stroke="#333" stroke-width="1.4"/>')
    px = x + 30
    o.append(f'<rect x="{px}" y="{y + 14}" width="176" height="56" rx="3" fill="#15130c" stroke="#000"/>')
    for k, line in enumerate((dev, what, res)):
        text(px + 8, y + 30 + k * 16, n.fit(line, 160, 9), 9 if k == 0 else 8.5, "#f2b13c", k == 0, font=M)
    text(px, y + 90, kname, 12, prn, True, extra='letter-spacing="1.5"')
    text(px, y + 106, f"{name}  {base}", 10, dim, font=M)
    n.ink["print"], n.ink["dim"] = prn, dim
    fx = x + 232
    for f in frames:
        fx += n.frame(fx, y + 8, f) + n.GAP


if __name__ == "__main__":
    W, y = 1760, 150
    for v, (title, sub, paints) in enumerate(VARIANTS):
        x = 60 + (v % 2) * (UW + 60)
        if v % 2 == 0 and v:
            y += 3 * (UH + 8) + 96
        text(x, y, title, 19, "#e58f2c", True, extra='letter-spacing="1.5"')
        text(x, y + 24, sub, 14, "#9a9da1")
        for k, kind in enumerate(KINDS):
            unit(x, y + 44 + k * (UH + 8), paints[k], kind)
    y += 3 * (UH + 8) + 110
    for k, t in enumerate([
        "HOW THEY ARE PICKED",
        "· Lightness first: one kind light, one dark, one in between. Two of a kind's lightness is what made the metal and the cream alike.",
        "· Then a hue only for the one in between, muted, so it doesn't fight the bands (teal, blue, green, rust, violet, plum, ochre).",
        "· The frames stay dark on every paint: the jacks and the bands read the same on all three.",
        "· Metal keeps its painted texture in the app (here flat), the others their noise.",
    ]):
        text(60, y + k * 26, t, 20 if k == 0 else 15, "#e58f2c" if k == 0 else "#c9cbcd", k == 0)
    y += 5 * 26 + 50
    head = f'<rect width="{W}" height="{y}" fill="#1f2023"/>'
    head += f'<text x="60" y="70" {F} font-weight="bold" font-size="40" letter-spacing="3" fill="#e58f2c">THREE PAINTS FOR THREE KINDS</text>'
    head += f'<text x="60" y="108" {F} font-size="20" fill="#9a9da1">HARDWARE, PROGRAM, NODE: one paint each. Variants, and today\'s paints to compare.</text>'
    open(sys.argv[1] if len(sys.argv) > 1 else "paints.svg", "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{y}" viewBox="0 0 {W} {y}">{head}{"".join(o)}</svg>\n')
