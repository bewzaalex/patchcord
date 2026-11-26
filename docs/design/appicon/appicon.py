# The app's own icon (the one the system shows: assets/icons/io.github.bewzaalex.patchcord.svg):
# twenty to choose from, in Patchcord's language: painted metal, 6.3 mm jacks with their hex nut,
# patch cords with weight and sag, the amber LCD, the knob. Each drawn on a 256 box (as the icon
# is), shown at 128, 48 and 24 px: the size a dock, a menu and a title bar show it at.
#   python3 appicon.py appicon.svg appicon-grounds.svg, then rsvg-convert each to a PNG
import math
import sys

ORANGE, BLUE, GREEN, RED, VIOLET, TEAL = "#f07a2a", "#4f7be0", "#3fae5a", "#d8463a", "#8a6ad8", "#1aa3a3"
DARK = {ORANGE: "#4a1e05", BLUE: "#14244d", GREEN: "#103a1b", RED: "#4a100b", VIOLET: "#2a1a52", TEAL: "#06393a"}
LIGHT = {ORANGE: "#ffb27a", BLUE: "#a9c1ff", GREEN: "#a5e6b4", RED: "#ff9a8e", VIOLET: "#cdbcff", TEAL: "#8fe3e3"}

DEFS = """
<linearGradient id="case" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#9a9ea3"/><stop offset="1" stop-color="#5d6065"/></linearGradient>
<linearGradient id="rim" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#c3c7cc"/><stop offset="1" stop-color="#2c2e31"/></linearGradient>
<linearGradient id="alu" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#d4d7db"/><stop offset="1" stop-color="#a6a9ad"/></linearGradient>
<linearGradient id="black" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#3a3b3f"/><stop offset="1" stop-color="#161719"/></linearGradient>
<linearGradient id="lcd" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f7bb62"/><stop offset="1" stop-color="#e58f2c"/></linearGradient>
<radialGradient id="nut" cx=".35" cy=".3" r=".9"><stop offset="0" stop-color="#eef0f2"/><stop offset="1" stop-color="#7d8084"/></radialGradient>
<radialGradient id="knob" cx=".38" cy=".3" r=".8"><stop offset="0" stop-color="#5a5b5f"/><stop offset="1" stop-color="#151618"/></radialGradient>
<radialGradient id="head" cx=".36" cy=".32" r=".75"><stop offset="0" stop-color="#ffc08a"/><stop offset=".55" stop-color="#f07a2a"/><stop offset="1" stop-color="#7a3208"/></radialGradient>
<filter id="soft" x="-20%" y="-20%" width="140%" height="140%"><feGaussianBlur stdDeviation="3"/></filter>
"""


def plate(fill="url(#case)", rim="url(#rim)", rx=44):
    return f'<rect x="12" y="12" width="232" height="232" rx="{rx}" fill="{rim}"/><rect x="15" y="15" width="226" height="226" rx="{rx - 3}" fill="{fill}"/>'


def cord(d, c=ORANGE, w=16, shadow=True):
    """A patch cord along path d: its shadow on the plate, a dark edge, its colour, a light."""
    s = f'<path d="{d}" fill="none" stroke="#000" stroke-opacity=".35" stroke-width="{w + 6}" stroke-linecap="round" filter="url(#soft)" transform="translate(4 7)"/>' if shadow else ""
    return s + (f'<path d="{d}" fill="none" stroke="{DARK[c]}" stroke-width="{w + 6}" stroke-linecap="round"/>'
                f'<path d="{d}" fill="none" stroke="{c}" stroke-width="{w}" stroke-linecap="round"/>'
                f'<path d="{d}" fill="none" stroke="{LIGHT[c]}" stroke-width="{max(2, w / 4)}" stroke-linecap="round" opacity=".55" transform="translate({-w / 5} {-w / 5})"/>')


def jack(cx, cy, r=26, ring=ORANGE):
    """A 6.3 mm jack as on the rack: its hex nut, a coloured ring, the dark hole."""
    pts = " ".join(f"{cx + r * math.sin(math.radians(30 + 60 * k)):.1f},{cy - r * math.cos(math.radians(30 + 60 * k)):.1f}" for k in range(6))
    return (f'<polygon points="{pts}" fill="url(#nut)" stroke="#2a2b2d" stroke-width="2"/>'
            f'<circle cx="{cx}" cy="{cy}" r="{r * .66}" fill="#26272a"/><circle cx="{cx}" cy="{cy}" r="{r * .56}" fill="none" stroke="{ring}" stroke-width="{r * .18}"/>'
            f'<circle cx="{cx}" cy="{cy}" r="{r * .34}" fill="#0b0b0c"/>')


