# Where ENGINE puts its blocks: by the flow, as now (each node a column further than the longest
# way to it), or by its links, as they are now, in four groups left to right: no links; only
# links out; links in and out (in as many columns as the flow takes); only links in, and
# Patchcord's listeners (its meters, the cue's in side). Each group in as many columns as keep
# the band low: from none to several.
# The same graphs side by side, NOW | BY LINKS.
#   python3 layout.py out/      writes out/<scheme>.svg, <scheme>-now.svg, <scheme>-links.svg
#   then: for f in out/*.svg; do rsvg-convert "$f" -o "${f%.svg}.png"; done
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "links"))
import links as L  # noqa: E402  (the blocks as engine.rs draws them)

NW, NH0, PR, COLW, BPAD, BHEAD, NGAP = L.NW, L.NH0, L.PR, L.COLW, L.BPAD, L.BHEAD, L.NGAP
node = L.node
# NKind's order, as engine.rs sorts a first column
KIND = {"device": 0, "app": 3, "meter": 4, "cue": 5, "system": 6}
# the view a band is fitted to: this many times wider than high
ASPECT = 2.2


def listener(n):
    """Patchcord's meters and the cue's in side: always last (they only listen)."""
    return n["kind"] == "meter" or (n["kind"] == "cue" and n["title"].endswith("in"))


# ---------------- the graphs ----------------

SCHEMES = {k: L.SCHEMES[k] for k in ("a", "b", "c")}

