# How ENGINE could draw its links: ten ways, each on three graphs (30 pictures).
#   python3 links.py out/      writes out/<scheme>-<nn>-<style>.svg and sheet.svg
#   then: for f in out/*.svg; do rsvg-convert "$f" -o "${f%.svg}.png"; done
# The blocks are drawn as engine.rs draws them; only the links change.
import math
import os
import sys

F = 'font-family="Noto Sans"'
M = 'font-family="Noto Sans Mono"'
NW, NH0, PR, COLW, BPAD, BHEAD, NGAP = 244.0, 40.0, 15.0, 364.0, 26.0, 36.0, 14.0
C = {"device": "#4f86d9", "app": "#9b6bd6", "system": "#2fb0a6", "meter": "#5fa36a", "cue": "#5fd4ff"}
CUE = "#5fd4ff"


def esc(s):
    return s.replace("&", "&amp;").replace("<", "&lt;")


# ---------------- the three graphs ----------------
# a node: (id, band, column, row y in the band, title, class, kind, ins, outs, tag)
# a link: (from id, out port index, to id, in port index, passive)

def node(i, band, col, y, title, cls, kind, ins=(), outs=(), tag="", lit=()):
    return dict(id=i, band=band, col=col, ry=y, title=title, cls=cls, kind=kind, ins=list(ins), outs=list(outs), tag=tag, lit=set(lit))


def nh(n):
    return NH0 + max(len(n["ins"]), len(n["outs"]), 1) * PR + 8


SCHEMES = {}

# A: simple. Two apps into the headphones; each measured, and the headphones' monitor too.
SCHEMES["a"] = dict(
    name="SIMPLE: two apps into the headphones, three meters",
    bands=[("DRIVER", "JLab GO Work 2", "Audio/Sink #155 · 48000 Hz · QUANT 512 · 10.7 ms")],
    nodes=[
        node(1, 0, 0, 0, "Firefox", "Stream/Output/Audio", "app", outs=["output_FL", "output_FR"]),
        node(2, 0, 0, 77, "fooyin", "Stream/Output/Audio", "app", outs=["output_FL", "output_FR"]),
        node(3, 0, 1, 0, "JLab GO Work 2", "Audio/Sink", "device", ins=["playback_FL", "playback_FR"], outs=["monitor_FL", "monitor_FR"], tag="DRIVER"),
        node(4, 0, 2, 0, "Patchcord signal meter", "Stream/Input/Audio", "meter", ins=["input_FL", "input_FR"], tag="PATCHCORD"),
        node(5, 0, 2, 77, "Patchcord signal meter", "Stream/Input/Audio", "meter", ins=["input_FL", "input_FR"], tag="PATCHCORD"),
        node(6, 0, 2, 154, "Patchcord signal meter", "Stream/Input/Audio", "meter", ins=["input_FL", "input_FR"], tag="PATCHCORD"),
    ],
    links=[(1, 0, 3, 0, 0), (1, 1, 3, 1, 0), (2, 0, 3, 0, 0), (2, 1, 3, 1, 0),
           (3, 0, 4, 0, 1), (3, 1, 4, 1, 1), (1, 0, 5, 0, 1), (1, 1, 5, 1, 1), (2, 0, 6, 0, 1), (2, 1, 6, 1, 1)],
)

