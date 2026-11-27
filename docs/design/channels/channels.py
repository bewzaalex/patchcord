# Issue #14: a volume for each channel of a stream or device with more than two (5.1, 7.1, a
# 4-channel recorder). PipeWire gives it: the node's Props `channelVolumes` (a linear gain each)
# and `channelMap` (which is which: FL FR RL RR FC LFE, in ALSA's order for an HDMI 5.1 sink); a
# card keeps it on its route. Today such a frame has one VOLUME knob for all and no BAL.
# The specimen is the Radeon HDMI sink in Digital Surround 5.1. Five ways:
#   A  CH ▾ key: a tray of six trim knobs opens under the face, as PROFILE's list does
#   B  a second unit: a TRIM row of small knobs, each under its own jack
#   C  as an AV receiver: VOLUME, then BAL (left-right), FADE (front-rear), CENTER, LFE
#   D  the meter as six faders: six LED columns, a cap on each sets its channel
#   E  MIXER: CHANNELS shows a fader a channel, as it shows L and R for stereo
#   F  every channel a block of its own: its IN and MON jacks, its knob, its meter; the node's own
#      VOLUME and MUTE in a MASTER block (2U). PipeWire has one node with a port a channel;
#      `volume` and `mute` are the node's, `channelVolumes` one a channel; there is no mute a
#      channel, so no MUTE key on one (and no CUE key: CUE is a click on a jack in CUE mode)
#   G  channels paired by their positions (the ports' audio.channel, SPA's fixed names): FL+FR
#      FRONT, RL+RR REAR, SL+SR SIDE…, each a stereo frame as any (VOLUME, BAL, SPLIT, two meter
#      columns); what has no partner (FC, LFE, AUX0…) a mono frame; MASTER the node's own VOLUME
#      and MUTE. No MUTE on a pair or a channel: PipeWire mutes only the node
#   python3 channels.py   (writes channels.png next to it; needs rsvg-convert)
import math
import os
import subprocess

HERE = os.path.dirname(os.path.abspath(__file__))
NAVY, PRINT, SINK, MON = "#2b3950", "#dde5f0", "#007c7d", "#73348d"
CH = ["FL", "FR", "RL", "RR", "FC", "LFE"]
LEV = [0.8, 0.75, 0.55, 0.5, 0.85, 0.6]    # the meter's level, for the picture
TRIM = [1.0, 1.0, 0.8, 0.8, 1.1, 0.7]      # each channel's volume, for the picture
FW, U = 1400.0, 112.0


def t(x, y, s, size=10, fill=PRINT, anchor="start", weight="bold", family="Noto Sans", ls=1.0):
    return f'<text x="{x:.1f}" y="{y:.1f}" font-family="{family}" font-weight="{weight}" font-size="{size}" fill="{fill}" text-anchor="{anchor}" letter-spacing="{ls}">{s}</text>'


def knob(cx, cy, r, v=0.75, mark=True):
    """A knob: ticks round it, its cap, the pointer at `v` (0..1.5 over 270°)."""
    s = ""
    if mark:
        for k in range(11):
            a = math.radians(135 + 270 * k / 10)
            s += f'<path d="M{cx + (r + 3) * math.cos(a):.1f},{cy + (r + 3) * math.sin(a):.1f} L{cx + (r + 6) * math.cos(a):.1f},{cy + (r + 6) * math.sin(a):.1f}" stroke="{PRINT}" stroke-opacity=".6" stroke-width="1"/>'
    a = math.radians(135 + 270 * min(v, 1.5) / 1.5)
    s += (f'<circle cx="{cx}" cy="{cy + 2}" r="{r}" fill="#000" opacity=".35"/>'
          f'<circle cx="{cx}" cy="{cy}" r="{r}" fill="#222326" stroke="#000"/>'
          f'<circle cx="{cx}" cy="{cy}" r="{r * .82:.1f}" fill="#2c2e32"/>'
          f'<path d="M{cx + r * .25 * math.cos(a):.1f},{cy + r * .25 * math.sin(a):.1f} L{cx + r * .8 * math.cos(a):.1f},{cy + r * .8 * math.sin(a):.1f}" stroke="#ffb02e" stroke-width="{max(1.5, r / 7):.1f}" stroke-linecap="round"/>')
    return s


