# A jack without its hex nut: the panel-mount 6.3 mm jacks of a small headphone amp, flush with
# the face, a printed ring round each. Every state a jack has (face::jack_pic_ring, badge_pic,
# cue_*_pic, target_ring_pic, the mult's outline, a split pair's rows), as it is now and in three
# ways without the nut, on the aluminium and the black paint (the printed ring must read on both).
#   python3 jacks.py jacks.svg && rsvg-convert jacks.svg -o jacks.png
import sys

CYAN, RED_CUE, GREEN_T, GOLD_T = "#5fd4ff", "#ff503c", "#78eb96", "#ffc85a"
PLAYBACK, CAPTURE, OUTPUT, INPUT, MONITOR, MIDI = "#007c7d", "#206ea6", "#7e5a42", "#427935", "#695ca6", "#934c7e"
PAINTS = {"ALUMINIUM": ("#c3c6ca", "#1d1e20"), "BLACK": ("#232427", "#d5d7d9")}
JP = 48  # between jacks

DEFS = """
<radialGradient id="gChrome" cx=".35" cy=".3" r=".85"><stop offset="0" stop-color="#fff"/><stop offset=".4" stop-color="#b9bcc1"/><stop offset=".75" stop-color="#65686d"/><stop offset="1" stop-color="#a6a9ad"/></radialGradient>
<radialGradient id="gHole" cx=".5" cy=".62" r=".6"><stop offset="0" stop-color="#000"/><stop offset=".7" stop-color="#0a0a0b"/><stop offset="1" stop-color="#3c3d40"/></radialGradient>
<linearGradient id="gNut" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#f6f7f8"/><stop offset=".45" stop-color="#b6b9bd"/><stop offset="1" stop-color="#55585d"/></linearGradient>
<radialGradient id="gWasher" cx=".4" cy=".35" r=".75"><stop offset="0" stop-color="#5e6166"/><stop offset="1" stop-color="#141517"/></radialGradient>
<radialGradient id="gWasherMidi" cx=".4" cy=".35" r=".75"><stop offset="0" stop-color="#c8483a"/><stop offset="1" stop-color="#4a120d"/></radialGradient>
<linearGradient id="gBrass" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#fff1c4"/><stop offset=".45" stop-color="#d9a945"/><stop offset="1" stop-color="#6e4a10"/></linearGradient>
<linearGradient id="gNickel" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#ffffff"/><stop offset=".45" stop-color="#b9bcc1"/><stop offset="1" stop-color="#55585d"/></linearGradient>
<radialGradient id="gBezel" cx=".45" cy=".35" r=".8"><stop offset="0" stop-color="#2c2d30"/><stop offset="1" stop-color="#0c0c0d"/></radialGradient>
<radialGradient id="gDeep" cx=".5" cy=".55" r=".6"><stop offset="0" stop-color="#000"/><stop offset=".8" stop-color="#050506"/><stop offset="1" stop-color="#262729"/></radialGradient>
<radialGradient id="gPlug" cx=".38" cy=".32" r=".8"><stop offset="0" stop-color="#4a4b4f"/><stop offset="1" stop-color="#0e0e10"/></radialGradient>
<filter id="soft" x="-40%" y="-40%" width="180%" height="180%"><feGaussianBlur stdDeviation="1.2"/></filter>
"""


def hexagon(r):
    import math
    return " ".join(f"{r * math.cos(math.radians(k * 60 + 30)):.2f},{r * math.sin(math.radians(k * 60 + 30)):.2f}" for k in range(6))


# ---- the jack itself, in each way ----------------------------------------------------------------

def jack_now(kind, c, print_):
    """Now: a hex nut, a washer, a chrome bushing, the hole; the group's colour in one ring (two
    for stereo) on the washer."""
    washer = "url(#gWasherMidi)" if kind == "midi" else "url(#gWasher)"
    rings = (f'<circle r="10.7" fill="none" stroke="{c}" stroke-width="1"/><circle r="9.3" fill="none" stroke="{c}" stroke-width="1"/>' if kind == "stereo"
             else f'<circle r="10.1" fill="none" stroke="{c}" stroke-width="1.3"/>')
    return (f'<polygon points="{hexagon(15)}" fill="#000" opacity=".4" transform="translate(1.2,2.4)" filter="url(#soft)"/>'
            f'<polygon points="{hexagon(15)}" fill="url(#gNut)" stroke="#43454a" stroke-width=".8"/>'
            f'<circle r="11.4" fill="{washer}"/><circle r="8.7" fill="url(#gChrome)" stroke="#2a2b2e" stroke-width=".8"/><circle r="5.5" fill="url(#gHole)"/>{rings}')