# B: busy, as on your screen: five sources, three things they play into, seven meters and the cue.
B_MON = ["input_MONO"]
SCHEMES["b"] = dict(
    name="BUSY: as on your screen (five sources, seven meters, the cue)",
    bands=[("DRIVER", "Ryzen HD Audio Controller Analog Stereo", "Audio/Source #51 · 48000 Hz · QUANT 512 · 10.7 ms")],
    nodes=[
        node(10, 0, 0, 0, "JLab GO Work 2", "Audio/Source", "device", outs=["capture_MONO"], lit=["o0"]),
        node(11, 0, 0, 78, "Ryzen HD Audio Controller…", "Audio/Source", "device", outs=["capture_FL", "capture_FR"], tag="DRIVER", lit=["o0", "o1"]),
        node(12, 0, 0, 171, "ALSA plug-in [soundcraft]", "Stream/Output/Audio", "app", outs=["output_FL", "output_FR"], lit=["o0", "o1"]),
        node(13, 0, 0, 264, "fooyin", "Stream/Output/Audio", "app", outs=["output_FL", "output_FR"], lit=["o0", "o1"]),
        node(14, 0, 0, 357, "Patchcord cue · out", "Stream/Output/Audio", "cue", outs=["output_FL", "output_FR"], tag="PATCHCORD"),
        node(20, 0, 1, 0, "ALSA plug-in [soundcraft]", "Stream/Input/Audio", "app", ins=["input_MONO"], outs=["monitor_MONO"]),
        node(21, 0, 1, 78, "Ryzen HD Audio Controller…", "Audio/Sink", "device", ins=["playback_FL", "playback_FR"], outs=["monitor_FL", "monitor_FR"], lit=["o0", "o1"]),
        node(22, 0, 1, 171, "JLab GO Work 2", "Audio/Sink", "device", ins=["playback_FL", "playback_FR"], outs=["monitor_FL", "monitor_FR"]),
        node(30, 0, 2, 0, "Patchcord signal meter", "Stream/Input/Audio", "meter", ins=B_MON, tag="PATCHCORD"),
        node(31, 0, 2, 78, "Patchcord signal meter", "Stream/Input/Audio", "meter", ins=B_MON, tag="PATCHCORD"),
        node(32, 0, 2, 156, "Patchcord cue · in", "Stream/Input/Audio", "cue", ins=["input_FL", "input_FR"], tag="PATCHCORD"),
        node(33, 0, 2, 249, "Patchcord signal meter", "Stream/Input/Audio", "meter", ins=["input_FL", "input_FR"], tag="PATCHCORD"),
        node(34, 0, 2, 342, "Patchcord signal meter", "Stream/Input/Audio", "meter", ins=["input_FL", "input_FR"], tag="PATCHCORD"),
        node(35, 0, 2, 435, "Patchcord signal meter", "Stream/Input/Audio", "meter", ins=["input_FL", "input_FR"], tag="PATCHCORD"),
        node(36, 0, 2, 528, "Patchcord signal meter", "Stream/Input/Audio", "meter", ins=["input_FL", "input_FR"], tag="PATCHCORD"),
        node(37, 0, 2, 621, "Patchcord signal meter", "Stream/Input/Audio", "meter", ins=["input_FL", "input_FR"], tag="PATCHCORD"),
    ],
    links=[
        # what plays
        (10, 0, 20, 0, 0), (11, 0, 20, 0, 0), (11, 1, 20, 0, 0),
        (12, 0, 21, 0, 0), (12, 1, 21, 1, 0), (12, 0, 22, 0, 0), (12, 1, 22, 1, 0),
        (13, 0, 21, 0, 0), (13, 1, 21, 1, 0), (13, 0, 22, 0, 0), (13, 1, 22, 1, 0),
        (14, 0, 22, 0, 0), (14, 1, 22, 1, 0),
        # what listens
        (20, 0, 30, 0, 1), (10, 0, 31, 0, 1),
        (10, 0, 32, 0, 1), (10, 0, 32, 1, 1), (11, 0, 32, 0, 1), (11, 1, 32, 1, 1), (12, 0, 32, 0, 1), (12, 1, 32, 1, 1),
        (13, 0, 32, 0, 1), (13, 1, 32, 1, 1), (21, 0, 32, 0, 1), (21, 1, 32, 1, 1),
        (21, 0, 33, 0, 1), (21, 1, 33, 1, 1), (22, 0, 34, 0, 1), (22, 1, 34, 1, 1),
        (11, 0, 35, 0, 1), (11, 1, 35, 1, 1), (12, 0, 36, 0, 1), (12, 1, 36, 1, 1), (13, 0, 37, 0, 1), (13, 1, 37, 1, 1),
    ],
)

# C: across: two clocks; a link back (a recorder takes the headphones' monitor), links from one
# band to the other.
SCHEMES["c"] = dict(
    name="ACROSS: two clocks, a link back, links between the bands",
    bands=[("DRIVER", "JLab GO Work 2", "Audio/Sink #155 · 48000 Hz · QUANT 512 · 10.7 ms"),
           ("DRIVER", "RØDE NT-USB", "Audio/Source #90 · 48000 Hz · QUANT 1024 · 21.3 ms")],
    nodes=[
        node(40, 0, 0, 0, "fooyin", "Stream/Output/Audio", "app", outs=["output_FL", "output_FR"]),
        node(41, 0, 0, 93, "Recorder", "Stream/Input/Audio", "app", ins=["input_FL", "input_FR"]),
        node(42, 0, 1, 0, "JLab GO Work 2", "Audio/Sink", "device", ins=["playback_FL", "playback_FR"], outs=["monitor_FL", "monitor_FR"], tag="DRIVER"),
        node(43, 0, 2, 0, "Patchcord signal meter", "Stream/Input/Audio", "meter", ins=["input_FL", "input_FR"], tag="PATCHCORD"),
        node(44, 0, 2, 93, "Patchcord signal meter", "Stream/Input/Audio", "meter", ins=["input_MONO"], tag="PATCHCORD"),
        node(50, 1, 0, 0, "RØDE NT-USB", "Audio/Source", "device", outs=["capture_MONO"], tag="DRIVER"),
        node(51, 1, 1, 0, "OBS", "Stream/Input/Audio", "app", ins=["input_MONO"], outs=["monitor_MONO"]),
        node(52, 1, 1, 78, "Ryzen HD Audio Controller…", "Audio/Sink", "device", ins=["playback_FL", "playback_FR"], outs=["monitor_FL", "monitor_FR"]),
        node(53, 1, 2, 0, "Patchcord signal meter", "Stream/Input/Audio", "meter", ins=["input_MONO"], tag="PATCHCORD"),
    ],
    links=[
        (40, 0, 42, 0, 0), (40, 1, 42, 1, 0),
        (42, 0, 41, 0, 0), (42, 1, 41, 1, 0),          # back: the recorder takes the monitor
        (42, 0, 43, 0, 1), (42, 1, 43, 1, 1),
        (50, 0, 51, 0, 0), (51, 0, 53, 0, 1),
        (40, 0, 52, 0, 0), (40, 1, 52, 1, 0),          # down: fooyin into the other band
        (50, 0, 44, 0, 1),                             # up: the mic measured in the band above
    ],
)


