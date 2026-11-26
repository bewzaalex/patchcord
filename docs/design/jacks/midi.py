# MIDI's jack after the nut went (ADR 0023): the DIN socket's five small pins read as dust and a
# cord going into it looked wrong. Six ways, each empty, with a plug, and a mult's free one (+),
# on the olive and the black paint, at the face's size and twice that.
#   python3 midi.py midi.svg && rsvg-convert midi.svg -o midi.png
import math
import sys

MIDI = "#934c7e"
PAINTS = {"OLIVE": "#66683f", "BLACK": "#232427"}
DEFS = """
<linearGradient id="gBrass" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#fff1c4"/><stop offset=".45" stop-color="#d9a945"/><stop offset="1" stop-color="#6e4a10"/></linearGradient>
<linearGradient id="gNickel" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#fff"/><stop offset=".45" stop-color="#b9bcc1"/><stop offset="1" stop-color="#55585d"/></linearGradient>
<linearGradient id="gBlackChrome" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#8a8d92"/><stop offset=".45" stop-color="#3a3c40"/><stop offset="1" stop-color="#111214"/></linearGradient>
<radialGradient id="gBezel" cx=".45" cy=".35" r=".8"><stop offset="0" stop-color="#2c2d30"/><stop offset="1" stop-color="#0c0c0d"/></radialGradient>
<radialGradient id="gDeep" cx=".5" cy=".55" r=".6"><stop offset="0" stop-color="#000"/><stop offset=".8" stop-color="#050506"/><stop offset="1" stop-color="#262729"/></radialGradient>
<radialGradient id="gPlug" cx=".38" cy=".32" r=".8"><stop offset="0" stop-color="#4a4b4f"/><stop offset="1" stop-color="#0e0e10"/></radialGradient>
"""


def bushing(c=MIDI, r=14):
    return f'<circle r="{r + .6}" fill="none" stroke="#000" stroke-opacity=".35" stroke-width="1.2"/><circle r="{r}" fill="{c}"/>'


def pins(r, pr, fill="#d7dade"):
    return "".join(f'<circle cx="{r * math.sin(math.radians(a)):.2f}" cy="{-r * math.cos(math.radians(a)):.2f}" r="{pr}" fill="{fill}"/>' for a in (-90, -45, 0, 45, 90))


WAYS = [
    ("NOW · DIN, SMALL PINS", lambda: bushing() + f'<circle r="11" fill="url(#gBezel)"/><circle r="9.2" fill="url(#gDeep)"/>{pins(6.2, 1.25, "#c9ccd0")}<rect x="-1.6" y="6.4" width="3.2" height="3" fill="#c9ccd0"/>'),
    ("1 · AN AUDIO JACK, BLACK CHROME", lambda: bushing() + '<circle r="10.4" fill="url(#gBezel)"/><circle r="8.2" fill="url(#gBlackChrome)"/><circle r="6.2" fill="url(#gDeep)"/>'),
    ("2 · DIN AS IT IS: A METAL SHELL, BIG PINS", lambda: bushing() + '<circle r="12" fill="url(#gNickel)"/><circle r="10.4" fill="#141416"/>' + pins(6.4, 1.6) + '<path d="M-2.4,10.4 v-2.6 h4.8 v2.6" fill="url(#gNickel)"/>'),
    ("3 · DIN, PINS IN A DARK FACE, NO SHELL", lambda: bushing() + '<circle r="11" fill="#141416"/>' + pins(6.6, 1.8, "#b9bcc1") + '<rect x="-2" y="7.6" width="4" height="3" rx="1" fill="#b9bcc1"/>'),
    ("4 · A SQUARE BUSHING, A ROUND JACK", lambda: f'<rect x="-14.6" y="-14.6" width="29.2" height="29.2" rx="5.6" fill="none" stroke="#000" stroke-opacity=".35" stroke-width="1.2"/><rect x="-14" y="-14" width="28" height="28" rx="5" fill="{MIDI}"/><circle r="10.4" fill="url(#gBezel)"/><circle r="8.2" fill="url(#gNickel)"/><circle r="6.2" fill="url(#gDeep)"/>'),
    ("5 · AN AUDIO JACK, 'MIDI' PRINTED ON THE BUSHING", lambda: bushing(r=15) + '<circle r="10.4" fill="url(#gBezel)"/><circle r="8.2" fill="url(#gNickel)"/><circle r="6.2" fill="url(#gDeep)"/>'
     + '<text y="-11" font-family="Noto Sans" font-weight="bold" font-size="4" letter-spacing=".5" fill="#f3efe6" text-anchor="middle">MIDI</text>'),
]