def jack(cx, cy, c):
    return (f'<circle cx="{cx}" cy="{cy}" r="17" fill="{c}"/><circle cx="{cx}" cy="{cy}" r="12.5" fill="#cfd2d6"/>'
            f'<circle cx="{cx}" cy="{cy}" r="9" fill="#0b0c0d"/>')


def tag(cx, y, s):
    w = len(s) * 5.6 + 10
    return f'<rect x="{cx - w / 2:.1f}" y="{y}" width="{w:.1f}" height="16" rx="3" fill="none" stroke="{PRINT}" stroke-width="1.2"/>' + t(cx, y + 11.5, s, 8, anchor="middle", ls=.4)


def key(x, y, w, s, lit=False):
    return (f'<rect x="{x}" y="{y}" width="{w}" height="18" rx="1.5" fill="{"#ffb02e" if lit else "#e9e4d6"}"/>' + t(x + w / 2, y + 12.5, s, 8.5, "#1d1408", "middle", ls=.8))


def leds(x, y, h, v, n=10, w=8):
    s = f'<rect x="{x - 2}" y="{y - 2}" width="{w + 4}" height="{h + 4}" rx="2" fill="#0d0e10"/>'
    step = h / n
    for i in range(n):
        on = (n - i) / n <= v
        c = "#ff4a32" if i < 1 else "#ffb02e" if i < 3 else "#5cff7a"
        s += f'<rect x="{x}" y="{y + i * step + 1:.1f}" width="{w}" height="{step - 2:.1f}" rx="1" fill="{c if on else "#1e2a20"}" opacity="{1 if on else .7}"/>'
    return s


def face(units=1, extra=""):
    """The Radeon's face: name, screen, the HDMI frame's jacks; `extra`: what the variant puts
    in the controls part (from x 1110) and below."""
    h = U * units - 4
    s = f'<rect width="{FW}" height="{h}" rx="3" fill="{NAVY}"/>'
    for sy in (16, h - 16):
        for sx in (22, FW - 22):
            s += f'<circle cx="{sx}" cy="{sy}" r="8" fill="#cfd2d6" stroke="#555" stroke-width=".8"/>'
    s += t(48, 30, "RADEON HIGH DEFINITI…", 16, weight="900", ls=.5)
    s += '<rect x="48" y="40" width="200" height="52" rx="3" fill="#f3a53c" stroke="#111" stroke-width="2"/>'
    s += t(56, 56, "HDMI AUDIO", 10, "#3a2408", family="Noto Sans Mono") + t(240, 56, "SUSPENDED", 10, "#3a2408", "end", family="Noto Sans Mono")
    s += t(56, 71, "Digital Surround 5.1 (HDMI)", 10, "#3a2408", weight="400", family="Noto Sans Mono", ls=0)
    s += t(56, 86, "1 NODE  12 PORTS  NO LINKS", 10, "#3a2408", weight="400", family="Noto Sans Mono", ls=0)
    # the frame
    s += f'<rect x="264" y="8" width="1120" height="94" rx="6" fill="none" stroke="{SINK}" stroke-width="1.4"/>'
    s += f'<path d="M264,26 V14 a6,6 0 0 1 6,-6 H1378 a6,6 0 0 1 6,6 V26 Z" fill="{SINK}"/>' + t(276, 21, "HDMI", 9.5, "#f3efe6", ls=1.4)
    for i, c in enumerate(CH):
        x = 300 + i * 60
        s += jack(x, 52, SINK) + tag(x, 78, f"IN · {c}")
    s += f'<path d="M630,36 V96" stroke="{PRINT}" stroke-opacity=".5" stroke-dasharray="4 3"/>'
    for i, c in enumerate(CH):
        x = 660 + i * 64
        s += jack(x, 52, MON) + tag(x, 78, f"MON · {c}")
    s += f'<path d="M1010,34 V98" stroke="{SINK}" stroke-opacity=".7"/>'
    return s + extra


