# Variant 1 for a program under one roof: its edge ports in one run of frames, every input first,
# then every output (as on every face), each frame named as the program named its node
# (node.description), IN or OUT on its tag. Only what PipeWire says. Frames that don't fit a unit
# go on as the rack does (variant E: the lower units fill first, all from the left).
#   python3 flat.py flat.svg && rsvg-convert -z .5 flat.svg -o flat.png
import sys

import roof as r
from roof import o, text, frame, cord, away, U, GAP, NUMS, EAR, PAD, IDW, FGAP, BLACK, CREAM, CORD, fw

LEFT = 70
r.PW = 1100.0


def width(f):
    return fw(f[0], len(f[3]) if len(f) > 3 else 2)


def fits(row, room):
    return sum(width(f) for f in row) + FGAP * max(0, len(row) - 1) <= room


def units(frames, pw):
    """Variant E: the lower units first, the whole width; beside the name what is left."""
    whole = pw - 2 * (EAR + PAD)
    first = whole - IDW - 14
    for n in range(1, 10):
        rows = [[] for _ in range(n)]
        k, ok = n - 1, True
        for f in reversed(frames):
            while k >= 0 and not fits([f] + rows[k], first if k == 0 else whole):
                k -= 1
            if k < 0:
                ok = False
                break
            rows[k].insert(0, f)
        if ok:
            return rows
    return [frames]


def device(x, y, pw, name, kind, line2, line3, line4, frames, base=BLACK, keys=("INSIDE ▾",)):
    r.PW = pw
    rows = units(frames, pw)
    n = len(rows)
    o.append(f'<rect x="{x}" y="{y}" width="{NUMS}" height="{n * U + GAP}" fill="#303134"/><rect x="{x + pw + NUMS}" y="{y}" width="{NUMS}" height="{n * U + GAP}" fill="#303134"/>')
    j, h = r.face(x + NUMS, y + GAP, n, base, name, kind, line2, line3, rows, keys=keys, line4=line4)
    return j, n * U + GAP


def section(y, t, sub):
    text(LEFT, y, t, 30, "#e58f2c", True, extra='letter-spacing="2"')
    text(LEFT, y + 36, sub, 20, "#c9cbcd")


def notes(x, y, head, lines):
    text(x, y, head, 22, "#e58f2c", True, extra='letter-spacing="2"')
    yy = y + 38
    col = "#c9cbcd"
    for ln in lines:
        if ln.startswith("·"):
            col = "#c9cbcd"
            text(x + 6, yy, ln, 19, col)
        elif ln.startswith("?"):
            col = "#8fd0a0"
            text(x + 6, yy, "◇ " + ln[1:].strip(), 19, col)
        elif ln.startswith(" "):
            text(x + 6, yy, "  " + ln.strip(), 19, col, extra='xml:space="preserve"')
        else:
            yy += 10
            text(x, yy, ln, 19, "#9a9da1", True, extra='letter-spacing="1"')
        yy += 29
    return yy


def tip(x, y, lines):
    w = max(len(s) for s in lines) * 9.2 + 30
    h = 22 + 24 * len(lines)
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="6" fill="#0e0f10" stroke="#e6e7e8" stroke-opacity=".5"/>')
    for i, s in enumerate(lines):
        text(x + 14, y + 30 + 24 * i, s, 15, "#e6e7e8" if i == 0 else "#9a9da1", i == 0, font=r.M)