# D: a studio. Apps through EasyEffects into the interface, one straight into it, the interface's
# capture recorded and streamed, a headset idle but measured, Discord on the mic.
D_ST = ["output_FL", "output_FR"]
SCHEMES["d"] = dict(
    name="STUDIO: apps through EasyEffects, one straight in, the mic recorded twice, an idle headset",
    bands=[("DRIVER", "Scarlett 2i2", "Audio/Sink #50 · 48000 Hz · QUANT 256 · 5.3 ms"), ("IDLE", "not running", "")],
    nodes=[
        node(1, 0, 0, 0, "Firefox", "Stream/Output/Audio", "app", outs=D_ST),
        node(2, 0, 0, 0, "Spotify", "Stream/Output/Audio", "app", outs=D_ST),
        node(3, 0, 0, 0, "fooyin", "Stream/Output/Audio", "app", outs=D_ST),
        node(4, 0, 0, 0, "Easy Effects Sink", "Audio/Sink", "system", ins=["playback_FL", "playback_FR"], outs=["monitor_FL", "monitor_FR"]),
        node(5, 0, 0, 0, "Easy Effects", "Stream/Input/Audio", "app", ins=["input_FL", "input_FR"], outs=["output_FL", "output_FR"]),
        node(6, 0, 0, 0, "Scarlett 2i2", "Audio/Sink", "device", ins=["playback_FL", "playback_FR"], outs=["monitor_FL", "monitor_FR"], tag="DRIVER"),
        node(7, 0, 0, 0, "Scarlett 2i2", "Audio/Source", "device", outs=["capture_FL", "capture_FR"]),
        node(8, 0, 0, 0, "OBS", "Stream/Input/Audio", "app", ins=["input_FL", "input_FR"]),
        node(9, 0, 0, 0, "Recorder", "Stream/Input/Audio", "app", ins=["input_FL", "input_FR"]),
        node(10, 0, 0, 0, "Discord", "Stream/Input/Audio", "app", ins=["input_MONO"]),
        node(12, 0, 0, 0, "Audacity", "Stream/Input/Audio", "app", ins=["input_FL", "input_FR"]),
        node(11, 0, 0, 0, "JLab GO Work 2", "Audio/Sink", "device", ins=["playback_FL", "playback_FR"], outs=["monitor_FL", "monitor_FR"]),
        node(13, 0, 0, 0, "Patchcord cue · out", "Stream/Output/Audio", "cue", outs=D_ST, tag="PATCHCORD"),
        node(14, 0, 0, 0, "Patchcord cue · in", "Stream/Input/Audio", "cue", ins=["input_FL", "input_FR"], tag="PATCHCORD"),
        node(30, 1, 0, 0, "HDMI Output", "Audio/Sink", "device", ins=["playback_FL", "playback_FR"], outs=["monitor_FL", "monitor_FR"]),
        node(31, 1, 0, 0, "Ryzen HD Audio Controller…", "Audio/Sink", "device", ins=["playback_FL", "playback_FR"], outs=["monitor_FL", "monitor_FR"]),
        node(32, 1, 0, 0, "Ryzen HD Audio Controller…", "Audio/Source", "device", outs=["capture_FL", "capture_FR"]),
        node(33, 1, 0, 0, "Midi-Bridge", "Midi/Bridge", "device", ins=["Midi Through Port-0"], outs=["Midi Through Port-0"]),
        node(34, 1, 0, 0, "Telegram", "Stream/Output/Audio", "app", outs=D_ST),
        node(35, 1, 0, 0, "Webcam", "Video/Source", "device", outs=["capture_1"]),
        node(20, 0, 0, 0, "Patchcord signal meter", "Stream/Input/Audio", "meter", ins=["input_FL", "input_FR"], tag="PATCHCORD"),
        node(21, 0, 0, 0, "Patchcord signal meter", "Stream/Input/Audio", "meter", ins=["input_FL", "input_FR"], tag="PATCHCORD"),
        node(22, 0, 0, 0, "Patchcord signal meter", "Stream/Input/Audio", "meter", ins=["input_FL", "input_FR"], tag="PATCHCORD"),
        node(23, 0, 0, 0, "Patchcord signal meter", "Stream/Input/Audio", "meter", ins=["input_FL", "input_FR"], tag="PATCHCORD"),
        node(24, 0, 0, 0, "Patchcord signal meter", "Stream/Input/Audio", "meter", ins=["input_FL", "input_FR"], tag="PATCHCORD"),
    ],
    links=[
        (1, 0, 4, 0, 0), (1, 1, 4, 1, 0), (2, 0, 4, 0, 0), (2, 1, 4, 1, 0),
        (4, 0, 5, 0, 0), (4, 1, 5, 1, 0), (5, 0, 6, 0, 0), (5, 1, 6, 1, 0),
        (3, 0, 6, 0, 0), (3, 1, 6, 1, 0),
        (4, 0, 12, 0, 0), (4, 1, 12, 1, 0),
        (7, 0, 8, 0, 0), (7, 1, 8, 1, 0), (7, 0, 9, 0, 0), (7, 1, 9, 1, 0), (7, 0, 10, 0, 0),
        (6, 0, 20, 0, 1), (6, 1, 20, 1, 1),
        (3, 0, 14, 0, 1), (3, 1, 14, 1, 1), (13, 0, 11, 0, 0), (13, 1, 11, 1, 0),
        (34, 0, 31, 0, 0), (34, 1, 31, 1, 0), (11, 0, 21, 0, 1), (11, 1, 21, 1, 1),
        (1, 0, 22, 0, 1), (1, 1, 22, 1, 1), (3, 0, 23, 0, 1), (3, 1, 23, 1, 1), (7, 0, 24, 0, 1), (7, 1, 24, 1, 1),
    ],
)


# E: as on your screen: an app into the headphones, measured, and the headphones' monitor
# measured: the app's meter in line with the headphones' ports.
SCHEMES["e"] = dict(
    name="SCREEN: the app's meter link goes straight through the headphones' dots",
    bands=[("DRIVER", "JLab GO Work 2", "Audio/Sink #179 · 48000 Hz · QUANT 256 · 5.3 ms")],
    nodes=[
        node(71, 0, 0, 0, "ALSA plug-in [kadence]", "Stream/Output/Audio", "app", outs=D_ST),
        node(179, 0, 0, 0, "JLab GO Work 2", "Audio/Sink", "device", ins=["playback_FL", "playback_FR"], outs=["monitor_FL", "monitor_FR"], tag="DRIVER"),
        node(70, 0, 0, 0, "Patchcord signal meter", "Stream/Input/Audio", "meter", ins=["input_FL", "input_FR"], tag="PATCHCORD"),
        node(88, 0, 0, 0, "Patchcord signal meter", "Stream/Input/Audio", "meter", ins=["input_FL", "input_FR"], tag="PATCHCORD"),
    ],
    links=[(71, 0, 179, 0, 0), (71, 1, 179, 1, 0), (71, 0, 70, 0, 1), (71, 1, 70, 1, 1), (179, 0, 88, 0, 1), (179, 1, 88, 1, 1)],
)


