# Variant B of roof.py up close: a program as one device showing only its edges (what reaches
# outside it), its insides behind INSIDE. Easy Effects as it runs here. Three states: closed,
# INSIDE open, and after a cord from outside to one of its inner ports.
#   python3 edges.py edges.svg && rsvg-convert -z .5 edges.svg -o edges.png
import sys

import roof as r
from roof import o, text, frame, face, cord, away, rails, U, GAP, NUMS, PW, EAR, PAD, IDW, FW, FGAP, BLACK, CORD

LEFT = 70
NOTE_X = LEFT + PW + 2 * NUMS + 300


def notes(y, head, lines):
    text(NOTE_X, y, head, 22, "#e58f2c", True, extra='letter-spacing="2"')
    yy = y + 38
    col = "#c9cbcd"
    for ln in lines:
        if ln.startswith("·"):
            col = "#c9cbcd"
            text(NOTE_X + 6, yy, ln, 19, col)
        elif ln.startswith("?"):
            col = "#8fd0a0"
            text(NOTE_X + 6, yy, "◇ " + ln[1:].strip(), 19, col)
        elif ln.startswith(" "):
            text(NOTE_X + 6, yy, "  " + ln.strip(), 19, col, extra='xml:space="preserve"')
        else:
            yy += 10
            text(NOTE_X, yy, ln, 19, "#9a9da1", True, extra='letter-spacing="1"')
        yy += 29
    return yy


def section(y, t, sub):
    text(LEFT, y, t, 30, "#e58f2c", True, extra='letter-spacing="2"')
    text(LEFT, y + 36, sub, 20, "#c9cbcd")


EDGE = [("EASY EFFECTS SINK", "in", "sink_in"), ("SPECTRUM", "in", "spc_i_in"), ("OUTPUT LEVEL METER", "out", "lvl_o_out"), ("EASY EFFECTS SOURCE", "out", "src_out")]


def closed_face(y, extra=(), line2="4 LINKS"):
    rails(LEFT, y, U + GAP)
    j, h = face(LEFT + NUMS, y + GAP, 1, BLACK, "EASYEFFECTS", "APP", line2, "7 NODES  26 PORTS", [EDGE + list(extra)], keys=("INSIDE ▾",), line4="STEREO")
    away(j["sink_in"], "← VIVALDI, KADENCE", CORD["app"], 1)
    away(j["spc_i_in"], "← JLAB MIC", CORD["mic"], 1)
    away(j["lvl_o_out"], "→ JLAB HEADPHONES", CORD["out"], 1)
    return j