def plug(cx, cy, r=19):
    """A plug's head seen from the front, in a jack: a ball of the cord's colour (the icon's now)."""
    return (f'<circle cx="{cx}" cy="{cy}" r="{r + 1.5}" fill="#3d1803"/><circle cx="{cx}" cy="{cy}" r="{r}" fill="url(#head)"/>'
            f'<ellipse cx="{cx - r * .34}" cy="{cy - r * .4}" rx="{r * .32}" ry="{r * .16}" transform="rotate(-40 {cx - r * .34} {cy - r * .4})" fill="#fff" opacity=".55"/>')


def trs(x, y, ang, c=ORANGE, s=1.0):
    """A TRS plug from the side, its tip at (x, y), pointing at `ang` degrees: tip, rings, sleeve,
    the grip, and where the cord goes in."""
    g = (f'<g transform="translate({x} {y}) rotate({ang}) scale({s})">'
         '<path d="M0,0 l10,-6 h14 v12 h-14 z" fill="#d9dcdf" stroke="#2a2b2d" stroke-width="2"/>'
         '<rect x="24" y="-7" width="5" height="14" fill="#1b1c1e"/><rect x="29" y="-7" width="14" height="14" fill="#c9ccd0" stroke="#2a2b2d" stroke-width="2"/>'
         '<rect x="43" y="-7" width="4" height="14" fill="#1b1c1e"/><rect x="47" y="-9" width="40" height="18" rx="2" fill="#c9ccd0" stroke="#2a2b2d" stroke-width="2"/>'
         f'<rect x="87" y="-15" width="44" height="30" rx="7" fill="url(#black)" stroke="#000" stroke-width="2"/><rect x="92" y="-15" width="4" height="30" fill="{c}"/></g>')
    return g


def knob(cx, cy, r, a):
    a = math.radians(a)
    return (f'<circle cx="{cx}" cy="{cy + 3}" r="{r + 2}" fill="#000" opacity=".35"/><circle cx="{cx}" cy="{cy}" r="{r}" fill="url(#knob)" stroke="#0c0c0d" stroke-width="2"/>'
            f'<path d="M{cx + r * .3 * math.sin(a):.1f},{cy - r * .3 * math.cos(a):.1f} L{cx + r * .85 * math.sin(a):.1f},{cy - r * .85 * math.cos(a):.1f}" stroke="#ffb02e" stroke-width="{r * .16:.1f}" stroke-linecap="round"/>')


def screw(cx, cy, r=7):
    return f'<circle cx="{cx}" cy="{cy}" r="{r}" fill="url(#nut)" stroke="#333" stroke-width="1.5"/><path d="M{cx - r * .6},{cy - r * .6} l{r * 1.2},{r * 1.2}" stroke="#333" stroke-width="2"/>'


def sag(x1, y1, x2, y2, drop):
    """A cord hanging between two points: a curve sagging `drop` below their middle."""
    mx = (x1 + x2) / 2
    return f"M{x1},{y1} C{x1},{y1 + drop} {x2},{y2 + drop} {x2},{y2}" if False else f"M{x1},{y1} Q{mx},{max(y1, y2) + drop * 2} {x2},{y2}"


V = []  # (name, svg body)

# 01 · now: the P of an orange cord between two plugs and a blue bowl
V.append(("THE P (NOW)", plate() + cord("M96 60 H134 A40 37 0 0 1 134 134 H96", BLUE) + cord("M96 60 V196") + plug(96, 60) + plug(96, 196)))
# 02 · one jack, a cord plugged in hanging down
V.append(("ONE JACK, A CORD", plate() + cord("M128 108 C128 170 70 150 64 236", ORANGE, 20) + jack(128, 104, 46) + plug(128, 104, 22)))
# 03 · two jacks, a cord sagging between
V.append(("TWO JACKS, A CORD", plate() + jack(70, 92, 34, BLUE) + jack(186, 92, 34, ORANGE) + cord(sag(70, 92, 186, 92, 70)) + plug(70, 92, 17) + plug(186, 92, 17)))
# 04 · a rack unit: aluminium, its ears' screws, two jacks and a loop
V.append(("A RACK UNIT", '<rect x="12" y="60" width="232" height="136" rx="10" fill="#2c2e31"/><rect x="15" y="63" width="226" height="130" rx="8" fill="url(#alu)"/>'
          + screw(34, 84) + screw(34, 172) + screw(222, 84) + screw(222, 172) + jack(98, 128, 28, BLUE) + jack(160, 128, 28, ORANGE)
          + cord("M98 128 C98 236 160 236 160 128") + plug(98, 128, 14) + plug(160, 128, 14)))