def layout(sc):
    """Each node's place, each band's rectangle."""
    pos, bands, y = {}, [], 70.0
    for bi, _ in enumerate(sc["bands"]):
        ns = [n for n in sc["nodes"] if n["band"] == bi]
        h = max(n["ry"] + nh(n) for n in ns)
        for n in ns:
            pos[n["id"]] = (40 + BPAD + n["col"] * COLW, y + BHEAD + n["ry"])
        bh = BHEAD + h + 16
        bands.append((y, bh))
        y += bh + 26
    return pos, bands, y


def port(sc, pos, nid, out, i):
    n = next(n for n in sc["nodes"] if n["id"] == nid)
    x, y = pos[nid]
    return (x + (NW if out else 0), y + NH0 + i * PR + PR / 2 + 2)


# ---------------- drawing the blocks (as engine.rs) ----------------

def block(n, x, y, o, taps=None, notes=None):
    c = C[n["kind"]]
    h = nh(n)
    s = [f'<g transform="translate({x},{y})">',
         f'<path d="M0,6 A6,6 0 0 1 6,0 H{NW-6} A6,6 0 0 1 {NW},6 V12 H0 Z" fill="{c}"/><rect x=".75" y="4" width="{NW-1.5}" height="{h-4.75}" rx="5.25" fill="#1d1e21"/>',
         f'<rect width="{NW}" height="{h}" rx="6" fill="none" stroke="{c}" stroke-width="1.5"/><circle cx="12" cy="17" r="4" fill="#5cff7a"/>',
         f'<text x="22" y="21" {F} font-size="12" font-weight="bold" fill="#e6e7e8">{esc(n["title"])}</text>',
         f'<text x="12" y="34" {M} font-size="10" fill="#7d8085">{n["id"]} · {n["cls"]}</text>']
    if n["tag"]:
        s.append(f'<text x="{NW-10}" y="34" text-anchor="end" {F} font-size="8.5" font-weight="bold" letter-spacing=".5" fill="#e58f2c">{n["tag"]}</text>')
    for out, ports in ((False, n["ins"]), (True, n["outs"])):
        for i, name in enumerate(ports):
            py = NH0 + i * PR + PR / 2 + 2
            w = len(name) * 6.02 + 20
            lit = f'{"o" if out else "i"}{i}' in n["lit"]
            if out:
                px, tx, anc = NW, NW - 10, ' text-anchor="end"'
                d = f"M{NW-.75},{py-6.5} H{NW-w+6.5} A6.5,6.5 0 0 0 {NW-w+6.5},{py+6.5} H{NW-.75} Z"
            else:
                px, tx, anc = 0, 10, ""
                d = f"M.75,{py-6.5} H{w-6.5} A6.5,6.5 0 0 1 {w-6.5},{py+6.5} H.75 Z"
            if lit:
                s.append(f'<path d="{d}" fill="{CUE}"/><circle cx="{px}" cy="{py}" r="4" fill="{CUE}" stroke="#0f1011" stroke-width="1.5"/><text x="{tx}" y="{py+3.5}"{anc} {M} font-size="10" font-weight="bold" fill="#062533">{name}</text>')
            else:
                s.append(f'<path d="{d}" fill="{c}" fill-opacity=".1"/><circle cx="{px}" cy="{py}" r="4" fill="{c}" stroke="#0f1011" stroke-width="1.5"/><text x="{tx}" y="{py+3.5}"{anc} {M} font-size="10" fill="#a8abae">{name}</text>')
            key = (n["id"], out, i)
            if taps and key in taps:
                # a tap: little meter bars beside the port, how many listen
                k = taps[key]
                bx = px + 9 if out else px - 22
                s.append(f'<g transform="translate({bx},{py-5})"><rect x="0" y="4" width="2.4" height="6" fill="{C["meter"]}"/><rect x="3.4" y="1" width="2.4" height="9" fill="{C["meter"]}"/><rect x="6.8" y="6" width="2.4" height="4" fill="{C["meter"]}"/></g>')
                if k > 1:
                    s.append(f'<text x="{bx+12}" y="{py+3.5}" {M} font-size="9" fill="{C["meter"]}">{k}</text>')
            if notes and key in notes:
                s.append(f'<text x="{px-8}" y="{py+3.5}" text-anchor="end" {M} font-size="9" fill="#7d8085">{esc(notes[key])} ▸</text>')
    s.append("</g>")
    o.append("".join(s))