# ---------------- the two ways ----------------

def columns_now(ns, ls):
    """As engine.rs: each node a column further than the longest way to it; listeners last."""
    by = {n["id"]: n for n in ns}
    col = {n["id"]: 0 for n in ns}
    for _ in range(len(ns)):
        for l in ls:
            if not listener(by[l[2]]) and col[l[2]] <= col[l[0]]:
                col[l[2]] = col[l[0]] + 1
    mx = max((col[i] for i in col if not listener(by[i])), default=-1)
    for i in col:
        if listener(by[i]):
            col[i] = mx + 1
    return col


def group(n, links):
    """Which of the four groups a node is in, by its links (all of them, across bands too)."""
    if listener(n):
        return 3
    i, o = any(l[2] == n["id"] for l in links), any(l[0] == n["id"] for l in links)
    return 0 if not (i or o) else 1 if not i else 2 if o else 3


# ---------------- links through other ports' dots ----------------

DOT = 6.0  # a line this close to a dot goes through it


def port_y(i):
    return NH0 + i * PR + PR / 2 + 2


def curve_pts(a, b, k=40):
    d = max(abs(b[0] - a[0]) / 2, 50)
    c = [a, (a[0] + d, a[1]), (b[0] - d, b[1]), b]
    out = []
    for j in range(k + 1):
        t = j / k
        u = 1 - t
        out.append(tuple(u**3 * c[0][q] + 3 * u * u * t * c[1][q] + 3 * u * t * t * c[2][q] + t**3 * c[3][q] for q in (0, 1)))
    return out


def seg_dist(p, a, b):
    dx, dy = b[0] - a[0], b[1] - a[1]
    t = 0 if dx == dy == 0 else max(0, min(1, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / (dx * dx + dy * dy)))
    return ((p[0] - a[0] - t * dx) ** 2 + (p[1] - a[1] - t * dy) ** 2) ** 0.5


def hits(by, pos, ls):
    """The dots links go through that aren't theirs: (link, node, out?, port)."""
    dots = [((x + (NW if out else 0), y + port_y(k)), (i, out, k)) for i, (x, y) in pos.items() for out, ps in ((False, by[i]["ins"]), (True, by[i]["outs"])) for k in range(len(ps))]
    found = []
    for l in ls:
        if l[0] not in pos or l[2] not in pos:
            continue
        a = (pos[l[0]][0] + NW, pos[l[0]][1] + port_y(l[1]))
        b = (pos[l[2]][0], pos[l[2]][1] + port_y(l[3]))
        pts = curve_pts(a, b)
        x0, x1 = min(a[0], b[0]) - DOT, max(a[0], b[0]) + DOT
        for p, who in dots:
            if who in ((l[0], True, l[1]), (l[2], False, l[3])) or not x0 <= p[0] <= x1:
                continue
            if min(seg_dist(p, q, r) for q, r in zip(pts, pts[1:])) < DOT:
                found.append((l, *who))
    return found


