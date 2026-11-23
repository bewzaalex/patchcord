# The whole palette as it is in the code: every kind of block (ENGINE's blocks, Patchcord's own
# too; the rack's bands on each of the three paints), every group of ports (jacks' rings), the
# device paints, the cord colours, and the signal colours ENGINE uses. Kept in step with
# face::kind_colour, face::ACCENTS, face::fin, graph::PALETTE. (The candidates for OUTPUT's colour:
# outcolour.py.)
#   python3 palette.py palette.svg && rsvg-convert palette.svg -o palette.png
import sys

from nodes import F, M, text, o

INK, DARK = "#f3efe6", "#1d1e20"
KINDS = [("SINK", "#007c7d"), ("SOURCE", "#206ea6"), ("PLAYBACK STREAM", "#7e5a42"), ("RECORDING STREAM", "#427935"),
         ("MIDI", "#934c7e"), ("VIDEO", "#876200"), ("DUPLEX", "#73348d"), ("NO CLASS", "#cba553")]
GROUPS = [("PLAYBACK", "#007c7d"), ("CAPTURE", "#206ea6"), ("INPUT", "#427935"), ("OUTPUT", "#7e5a42"),
          ("MONITOR", "#695ca6"), ("MIDI", "#934c7e"), ("VIDEO", "#876200"), ("OTHER", "#cba553")]
# a device's paint (face::fin): base, print
PAINTS = [("ALUMINIUM", "hardware: the top panel's metal", "#8c8e91", "#1d1e20"), ("BLACK", "a program", "#232427", "#d5d7d9"),
          ("OLIVE", "PipeWire's or WirePlumber's own", "#66683f", "#f1f1e6")]
# the cords: each device its own (graph::PALETTE)
CORDS = ["#d4232a", "#ec7a1c", "#f0cc1c", "#14965a", "#12a5b8", "#1752c8", "#7b4fb8", "#d63c8c"]
SIGNALS = [("xruns", "#ff4a32"), ("running", "#5cff7a"), ("idle", "#ffb02e"), ("chosen", "#f7bb62"),
           ("cue", "#5fd4ff")]


def eblock(x, y, name, c, ins, outs, w=150):
    """A small ENGINE block: its cap and outline its kind's colour, its ports their groups'."""
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="62" rx="7" fill="#1d1e21" stroke="{c}" stroke-width="2"/>')
    o.append(f'<path d="M{x},{y + 7} a7,7 0 0 1 7,-7 h{w - 14} a7,7 0 0 1 7,7 v-2 h-{w} z" fill="{c}"/>')
    text(x + 10, y + 22, name, 9.5, "#e6e7e8", True, extra='letter-spacing=".5"')
    for k, pc in enumerate(ins):
        o.append(f'<rect x="{x + 3}" y="{y + 32 + k * 14}" width="40" height="11" rx="5.5" fill="{pc}" fill-opacity=".28"/><circle cx="{x}" cy="{y + 37.5 + k * 14}" r="3.6" fill="{pc}" stroke="#0f1011"/>')
    for k, pc in enumerate(outs):
        o.append(f'<rect x="{x + w - 43}" y="{y + 32 + k * 14}" width="40" height="11" rx="5.5" fill="{pc}" fill-opacity=".28"/><circle cx="{x + w}" cy="{y + 37.5 + k * 14}" r="3.6" fill="{pc}" stroke="#0f1011"/>')


def band(x, y, name, c, w=176):
    """A frame's band on the rack."""
    ink = DARK if c == "#cba553" else INK
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="44" rx="7" fill="#000" fill-opacity=".05" stroke="{c}" stroke-width="2.2"/>')
    o.append(f'<path d="M{x},{y + 7} a7,7 0 0 1 7,-7 h{w - 14} a7,7 0 0 1 7,7 v14 h-{w} z" fill="{c}"/>')
    text(x + w / 2, y + 15.5, name, 10, ink, True, "middle", 'letter-spacing="1.1"')


def heading(y, t, sub=""):
    text(60, y, t, 20, "#e58f2c", True, extra='letter-spacing="1.5"')
    if sub:
        text(60, y + 22, sub, 13.5, "#9a9da1")