def variant(title, note, body, h):
    return title, note, body, h


def A():
    ex = t(1300, 21, "VOLUME", 9.5, "#f3efe6", "end") + t(1372, 21, "SIG", 8, "#f3efe6", "end")
    ex += knob(1060, 56, 20, 1.0) + key(1035, 80, 50, "MUTE") + key(1090, 80, 50, "CH ▾", True)
    ex += "".join(leds(1160 + i * 12, 34, 62, LEV[i], w=8) for i in range(6))
    ex += t(1195, 106, "FL FR RL RR FC LFE", 7, PRINT, "middle", weight="400", family="Noto Sans Mono", ls=0)
    # the tray under the face, as PROFILE's list
    tray = '<g transform="translate(900,112)"><rect width="440" height="118" rx="6" fill="#1b1c1f" stroke="#000"/>'
    tray += '<rect x="0" y="0" width="440" height="20" rx="6" fill="#2a2b2e"/>' + t(12, 14, "CHANNELS · HDMI", 9, "#e6e7e8", ls=1.2) + t(428, 14, "RESET", 8.5, "#ffb02e", "end")
    for i, c in enumerate(CH):
        x = 40 + i * 70
        tray += knob(x, 58, 17, TRIM[i]) + t(x, 94, c, 9.5, "#e6e7e8", "middle") + t(x, 108, f"{TRIM[i] * 100:.0f}%", 8.5, "#9a9da2", "middle", weight="400", family="Noto Sans Mono", ls=0)
        tray += leds(x + 24, 40, 40, LEV[i], 8, 4)
    tray += '</g>'
    return ("A  CH ▾: a tray of trims", "The face stays 1U, the VOLUME knob sets all. CH ▾ opens a tray under the face, as PROFILE's list: a knob a channel, named as PipeWire names it, with its meter.", face(1, ex) + tray, 240)


def B():
    ex = t(1300, 21, "VOLUME", 9.5, "#f3efe6", "end") + t(1372, 21, "SIG", 8, "#f3efe6", "end")
    ex += knob(1060, 56, 20, 1.0) + key(1035, 80, 50, "MUTE")
    ex += leds(1160, 34, 62, max(LEV), w=10)
    # the second unit: TRIM under the jacks
    tr = f'<rect x="264" y="{U + 8}" width="746" height="94" rx="6" fill="none" stroke="{SINK}" stroke-width="1.4" stroke-dasharray="1 0"/>'
    tr += f'<path d="M264,{U + 26} V{U + 14} a6,6 0 0 1 6,-6 H1004 a6,6 0 0 1 6,6 V{U + 26} Z" fill="{SINK}"/>' + t(276, U + 21, "HDMI · TRIM", 9.5, "#f3efe6", ls=1.4)
    for i, c in enumerate(CH):
        x = 300 + i * 60
        tr += knob(x, U + 56, 15, TRIM[i]) + t(x, U + 90, c, 9, PRINT, "middle")
    for i, c in enumerate(CH):
        x = 660 + i * 64
        tr += leds(x - 4, U + 36, 56, LEV[i], 8, 8)
    return ("B  a second unit: TRIM under the jacks", "The device becomes 2U: under each IN jack its own small knob, under each MON its meter. Always there, nothing to open; takes a unit.", face(2, ex) + tr, 230)


def C():
    ex = t(1300, 21, "VOLUME · BAL · FADE · CTR · LFE", 9.5, "#f3efe6", "end") + t(1372, 21, "SIG", 8, "#f3efe6", "end")
    ex += knob(1045, 56, 20, 1.0) + key(1020, 82, 50, "MUTE")
    for i, (n, v) in enumerate([("BAL", .75), ("FADE", .65), ("CTR", .8), ("LFE", .6)]):
        x = 1100 + i * 52
        ex += knob(x, 52, 13, v) + t(x, 86, n, 8.5, PRINT, "middle")
    ex += leds(1345, 34, 62, max(LEV), w=10)
    return ("C  as an AV receiver", "1U as now: VOLUME, then BAL (left ↔ right), FADE (front ↔ rear), CTR (centre) and LFE. Five knobs for six channels: Patchcord works out each channel's volume from them.", face(1, ex), 130)


