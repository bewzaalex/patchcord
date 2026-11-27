# The dark olive paint (PipeWire's own nodes) with MIDI's plum frames on it: the two are nearly
# opposite on the colour wheel and alike in depth, so they clash, where navy with teal and blue,
# or black with orange, sit well. Ways out, each a row of the same three faces (hardware in navy,
# an app in black, PipeWire's own in olive) so a change is seen against the others too:
#   now   olive #45512f, MIDI #934c7e
#   1a-c  MIDI a dusty violet, nearer the blue half (olive kept)
#   2a-b  olive towards pine, cooler (MIDI kept)
#   3     olive towards khaki, greyer (MIDI kept)
#   4     MIDI lighter, a dusty rose, on olive only
#   python3 olive.py   (writes olive.png next to it; needs rsvg-convert)
import os
import subprocess

HERE = os.path.dirname(os.path.abspath(__file__))
NAVY, BLACK, OLIVE = "#2b3950", "#232427", "#45512f"
KIND = {"SINK": "#007c7d", "SOURCE": "#206ea6", "PLAYBACK": "#bb5a2a", "RECORDING": "#427935", "MIDI": "#934c7e"}
PRINT = {NAVY: "#dde5f0", BLACK: "#d5d7d9"}

ROWS = [
    ("now", OLIVE, "#934c7e", None),
    ("1a  MIDI #5f5a9a", OLIVE, "#5f5a9a", None),
    ("1b  MIDI #6e5c94", OLIVE, "#6e5c94", None),
    ("1c  MIDI #7f5f8c", OLIVE, "#7f5f8c", None),
    ("2a  olive → pine #34493f", "#34493f", "#934c7e", None),
    ("2b  olive → pine #2f4a44", "#2f4a44", "#934c7e", None),
    ("3   olive → khaki #4d4c36", "#4d4c36", "#934c7e", None),
    ("4   MIDI on olive #a8708a", OLIVE, "#934c7e", "#a8708a"),
]
FW, FH = 500.0, 108.0      # a face
GAP = 14.0


def frame(x, label, colour, jacks):
    w = 40 + 56 * jacks
    s = (f'<rect x="{x}" y="18" width="{w}" height="80" rx="6" fill="rgba(0,0,0,.15)" stroke="{colour}" stroke-width="1.2"/>'
         f'<path d="M{x},36 V24 a6,6 0 0 1 6,-6 H{x + w - 6} a6,6 0 0 1 6,6 V36 Z" fill="{colour}"/>'
         f'<text x="{x + 10}" y="31" fill="#f3efe6" font-size="10" letter-spacing="1.2">{label}</text>')
    for k in range(jacks):
        cx = x + 40 + 56 * k
        s += (f'<circle cx="{cx}" cy="64" r="15" fill="{colour}"/><circle cx="{cx}" cy="64" r="11" fill="#cfd2d6"/>'
              f'<circle cx="{cx}" cy="64" r="8" fill="#0b0c0d"/>')
    return s, w


def face(x, y, paint, print_, name, frames):
    s = [f'<g transform="translate({x},{y})">', f'<rect width="{FW}" height="{FH}" rx="3" fill="{paint}"/>',
         f'<text x="16" y="30" fill="{print_}" font-size="17" font-weight="900">{name}</text>',
         '<rect x="16" y="40" width="130" height="50" rx="3" fill="#f3a53c" stroke="#111" stroke-width="2"/>']
    fx = 164
    for label, colour, jacks in frames:
        g, w = frame(fx, label, colour, jacks)
        s.append(g)
        fx += w + 10
    s.append("</g>")
    return "".join(s)


body = []
y = 20.0
for title, olive, midi, midi_on_olive in ROWS:
    body.append(f'<text x="20" y="{y + 14}" fill="#cfd2d6" font-size="15" font-weight="bold">{title}</text>')
    y += 24
    body.append(face(20, y, NAVY, PRINT[NAVY], "SOUND CARD", [("SPEAKERS", KIND["SINK"], 2), ("MIC", KIND["SOURCE"], 1)]))
    body.append(face(20 + FW + GAP, y, BLACK, PRINT[BLACK], "PLAYER", [("OUTPUT", KIND["PLAYBACK"], 2), ("INPUT", KIND["RECORDING"], 1)]))
    body.append(face(20 + 2 * (FW + GAP), y, olive, "#ebedd9", "MIDI-BRIDGE", [("MIDI", midi_on_olive or midi, 2), ("SINK", KIND["SINK"], 1)]))
    y += FH + 22

W = 40 + 3 * FW + 2 * GAP
svg = (f'<svg xmlns="http://www.w3.org/2000/svg" width="{W:.0f}" height="{y:.0f}" font-family="Noto Sans" font-weight="bold">'
       f'<rect width="100%" height="100%" fill="#16171a"/>' + "".join(body) + "</svg>")
path = os.path.join(HERE, "olive.svg")
with open(path, "w") as f:
    f.write(svg)
subprocess.run(["rsvg-convert", "-o", os.path.join(HERE, "olive.png"), path], check=True)
os.remove(path)