def din(c):
    """A MIDI jack is a DIN socket: five pins in an arc and the key at the bottom."""
    import math
    pins = "".join(f'<circle cx="{6.2 * math.sin(math.radians(a)):.2f}" cy="{-6.2 * math.cos(math.radians(a)):.2f}" r="1.25" fill="#c9ccd0"/>' for a in (-90, -45, 0, 45, 90))
    return f'<circle r="9.2" fill="url(#gDeep)"/>{pins}<rect x="-1.6" y="6.4" width="3.2" height="3" fill="#c9ccd0"/>'


def jack_a(kind, c, print_):
    """A: as the photos: a ring printed on the face round the jack, in the group's colour; the
    jack flush in it: a black bezel, a bright bushing (brass for stereo, nickel for mono), the
    deep hole. MIDI a DIN socket in the same ring."""
    ring = f'<circle r="14.2" fill="none" stroke="{c}" stroke-width="2.8"/>'
    if kind == "midi":
        return ring + f'<circle r="11.2" fill="url(#gBezel)"/>{din(c)}'
    metal = "url(#gBrass)" if kind == "stereo" else "url(#gNickel)"
    return (ring + f'<circle r="11.2" fill="url(#gBezel)"/><circle r="8.6" fill="{metal}"/><circle r="6.6" fill="url(#gDeep)"/>'
            '<path d="M-4,3.4 a5.2,5.2 0 0 0 8,0" fill="none" stroke="#6b5a2c" stroke-width="1" opacity=".6"/>')


def jack_b(kind, c, print_):
    """B: the printed ring in the paint's print (white on black, dark on aluminium) as on the
    photos; the group's colour on the bushing, anodised."""
    ring = f'<circle r="14.2" fill="none" stroke="{print_}" stroke-width="2.4"/>'
    if kind == "midi":
        return ring + f'<circle r="11.2" fill="{c}"/><circle r="9.6" fill="url(#gBezel)"/>{din(c)}'
    stereo = f'<circle r="7.6" fill="none" stroke="url(#gBrass)" stroke-width="1.4"/>' if kind == "stereo" else ""
    return ring + f'<circle r="11.2" fill="{c}"/><circle r="11.2" fill="none" stroke="#000" stroke-opacity=".45"/><circle r="8.8" fill="url(#gBezel)"/><circle r="6.4" fill="url(#gDeep)"/>{stereo}'


def jack_c(kind, c, print_):
    """C: no printed ring; a wide bushing flush with the face, in the group's colour, its edge
    catching the light; brass inside for stereo."""
    if kind == "midi":
        return f'<circle r="14" fill="{c}"/><circle r="14" fill="none" stroke="#fff" stroke-opacity=".35" stroke-width="1"/><circle r="11" fill="url(#gBezel)"/>{din(c)}'
    metal = "url(#gBrass)" if kind == "stereo" else "url(#gNickel)"
    return (f'<circle r="14" fill="{c}"/><circle r="14" fill="none" stroke="#fff" stroke-opacity=".35" stroke-width="1"/>'
            f'<circle r="10.4" fill="url(#gBezel)"/><circle r="8.2" fill="{metal}"/><circle r="6.2" fill="url(#gDeep)"/>')


WAYS = [("NOW · THE NUT", jack_now), ("A · PRINTED RING IN ITS COLOUR", jack_a), ("B · WHITE RING, COLOUR ON THE BUSHING", jack_b), ("C · A COLOURED BUSHING", jack_c)]

# ---- what lies on and round a jack: the same in every way --------------------------------------