# 05 · a TRS plug, its cord curling
V.append(("A PLUG", plate() + cord("M175 170 C215 205 190 236 140 226", ORANGE, 18) + trs(54, 54, 45, ORANGE, 1.25)))
# 06 · the amber LCD: PC-1, a cord under it
V.append(("THE LCD", plate("url(#black)") + '<rect x="40" y="58" width="176" height="84" rx="8" fill="#0c0c0d"/><rect x="46" y="64" width="164" height="72" rx="5" fill="url(#lcd)"/>'
          '<text x="128" y="117" font-family="Noto Sans Mono" font-weight="bold" font-size="46" fill="#3a2408" text-anchor="middle">PC-1</text>'
          + cord("M60 180 Q128 230 196 180", ORANGE, 14) + plug(60, 180, 13) + plug(196, 180, 13)))
# 07 · the knob, a cord round it
V.append(("THE KNOB", plate() + cord("M40 196 C80 250 230 200 196 150", ORANGE, 14) + knob(128, 118, 66, 40)
          + "".join(f'<path d="M{128 + 76 * math.sin(math.radians(a)):.1f},{118 - 76 * math.cos(math.radians(a)):.1f} L{128 + 84 * math.sin(math.radians(a)):.1f},{118 - 84 * math.cos(math.radians(a)):.1f}" stroke="#1d1e20" stroke-width="4" stroke-linecap="round"/>' for a in range(-135, 136, 45))))
# 08 · a patchbay: cords crossing between two rows of jacks
V.append(("A PATCHBAY", plate() + "".join(jack(x, y, 20, ORANGE) for x in (64, 128, 192) for y in (72, 184))
          + cord("M64 72 C64 150 192 110 192 184", ORANGE, 11) + cord("M192 72 C192 140 64 120 64 184", BLUE, 11) + cord("M128 72 C150 120 110 140 128 184", GREEN, 11)
          + "".join(plug(x, y, 10) for x, y in ((64, 72), (192, 184), (192, 72), (64, 184), (128, 72), (128, 184)))))
# 09 · a big hex nut head on: the jack is the icon
V.append(("THE JACK", plate() + jack(128, 128, 98, ORANGE)))
# 10 · a cord in a loop, plug at each end, crossing itself
V.append(("A LOOP", plate() + cord("M54 200 C54 60 202 60 202 140 C202 200 120 200 120 140", ORANGE, 16) + plug(54, 200, 18) + plug(120, 140, 18)))
# 11 · PC in the LCD's amber, letters only
V.append(("PC ON THE LCD", plate("url(#black)") + '<rect x="34" y="66" width="188" height="124" rx="10" fill="#0c0c0d"/><rect x="41" y="73" width="174" height="110" rx="6" fill="url(#lcd)"/>'
          '<text x="128" y="158" font-family="Noto Sans Mono" font-weight="bold" font-size="88" fill="#3a2408" text-anchor="middle">PC</text>'))
# 12 · three units stacked in a rack, a cord from top to bottom
V.append(("A SMALL RACK", '<rect x="12" y="12" width="232" height="232" rx="22" fill="#141516"/>'
          + "".join(f'<rect x="30" y="{y}" width="196" height="56" rx="4" fill="{f}" stroke="#000" stroke-width="2"/><circle cx="54" cy="{y + 28}" r="6" fill="{led}"/>'
                    for y, f, led in ((34, "url(#alu)", "#ff4a32"), (100, "url(#black)", "#5cff7a"), (166, "#66683f", "#ffb02e")))
          + jack(176, 62, 18, BLUE) + jack(176, 194, 18, ORANGE) + cord("M176 62 C250 100 250 160 176 194", ORANGE, 11) + plug(176, 62, 9) + plug(176, 194, 9)))
# 13 · a signal between two jacks: the cord a wave
V.append(("A SIGNAL", plate() + cord("M48 128 C80 40 112 40 128 128 S176 216 208 128", ORANGE, 15) + jack(48, 128, 22, BLUE) + jack(208, 128, 22, ORANGE) + plug(48, 128, 11) + plug(208, 128, 11)))
# 14 · flat and plain: one orange cord sagging between two dots, on dark
V.append(("FLAT", '<rect x="12" y="12" width="232" height="232" rx="44" fill="#1d1e21"/>'
          + f'<path d="M56 96 Q128 260 200 96" fill="none" stroke="{ORANGE}" stroke-width="20" stroke-linecap="round"/><circle cx="56" cy="96" r="22" fill="{ORANGE}"/><circle cx="200" cy="96" r="22" fill="{ORANGE}"/>'))