def drawer(y, j, monitor_out=False):
    """INSIDE open: a tray under the face, as wide as it, the program's nodes as small blocks in
    the order the sound goes, its own cords between them; the edges marked with their frames."""
    x0 = LEFT + NUMS + EAR + PAD
    w = PW - 2 * (EAR + PAD)
    h = 250
    o.append(f'<rect x="{LEFT + NUMS + 6}" y="{y}" width="{PW - 12}" height="{h}" rx="4" fill="#141517" stroke="#3a3c40" stroke-width="2"/>')
    o.append(f'<path d="M{LEFT + NUMS + 6},{y + 1} H{LEFT + NUMS + PW - 6}" stroke="#000" stroke-width="5" opacity=".6"/>')
    text(x0, y + 30, "INSIDE ▴   7 NODES · 4 LINKS OF ITS OWN · AS EASY EFFECTS WIRED THEM", 13, "#9a9da1", True, extra='letter-spacing="1.5"')
    bw, bh = 176, 46
    xs = [x0 + 30 + i * 215 for i in range(5)]

    def block(cx, cy, name, ins, outs, edge=None, dim=False):
        bx = cx - bw / 2
        o.append(f'<rect x="{bx}" y="{cy - bh / 2}" width="{bw}" height="{bh}" rx="5" fill="#26282b" stroke="{"#4a4c50" if not edge else "#e58f2c"}" stroke-width="2"/>')
        text(cx, cy - 4, name, 12, "#e6e7e8" if not dim else "#8a8d91", True, "middle")
        pts = {}
        if ins:
            o.append(f'<circle cx="{bx}" cy="{cy}" r="6" fill="#2e8580" stroke="#111" stroke-width="1.5"/>')
            pts["in"] = (bx, cy)
        if outs:
            o.append(f'<circle cx="{bx + bw}" cy="{cy}" r="6" fill="#3a6596" stroke="#111" stroke-width="1.5"/>')
            pts["out"] = (bx + bw, cy)
        return pts

    def wire(a, b, col):
        (x1, y1), (x2, y2) = a, b
        mx = (x1 + x2) / 2
        o.append(f'<path d="M{x1},{y1} C{mx},{y1} {mx},{y2} {x2},{y2}" fill="none" stroke="{col}" stroke-width="3"/>')

    def chip(p, label, side, col):
        x, yy = p
        cw = 9 * len(label) + 24
        cx = x - cw - 14 if side < 0 else x + 14
        o.append(f'<path d="M{x},{yy} H{cx + (cw if side < 0 else 0)}" stroke="{col}" stroke-width="3" stroke-dasharray="2 6"/>')
        o.append(f'<rect x="{cx}" y="{yy - 12}" width="{cw}" height="24" rx="12" fill="#1f2023" stroke="{col}" stroke-width="2"/>')
        text(cx + cw / 2, yy + 5, label, 11.5, col, True, "middle")

    r1, r2, r3 = y + 85, y + 155, y + 220
    sink = block(xs[1], r1, "EASY EFFECTS SINK", True, True, edge=True)
    s1 = block(xs[2], r1, "SPECTRUM", True, True)
    l1 = block(xs[3], r1, "OUTPUT LEVEL METER", True, True, edge=True)
    s2 = block(xs[1], r2, "SPECTRUM", True, True, edge=True)
    l2 = block(xs[2], r2, "OUTPUT LEVEL METER", True, True)
    src = block(xs[3], r2, "EASY EFFECTS SOURCE", True, True, edge=True)
    test = block(xs[1], r3, "EASY EFFECTS FILTER", False, True, dim=True)
    text(xs[1] + bw / 2 + 20, r3 + 5, "no links", 13, "#8a8d91")
    wire(sink["out"], s1["in"], CORD["out"] if not monitor_out else CORD["out"])
    wire(s1["out"], l1["in"], CORD["out"])
    wire(s2["out"], l2["in"], CORD["mic"])
    wire(l2["out"], src["in"], CORD["mic"])
    chip(sink["in"], "ON THE FACE", -1, CORD["app"])
    chip(s2["in"], "ON THE FACE", -1, CORD["mic"])
    chip(l1["out"], "ON THE FACE", 1, CORD["out"])
    chip(src["out"], "ON THE FACE", 1, CORD["in"])
    if monitor_out:
        sx, sy = sink["out"]
        o.append(f'<path d="M{sx},{sy} C{sx + 30},{sy - 40} {sx + 60},{sy - 40} {sx + 80},{sy - 42}" fill="none" stroke="#c77dff" stroke-width="3" stroke-dasharray="2 6"/>')
        o.append(f'<rect x="{sx + 80}" y="{sy - 54}" width="150" height="24" rx="12" fill="#1f2023" stroke="#c77dff" stroke-width="2"/>')
        text(sx + 155, sy - 37, "ON THE FACE", 11.5, "#c77dff", True, "middle")
    return h