# ---------------- geometry helpers ----------------

def cubic_pt(c, t):
    u = 1 - t
    return (u**3 * c[0][0] + 3 * u * u * t * c[1][0] + 3 * u * t * t * c[2][0] + t**3 * c[3][0],
            u**3 * c[0][1] + 3 * u * u * t * c[1][1] + 3 * u * t * t * c[2][1] + t**3 * c[3][1])


def plain(a, b):
    d = max(abs(b[0] - a[0]) / 2, 50)
    return [a, (a[0] + d, a[1]), (b[0] - d, b[1]), b]


def d_of(cubics):
    if not cubics:
        return ""
    s = f"M{cubics[0][0][0]:.1f},{cubics[0][0][1]:.1f}"
    for c in cubics:
        s += f" C{c[1][0]:.1f},{c[1][1]:.1f} {c[2][0]:.1f},{c[2][1]:.1f} {c[3][0]:.1f},{c[3][1]:.1f}"
    return s


def rounded_d(pts, r=10):
    """A polyline with rounded corners, as an SVG path."""
    s = f"M{pts[0][0]:.1f},{pts[0][1]:.1f}"
    for i in range(1, len(pts) - 1):
        p, c, n = pts[i - 1], pts[i], pts[i + 1]
        din, dout = math.dist(p, c), math.dist(c, n)
        if din < .01 or dout < .01:
            continue
        rr = min(r, din / 2, dout / 2)
        a = (c[0] + (p[0] - c[0]) * rr / din, c[1] + (p[1] - c[1]) * rr / din)
        b = (c[0] + (n[0] - c[0]) * rr / dout, c[1] + (n[1] - c[1]) * rr / dout)
        s += f" L{a[0]:.1f},{a[1]:.1f} Q{c[0]:.1f},{c[1]:.1f} {b[0]:.1f},{b[1]:.1f}"
    s += f" L{pts[-1][0]:.1f},{pts[-1][1]:.1f}"
    return s