def unblock(by, cols, ls):
    """Columns (lists of node ids) and how far each node is moved down, so that no link goes
    through a dot not its own: neighbours in a column change places while that helps; what is
    left, a block goes down by half rows (see below)."""
    cols = [list(c) for c in cols]
    gap = {i: 0.0 for c in cols for i in c}

    def place():
        pos = {}
        for ci, c in enumerate(cols):
            y = 0.0
            for i in c:
                y += gap[i]
                pos[i] = (ci * COLW, y)
                y += L.nh(by[i]) + NGAP
        return pos
    n = len(hits(by, place(), ls))
    for _ in range(60):
        if n == 0:
            break
        better = None
        for c in cols:
            for k in range(len(c) - 1):
                c[k], c[k + 1] = c[k + 1], c[k]
                m = len(hits(by, place(), ls))
                c[k], c[k + 1] = c[k + 1], c[k]
                if m < n and (better is None or m < better[0]):
                    better = (m, c, k)
        if better is None:
            break
        n, c, k = better
        c[k], c[k + 1] = c[k + 1], c[k]
    # what is left: of the blocks that can set a line free (the one whose dot it goes through,
    # the line's two ends), the one that sets most free with the least step down, in half rows
    # (so that the columns stay in step with each other)
    step = PR / 2
    for _ in range(60):
        h = hits(by, place(), ls)
        if not h:
            break
        best = None
        for i in {x for l, owner, _, _ in h for x in (owner, l[0], l[2])}:
            for k in range(1, 13):
                gap[i] += step * k
                m = len(hits(by, place(), ls))
                gap[i] -= step * k
                if m < len(h):
                    if best is None or (m, k) < best[0]:
                        best = ((m, k), i)
                    break
        if best is None:
            break
        (_, k), i = best
        gap[i] += step * k
    return place()


def band_by_links(ns, ls, links, fix=False):
    """A band laid out by the links: (node, x, y) from its top left, width, height."""
    by = {n["id"]: n for n in ns}
    g = {n["id"]: group(n, links) for n in ns}
    # the middle: a column further than the longest way to it through the middle
    depth = {i: 0 for i in g if g[i] == 2}
    mid = [l for l in ls if l[0] in depth and l[2] in depth]
    for _ in range(len(depth)):
        for l in mid:
            if depth[l[2]] <= depth[l[0]]:
                depth[l[2]] = depth[l[0]] + 1
    h_of = lambda ids: sum(L.nh(by[i]) for i in ids) + NGAP * max(len(ids) - 1, 0)
    mids = [[i for i in depth if depth[i] == d] for d in range(max(depth.values(), default=-1) + 1)]

    def port_y(i):
        return NH0 + i * PR + PR / 2 + 2

    def by_kind(i):
        return (KIND[by[i]["kind"]], by[i]["title"], i)

    def columns(cap):
        """The columns, each group going on in the next column past `cap` tall."""
        at, cols = {}, []

        def bary(i):
            ys = [at[l[0]] + port_y(l[1]) for l in ls if l[2] == i and l[0] in at]
            return sum(ys) / len(ys) if ys else 1e6 + KIND[by[i]["kind"]]
        last = [i for i in g if g[i] == 3]
        # no links and only links out: by kind and name; then the middle by where their links
        # come in; last what only listens, Patchcord's listeners after the others
        for ids, key in (([i for i in g if g[i] == 0], by_kind), ([i for i in g if g[i] == 1], by_kind), *[(list(c), None) for c in mids], (last, "last")):
            if not ids:
                continue
            if key == "last":
                ids.sort(key=lambda i: (listener(by[i]), bary(i)) + by_kind(i))
            else:
                ids.sort(key=key or (lambda i: (bary(i),) + by_kind(i)))
            col, y = [], 0.0
            for i in ids:
                if col and y + L.nh(by[i]) > cap:
                    cols.append(col)
                    col, y = [], 0.0
                col.append((i, y))
                at[i] = y
                y += L.nh(by[i]) + NGAP
            cols.append(col)
        return cols

    def size(cols):
        h = max((y + L.nh(by[i]) for c in cols for i, y in c), default=0.0)
        return (len(cols) - 1) * COLW + NW, h

    # as tall as needs the least zoom to fit a wide view (ASPECT wide to 1 high); never lower
    # than the middle's tallest column (its depth is its own) or the tallest node
    low = max([h_of(c) for c in mids] + [L.nh(n) for n in ns] + [0.0])
    top = max(h_of([n["id"] for n in ns]), low)
    best = None
    cap = low
    while cap <= top + 1:
        cols = columns(cap)
        w, h = size(cols)
        score = max(w / ASPECT, h)
        if best is None or score < best[0] - 1e-6:
            best = (score, cols)
        cap += 10
    cols = best[1]
    place = []
    if fix:
        pos = unblock(by, [[i for i, _ in c] for c in cols], ls)
        h = max((y + L.nh(by[i]) for i, (x, y) in pos.items()), default=0.0)
        return [(i, x, y) for i, (x, y) in pos.items()], (len(cols) - 1) * COLW + NW if cols else 0, h
    h = 0.0
    for ci, col in enumerate(cols):
        for i, y in col:
            place.append((i, ci * COLW, y))
            h = max(h, y + L.nh(by[i]))
    return place, (len(cols) - 1) * COLW + NW if cols else 0, h