if __name__ == "__main__":
    W = NOTE_X + 1000
    y = 220
    # ---- 1 closed ----
    section(y, "1 · CLOSED", "What a program shows by itself: only the ports that meet the world outside it.")
    fy = y + 80
    j = closed_face(fy)
    notes(fy - 10, "WHICH PORTS ARE ON THE FACE", [
        "· a port with a cord to anything outside the program (SPECTRUM IN ← the mic,",
        "   LEVEL METER OUT → the headphones)",
        "· always: what other programs pick by name, the sink's input and the source's",
        "   output (media.class Audio/Sink, Audio/Source), even with no cords (SOURCE OUT)",
        "· not: ports whose cords all stay inside it (the sink's MONITOR, the meters'",
        "   other ends), nor a node with no cords at all (TEST SIGNAL)",
        "· our own meters' links don't count: they are not the program's",
        "FRAMES",
        "· named as the program named its node (node.description), nothing cut,",
        "   nothing numbered: EASY EFFECTS SINK, SPECTRUM, OUTPUT LEVEL METER",
        "· in the order the sound goes: inputs, then outputs, as on every face",
        "· they keep their controls and meters: the sink's VOLUME is the sink's",
    ])
    # ---- 2 inside open ----
    y = fy + U + 330
    section(y, "2 · INSIDE ▾ OPEN", "A tray slides out under the face: the program's own wiring, every node and link, nothing hidden.")
    fy = y + 80
    j = closed_face(fy)
    o.append(f'<rect x="{LEFT + NUMS + EAR + PAD + 32 - 2}" y="{fy + GAP + 107 - 2}" width="{(IDW - 8 - 28)}" height="18" rx="3" fill="none" stroke="#e58f2c" stroke-width="2.5"/>')
    dh = drawer(fy + U + 70, j)
    notes(fy - 10, "THE TRAY", [
        "· every node of the program as a block, in the order the sound goes;",
        "   the program's cords between them (read from PipeWire, as ENGINE does)",
        "· the edges (on the face) marked with their frames' names",
        "· nodes that come and go with the program (an effect switched on in",
        "   Easy Effects) appear here; the face stays as it is",
        "· in the rack it pushes the devices below down while open; on the desk it",
        "   folds out under the face",
        "· read-only: no cords are plugged here",
        "· a click on a block: that node in ENGINE",
        "· it opens only by its key (not by itself while a cord is carried)",
    ])
    # ---- 3 a cord to an inner port ----
    y = fy + U + 70 + dh + 110
    section(y, "3 · A CORD TO AN INNER PORT", "Made by another program (OBS recording what Easy Effects plays): the port meets the world now, so it goes on the face.")
    fy = y + 80
    rails(LEFT, fy, 2 * U + GAP)
    rows = [EDGE, [("EASY EFFECTS SINK", "out", "mon_out")]]
    j, _ = face(LEFT + NUMS, fy + GAP, 2, BLACK, "EASYEFFECTS", "APP", "5 LINKS", "7 NODES  26 PORTS", rows, keys=("INSIDE ▾",), line4="STEREO")
    mx, mw = LEFT + NUMS + EAR + PAD, r.fw("EASY EFFECTS SINK")
    o.append(f'<rect x="{mx - 5}" y="{fy + GAP + U + 13}" width="{mw + 10}" height="102" rx="8" fill="none" stroke="#c77dff" stroke-width="3" stroke-dasharray="8 6"/>')
    away(j["sink_in"], "← VIVALDI, KADENCE", CORD["app"], 1)
    away(j["spc_i_in"], "← JLAB MIC", CORD["mic"], 1)
    away(j["lvl_o_out"], "→ JLAB HEADPHONES", CORD["out"], 1)
    away(j["mon_out"], "→ OBS", "#c77dff", 1)
    notes(fy - 10, "WHEN THE EDGES CHANGE", [
        "· a frame comes onto the face when its port gets a cord to the outside, and",
        "   goes back inside when that cord goes (the face never loses a cord)",
        "· the program wiring itself differently (Easy Effects moving to another",
        "   output) moves its frames the same way",
        "· the moment that cord goes, so does the frame",
    ])
    # ---- the open questions ----
    qy = fy + 2 * U + 120
    notes(qy, "THE SCREEN", [
        "· line 1: APP, as every program; its state at the right",
        "· line 2: its cords to the outside (5 LINKS)",
        "· line 3: what it takes of PipeWire, on every device: 7 NODES  26 PORTS",
        "· line 4: what was line 3 (STEREO; a card's profile)",
        "THE NAME",
        "? as every app's (graph.rs): application.name, here from its client: easyeffects",
        "? or the name its .desktop file gives (application.id → Name=): Easy Effects",
    ])
    H = qy + 260
    head = f'<rect width="{W}" height="{H}" fill="#1f2023"/>'
    head += f'<text x="{LEFT}" y="70" font-family="Noto Sans" font-weight="bold" font-size="44" letter-spacing="3" fill="#e58f2c">B · ONE FACE, ITS EDGES</text>'
    head += f'<text x="{LEFT}" y="116" font-family="Noto Sans" font-size="24" fill="#9a9da1">Easy Effects as one device: its 7 nodes under one roof, on the face only what meets the world. Dotted cords go to devices outside the picture.</text>'
    open(sys.argv[1] if len(sys.argv) > 1 else "edges.svg", "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">{head}{"".join(o)}</svg>\n')