class Ctx:
    """A graph laid out: blocks by column, for routing round them."""

    def __init__(self, sc):
        self.sc = sc
        self.pos, self.bands, self.h = layout(sc)
        self.rects = []
        for n in sc["nodes"]:
            x, y = self.pos[n["id"]]
            self.rects.append((x, y, x + NW, y + nh(n), n["band"]))
        self.byid = {n["id"]: n for n in sc["nodes"]}
        self.lane = {}

    def band_of(self, nid):
        return self.byid[nid]["band"]

    def band_rect(self, bi):
        y, bh = self.bands[bi]
        return y, y + bh

    def blocks_between(self, a, b, band):
        """The blocks of the band strictly between the two ends' columns, by column x."""
        cols = {}
        for (x0, y0, x1, y1, bi) in self.rects:
            if bi == band and x0 > a[0] + 1 and x1 < b[0] - 1:
                cols.setdefault(x0, []).append((y0, y1))
        return sorted(cols.items())

    def cuts(self, c, x, rects):
        for k in range(49):
            px, py = cubic_pt(c, k / 48)
            if x - 4 < px < x + NW + 4 and any(t - 4 < py < b + 4 for t, b in rects):
                return True
        return False

    def free_y(self, rects, y, floor):
        def clear(f, m):
            return not any(f > t - m and f < b + m for t, b in rects)
        if clear(y, 5):
            return y
        cands = [f for t, b in rects for f in (t - NGAP / 2, b + NGAP / 2)]
        cands = [f for f in cands if f >= floor and clear(f, 2)]
        return min(cands, key=lambda f: abs(f - y)) if cands else y

    def waypoints(self, a, b, band):
        """Points either side of each column the plain curve would cut, at a free height near the
        straight way (the current routing)."""
        pts, frm, passed = [a], a, []
        for x, rects in self.blocks_between(a, b, band):
            if not self.cuts(plain(frm, b), x, rects):
                continue
            mid = x + NW / 2
            along = frm[1] + (b[1] - frm[1]) * min(1, max(0, (mid - frm[0]) / max(1, b[0] - frm[0])))
            wy = self.free_y(rects, along, self.band_rect(band)[0] + 28)
            pts += [(x - 12, wy), (x + NW + 12, wy)]
            passed.append((x, rects))
            frm = (x + NW + 12, wy)
        pts.append(b)
        return pts, passed

    def current(self, a, b, band):
        pts, passed = self.waypoints(a, b, band)
        n = len(pts)
        if n == 2:
            return [plain(a, b)]
        slope = [0.0] * n
        for i in range(1, n - 1):
            slope[i] = (pts[i + 1][1] - pts[i - 1][1]) / max(1, pts[i + 1][0] - pts[i - 1][0])

        def piece(i):
            p, q = pts[i], pts[i + 1]
            end = i == 0 or i + 2 == n
            h = max((q[0] - p[0]) * (0.45 if end else 1 / 3), 30 if end else 1)
            return [p, (p[0] + h, p[1] + slope[i] * h), (q[0] - h, q[1] - slope[i + 1] * h), q]
        for k, (x, rects) in enumerate(passed):
            i = 1 + 2 * k
            if self.cuts(piece(i), x, rects):
                slope[i] = slope[i + 1] = 0
        return [piece(i) for i in range(n - 1)]

    def lane_x(self, key, x0, x1):
        """A vertical lane of its own in the gap between x0 and x1."""
        k = self.lane.setdefault((round(x0), round(x1)), {})
        if key not in k:
            k[key] = len(k)
        i = k[key]
        mid = (x0 + x1) / 2
        return mid + (i - 0) * 7 - 7 * 3


# ---------------- the ten ways ----------------

def link_color(ctx, l):
    return C[ctx.byid[l[0]]["kind"]] if ctx.byid[l[0]]["kind"] != "meter" else C["meter"]


def ends(ctx, l):
    a = port(ctx.sc, ctx.pos, l[0], True, l[1])
    b = port(ctx.sc, ctx.pos, l[2], False, l[3])
    return a, b


def stroke(l, color, w_active=2.0, w_passive=1.2, op=1.0):
    if l[4]:
        return f'fill="none" stroke="{color}" stroke-width="{w_passive}" stroke-dasharray="3 4" opacity="{op}"'
    return f'fill="none" stroke="{color}" stroke-width="{w_active}" opacity="{op}"'


def route_any(ctx, l, a, b):
    """Back or across bands: out to the side, along the gap between the bands, in from the side."""
    ba, bb = ctx.band_of(l[0]), ctx.band_of(l[2])
    if ba == bb:
        gy = ctx.band_rect(ba)[1] - 7
    elif b[1] < a[1]:
        gy = ctx.band_rect(bb)[1] + 13
    else:
        gy = ctx.band_rect(bb)[0] - 13
    # a stereo pair side by side, not one over the other
    k = 5 * l[3]
    xa, xb = a[0] + 18 + k, b[0] - 14 - k
    return [a, (xa, a[1]), (xa, gy + k), (xb, gy + k), (xb, b[1]), b]


def special(ctx, l, a, b):
    return ctx.band_of(l[0]) != ctx.band_of(l[2]) or b[0] < a[0] + 40


def s_current(ctx, o):
    for l in ctx.sc["links"]:
        a, b = ends(ctx, l)
        col = link_color(ctx, l)
        if special(ctx, l, a, b):
            o.append(f'<path d="{rounded_d(route_any(ctx, l, a, b))}" {stroke(l, col)}/>')
        else:
            o.append(f'<path d="{d_of(ctx.current(a, b, ctx.band_of(l[0])))}" {stroke(l, col)}/>')


def s_bezier(ctx, o):
    for l in ctx.sc["links"]:
        a, b = ends(ctx, l)
        if b[0] < a[0] + 40:
            # back: a loop out and round
            d = max(120, abs(b[1] - a[1]))
            c = [a, (a[0] + d, a[1]), (b[0] - d, b[1]), b]
        else:
            c = plain(a, b)
        o.append(f'<path d="{d_of([c])}" {stroke(l, link_color(ctx, l))}/>')