def plug(cord):
    """A plug in it, seen from the front: its barrel, round, the cord's colour on its strain
    relief, the cord leaving down."""
    return (f'<path d="M0,4 C0,22 -6,30 -16,44" fill="none" stroke="#000" stroke-opacity=".35" stroke-width="9" transform="translate(2 3)"/>'
            f'<path d="M0,4 C0,22 -6,30 -16,44" fill="none" stroke="{cord}" stroke-width="7" stroke-linecap="round"/>'
            f'<circle r="10.5" fill="url(#gPlug)" stroke="#000" stroke-width=".8"/><circle r="7" fill="none" stroke="{cord}" stroke-width="2.6"/><circle cx="-3" cy="-3.5" r="2.2" fill="#fff" opacity=".25"/>')


def badge():
    return '<g transform="translate(11.5,-11.5)"><circle r="5.8" fill="#1b1c1e" stroke="#f2f2f2" stroke-opacity=".35" stroke-width=".8"/><path d="M-3,0 h6 M0,-3 v6" stroke="#f2f2f2" stroke-width="1.6" stroke-linecap="round"/></g>'


def target(hot):
    c, r, w = (GOLD_T, 19.0, 3.5) if hot else (GREEN_T, 17.5, 2.5)
    return (f'<circle r="{r + 9}" fill="none" stroke="{c}" stroke-opacity=".22" stroke-width="10"/>'
            f'<circle r="{r}" fill="none" stroke="{c}" stroke-opacity=".95" stroke-width="{w}"/>')


def phones(c, bx=17, by=-17):
    return (f'<circle cx="{bx}" cy="{by}" r="8.5" fill="#121315" stroke="{c}" stroke-width="1.4"/>'
            f'<g transform="translate({bx},{by}) scale(.4583) translate(-12,-12.5)"><path d="M4 15v-3a8 8 0 0 1 16 0v3" fill="none" stroke="{c}" stroke-width="2.4"/><rect x="3" y="14" width="4.5" height="7" fill="{c}"/><rect x="16.5" y="14" width="4.5" height="7" fill="{c}"/></g>')


def cue_ring(red=False):
    c = RED_CUE if red else CYAN
    return f'<circle r="20" fill="none" stroke="{c}" stroke-width="2.2"/>' + phones(c)


def cue_disc(half=None):
    if half is None:
        return f'<circle r="21.1" fill="{CYAN}"/><circle r="20.7" fill="none" stroke="#000" stroke-opacity=".35" stroke-width=".8"/>'
    sweep = 1 if half == "R" else 0
    return f'<path d="M0,-21.1 A21.1,21.1 0 0 {sweep} 0,21.1 Z" fill="{CYAN}"/>'


def tag(x, y, t, print_):
    w = len(t) * 5.6 + 10
    return f'<rect x="{x - w / 2}" y="{y - 7}" width="{w}" height="14" rx="2.5" fill="none" stroke="{print_}" stroke-width=".9"/><text x="{x}" y="{y + 3}" font-family="Noto Sans" font-weight="bold" font-size="7.5" letter-spacing=".5" fill="{print_}" text-anchor="middle">{t}</text>'


# ---- the states: each a little scene of jacks at (x, y) within a cell -------------------------
# a scene: list of (dx, dy, kind, colour, under, over) and extra (svg in cell coordinates)

def at(x, y, body):
    return f'<g transform="translate({x},{y})">{body}</g>'