# 15 · a face: aluminium, a red LED, a jack and a knob, a cord out
V.append(("A FACE", plate("url(#alu)", "#2c2e31") + screw(40, 40) + screw(216, 40) + screw(40, 216) + screw(216, 216)
          + '<circle cx="70" cy="74" r="9" fill="#ff4a32" stroke="#3a0f0a" stroke-width="2"/>' + knob(162, 96, 38, -60) + jack(80, 164, 30, ORANGE)
          + cord("M80 164 C80 236 170 220 236 236", ORANGE, 14, False) + plug(80, 164, 15)))
# 16 · ENGINE: two blocks, a link between
V.append(("TWO BLOCKS", '<rect x="12" y="12" width="232" height="232" rx="44" fill="#1d1e21"/>'
          f'<rect x="30" y="56" width="86" height="72" rx="8" fill="#26272a" stroke="{TEAL}" stroke-width="4"/><path d="M30 76 V64 a8 8 0 0 1 8 -8 h70 a8 8 0 0 1 8 8 v12 z" fill="{TEAL}"/>'
          f'<rect x="140" y="128" width="86" height="72" rx="8" fill="#26272a" stroke="{GREEN}" stroke-width="4"/><path d="M140 148 V136 a8 8 0 0 1 8 -8 h70 a8 8 0 0 1 8 8 v12 z" fill="{GREEN}"/>'
          f'<path d="M116 104 C140 104 116 176 140 176" fill="none" stroke="{ORANGE}" stroke-width="8" stroke-linecap="round"/><circle cx="116" cy="104" r="8" fill="{ORANGE}"/><circle cx="140" cy="176" r="8" fill="{ORANGE}"/>'))
# 17 · the P of one cord: the bowl and stem one line, a plug at its end
V.append(("THE P, ONE CORD", plate() + cord("M92 214 V58 H140 A42 40 0 0 1 140 138 H96", ORANGE, 18) + plug(92, 214, 21) + plug(96, 138, 16)))
# 18 · a jack and its plug about to go in
V.append(("PLUGGING IN", plate() + jack(88, 168, 44, ORANGE) + trs(116, 140, -45, ORANGE, 1.0) + cord("M196 60 C220 30 240 24 244 22", ORANGE, 12, False)))
# 19 · headphones' cord into a jack: listening (CUE)
V.append(("CUE", plate() + '<path d="M68 132 v-20 a60 60 0 0 1 120 0 v20" fill="none" stroke="#1d1e20" stroke-width="16" stroke-linecap="round"/>'
          '<rect x="52" y="122" width="36" height="58" rx="12" fill="url(#black)" stroke="#000" stroke-width="2"/><rect x="168" y="122" width="36" height="58" rx="12" fill="url(#black)" stroke="#000" stroke-width="2"/>'
          + cord("M186 180 C186 224 128 214 128 236", ORANGE, 10, False)))
# 20 · the panel's key: LAYOUT's RACK key lit, a cord over it
V.append(("THE KEY", plate() + '<rect x="40" y="76" width="176" height="104" rx="14" fill="url(#black)" stroke="#000" stroke-width="3"/><rect x="40" y="76" width="176" height="104" rx="14" fill="none" stroke="#fff" stroke-opacity=".15" stroke-width="2" transform="translate(0 2)"/>'
          '<circle cx="66" cy="100" r="9" fill="#ff4a32"/><text x="140" y="146" font-family="Noto Sans" font-weight="bold" font-size="40" letter-spacing="3" fill="#e6e7e8" text-anchor="middle">PC</text>'
          + cord("M30 214 Q128 260 226 214", ORANGE, 12) + plug(30, 214, 12) + plug(226, 214, 12)))