def ortho_pts(ctx, l, a, b):
    """Right angles: out level, down a lane of its own in the gap, level in; past a column in a
    free gap along it."""
    if special(ctx, l, a, b):
        return route_any(ctx, l, a, b)
    band = ctx.band_of(l[0])
    pts, _ = ctx.waypoints(a, b, band)
    out = [a]
    frm = a
    key = (l[0], l[1], l[2])
    for i in range(1, len(pts) - 1, 2):
        l_, r_ = pts[i], pts[i + 1]
        lx = ctx.lane_x(key, frm[0], l_[0] + 12)
        out += [(lx, frm[1]), (lx, l_[1]), r_]
        frm = r_
    lx = ctx.lane_x(key, frm[0], b[0])
    out += [(lx, frm[1]), (lx, b[1]), b]
    return out


def s_ortho(ctx, o):
    for l in ctx.sc["links"]:
        a, b = ends(ctx, l)
        o.append(f'<path d="{rounded_d(ortho_pts(ctx, l, a, b), 9)}" {stroke(l, link_color(ctx, l))}/>')


def s_metro(ctx, o):
    """Level, 45°, level: as a metro map; the turns softened."""
    for l in ctx.sc["links"]:
        a, b = ends(ctx, l)
        pts = ortho_pts(ctx, l, a, b)
        # each vertical piece made a diagonal as wide as it is tall (as far as the room allows)
        mp = [pts[0]]
        for i in range(1, len(pts) - 1):
            p, c, n = pts[i - 1], pts[i], pts[i + 1]
            if abs(c[0] - n[0]) < .5 and abs(c[1] - n[1]) > .5:
                dy = abs(n[1] - c[1])
                room = min(abs(c[0] - p[0]), abs(pts[i + 2][0] - n[0]) if i + 2 < len(pts) else 1e9) - 6
                w = min(dy, max(room, 0) * 2) / 2
                mp += [(c[0] - w, c[1]), (n[0] + w, n[1])]
            elif abs(p[0] - c[0]) < .5 and abs(p[1] - c[1]) > .5:
                continue
            else:
                mp.append(c)
        mp.append(pts[-1])
        o.append(f'<path d="{rounded_d(mp, 6)}" {stroke(l, link_color(ctx, l))} stroke-linejoin="round"/>')


def s_ribbon(ctx, o):
    """A stereo pair between the same two blocks: one twin cable, fanned out at its ends."""
    links = ctx.sc["links"]
    done = set()
    for l in links:
        if id(l) in done:
            continue
        pair = next((m for m in links if m is not l and m[0] == l[0] and m[2] == l[2] and m[1] == l[1] + 1 and m[3] == l[3] + 1 and m[4] == l[4]), None)
        col = link_color(ctx, l)
        if pair is None:
            a, b = ends(ctx, l)
            cs = [route_any(ctx, l, a, b)] if special(ctx, l, a, b) else None
            d = rounded_d(cs[0]) if cs else d_of(ctx.current(a, b, ctx.band_of(l[0])))
            o.append(f'<path d="{d}" {stroke(l, col)}/>')
            continue
        done.add(id(pair))
        (a1, b1), (a2, b2) = ends(ctx, l), ends(ctx, pair)
        a, b = ((a1[0] + 14, (a1[1] + a2[1]) / 2), (b1[0] - 14, (b1[1] + b2[1]) / 2))
        for (p, q) in ((a1, a), (a2, a)):
            o.append(f'<path d="M{p[0]},{p[1]} C{p[0]+8},{p[1]} {q[0]-6},{q[1]} {q[0]},{q[1]}" fill="none" stroke="{col}" stroke-width="1.6" opacity="{.6 if l[4] else 1}"/>')
        for (p, q) in ((b, b1), (b, b2)):
            o.append(f'<path d="M{p[0]},{p[1]} C{p[0]+6},{p[1]} {q[0]-8},{q[1]} {q[0]},{q[1]}" fill="none" stroke="{col}" stroke-width="1.6" opacity="{.6 if l[4] else 1}"/>')
        if special(ctx, l, a, b):
            d = rounded_d(route_any(ctx, l, a, b))
        else:
            d = d_of(ctx.current(a, b, ctx.band_of(l[0])))
        if l[4]:
            o.append(f'<path d="{d}" fill="none" stroke="{col}" stroke-width="4" stroke-dasharray="4 4" opacity=".55"/>')
        else:
            o.append(f'<path d="{d}" fill="none" stroke="{col}" stroke-width="5.5"/><path d="{d}" fill="none" stroke="#1d1e21" stroke-width="1.4"/>')