STATES = [
    ("EMPTY · MONO", lambda J, p: at(55, 40, J("mono", CAPTURE, p))),
    ("EMPTY · STEREO", lambda J, p: at(55, 40, J("stereo", PLAYBACK, p))),
    ("MIDI", lambda J, p: at(55, 40, J("midi", MIDI, p))),
    ("OUTPUT · INPUT · MONITOR", lambda J, p: at(20, 40, J("stereo", OUTPUT, p)) + at(55, 40, J("stereo", INPUT, p)) + at(90, 40, J("stereo", MONITOR, p))),
    ("A PLUG IN IT", lambda J, p: at(55, 34, J("stereo", PLAYBACK, p) + plug("#f07a2a"))),
    ("A MULT: ONE TAKEN, ONE FREE (+)", lambda J, p: f'<rect x="10" y="19" width="90" height="42" rx="21" fill="rgba(0,0,0,.08)" stroke="{p}" stroke-width=".9" opacity=".5"/>'
     + at(31, 40, J("stereo", OUTPUT, p) + plug("#4f7be0")) + at(79, 40, J("stereo", OUTPUT, p) + badge())),
    ("A CORD IN THE HAND: MAY GO / PULLED TO", lambda J, p: at(31, 40, target(False) + J("stereo", INPUT, p)) + at(79, 40, target(True) + J("stereo", INPUT, p))),
    ("A CORD IN THE HAND: CAN'T GO (FADED)", lambda J, p: at(55, 40, f'<g opacity=".3">{J("stereo", OUTPUT, p)}</g>')),
    ("CUE: LISTENED TO / WOULD FEED BACK", lambda J, p: at(31, 40, J("stereo", OUTPUT, p) + cue_ring()) + at(79, 40, J("stereo", OUTPUT, p) + cue_ring(True))),
    ("CUE: KEPT ON / ONE CHANNEL", lambda J, p: at(31, 40, cue_disc() + J("stereo", OUTPUT, p)) + at(79, 40, cue_disc("L") + J("stereo", OUTPUT, p))),
    ("SPLIT: L OVER R, TAGS", lambda J, p: at(70, 22, J("mono", OUTPUT, p)) + at(70, 56, J("mono", OUTPUT, p)) + tag(30, 22, "OUT L", p) + tag(30, 56, "OUT R", p)),
    ("SPLIT, BOTH KEPT ON IN CUE", lambda J, p: at(70, 22, f'<rect x="-21.1" y="-21.1" width="42.2" height="76.2" rx="21.1" fill="{CYAN}"/>' + J("mono", OUTPUT, p)) + at(70, 56, J("mono", OUTPUT, p)) + tag(30, 22, "OUT L", p) + tag(30, 56, "OUT R", p)),
]

CW, CH = 120, 86
o = []
x0, y0 = 300, 116
for wi, (wname, J) in enumerate(WAYS):
    for pi, (pname, (base, print_)) in enumerate(PAINTS.items()):
        col = wi * 2 + pi
        cx = x0 + col * (CW + 8)
        o.append(f'<text x="{cx + CW / 2}" y="{y0 - 14}" font-family="Noto Sans" font-weight="bold" font-size="9" letter-spacing="1" fill="#9a9da1" text-anchor="middle">{pname}</text>')
        if pi == 0:
            o.append(f'<text x="{cx + CW + 4}" y="{y0 - 34}" font-family="Noto Sans" font-weight="bold" font-size="11.5" letter-spacing="1" fill="{"#9a9da1" if wi == 0 else "#e58f2c"}" text-anchor="middle">{wname}</text>')
        for si, (sname, scene) in enumerate(STATES):
            cy = y0 + si * (CH + 8)
            o.append(f'<rect x="{cx}" y="{cy}" width="{CW}" height="{CH}" rx="4" fill="{base}"/><rect x="{cx + 4}" y="{cy + 4}" width="{CW - 8}" height="{CH - 8}" rx="5" fill="rgba(0,0,0,.15)" stroke="rgba(0,0,0,.3)"/>')
            o.append(f'<svg x="{cx + 5}" y="{cy + 3}" width="{CW - 10}" height="{CH - 6}" viewBox="0 0 110 80" overflow="hidden">{scene(J, print_)}</svg>')
for si, (sname, _) in enumerate(STATES):
    cy = y0 + si * (CH + 8)
    o.append(f'<text x="30" y="{cy + CH / 2 + 4}" font-family="Noto Sans" font-weight="bold" font-size="10" letter-spacing=".8" fill="#c9cbcd">{sname}</text>')
W = x0 + 8 * (CW + 8) + 20
H = y0 + len(STATES) * (CH + 8) + 30
head = (f'<rect width="{W}" height="{H}" fill="#1b1c1e"/><text x="30" y="40" font-family="Noto Sans" font-weight="bold" font-size="22" letter-spacing="2" fill="#e58f2c">JACKS WITHOUT THE NUT</text>'
        f'<text x="30" y="62" font-family="Noto Sans" font-size="13" fill="#8d9095">every state a jack has, now and three ways as a small amp\'s panel-mount jacks; on the aluminium and the black paint</text>')
out = sys.argv[1] if len(sys.argv) > 1 else "jacks.svg"
open(out, "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}"><defs>{DEFS}</defs>{head}{"".join(o)}</svg>\n')