def block(ns, ls, columns):
    """One block (nodes joined by links), as engine.rs places it: its columns (empty ones
    dropped), the first sorted by kind and name, each next by where its links come in."""
    by = {n["id"]: n for n in ns}
    col = columns(ns, ls)
    used = sorted(set(col.values()))
    cols = [[i for i in col if col[i] == c] for c in used]
    at, place, w, h = {}, [], 0.0, 0.0

    def port_y(i):
        return NH0 + i * PR + PR / 2 + 2

    for ci, c in enumerate(cols):
        def bary(i):
            ys = [at[l[0]] + port_y(l[1]) for l in ls if l[2] == i and l[0] in at]
            return sum(ys) / len(ys) if ys else 1e6 + KIND[by[i]["kind"]]
        if ci == 0:
            c.sort(key=lambda i: (KIND[by[i]["kind"]], by[i]["title"]))
        else:
            c.sort(key=lambda i: (bary(i), KIND[by[i]["kind"]], by[i]["title"], i))
        y = 0.0
        for i in c:
            at[i] = y
            place.append((i, ci * COLW, y))
            y += L.nh(by[i]) + NGAP
        h = max(h, y - NGAP)
        w = ci * COLW + NW
    first = min(ns, key=lambda n: (KIND[n["kind"]], n["title"]))
    return dict(key=(KIND[first["kind"]], first["title"]), place=place, w=w, h=h)


def layout(sc, columns):
    """Each node's place and each band's rectangle: blocks stacked, then side by side."""
    pos, bands, y, width = {}, [], 70.0, 0.0
    for bi, _ in enumerate(sc["bands"]):
        ns = [n for n in sc["nodes"] if n["band"] == bi]
        ids = {n["id"] for n in ns}
        ls = [l for l in sc["links"] if l[0] in ids and l[2] in ids and l[0] != l[2]]
        if columns in ("links", "fixed"):
            place, w, h = band_by_links(ns, ls, sc["links"], columns == "fixed")
            for i, dx, dy in place:
                pos[i] = (40 + BPAD + dx, y + BHEAD + dy)
            width = max(width, 40 + BPAD + w + BPAD)
            bh = BHEAD + h + 14
            bands.append((y, bh))
            y += bh + 26
            continue
        root = {i: i for i in ids}

        def find(x):
            while root[x] != x:
                x = root[x]
            return x
        for l in ls:
            root[find(l[0])] = find(l[2])
        comps = {}
        for n in ns:
            comps.setdefault(find(n["id"]), []).append(n)
        blocks = sorted((block(c, [l for l in ls if l[0] in {n["id"] for n in c}], columns) for c in comps.values()), key=lambda b: b["key"])
        cap = max([380.0] + [b["h"] for b in blocks])
        x, cy, cw, h = 40 + BPAD, 0.0, 0.0, 0.0
        for b in blocks:
            if cy > 0 and cy + b["h"] > cap:
                x += cw + 70
                cy, cw = 0.0, 0.0
            for i, dx, dy in b["place"]:
                pos[i] = (x + dx, y + BHEAD + cy + dy)
            cy += b["h"] + NGAP * 2
            cw = max(cw, b["w"])
            h = max(h, cy - NGAP * 2)
        width = max(width, x + cw + BPAD)
        bh = BHEAD + h + 14
        bands.append((y, bh))
        y += bh + 26
    return pos, bands, width, y