def s_bus(ctx, o):
    """One trunk per output port; it runs to the gap before its first target column, a bus goes
    up and down there, and branches into each input, a dot where they part."""
    by_port = {}
    for l in ctx.sc["links"]:
        by_port.setdefault((l[0], l[1]), []).append(l)
    for (nid, pi), ls in by_port.items():
        a = port(ctx.sc, ctx.pos, nid, True, pi)
        col = link_color(ctx, ls[0])
        normal = [l for l in ls if not special(ctx, l, a, ends(ctx, l)[1])]
        for l in ls:
            if l not in normal:
                o.append(f'<path d="{rounded_d(route_any(ctx, l, a, ends(ctx, l)[1]))}" {stroke(l, col)}/>')
        for passive in (0, 1):
            grp = [l for l in normal if l[4] == passive]
            if not grp:
                continue
            tgts = [ends(ctx, l)[1] for l in grp]
            # the trunk past what lies between (as the current routing finds it)
            near = min(tgts, key=lambda t: t[0])
            pts, _ = ctx.waypoints(a, (near[0], a[1]), ctx.band_of(nid))
            bx = ctx.lane_x((nid, pi, passive), max(p[0] for p in pts[:-1]) if len(pts) > 2 else a[0], near[0])
            trunk_y = pts[-2][1] if len(pts) > 2 else a[1]
            ys = [trunk_y] + [t[1] for t in tgts]
            st = stroke(grp[0], col)
            tp = [a] + pts[1:-1] + [(bx, trunk_y)]
            o.append(f'<path d="{rounded_d(tp, 8)}" {st}/>')
            o.append(f'<path d="M{bx},{min(ys)} V{max(ys)}" {st}/>')
            for t in tgts:
                o.append(f'<path d="M{bx},{t[1]} H{t[0]}" {st}/>')
                if t[1] not in (min(ys), max(ys)) or len(tgts) > 1:
                    o.append(f'<circle cx="{bx}" cy="{t[1]}" r="2.6" fill="{col}"/>')


def s_taps(ctx, o):
    """What only listens (meters, the cue's in side) drawn as taps on the ports, not as links:
    bars and a count by the port it listens to; by the meter's input, whose it is."""
    for l in ctx.sc["links"]:
        if l[4]:
            continue
        a, b = ends(ctx, l)
        if special(ctx, l, a, b):
            o.append(f'<path d="{rounded_d(route_any(ctx, l, a, b))}" {stroke(l, link_color(ctx, l))}/>')
        else:
            o.append(f'<path d="{d_of(ctx.current(a, b, ctx.band_of(l[0])))}" {stroke(l, link_color(ctx, l))}/>')


def taps_of(ctx):
    taps, notes = {}, {}
    for l in ctx.sc["links"]:
        if not l[4]:
            continue
        taps[(l[0], True, l[1])] = taps.get((l[0], True, l[1]), 0) + 1
        src = ctx.byid[l[0]]
        nm = src["title"].split(" [")[0].split(" HD")[0][:14]
        k = (l[2], False, l[3])
        notes[k] = nm if k not in notes else notes[k] + " +"
    return taps, notes


def s_cables(ctx, o):
    """Cords as on the desk: they sag under their weight, a darker edge and a shadow."""
    for l in ctx.sc["links"]:
        a, b = ends(ctx, l)
        col = link_color(ctx, l)
        dist = math.dist(a, b)
        sag = 30 + dist * 0.18
        dx = max(abs(b[0] - a[0]) * 0.3, 60)
        c = [a, (a[0] + dx, a[1] + sag), (b[0] - dx, b[1] + sag), b]
        d = d_of([c])
        w = 2.2 if l[4] else 3.4
        op = .55 if l[4] else 1
        o.append(f'<path d="{d}" transform="translate(2,4)" fill="none" stroke="#000" stroke-opacity=".35" stroke-width="{w+1}"/>')
        o.append(f'<path d="{d}" fill="none" stroke="#0b0b0c" stroke-width="{w+1.4}" opacity="{op}"/><path d="{d}" fill="none" stroke="{col}" stroke-width="{w}" opacity="{op}"/>')


def s_straight(ctx, o):
    """Straight: a short level stub at each port, a straight line between, an arrow halfway."""
    for l in ctx.sc["links"]:
        a, b = ends(ctx, l)
        col = link_color(ctx, l)
        if special(ctx, l, a, b):
            pts = route_any(ctx, l, a, b)
            o.append(f'<path d="{rounded_d(pts, 0.1)}" {stroke(l, col)}/>')
            continue
        p, q = (a[0] + 14, a[1]), (b[0] - 14, b[1])
        o.append(f'<path d="M{a[0]},{a[1]} L{p[0]},{p[1]} L{q[0]},{q[1]} L{b[0]},{b[1]}" {stroke(l, col)}/>')
        m = ((p[0] + q[0]) / 2, (p[1] + q[1]) / 2)
        ang = math.degrees(math.atan2(q[1] - p[1], q[0] - p[0]))
        o.append(f'<path d="M-5,-3.5 L3,0 L-5,3.5 Z" transform="translate({m[0]:.1f},{m[1]:.1f}) rotate({ang:.1f})" fill="{col}" opacity="{.6 if l[4] else 1}"/>')


