# A colour for every kind of block: the six we have, and two to pick for DUPLEX and NO CLASS.
# The palette's mid tones are taken (OKLCH L .52, C .115, at seven hues), so the two new ones are
# picked to stand apart from all of them and from the three paints: a crimson the hues left free,
# or a light colour (dark lettering) that differs by lightness. Each variant drawn as the bands of
# all eight kinds on each paint.
#   python3 kinds.py kinds.svg && rsvg-convert kinds.svg -o kinds.png
import sys

from nodes import F, M, FIN, ROLE, text, o

DARK_INK, INK = "#1d1e20", "#f3efe6"
KINDS = [
    ("SINK", ROLE["PLAYBACK"]), ("SOURCE", ROLE["CAPTURE"]), ("PLAYBACK STREAM", ROLE["OUTPUT"]),
    ("RECORDING STREAM", ROLE["INPUT"]), ("MIDI", ROLE["MIDI"]), ("VIDEO", ROLE["VIDEO"]),
]
LIGHT = {"#cba553", "#62b995", "#419fc0"}  # bands lettered dark
VARIANTS = [
    ("1 · CRIMSON AND SAND", "#962339", "#cba553", "Duplex the red the hues left free; no class a light sand, apart by its lightness."),
    ("2 · CRIMSON AND MINT", "#962339", "#62b995", "As 1, the light one a mint."),
    ("3 · CRIMSON AND SKY", "#962339", "#419fc0", "As 1, the light one a sky blue (nearest the sink's teal of the three)."),
    ("4 · MAGENTA AND SAND", "#73348d", "#cba553", "Duplex a deep magenta, between MONITOR's violet and MIDI's plum."),
]
BW, BH = 176, 46


def band(x, y, name, c):
    o.append(f'<rect x="{x}" y="{y}" width="{BW}" height="{BH}" rx="7" fill="#000" fill-opacity=".05" stroke="{c}" stroke-width="2.4"/>')
    o.append(f'<path d="M{x},{y + 7} a7,7 0 0 1 7,-7 h{BW - 14} a7,7 0 0 1 7,7 v15 h-{BW} z" fill="{c}"/>')
    text(x + BW / 2, y + 16, name, 10.5, DARK_INK if c in LIGHT else INK, True, "middle", 'letter-spacing="1.2"')


if __name__ == "__main__":
    W, y = 1680, 150
    for title, duplex, noclass, sub in VARIANTS:
        text(60, y, title, 20, "#e58f2c", True, extra='letter-spacing="1.5"')
        text(60, y + 24, f"{sub}   DUPLEX {duplex}   NO CLASS {noclass}", 14, "#9a9da1")
        y += 44
        kinds = KINDS + [("DUPLEX", duplex), ("NO CLASS", noclass)]
        for paint in ("ALUMINIUM", "BLACK", "OLIVE"):
            base = FIN[paint][0]
            o.append(f'<rect x="60" y="{y}" width="{W - 120}" height="{BH + 24}" rx="6" fill="{base}" stroke="#000" stroke-opacity=".6"/>')
            text(70, y + BH + 18, paint, 9, FIN[paint][2], True, extra='letter-spacing="1"')
            for k, (name, c) in enumerate(kinds):
                band(76 + k * (BW + 18), y + 10, name, c)
            y += BH + 32
        y += 40
    head = f'<rect width="{W}" height="{y}" fill="#1f2023"/>'
    head += f'<text x="60" y="70" {F} font-weight="bold" font-size="40" letter-spacing="3" fill="#e58f2c">A COLOUR FOR EVERY KIND OF BLOCK</text>'
    head += f'<text x="60" y="108" {F} font-size="20" fill="#9a9da1">The six kinds\' colours as they are, and two to pick for DUPLEX and NO CLASS, on each paint.</text>'
    open(sys.argv[1] if len(sys.argv) > 1 else "kinds.svg", "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{y}" viewBox="0 0 {W} {y}">{head}{"".join(o)}</svg>\n')