def curve(a, b):
    """engine.rs's link_curve."""
    d = max(abs(b[0] - a[0]) / 2, 50)
    return f"M{a[0]:.1f},{a[1]:.1f} C{a[0]+d:.1f},{a[1]:.1f} {b[0]-d:.1f},{b[1]:.1f} {b[0]:.1f},{b[1]:.1f}"


def panel(sc, columns, title, note, dx):
    pos, bands, W, H = layout(sc, columns)
    by = {n["id"]: n for n in sc["nodes"]}
    o = [f'<g transform="translate({dx},0)">',
         f'<text x="40" y="30" {L.F} font-weight="bold" font-size="16" letter-spacing="1.5" fill="#e58f2c">{title}</text>',
         f'<text x="40" y="52" {L.F} font-size="12.5" fill="#9a9da1">{L.esc(note)}</text>']
    for bi, (y, bh) in enumerate(bands):
        kind, nm, rest = sc["bands"][bi]
        o.append(f'<rect x="40" y="{y}" width="{W-40}" height="{bh}" rx="10" fill="#fff" fill-opacity=".025" stroke="#4f86d9" stroke-opacity=".35" stroke-width="1.2"/>')
        o.append(f'<text x="{40+BPAD}" y="{y+22}" {L.M} font-weight="bold" font-size="12" fill="#c9cbcd" xml:space="preserve"><tspan fill="#e58f2c" letter-spacing="1">{kind}</tspan>  {L.esc(nm)}  <tspan fill="#7d8085" font-weight="normal">{L.esc(rest)}</tspan></text>')
    for l in sc["links"]:
        (ax, ay), (bx, by_) = pos[l[0]], pos[l[2]]
        a = (ax + NW, ay + NH0 + l[1] * PR + PR / 2 + 2)
        b = (bx, by_ + NH0 + l[3] * PR + PR / 2 + 2)
        src = by[l[0]]
        color = L.C["meter"] if src["kind"] == "meter" else L.C[src["kind"]]
        o.append(f'<path d="{curve(a, b)}" {L.stroke(l, color)}/>')
    for n in sc["nodes"]:
        x, y = pos[n["id"]]
        L.block(n, x, y, o)
    # the dots links go through that aren't theirs
    for _, i, out, k in hits(by, pos, sc["links"]):
        x, y = pos[i]
        o.append(f'<circle cx="{x + (NW if out else 0)}" cy="{y + port_y(k)}" r="9" fill="none" stroke="#ff4d4d" stroke-width="2.5"/>')
    o.append("</g>")
    return "".join(o), W + 40, H


def picture(key, sc):
    left, w1, h1 = panel(sc, columns_now, "NOW", f"{key.upper()}: {sc['name'].lower()}", 0)
    right, w2, h2 = panel(sc, "links", "BY LINKS", "no links · only out · in and out · only in, and listeners", w1)
    W, H = w1 + w2, max(h1, h2) + 10
    sep = f'<path d="M{w1+10},20 V{H-20}" stroke="#3a3c40" stroke-width="2"/>'
    return f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}"><rect width="{W}" height="{H}" fill="#1b1c1e"/>{left}{sep}{right}</svg>\n'


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "out"
    os.makedirs(out, exist_ok=True)
    for key, sc in SCHEMES.items():
        open(os.path.join(out, f"{key}.svg"), "w").write(picture(key, sc))
        # each side on its own too, to look at closely
        for name, cols, title in (("now", columns_now, "NOW"), ("links", "links", "BY LINKS"), ("fixed", "fixed", "BY LINKS, NO LINE THROUGH ANOTHER'S DOT")):
            g, W, H = panel(sc, cols, title, f"{key.upper()}: {sc['name'].lower()}", 0)
            open(os.path.join(out, f"{key}-{name}.svg"), "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H+10}" viewBox="0 0 {W} {H+10}"><rect width="{W}" height="{H+10}" fill="#1b1c1e"/>{g}</svg>\n')
