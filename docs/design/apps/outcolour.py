# The OUTPUT colour (a playback stream's block, an output's jacks and links) read as an error in
# ENGINE: rust sits next to the error red of a link and an xrun light. Candidates, each on a
# block in ENGINE beside an error, and on the rack's frame, with VIDEO's ochre and the olive paint
# beside them (the warm hues left are few).
#   python3 outcolour.py outcolour.svg && rsvg-convert outcolour.svg -o outcolour.png
import sys

import nodes as n
import progframes as pf
from hwframes import DEFS
from nodes import F, M, ROLE, text, o

ERR = "#ff4a32"
CANDS = [
    ("NOW · RUST", "#9f4f32", "hue 40: next to the error red"),
    ("A · MUTED TERRACOTTA", "#8c5b4a", "the same hue, less chroma: dustier, less alarm"),
    ("B · BROWN", "#7e5a42", "a warm brown, like a leather strap"),
    ("C · COPPER", "#8f5c2f", "towards orange (hue 60), still apart from VIDEO's ochre"),
]


def error_link(x, y):
    o.append(f'<path d="M{x},{y} C{x + 60},{y} {x + 60},{y + 40} {x + 120},{y + 40}" fill="none" stroke="{ERR}" stroke-width="2"/>')
    text(x + 124, y + 44, "error link", 11, "#ff6a52", font=M)
    o.append(f'<circle cx="{x + 10}" cy="{y + 70}" r="4.5" fill="{ERR}"/>')
    text(x + 22, y + 74, "xruns", 11, "#ff6a52", font=M)


if __name__ == "__main__":
    W, y = 1500, 150
    for title, c, sub in CANDS:
        text(60, y, title, 18, "#e58f2c", True, extra='letter-spacing="1.5"')
        text(60, y + 22, f"{c} · {sub}", 13.5, "#9a9da1")
        y += 40
        pf.eblock(60, y, "fooyin", 180, "Stream/Output/Audio", [c], [], [("output_FL", c), ("output_FR", c)])
        o.append(f'<path d="M390,{y + 62} C430,{y + 62} 430,{y + 90} 470,{y + 90}" fill="none" stroke="{c}" stroke-width="2"/>')
        error_link(500, y + 10)
        pf.eblock(700, y, "Video", 60, "Video/Source", [ROLE["VIDEO"]], [], [("capture_1", ROLE["VIDEO"])], w=200)
        fr = {"name": "PLAYBACK", "nid": 180, "bands": [c], "zones": [{"tag": "OUT", "jacks": [("STEREO", c)], "cols": 2, "split": True, "lvl": .6}],
              "mix": ("VOLUME", True, 1.0, False)}
        o.append(f'<rect x="940" y="{y - 10}" width="500" height="{pf.FH + 20}" rx="8" fill="{n.FIN["BLACK"][0]}"/>')
        pf.frame(950, y, fr, n.FIN["BLACK"])
        y += pf.FH + 50
    head = f'{DEFS}<rect width="{W}" height="{y}" fill="#1f2023"/>'
    head += f'<text x="60" y="70" {F} font-weight="bold" font-size="40" letter-spacing="3" fill="#e58f2c">OUTPUT, NOT AN ERROR</text>'
    head += f'<text x="60" y="108" {F} font-size="20" fill="#9a9da1">A playback stream\'s colour beside ENGINE\'s error red, VIDEO\'s ochre, and on the rack.</text>'
    open(sys.argv[1] if len(sys.argv) > 1 else "outcolour.svg", "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{y}" viewBox="0 0 {W} {y}">{head}{"".join(o)}</svg>\n')