o = []
COLS, CW, RH = 4, 380, 250
for i, (name, body) in enumerate(V):
    x = 30 + (i % COLS) * CW
    y = 80 + (i // COLS) * RH
    o.append(f'<text x="{x}" y="{y}" font-family="Noto Sans Mono" font-weight="bold" font-size="14" fill="#e58f2c">{i + 1:02}</text><text x="{x + 30}" y="{y}" font-family="Noto Sans" font-weight="bold" font-size="12" letter-spacing="1" fill="#9a9da1">{name}</text>')
    for s, dx, dy in ((128, 0, 14), (48, 148, 14), (24, 210, 14)):
        o.append(f'<svg x="{x + dx}" y="{y + dy}" width="{s}" height="{s}" viewBox="0 0 256 256">{body}</svg>')
    # the small ones also on a light ground: a light theme's panel
    o.append(f'<rect x="{x + 148}" y="{y + 80}" width="96" height="62" rx="6" fill="#e8e8e6"/>')
    for s, dx in ((48, 152), (24, 210)):
        o.append(f'<svg x="{x + dx}" y="{y + 86}" width="{s}" height="{s}" viewBox="0 0 256 256">{body}</svg>')
W = 30 + COLS * CW
H = 80 + 5 * RH
head = (f'<rect width="{W}" height="{H}" fill="#1b1c1e"/><text x="30" y="44" font-family="Noto Sans" font-weight="bold" font-size="22" letter-spacing="2" fill="#e58f2c">'
        'PATCHCORD · ITS ICON, TWENTY TO CHOOSE FROM</text><text x="700" y="44" font-family="Noto Sans" font-size="14" fill="#8d9095">each at 128, 48 and 24 px; the small ones on a light ground too</text>')
out = sys.argv[1] if len(sys.argv) > 1 else "appicon.svg"
open(out, "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{W}" height="{H}" viewBox="0 0 {W} {H}"><defs>{DEFS}</defs>{head}{"".join(o)}</svg>\n')


# ---- the same icons on other grounds than the panel's grey (a second sheet) ----
GROUNDS = [
    ("GREY (NOW)", plate()),
    ("ALUMINIUM", plate("url(#alu)", "#3a3c40")),
    ("BLACK", plate("url(#black)", "#000")),
    ("OLIVE", plate("url(#olive)", "#1e1f12")),
    ("AMBER", plate("url(#lcd)", "#3a2408")),
    ("NIGHT BLUE", plate("url(#night)", "#05070d")),
    ("NONE", ""),
]
PICKS = [0, 1, 2, 8, 12, 16]  # 01, 02, 03, 09, 13, 17
MORE_DEFS = ('<linearGradient id="olive" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#7a7c4c"/><stop offset="1" stop-color="#55573a"/></linearGradient>'
             '<linearGradient id="night" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#2a3550"/><stop offset="1" stop-color="#121826"/></linearGradient>')
o2 = []
GW, GH = 210, 250
for gi, (gname, ground) in enumerate(GROUNDS):
    o2.append(f'<text x="{150 + gi * GW + 64}" y="70" font-family="Noto Sans" font-weight="bold" font-size="12" letter-spacing="1" fill="#9a9da1" text-anchor="middle">{gname}</text>')
for ri, i in enumerate(PICKS):
    name, body = V[i]
    y = 90 + ri * GH
    o2.append(f'<text x="30" y="{y + 64}" font-family="Noto Sans Mono" font-weight="bold" font-size="14" fill="#e58f2c">{i + 1:02}</text><text x="30" y="{y + 84}" font-family="Noto Sans" font-weight="bold" font-size="11" letter-spacing="1" fill="#9a9da1">{name}</text>')
    for gi, (gname, ground) in enumerate(GROUNDS):
        b = body.replace(plate(), ground, 1) if body.startswith(plate()) else body
        x = 150 + gi * GW
        o2.append(f'<svg x="{x}" y="{y}" width="128" height="128" viewBox="0 0 256 256">{b}</svg>')
        o2.append(f'<svg x="{x + 20}" y="{y + 140}" width="48" height="48" viewBox="0 0 256 256">{b}</svg><svg x="{x + 78}" y="{y + 152}" width="24" height="24" viewBox="0 0 256 256">{b}</svg>')
        o2.append(f'<rect x="{x + 14}" y="{y + 194}" width="100" height="50" rx="6" fill="#e8e8e6"/><svg x="{x + 20}" y="{y + 195}" width="48" height="48" viewBox="0 0 256 256">{b}</svg><svg x="{x + 78}" y="{y + 207}" width="24" height="24" viewBox="0 0 256 256">{b}</svg>')
W2, H2 = 150 + len(GROUNDS) * GW, 90 + len(PICKS) * GH
head2 = (f'<rect width="{W2}" height="{H2}" fill="#1b1c1e"/><text x="30" y="40" font-family="Noto Sans" font-weight="bold" font-size="22" letter-spacing="2" fill="#e58f2c">'
         'THE ICON ON OTHER GROUNDS</text>')
open(sys.argv[2] if len(sys.argv) > 2 else "appicon-grounds.svg", "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W2}" height="{H2}" viewBox="0 0 {W2} {H2}"><defs>{DEFS}{MORE_DEFS}</defs>{head2}{"".join(o2)}</svg>\n')
