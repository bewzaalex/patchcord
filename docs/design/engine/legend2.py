# The legend: how to read the graph, as a map's legend, nothing more.
#   python3 legend2.py legend-variants.svg          on a 1280×720 window, to see its size
#   python3 legend2.py --into specimen.svg          under the four graphs of the specimen
#   python3 legend2.py --app ../../../src/ui/legend.svg   the card alone, for the app
import sys

F = 'font-family="Noto Sans"'
M = 'font-family="Noto Sans Mono"'
o = []


def t(x, y, s, size=11, fill="#c9cbcd", mono=False, bold=False, ls=0, anchor="start"):
    o.append(f'<text x="{x:.1f}" y="{y:.1f}" {M if mono else F} font-size="{size}" fill="{fill}"{" font-weight=\"bold\"" if bold else ""} text-anchor="{anchor}"{f" letter-spacing=\"{ls}\"" if ls else ""} xml:space="preserve">{s}</text>')


def head(x, y, s):
    t(x, y, s, 9.5, "#e58f2c", True, True, ls=1.2)


COLOURS = [("#4f86d9", "device"), ("#9b6bd6", "app"), ("#2fb0a6", "system"), ("#e08a3c", "MIDI"), ("#d9578f", "video"), ("#5fa36a", "Patchcord")]
LIGHTS = [("#5cff7a", "running"), ("#ffb02e", "idle"), ("#4a4c50", "asleep"), ("#ff4a32", "xruns")]
LINKS = [('stroke="#9b6bd6" stroke-width="2"', "active"), ('stroke="#9b6bd6" stroke-width="1.6" stroke-opacity=".45"', "paused"),
         ('stroke="#9b6bd6" stroke-width="1.2" stroke-dasharray="3 4"', "passive: listens only"), ('stroke="#ff4a32" stroke-width="2"', "error: no common format")]
TAGS = [("DRIVER", "clocks its group"), ("SYSTEM", "WirePlumber's own"), ("PATCHCORD", "ours: a meter, the cue"), ("HIDDEN ON DESK", "you hid it"), ("44100→48000", "resampled")]
GROUPS = [("DRIVER", "one clock and what it runs"), ("NOT RUNNING", "no clock now, no CPU"), ("INTERNAL DRIVERS", "PipeWire's own clocks")]


def colour(x, y, c, s):
    o.append(f'<rect x="{x}" y="{y-7}" width="18" height="5" rx="2.5" fill="{c}"/>')
    t(x + 26, y, s)


def light(x, y, c, s):
    o.append(f'<circle cx="{x+9}" cy="{y-4}" r="4" fill="{c}"/>')
    t(x + 26, y, s)


def line(x, y, st, s):
    o.append(f'<path d="M{x},{y-4} H{x+20}" {st}/>')
    t(x + 26, y, s)


def tag(x, y, name, s, w):
    t(x, y, name, 8.5, "#ff6a52" if "→" in name else "#e58f2c", bold=True, ls=.5)
    t(x + w, y, s)


def screen(x0, y0, title, note):
    """A 1280×720 window, the graph behind the legend only hinted."""
    o.append(f'<text x="{x0}" y="{y0-30}" {F} font-weight="bold" font-size="15" letter-spacing="1.5" fill="#e58f2c">{title}</text>')
    t(x0, y0 - 12, note, 12, "#9a9da1")
    o.append(f'<g transform="translate({x0},{y0})"><rect width="1280" height="720" fill="#1b1c1e" stroke="#3a3c40"/>')
    o.append('<rect width="1280" height="44" fill="#8c8e91"/><rect y="44" width="1280" height="30" fill="#1a1b1d"/>')
    t(14, 30, "PATCHCORD", 20, "#3a3c40", bold=True)
    for k, (bx, by, c) in enumerate([(120, 150, "#9b6bd6"), (520, 150, "#4f86d9"), (900, 150, "#5fa36a"), (120, 330, "#4f86d9"), (520, 330, "#5fa36a"), (900, 330, "#e08a3c")]):
        o.append(f'<rect x="{bx}" y="{by}" width="244" height="78" rx="6" fill="#1d1e21" stroke="{c}" stroke-width="1.5" opacity=".6"/>')
    o.append('<rect x="8" y="682" width="104" height="24" rx="4" fill="#3a3b3f" stroke="#0b0b0c"/>')
    t(36, 698, "LEGEND", 10, "#e6e7e8", bold=True, ls=.5)
    o.append('<circle cx="22" cy="694" r="7" fill="none" stroke="#9a9da1" stroke-width="1.4"/>')
    t(22, 697.5, "?", 9.5, "#e6e7e8", bold=True, anchor="middle")
    o.append("</g>")