if __name__ == "__main__":
    W, y = 1720, 150
    gc = dict(GROUPS)
    # ENGINE: every kind of block, Patchcord's own, a link out of each, the signal colours
    heading(y, "ENGINE", "A block in its kind's colour, its ports in their groups', a link in the colour of the port it leaves.")
    y += 40
    o.append(f'<rect x="50" y="{y}" width="{W - 100}" height="250" rx="8" fill="#1a1b1d" stroke="#2c2d30"/>')
    ports = {
        "SINK": ([gc["PLAYBACK"]] * 2, [gc["MONITOR"]] * 2), "SOURCE": ([], [gc["CAPTURE"]] * 2),
        "PLAYBACK STREAM": ([], [gc["OUTPUT"]] * 2), "RECORDING STREAM": ([gc["INPUT"]] * 2, [gc["MONITOR"]] * 2),
        "MIDI": ([gc["MIDI"]], [gc["MIDI"]]), "VIDEO": ([], [gc["VIDEO"]]),
        "DUPLEX": ([gc["INPUT"]] * 2, [gc["OUTPUT"]] * 2), "NO CLASS": ([gc["INPUT"]] * 2, [gc["OUTPUT"]] * 2),
    }
    for k, (name, c) in enumerate(KINDS):
        x = 70 + k * 200
        eblock(x, y + 16, name if len(name) < 15 else name.split()[0], c, *ports[name])
        oc = ports[name][1][0]
        o.append(f'<path d="M{x + 150},{y + 53.5} C{x + 172},{y + 53.5} {x + 160},{y + 110} {x + 182},{y + 110}" fill="none" stroke="{oc}" stroke-width="2"/>')
    for k, (name, c, ins, outs) in enumerate([("PATCHCORD METER", "#427935", [gc["INPUT"]] * 2, [gc["MONITOR"]] * 2),
                                               ("CUE IN", "#427935", [gc["INPUT"]] * 2, [gc["MONITOR"]] * 2),
                                               ("CUE OUT", "#7e5a42", [], [gc["OUTPUT"]] * 2)]):
        eblock(70 + k * 200, y + 116, name, c, ins, outs)
    o.append(f'<path d="M70,{y + 214} C110,{y + 214} 110,{y + 230} 160,{y + 230}" fill="none" stroke="#ff4a32" stroke-width="2"/>')
    text(166, y + 234, "error", 11, "#ff6a52", font=M)
    for k, (name, c) in enumerate(SIGNALS):
        cx = 260 + k * 130
        o.append(f'<circle cx="{cx}" cy="{y + 224}" r="5" fill="{c}"/>')
        text(cx + 10, y + 228, name, 11, "#9a9da1", font=M)
    y += 290
    # RACK: every kind's band on each paint
    heading(y, "RACK · KINDS OF BLOCK ON EACH PAINT", "A frame's band and outline in its kind's colour; the device's paint is who owns it.")
    y += 40
    for pname, who, base, prn in PAINTS:
        o.append(f'<rect x="50" y="{y}" width="{W - 100}" height="86" rx="8" fill="{base}" stroke="#000" stroke-opacity=".6"/>')
        text(66, y + 20, f"{pname}  {base}", 11, prn, True, extra='letter-spacing="1"')
        text(66 + 8.5 * len(pname) + 90, y + 20, who, 11, prn)
        for k, (name, c) in enumerate(KINDS):
            band(66 + k * 200, y + 32, name, c)
        y += 96
    y += 30
    # GROUPS: the jacks' rings
    heading(y, "GROUPS OF PORTS", "A jack's ring on the rack, a port's dot in ENGINE.")
    y += 46
    for k, (name, c) in enumerate(GROUPS):
        x = 74 + k * 200
        o.append(f'<circle cx="{x}" cy="{y + 14}" r="13" fill="#2b2c2f" stroke="#111" stroke-width="2"/><circle cx="{x}" cy="{y + 14}" r="9.5" fill="none" stroke="{c}" stroke-width="3"/><circle cx="{x}" cy="{y + 14}" r="5" fill="#0c0c0d"/>')
        text(x + 22, y + 12, name, 12, "#e6e7e8", True, extra='letter-spacing=".8"')
        text(x + 22, y + 28, c, 11, "#9a9da1", font=M)
    y += 80
    # CORDS: each device its own colour
    heading(y, "CORDS", "Each device its own colour, a new one the least used; a cord in the colour of the device it leaves.")
    y += 40
    o.append(f'<rect x="50" y="{y}" width="{W - 100}" height="110" rx="8" fill="#232427"/>')
    for k, c in enumerate(CORDS):
        x = 80 + k * 200
        d = f'M{x},{y + 30} C{x + 40},{y + 90} {x + 110},{y + 90} {x + 150},{y + 30}'
        o.append(f'<path d="{d}" fill="none" stroke="#000" stroke-opacity=".45" stroke-width="8" stroke-linecap="round"/><path d="{d}" fill="none" stroke="{c}" stroke-width="5.5" stroke-linecap="round"/>')
        for px in (x, x + 150):
            o.append(f'<circle cx="{px}" cy="{y + 30}" r="7" fill="{c}" stroke="#0b0b0b" stroke-width="2"/>')
        text(x + 75, y + 102, c, 11, "#9a9da1", anchor="middle", font=M)
    y += 140
    head = f'<rect width="{W}" height="{y}" fill="#1f2023"/>'
    head += f'<text x="60" y="70" {F} font-weight="bold" font-size="40" letter-spacing="3" fill="#e58f2c">THE WHOLE PALETTE</text>'
    head += f'<text x="60" y="108" {F} font-size="20" fill="#9a9da1">As it is in the code: blocks, groups of ports, paints, cords, and ENGINE\'s signals.</text>'
    open(sys.argv[1] if len(sys.argv) > 1 else "palette.svg", "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{y}" viewBox="0 0 {W} {y}">{head}{"".join(o)}</svg>\n')