def D():
    ex = t(1300, 21, "VOLUME · CHANNELS", 9.5, "#f3efe6", "end") + t(1372, 21, "SIG", 8, "#f3efe6", "end")
    ex += knob(1050, 56, 20, 1.0) + key(1025, 80, 50, "MUTE")
    for i, c in enumerate(CH):
        x = 1115 + i * 45
        ex += f'<rect x="{x - 2}" y="30" width="14" height="62" rx="2" fill="#0d0e10"/>' + leds(x, 32, 58, LEV[i], 10, 10)
        cy = 92 - TRIM[i] / 1.5 * 58
        ex += f'<rect x="{x - 8}" y="{cy - 4:.1f}" width="26" height="8" rx="1.5" fill="#e9e4d6" stroke="#000" stroke-width=".6"/><path d="M{x - 6},{cy:.1f} h22" stroke="#1d1408" stroke-width="1"/>'
        ex += t(x + 5, 106, c, 7.5, PRINT, "middle", weight="400", family="Noto Sans Mono", ls=0)
    return ("D  the meter as six faders", "1U: SIG becomes six LED columns, one a channel; a small cap on each is that channel's volume, dragged up and down. Level and trim in one place.", face(1, ex), 130)


def E():
    # a MIXER strip, CHANNELS on: six faders
    s = '<rect width="470" height="300" rx="6" fill="#2b3950"/>'
    s += f'<rect width="470" height="40" rx="6" fill="{SINK}"/><rect y="30" width="470" height="10" fill="{SINK}"/>'
    s += t(14, 18, "RADEON HIGH DEFINITION", 10, "#f3efe6", ls=1.2) + t(14, 33, "HDMI", 9, "#f3efe6", ls=1.2)
    s += t(235, 60, "VOLUME", 9, PRINT, "middle", ls=1.2)
    s += key(20, 70, 90, "CUE") + key(115, 70, 90, "MUTE") + key(210, 70, 110, "CHANNELS", True)
    for i, c in enumerate(CH):
        x = 40 + i * 72
        s += f'<rect x="{x - 2}" y="104" width="4" height="150" rx="2" fill="#0d0e10"/>'
        cy = 254 - TRIM[i] / 1.5 * 150
        s += f'<rect x="{x - 14}" y="{cy - 8:.1f}" width="28" height="16" rx="2" fill="#e9e4d6" stroke="#000" stroke-width=".6"/>' + t(x, cy + 4, f"{TRIM[i] * 100:.0f}", 8, "#1d1408", "middle", weight="400", family="Noto Sans Mono", ls=0)
        s += leds(x + 20, 104, 150, LEV[i], 12, 6)
        s += t(x, 276, c, 10, PRINT, "middle")
    return ("E  MIXER: CHANNELS, a fader a channel", "As MIXER shows L and R for a stereo strip today, with more channels: the strip as wide as its channels need.", s, 310)