def card(x, y, w, h):
    o.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="8" fill="#17181a" fill-opacity=".97" stroke="#000" stroke-width="1.5"/>')
    o.append(f'<path d="M{x+w-22},{y+12} l8,8 M{x+w-14},{y+12} l-8,8" stroke="#7d8085" stroke-width="1.6" stroke-linecap="round"/>')


# ---------------- A: a card over the LEGEND key ----------------


def section(x, y, s, w):
    """A section's title: grey, spaced, a rule under it (orange is for tags, as on the graph)."""
    t(x, y, s, 9, "#8a8d91", True, True, ls=1.6)
    o.append(f'<path d="M{x},{y+6} H{x+w}" stroke="#2c2d31"/>')


def table(x, y, rows, red=()):
    """Names (as the graph shows them) and their words, the words lined up after the longest name."""
    k = max(len(n) for n, _ in rows) * 6.1 + 14
    for i, (n, s) in enumerate(rows):
        t(x, y + i * 18, n, 8.5, "#ff6a52" if n in red else "#e58f2c", bold=True, ls=.5)
        t(x + k, y + i * 18, s, 10.5)


CW, CH = 640, 248


def legend_card():
    """The legend, its top left at (0, 0): CW × CH."""
    card(0, 0, CW, CH)
    t(16, 24, "HOW TO READ THE GRAPH", 11, "#e6e7e8", bold=True, ls=1)
    c1, c2, c3, top, cwid = 16, 206, 412, 54, 176
    section(c1, top, "COLOURS", cwid)
    for i, (c, s_) in enumerate(COLOURS):
        colour(c1 + (i // 3) * 88, top + 24 + (i % 3) * 18, c, s_)
    section(c1, top + 96, "LIGHTS", cwid)
    for i, (c, s_) in enumerate(LIGHTS):
        light(c1 + (i // 2) * 88, top + 120 + (i % 2) * 18, c, s_)
    t(c1, top + 172, "inputs left, outputs right", 10, "#7d8085")
    section(c2, top, "LINKS", 190)
    for i, (st, s_) in enumerate(LINKS):
        line(c2, top + 24 + i * 18, st, s_)
    t(c2, top + 100, "in the colour they come from", 10, "#7d8085")
    section(c2, top + 126, "GROUPS", 190)
    table(c2, top + 150, [("NOT RUNNING", "no clock now"), ("INTERNAL DRIVERS", "PipeWire's own")])
    section(c3, top, "TAGS", 212)
    table(c3, top + 24, TAGS, red=("44100→48000",))


if len(sys.argv) > 2 and sys.argv[1] == "--app":
    legend_card()
    open(sys.argv[2], "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{CW}" height="{CH}" viewBox="0 0 {CW} {CH}">{"".join(o)}</svg>\n')
elif len(sys.argv) > 2 and sys.argv[1] == "--into":
    # into specimen.svg, under the four graphs, in place of the legend drawn there before
    import re
    path = sys.argv[2]
    s = open(path).read()
    k = s.find('<text x="30" y="2846"')
    if k >= 0:
        s = s[:k] + "</svg>"
    X0, Y0 = 30, 2860
    o.append(f'<text x="{X0}" y="{Y0-14}" {F} font-weight="bold" font-size="14" letter-spacing="1.5" fill="#e58f2c">THE LEGEND: OPENED BY ? LEGEND (OR THE ? KEY), OVER THE GRAPH\'S BOTTOM LEFT</text><g transform="translate({X0},{Y0})">')
    legend_card()
    o.append("</g>")
    m = re.search(r'width="(\d+(?:\.\d+)?)" height="(\d+(?:\.\d+)?)" viewBox="0 0 [\d.]+ [\d.]+"', s)
    w0, h0 = m.group(1), m.group(2)
    h1 = Y0 + CH + 40
    s = s.replace(m.group(0), f'width="{w0}" height="{h1}" viewBox="0 0 {w0} {h1}"', 1)
    s = s.replace(f'<rect width="{w0}" height="{h0}" fill="#1b1c1e"/>', f'<rect width="{w0}" height="{h1}" fill="#1b1c1e"/>', 1)
    s = s[:s.rfind("</svg>")] + "".join(o) + "</svg>"
    open(path, "w").write(s)
else:
    X0, Y0 = 30, 70
    screen(X0, Y0, "THE LEGEND, IN A 1280 × 720 WINDOW", f"Over the graph's bottom left, above the LEGEND key: {CW} × {CH}.")
    o.append(f'<g transform="translate({X0 + 8},{Y0 + 426})">')
    legend_card()
    o.append("</g>")
    W, H = 1340, Y0 + 760
    open(sys.argv[1], "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}"><rect width="{W}" height="{H}" fill="#111214"/>{"".join(o)}</svg>\n')