def s_hierarchy(ctx, o):
    """What plays bold, what only listens a hairline, faint: the eye goes to the sound."""
    for passive in (1, 0):
        for l in ctx.sc["links"]:
            if l[4] != passive:
                continue
            a, b = ends(ctx, l)
            col = link_color(ctx, l)
            d = rounded_d(route_any(ctx, l, a, b)) if special(ctx, l, a, b) else d_of(ctx.current(a, b, ctx.band_of(l[0])))
            if passive:
                o.append(f'<path d="{d}" fill="none" stroke="{col}" stroke-width=".9" opacity=".3"/>')
            else:
                o.append(f'<path d="{d}" fill="none" stroke="#0b0b0c" stroke-width="4.6"/><path d="{d}" fill="none" stroke="{col}" stroke-width="2.6"/>')


STYLES = [
    ("current", "CURRENT", "As now: a curve, past the columns in between through a gap, one smooth line."),
    ("bezier", "PLAIN CURVES", "Every link one curve from port to port, over whatever is between."),
    ("ortho", "RIGHT ANGLES", "Level, down a lane of its own in the gap, level: as a circuit diagram, the corners rounded."),
    ("metro", "METRO", "Level, 45°, level, as a metro map: lines run side by side, turns softened."),
    ("ribbon", "STEREO RIBBONS", "A stereo pair between the same two blocks is one twin cable, fanned out at its ends."),
    ("bus", "BUS", "One trunk per output; a bus in the gap before the targets, a branch into each, a dot where they part."),
    ("taps", "TAPS", "What only listens (meters, the cue) is no line: bars and a count by the port; the meter says whose it is."),
    ("cables", "CABLES", "Cords as on the desk: they sag under their weight, with an edge and a shadow."),
    ("straight", "STRAIGHT", "A short stub at each port, a straight line between, an arrow halfway: the direction at a glance."),
    ("hierarchy", "HIERARCHY", "What plays bold, what only listens a faint hairline: the eye goes to the sound."),
]
DRAW = {"current": s_current, "bezier": s_bezier, "ortho": s_ortho, "metro": s_metro, "ribbon": s_ribbon, "bus": s_bus, "taps": s_taps, "cables": s_cables, "straight": s_straight, "hierarchy": s_hierarchy}


def picture(key, sc, si):
    style, sname, note = STYLES[si]
    ctx = Ctx(sc)
    W = 40 + BPAD * 2 + 2 * COLW + NW + 40
    H = ctx.h + 10
    o = []
    o.append(f'<text x="40" y="30" {F} font-weight="bold" font-size="16" letter-spacing="1.5" fill="#e58f2c">{si+1} · {sname}</text>')
    o.append(f'<text x="40" y="52" {F} font-size="12.5" fill="#9a9da1">{esc(note)}  ·  {key.upper()}: {esc(sc["name"].lower())}</text>')
    for bi, (y, bh) in enumerate(ctx.bands):
        kind, nm, rest = sc["bands"][bi]
        o.append(f'<rect x="40" y="{y}" width="{W-80}" height="{bh}" rx="10" fill="#fff" fill-opacity=".025" stroke="#4f86d9" stroke-opacity=".35" stroke-width="1.2"/>')
        o.append(f'<text x="{40+BPAD}" y="{y+22}" {M} font-weight="bold" font-size="12" fill="#c9cbcd" xml:space="preserve"><tspan fill="#e58f2c" letter-spacing="1">{kind}</tspan>  {esc(nm)}  <tspan fill="#7d8085" font-weight="normal">{esc(rest)}</tspan></text>')
    links = []
    DRAW[style](ctx, links)
    taps, notes = taps_of(ctx) if style == "taps" else (None, None)
    o += links
    for n in sc["nodes"]:
        x, y = ctx.pos[n["id"]]
        block(n, x, y, o, taps, notes)
    return f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}"><rect width="{W}" height="{H}" fill="#1b1c1e"/>{"".join(o)}</svg>\n', W, H


def main(out):
    os.makedirs(out, exist_ok=True)
    for key, sc in SCHEMES.items():
        for si, (style, *_) in enumerate(STYLES):
            svg, W, H = picture(key, sc, si)
            open(os.path.join(out, f"{key}-{si+1:02d}-{style}.svg"), "w").write(svg)


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "out")