def F():
    h = U * 2 - 4
    s = f'<rect width="{FW}" height="{h}" rx="3" fill="{NAVY}"/>'
    for sy in (16, h - 16):
        for sx in (22, FW - 22):
            s += f'<circle cx="{sx}" cy="{sy}" r="8" fill="#cfd2d6" stroke="#555" stroke-width=".8"/>'
    s += t(48, 30, "RADEON HIGH DEFINITI…", 16, weight="900", ls=.5)
    s += '<rect x="48" y="40" width="200" height="52" rx="3" fill="#f3a53c" stroke="#111" stroke-width="2"/>'
    s += t(56, 56, "HDMI AUDIO", 10, "#3a2408", family="Noto Sans Mono") + t(240, 56, "SUSPENDED", 10, "#3a2408", "end", family="Noto Sans Mono")
    s += t(56, 71, "Digital Surround 5.1 (HDMI)", 10, "#3a2408", weight="400", family="Noto Sans Mono", ls=0)
    s += t(56, 86, "1 NODE  12 PORTS  NO LINKS", 10, "#3a2408", weight="400", family="Noto Sans Mono", ls=0)

    def block(x, y, w, title, body):
        b = f'<rect x="{x}" y="{y}" width="{w}" height="94" rx="6" fill="none" stroke="{SINK}" stroke-width="1.4"/>'
        b += f'<path d="M{x},{y + 18} V{y + 6} a6,6 0 0 1 6,-6 H{x + w - 6} a6,6 0 0 1 6,6 V{y + 18} Z" fill="{SINK}"/>'
        return b + t(x + 10, y + 13, title, 9.5, "#f3efe6", ls=1.4) + body

    def chan(x, y, i):
        c = CH[i]
        b = jack(x + 34, y + 44, SINK) + tag(x + 34, y + 70, "IN")
        b += f'<path d="M{x + 66},{y + 28} V{y + 88}" stroke="{PRINT}" stroke-opacity=".5" stroke-dasharray="4 3"/>'
        b += jack(x + 98, y + 44, MON) + tag(x + 98, y + 70, "MON")
        b += f'<path d="M{x + 130},{y + 26} V{y + 90}" stroke="{SINK}" stroke-opacity=".7"/>'
        b += knob(x + 162, y + 50, 17, TRIM[i])
        b += leds(x + 198, y + 26, 62, LEV[i], 10, 8)
        return block(x, y, 216, f"HDMI · {c}", b + t(x + 206, y + 13, "SIG", 8, "#f3efe6", "end"))

    # the master: the node's own volume and mute, all channels' meter
    m = knob(330, 52, 20, 1.0) + key(300, 80, 62, "MUTE") + "".join(leds(380 + k * 9, 30, 62, LEV[k], 10, 6) for k in range(6))
    s += block(264, 8, 180, "HDMI · MASTER", m + t(434, 21, "SIG", 8, "#f3efe6", "end"))
    for k in range(4):
        s += chan(454 + k * 226, 8, k)
    for k in range(4, 6):
        s += chan(264 + (k - 4) * 226, U + 8, k)
    return ("F  a block a channel", "2U: MASTER is the node's own VOLUME (`volume`) and MUTE (`mute`); every channel a block: its IN and MON jacks, its knob (its `channelVolumes`), its meter. No MUTE a channel: PipeWire has none.", s, 230)