if __name__ == "__main__":
    NX = LEFT + 1100 + 2 * NUMS + 300
    W = NX + 1050
    y = 210

    # ---- 1: Easy Effects as it is now ----
    section(y, "1 · EASY EFFECTS NOW", "This graph now, equalizers on: two inputs, two outputs meet the world, inputs first, each named after its node.")
    fy = y + 80
    E = [("EASY EFFECTS SINK", "in", "sink_in"), ("EQUALIZER (MIC)", "in", "eq_i_in"), ("OUTPUT LEVEL METER", "out", "lvl_o_out"), ("EASY EFFECTS SOURCE", "out", "src_out")]
    j, h = device(LEFT, fy, 1100, "EASYEFFECTS", "APP", "9 LINKS", "9 NODES  34 PORTS", "STEREO", E)
    away(j["sink_in"], "← 6 APPS", CORD["app"], 1)
    away(j["eq_i_in"], "← JLAB MIC", CORD["mic"], 1)
    away(j["lvl_o_out"], "→ JLAB HEADPHONES", CORD["out"], 1)
    away(j["src_out"], "→ CARLA", CORD["in"], 1)
    notes(NX, fy - 10, "THE RUN OF FRAMES", [
        "· every edge port's group a frame: what reaches outside the program, and",
        "   the sink's input and the source's output always",
        "· inputs first, then outputs, as on every face: IN and OUT on the tags",
        "· inputs among themselves, outputs among themselves: in the order the",
        "   program made its nodes (object.serial), so they don't move about",
        "· a frame's name: its node's node.description, as the program named it",
        "· the chain changes, the edges follow: an equalizer put first on the mic",
        "   takes SPECTRUM's place: EQUALIZER (MIC) IN",
        "THE NAME ON THE PLATE",
        "· as every app's, from PipeWire: application.name (the node's, else its",
        "   client's): easyeffects → EASYEFFECTS",
    ])

    # ---- 2: more edges, two of a name ----
    y = fy + h + 200
    section(y, "2 · MORE EDGES, TWO OF A NAME", "OBS records the sink's monitor; the other spectrum's input gets a cord too: six frames, two units.")
    fy = y + 80
    E2 = [("EASY EFFECTS SINK", "in", "sink_in"), ("SPECTRUM", "in", "spc_i_in"), ("SPECTRUM", "in", "spc_o_in"), ("EASY EFFECTS SINK", "out", "mon_out"), ("OUTPUT LEVEL METER", "out", "lvl_o_out"), ("EASY EFFECTS SOURCE", "out", "src_out")]
    j, h = device(LEFT, fy, 1100, "EASYEFFECTS", "APP", "6 LINKS", "7 NODES  26 PORTS", "STEREO", E2)
    away(j["sink_in"], "← VIVALDI, KADENCE", CORD["app"], 1)
    away(j["spc_i_in"], "← JLAB MIC", CORD["mic"], 1)
    away(j["spc_o_in"], "← RYZEN MIC", "#c77dff", 1)
    away(j["mon_out"], "→ OBS", "#c77dff", 1)
    away(j["lvl_o_out"], "→ JLAB HEADPHONES", CORD["out"], 1)
    sx, sy = j["spc_o_in"]
    tx, ty = LEFT + NUMS + EAR + PAD + IDW + 14 + r.fw("EASY EFFECTS SINK") + 60, fy + GAP + 12
    o.append(f'<path d="M{sx},{sy - 20} L{tx + 20},{ty + 100}" stroke="#e6e7e8" stroke-opacity=".5" stroke-width="2"/>')
    tip(tx, ty, ["SPECTRUM  IN", "node ee_soe_spectrum · id 248", "inside: → OUTPUT LEVEL METER", "        → out of OUTPUT LEVEL METER"])
    notes(NX, fy - 10, "TWO OF A NAME", [
        "· both stay SPECTRUM, as Easy Effects named them",
        "· the sink's input and its monitor: both EASY EFFECTS SINK, IN and OUT",
        "   tell them apart",
        "· the hover tooltip tells which is which: the node's node.name and id,",
        "   and where it leads inside (from the program's own links)",
        "? the tray marks the hovered frame's node and its path",
        "MORE THAN A UNIT HOLDS",
        "· as every face in the rack: the lower unit fills first, the whole width;",
        "   beside the name what is left; all from the left, inputs first",
    ])

    # ---- 3: without names of their own ----
    y = fy + h + 200
    section(y, "3 · NODES WITHOUT NAMES OF THEIR OWN", "When a node has no description, or only the program's own name, the frame says what it does, as today.")
    fy = y + 80
    T = [("INPUT", "in", "t_in"), ("OUTPUT 1", "out", "t1"), ("OUTPUT 2", "out", "t2"), ("OUTPUT 3", "out", "t3")]
    j, h = device(LEFT, fy, 1100, "TEAMS FOR LINUX", "APP", "2 LINKS", "4 NODES  10 PORTS", "STEREO", T, keys=())
    away(j["t_in"], "← JLAB MIC", CORD["mic"], 1)
    away(j["t1"], "→ JLAB HEADPHONES", CORD["out"], 1)
    fy2 = fy + h + 120
    F = [("NOISE CANCELING SOURCE", "in", "f_in"), ("NOISE CANCELING SOURCE", "out", "f_out")]
    j, h2 = device(LEFT, fy2, 1100, "NOISE CANCELING S…", "FILTER", "2 LINKS", "2 NODES  3 PORTS", "MONO", F, base=CREAM, keys=())
    away(j["f_in"], "← RYZEN MIC", CORD["mic"], 1)
    away(j["f_out"], "→ TEAMS FOR LINUX", CORD["in"], 1)
    notes(NX, fy - 10, "A FRAME'S NAME", [
        "· node.description, when the node has one and it isn't just the",
        "   device's own name (Easy Effects: SPECTRUM, OUTPUT LEVEL METER)",
        "· else what the group does, numbered when there are several, as today:",
        "   Teams' streams (no descriptions): INPUT, OUTPUT 1, 2, 3",
        "· a filter-chain or a loopback PipeWire runs itself: its two ends, joined",
        "   by node.link-group, are one device; both ends' description is the",
        "   filter's: NOISE CANCELING SOURCE, IN and OUT",
        "TEAMS",
        "· an app's streams have no tray: nothing inside (every stream is an edge)",
        "· INSIDE ▾ shows only where a program has nodes not on its face",
    ])
    # ---- 4: every program, one rule (this graph, now) ----
    y = fy2 + h2 + 200
    section(y, "4 · EVERY PROGRAM, ONE RULE", "This graph now: eight programs play. Today 16 plates (Easy Effects alone 9), under roofs 8.")
    fy = y + 80
    C = [("INPUT", "in", "c_in"), ("OUTPUT", "out", "c_out"), ("MONITOR", "out", "c_mon")]
    j, h = device(LEFT, fy, 1100, "CARLA", "APP", "2 LINKS", "2 NODES  6 PORTS", "STEREO", C, keys=())
    away(j["c_in"], "← EASY EFFECTS", CORD["in"], 1)
    away(j["c_out"], "→ EASY EFFECTS", CORD["app"], 1)
    by = fy + h + 110
    B = [("OUTPUT", "out", "b_out", ["out1", "out2"])]
    j, hb = device(LEFT, by, 1100, "BITWIG STUDIO", "APP", "1 LINK", "1 NODE  2 PORTS", "2 OUT", B, keys=())
    away(j["b_out"], "→ JLAB HEADPHONES", CORD["out"], 1)
    text(LEFT + NUMS + EAR + PAD + IDW + 14 + width(B[0]) + 220, by + GAP + 70, "today: VIRTUAL DEVICE, “BITWIG STUDIO'S AUDIO ENGINE”", 15, "#e58f2c", True)
    b8 = by + hb + 110
    B8 = [("INPUT", "in", "b8_in", ["in1", "in2", "in3", "in4"]), ("OUTPUT", "out", "b8_out", ["out1", "out2", "out3", "out4", "out5", "out6", "out7", "out8"])]
    j, h8 = device(LEFT, b8, 1100, "BITWIG STUDIO", "APP", "3 LINKS", "1 NODE  12 PORTS", "4 IN  8 OUT", B8, keys=())
    away(j["b8_in"], "← SCARLETT", CORD["mic"], 1)
    away(j["b8_out"], "→ SCARLETT", CORD["out"], 1)
    text(LEFT, b8 - 24, "the same, had Bitwig opened 4 inputs and 8 outputs (JACK style: mono ports, no channels, a port.group each)", 16, "#9a9da1")
    hy = b8 + h8 + 110
    j, hh = device(LEFT, hy, 1100, "HYDROGEN", "APP", "1 LINK", "1 NODE  2 PORTS", "STEREO", [("OUTPUT", "out", "h_out")], keys=())
    away(j["h_out"], "→ EASY EFFECTS", CORD["app"], 1)
    text(LEFT + NUMS + EAR + PAD + IDW + 14 + fw("OUTPUT") + 220, hy + GAP + 70, "its MIDI ports aren't here: PipeWire puts them on WirePlumber's Midi-Bridge (below)", 15, "#9a9da1")
    vy = hy + hh + 150
    text(LEFT, vy - 24, "two VLCs at once (native, pids differ): the first keeps its name, the next is numbered", 16, "#9a9da1")
    j, hv = device(LEFT, vy, 1100, "VLC MEDIA PLAYER (LIBVL…", "APP", "1 LINK", "1 NODE  2 PORTS", "STEREO", [("OUTPUT", "out", "v_out")], keys=())
    away(j["v_out"], "→ EASY EFFECTS", CORD["app"], 1)
    v2 = vy + hv + 100
    j, hv2 = device(LEFT, v2, 1100, "VLC MEDIA PLAYER (LI… 2", "APP", "1 LINK", "1 NODE  2 PORTS", "STEREO", [("OUTPUT", "out", "v2_out")], keys=())
    away(j["v2_out"], "→ JLAB HEADPHONES", CORD["out"], 1)
    my = v2 + hv2 + 150
    text(LEFT, my - 24, "WirePlumber's Midi-Bridge holds every ALSA MIDI client: a frame each (port.group, the alias's client part), not one MIDI frame", 16, "#9a9da1")
    MB = [("MIDI THROUGH", "in", "mt_in", ["Port-0"]), ("KEYSTEP 37", "in", "ks_in", ["MIDI"]), ("HYDROGEN", "in", "hm_in", ["Midi-In"]),
          ("MIDI THROUGH", "out", "mt_out", ["Port-0"]), ("KEYSTEP 37", "out", "ks_out", ["MIDI", "DIN"]), ("HYDROGEN", "out", "hm_out", ["Midi-Out"])]
    j, hm = device(LEFT, my, 1100, "MIDI-BRIDGE", "MIDI", "1 LINK", "1 NODE  7 PORTS", "", MB, base=CREAM, keys=())
    cord(j["ks_out"], j["hm_in"], "#c77dff", 70)
    text(LEFT, my + hm + 40, "Keystep 37: an example of a USB keyboard (port.physical), not on this system; its MIDI out cabled to Hydrogen's in", 16, "#9a9da1")
    yy = notes(NX, fy - 10, "WHO IS A PROGRAM", [
        "· a node's client.id leads to its client; clients join into one program",
        "   on application.name + application.process.binary + application.process.id",
        "   (the node's own, else its client's): Carla's two clients are one",
        "· not the pid alone: Easy Effects, Kadence, Soundcraft, Carla, qpwgraph are",
        "   Flatpaks, all pid 2; a Pulse app's pipewire.sec.pid is pipewire-pulse's",
        "· every node it made goes under its roof, whatever its media.class:",
        "   streams, its own sinks and sources (Easy Effects), a JACK-style node",
        "   (Bitwig: Audio/Duplex), nodes with no class (Easy Effects' meters)",
        "NOT A PROGRAM: THE SYSTEM, AS TODAY",
        "· a node with device.id: hardware, under its card",
        "· a node of PipeWire's own processes: core.daemon, binary pipewire",
        "   (pipewire-pulse, filter-chain), wireplumber.daemon: loopbacks and",
        "   filter-chains join by node.link-group, as today",
    ])
    notes(NX, b8 - 40, "A FRAME'S NAME, FOR ALL", [
        "· one node on the face: what the group does, as today (Bitwig: OUTPUT,",
        "   not BITWIG STUDIO'S AUDIO ENGINE: that's the program again)",
        "· several: node.description if they have one, else what each does,",
        "   numbered (Carla: no descriptions: INPUT, OUTPUT, MONITOR)",
        "· a JACK-style node: all its ports one way in one frame, however many,",
        "   each jack named by port.name; wraps onto more units as the rack does",
        "· every program is APP, named by application.name, even with one node",
    ])
    notes(NX, vy - 40, "ONE NAME TWICE", [
        "· the first (its client's object.serial) keeps the name; the next get",
        "   2, 3: a second VLC doesn't rename the first",
        "· the number goes on the name's end, the name is cut before it",
        "· two copies of one Flatpak Pulse app (same name, binary, pid 2):",
        "   PipeWire can't tell them apart: one roof",
    ])
    notes(NX, my - 40, "SEVERAL CLIENTS IN ONE NODE", [
        "· a node's ports split into frames when they say they're from different",
        "   clients: port.group differs and so does the alias's part before ':'",
        "   (Midi Through:…, Hydrogen:…); Bitwig's out1/out2 (Bitwig:…) stay one",
        "· the frame's name: that client part; a jack's: port.name after it",
        "· frames in ALSA's client order (port.group client_N): Midi Through 14,",
        "   hardware next (port.physical), programs from 128 on; inputs first",
        "· the same on any system: what clients it has, it shows; a node whose",
        "   ports are all one client's (BLE MIDI, JACK apps' MIDI) stays one frame",
        "· they stay on the bridge, not under Hydrogen's roof: PipeWire says",
        "   they're the bridge's ports",
    ])
    H = my + hm + 200
    head = f'<rect width="{W}" height="{H}" fill="#1f2023"/>'
    head += f'<text x="{LEFT}" y="70" font-family="Noto Sans" font-weight="bold" font-size="44" letter-spacing="3" fill="#e58f2c">ONE ROOF · 1: ONE RUN OF FRAMES</text>'
    head += f'<text x="{LEFT}" y="116" font-family="Noto Sans" font-size="24" fill="#9a9da1">A program\'s edges in one run: inputs, then outputs, named as the program named its nodes. Only what PipeWire gives. Dotted cords go outside the picture.</text>'
    open(sys.argv[1] if len(sys.argv) > 1 else "flat.svg", "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">{head}{"".join(o)}</svg>\n')