def plug(cord="#4f7be0"):
    return (f'<path d="M0,4 C0,22 -6,30 -16,44" fill="none" stroke="{cord}" stroke-width="7" stroke-linecap="round"/>'
            f'<circle r="10.5" fill="url(#gPlug)" stroke="#000" stroke-width=".8"/><circle r="7" fill="none" stroke="{cord}" stroke-width="2.6"/>')


def badge():
    return '<g transform="translate(11.5,-11.5)"><circle r="5.8" fill="#1b1c1e" stroke="#f2f2f2" stroke-opacity=".35" stroke-width=".8"/><path d="M-3,0 h6 M0,-3 v6" stroke="#f2f2f2" stroke-width="1.6" stroke-linecap="round"/></g>'


o = []
CW, CH = 300, 110
for wi, (name, J) in enumerate(WAYS):
    y = 90 + wi * (CH + 46)
    o.append(f'<text x="30" y="{y - 10}" font-family="Noto Sans" font-weight="bold" font-size="12" letter-spacing="1" fill="{"#9a9da1" if wi == 0 else "#e58f2c"}">{name}</text>')
    for pi, (pname, base) in enumerate(PAINTS.items()):
        x = 30 + pi * (CW + 20)
        o.append(f'<rect x="{x}" y="{y}" width="{CW}" height="{CH}" rx="4" fill="{base}"/><rect x="{x + 6}" y="{y + 6}" width="{CW - 12}" height="{CH - 12}" rx="6" fill="rgba(0,0,0,.15)" stroke="{MIDI}" stroke-width="1.2"/>')
        # empty; a mult: one with a plug, the free one (+)
        o.append(f'<g transform="translate({x + 50},{y + 48})">{J()}</g>')
        o.append(f'<rect x="{x + 106}" y="{y + 27}" width="138" height="42" rx="21" fill="rgba(0,0,0,.08)" stroke="#f1f1e6" stroke-opacity=".35" stroke-width=".9"/>')
        o.append(f'<svg x="{x + 100}" y="{y}" width="200" height="{CH}" viewBox="-27 -48 200 {CH}" overflow="hidden"><g>{J()}{plug()}</g><g transform="translate(48,0)">{J()}{badge()}</g></svg>')
    # twice the size
    xb = 30 + 2 * (CW + 20)
    o.append(f'<rect x="{xb}" y="{y}" width="{CH}" height="{CH}" rx="4" fill="{PAINTS["OLIVE"]}"/><g transform="translate({xb + CH / 2},{y + CH / 2}) scale(2)">{J()}</g>')
W = 30 + 2 * (CW + 20) + CH + 30
H = 90 + len(WAYS) * (CH + 46)
head = (f'<rect width="{W}" height="{H}" fill="#1b1c1e"/><text x="30" y="40" font-family="Noto Sans" font-weight="bold" font-size="22" letter-spacing="2" fill="#e58f2c">MIDI\'S JACK</text>'
        '<text x="30" y="60" font-family="Noto Sans" font-size="13" fill="#8d9095">empty; a mult with a plug in one and the other free; on olive and black; and twice the size</text>')
open(sys.argv[1] if len(sys.argv) > 1 else "midi.svg", "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}"><defs>{DEFS}</defs>{head}{"".join(o)}</svg>\n')
