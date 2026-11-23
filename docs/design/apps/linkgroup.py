# node.link-group as a roof: nodes PipeWire ties together (one module's two sides: a loopback, a
# filter-chain; WirePlumber's loopback behind a Bluetooth headset's mic) under one device, against
# each node a device of its own. The sound goes from one to the other inside, with no link in the
# graph: the panel prints that, it draws no cord.
#   python3 linkgroup.py linkgroup.svg && rsvg-convert linkgroup.svg -o linkgroup.png
import sys

import nodes as n
import progframes as pf
from hwframes import DEFS
from nodes import F, M, ROLE, text, o

R = ROLE
OLIVE, ALU = n.FIN["OLIVE"], n.FIN["ALUMINIUM"]


def unit(x, y, w, h, paint, lcd):
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="6" fill="{paint[0]}" stroke="#000" stroke-opacity=".6"/>')
    for sx in (x + 14, x + w - 14):
        for sy in (y + 14, y + h - 14):
            o.append(f'<circle cx="{sx}" cy="{sy}" r="6" fill="#8b8e93" stroke="#222"/>')
    o.append(f'<rect x="{x + 30}" y="{y + 18}" width="186" height="62" rx="3" fill="#15130c" stroke="#000"/>')
    for k, line in enumerate(lcd):
        text(x + 38, y + 35 + k * 17, line, 9.5 if k == 0 else 8.5, "#f2b13c", k == 0, font=M)


def inside(x0, x1, y, paint, label):
    """Printed on the panel between two frames: the sound goes on inside, no cord."""
    prn = paint[1]
    o.append(f'<path d="M{x0 + 6},{y} H{x1 - 10}" stroke="{prn}" stroke-width="2" stroke-opacity=".75"/>'
             f'<path d="M{x1 - 16},{y - 6} l8,6 l-8,6" fill="none" stroke="{prn}" stroke-width="2" stroke-opacity=".75"/>')
    text((x0 + x1) / 2, y - 10, label, 10, prn, True, "middle", 'letter-spacing="1"')
    text((x0 + x1) / 2, y + 18, "INSIDE", 9, prn, True, "middle", 'letter-spacing="1.5" opacity=".7"')


def device(x, y, paint, lcd, frames, links=None):
    """One unit: its screen, then its frames left to right; between two, what goes on inside."""
    widths = []
    for fr in frames:
        at = len(o)
        widths.append(pf.frame(0, 0, fr, paint))
        del o[at:]
    gap = 160 if links else 40
    w = 250 + sum(widths) + gap * (len(frames) - 1) + 50
    unit(x, y, w, pf.FH + 40, paint, lcd)
    fx = x + 240
    for k, (fr, fw) in enumerate(zip(frames, widths)):
        pf.frame(fx, y + 20, fr, paint)
        if links and k < len(frames) - 1:
            inside(fx + fw, fx + fw + gap, y + 20 + 66, paint, links[k])
        fx += fw + gap
    return pf.FH + 40


def gain(st=True):
    return ("GAIN", st, 1.0, False)


def vol(st=True):
    return ("VOLUME", st, 1.0, False)


# A filter-chain noise canceller (not on this machine): the mic goes into its capture stream, the
# filter runs inside, apps record from its source.
NC_IN = {"name": "INPUT", "bands": [R["INPUT"]], "zones": [{"tag": "IN", "jacks": [("MONO", R["INPUT"]), ("MONITOR", R["MONITOR"])], "cols": 1}], "mix": gain(False)}
NC_OUT = {"name": "CAPTURE", "bands": [R["CAPTURE"]], "zones": [{"tag": "OUT", "jacks": [("MONO", R["CAPTURE"])], "cols": 1}], "mix": gain(False)}
# A loopback as a virtual sink (not on this machine): apps play into its sink, it plays on.
LB_IN = {"name": "PLAYBACK", "bands": [R["PLAYBACK"]], "zones": [{"tag": "IN", "jacks": [("STEREO", R["PLAYBACK"]), ("MONITOR", R["MONITOR"])], "cols": 2, "split": True}], "mix": vol()}
LB_OUT = {"name": "OUTPUT", "bands": [R["OUTPUT"]], "zones": [{"tag": "OUT", "jacks": [("STEREO", R["OUTPUT"])], "cols": 2, "split": True}], "mix": vol()}
# WirePlumber's loopback behind the headset's mic (this machine): its internal capture stream, and
# the MIC apps see.
BT_SINK = {"name": "HEADPHONES", "bands": [R["PLAYBACK"]], "zones": [{"tag": "IN", "jacks": [("STEREO", R["PLAYBACK"]), ("MONITOR", R["MONITOR"])], "cols": 2, "split": True, "lvl": .6}], "mix": ("VOLUME", True, .55, False)}
BT_INT = {"name": "INTERNAL", "bands": [R["INPUT"]], "zones": [{"tag": "IN", "jacks": [("MONO", R["INPUT"]), ("MONITOR", R["MONITOR"])], "cols": 1}], "mix": gain(False)}
BT_MIC = {"name": "HANDSFREE", "bands": [R["CAPTURE"]], "zones": [{"tag": "OUT", "jacks": [("MONO", R["CAPTURE"])], "cols": 1, "lvl": .3}], "mix": gain(False)}