def G():
    h = U * 2 - 4
    s = f'<rect width="{FW}" height="{h}" rx="3" fill="{NAVY}"/>'
    for sy in (16, h - 16):
        for sx in (22, FW - 22):
            s += f'<circle cx="{sx}" cy="{sy}" r="8" fill="#cfd2d6" stroke="#555" stroke-width=".8"/>'
    s += t(48, 30, "RADEON HIGH DEFINITI…", 16, weight="900", ls=.5)
    s += '<rect x="48" y="40" width="200" height="52" rx="3" fill="#f3a53c" stroke="#111" stroke-width="2"/>'
    s += t(56, 56, "HDMI AUDIO", 10, "#3a2408", family="Noto Sans Mono") + t(240, 56, "SUSPENDED", 10, "#3a2408", "end", family="Noto Sans Mono")
    s += t(56, 71, "Digital Surround 5.1 (HDMI)", 10, "#3a2408", weight="400", family="Noto Sans Mono", ls=0)
    s += t(56, 86, "1 NODE  12 PORTS  NO LINKS", 10, "#3a2408", weight="400", family="Noto Sans Mono", ls=0)

    def frame(x, y, w, title, head, body):
        b = f'<rect x="{x}" y="{y}" width="{w}" height="94" rx="6" fill="none" stroke="{SINK}" stroke-width="1.4"/>'
        b += f'<path d="M{x},{y + 18} V{y + 6} a6,6 0 0 1 6,-6 H{x + w - 6} a6,6 0 0 1 6,6 V{y + 18} Z" fill="{SINK}"/>'
        return b + t(x + 10, y + 13, title, 9.5, "#f3efe6", ls=1.4) + t(x + w - 10, y + 13, head, 8.5, "#f3efe6", "end", ls=1.2) + body

    def pair(x, y, name, li, ri):
        b = jack(x + 36, y + 44, SINK) + tag(x + 36, y + 70, "IN")
        b += f'<path d="M{x + 70},{y + 28} V{y + 88}" stroke="{PRINT}" stroke-opacity=".5" stroke-dasharray="4 3"/>'
        b += jack(x + 104, y + 44, MON) + tag(x + 104, y + 70, "MON")
        b += f'<path d="M{x + 138},{y + 26} V{y + 90}" stroke="{SINK}" stroke-opacity=".7"/>'
        b += knob(x + 168, y + 44, 17, (TRIM[li] + TRIM[ri]) / 2) + knob(x + 212, y + 40, 11, .75)
        b += t(x + 204, y + 62, "L", 7, PRINT, "middle") + t(x + 221, y + 62, "R", 7, PRINT, "middle")
        b += key(x + 150, y + 70, 70, "SPLIT")
        b += leds(x + 236, y + 26, 62, LEV[li], 10, 6) + leds(x + 246, y + 26, 62, LEV[ri], 10, 6)
        return frame(x, y, 262, f"HDMI · {name}", "VOLUME · BAL  SIG", b)

    def mono(x, y, name, i):
        b = jack(x + 36, y + 44, SINK) + tag(x + 36, y + 70, "IN")
        b += f'<path d="M{x + 70},{y + 28} V{y + 88}" stroke="{PRINT}" stroke-opacity=".5" stroke-dasharray="4 3"/>'
        b += jack(x + 104, y + 44, MON) + tag(x + 104, y + 70, "MON")
        b += f'<path d="M{x + 138},{y + 26} V{y + 90}" stroke="{SINK}" stroke-opacity=".7"/>'
        b += knob(x + 168, y + 50, 17, TRIM[i])
        b += leds(x + 198, y + 26, 62, LEV[i], 10, 8)
        return frame(x, y, 216, f"HDMI · {name}", "VOLUME  SIG", b)

    m = knob(316, 54, 20, 1.0) + key(288, 80, 58, "MUTE") + leds(390, 30, 62, max(LEV), 10, 10)
    s += frame(264, 8, 150, "HDMI · MASTER", "SIG", m)
    s += pair(424, 8, "FRONT", 0, 1) + pair(696, 8, "REAR", 2, 3)
    s += mono(264, U + 8, "CENTER", 4) + mono(490, U + 8, "LFE", 5)
    s += t(730, U + 36, "FRONT: FL + FR · REAR: RL + RR (SIDE: SL + SR on 7.1)", 10, "#9a9da2", weight="400", ls=0)
    s += t(730, U + 54, "no partner (FC, LFE, AUX0…): a mono frame", 10, "#9a9da2", weight="400", ls=0)
    s += t(730, U + 72, "MASTER: the node's volume and mute; its meter the loudest channel", 10, "#9a9da2", weight="400", ls=0)
    return ("G  pairs by position: FRONT, REAR, CENTER, LFE under MASTER", "2U: channels paired by the ports' audio.channel (SPA's fixed positions), each pair a stereo frame as any (VOLUME, BAL, SPLIT, two columns); what has no partner a mono frame. MUTE only on MASTER: PipeWire mutes only the node.", s, 230)


VARIANTS = [A(), B(), C(), D(), E(), F(), G()]
body, y = [], 20.0
for title, note, svg, h in VARIANTS:
    body.append(t(20, y + 16, title, 15, "#e6e7e8", ls=0) + t(20, y + 36, note, 11, "#9a9da2", weight="400", ls=0))
    body.append(f'<g transform="translate(20,{y + 48})">{svg}</g>')
    y += h + 70
W = FW + 40
svg = (f'<svg xmlns="http://www.w3.org/2000/svg" width="{W:.0f}" height="{y:.0f}">'
       '<rect width="100%" height="100%" fill="#16171a"/>' + "".join(body) + "</svg>")
p = os.path.join(HERE, "channels.svg")
with open(p, "w") as f:
    f.write(svg)
subprocess.run(["rsvg-convert", "-o", os.path.join(HERE, "channels.png"), p], check=True)
os.remove(p)