CASES = [
    ("A FILTER-CHAIN: A NOISE CANCELLER (NOT ON THIS MACHINE)",
     "PipeWire's filter-chain module makes two nodes, node.link-group filter-chain-…: a capture stream the mic goes into, and a source apps record from.",
     [("WITHOUT: TWO DEVICES", [(OLIVE, ("CAPTURE.RNNOISE_SOURCE", "NODE · INTERNAL", "1 NODE  3 PORTS"), [NC_IN], None),
                                (OLIVE, ("NOISE CANCELING SOURCE", "NODE · VIRTUAL SOURCE", "1 NODE  1 PORT"), [NC_OUT], None)]),
      ("WITH: ONE DEVICE, TWO FRAMES", [(OLIVE, ("NOISE CANCELING SOURCE", "NODE · FILTER", "2 NODES  4 PORTS"), [NC_IN, NC_OUT], ["THROUGH THE FILTER"])])]),
    ("A LOOPBACK AS A VIRTUAL SINK (NOT ON THIS MACHINE)",
     "PipeWire's loopback module, node.link-group loopback-…: a sink apps play into, and a playback stream that plays it on to wherever it is plugged.",
     [("WITHOUT: TWO DEVICES", [(OLIVE, ("VIRTUAL SINK", "NODE · LOOPBACK", "1 NODE  4 PORTS"), [LB_IN], None),
                                (OLIVE, ("OUTPUT.LOOPBACK-2005-3", "NODE · LOOPBACK", "1 NODE  2 PORTS"), [LB_OUT], None)]),
      ("WITH: ONE DEVICE, TWO FRAMES", [(OLIVE, ("VIRTUAL SINK", "NODE · LOOPBACK", "2 NODES  6 PORTS"), [LB_IN, LB_OUT], ["AS IT IS"])])]),
    ("WIREPLUMBER'S LOOPBACK BEHIND THE HEADSET'S MIC (THIS MACHINE)",
     "node.link-group loopback-2005-21: the internal capture stream (WirePlumber's, no device) and the MIC (the headset's). It joins the headset's roof.",
     [("WITHOUT: THE STREAM A DEVICE OF ITS OWN", [(ALU, ("JLAB GO WORK 2", "BLUETOOTH · HEADSET", "2 NODES  5 PORTS"), [BT_SINK, BT_MIC], None),
                                                   (OLIVE, ("BLUETOOTH INTERNAL CAPT…", "NODE · INTERNAL", "1 NODE  2 PORTS"), [BT_INT], None)]),
      ("WITH: UNDER THE HEADSET'S ROOF", [(ALU, ("JLAB GO WORK 2", "BLUETOOTH · HEADSET", "3 NODES  7 PORTS"), [BT_SINK, BT_INT, BT_MIC], [None, "TO THE MIC"])])]),
]

if __name__ == "__main__":
    W, y = 1700, 150
    for title, sub, variants in CASES:
        text(60, y, title, 20, "#e58f2c", True, extra='letter-spacing="1.5"')
        text(60, y + 24, sub, 14, "#9a9da1")
        y += 50
        for name, devices in variants:
            text(60, y + 12, name, 14, "#e6e7e8", True, extra='letter-spacing="1"')
            y += 24
            for paint, lcd, frames, links in devices:
                if links and links[0] is None:  # the headset: the sink, then the stream inside to its MIC
                    widths = []
                    for fr in frames:
                        at = len(o)
                        widths.append(pf.frame(0, 0, fr, paint))
                        del o[at:]
                    w = 250 + widths[0] + 40 + widths[1] + 160 + widths[2] + 50
                    unit(60, y, w, pf.FH + 40, paint, lcd)
                    fx = 300
                    pf.frame(fx, y + 20, frames[0], paint)
                    fx += widths[0] + 40
                    pf.frame(fx, y + 20, frames[1], paint)
                    inside(fx + widths[1], fx + widths[1] + 160, y + 86, paint, links[1])
                    fx += widths[1] + 160
                    pf.frame(fx, y + 20, frames[2], paint)
                    y += pf.FH + 40 + 10
                else:
                    y += device(60, y, paint, lcd, frames, links) + 10
            y += 20
        y += 30
    notes = [
        "HOW IT READS",
        "· A link group is one module's (or WirePlumber's) doing: its nodes are one thing to the user, so one device, a frame a node, as everywhere.",
        "· The sound goes from one to the other inside: PipeWire has no link there, so no cord. The panel prints an arrow and INSIDE between the frames.",
        "· The device takes the group's name (the nodes' shared node.description); its screen says FILTER or LOOPBACK, as now.",
        "· Every jack stays: a cord into the internal stream or the filter's input is a real link, as any other.",
    ]
    for k, t in enumerate(notes):
        text(60, y + k * 26, t, 20 if k == 0 else 15, "#e58f2c" if k == 0 else "#c9cbcd", k == 0)
    y += len(notes) * 26 + 40
    head = f'{DEFS}<rect width="{W}" height="{y}" fill="#1f2023"/>'
    head += f'<text x="60" y="70" {F} font-weight="bold" font-size="40" letter-spacing="3" fill="#e58f2c">ONE LINK GROUP, ONE DEVICE</text>'
    head += f'<text x="60" y="108" {F} font-size="20" fill="#9a9da1">Nodes PipeWire ties with node.link-group, under one roof, against a device each.</text>'
    open(sys.argv[1] if len(sys.argv) > 1 else "linkgroup.svg", "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{y}" viewBox="0 0 {W} {y}">{head}{"".join(o)}</svg>\n')
