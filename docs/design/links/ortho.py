# Links only level or upright, turning at right angles rounded as the blocks are; a stereo pair
# pressed together after it leaves its ports (a line's width between the two), fanned out again
# into its target's ports. Each link (or pair) runs in a lane of its own in the gaps between the
# columns and between the blocks, so none lies on another.
#   python3 ortho.py out/      writes out/ortho-a.svg, -b, -c (the graphs of links.py)
import math
import os
import sys

import links as L

NW, NH0, PR, COLW = L.NW, L.NH0, L.PR, L.COLW
BPAD, BHEAD, BOTTOM, BETWEEN = 44.0, 36.0, 40.0, 44.0
R = 6.0  # the blocks' corner radius
# between two lanes (edge to edge), and from a lane to a block: a stereo pair's width, two lines
# and one between them
CLEAR = 3 * 2.0
GAP0 = COLW - NW  # a gap between two columns, before lanes widen it
# as the app has it now: several off one port share the pressed stretch and part after it (no
# three ways off the port), and a pair always steps a port spacing from the block
SHARE_WAYS = False
NEAR_TURN = False


def width(passive):
    return 1.2 if passive else 2.0


def col_x(sp, c):
    return 40 + sp["gap"].get(-1, BPAD) + c * NW + sum(sp["gap"].get(k, GAP0) for k in range(c))


def layout(sc, sp):
    """Places, given the room the lanes asked for: each gap's width, each block moved down, each
    band's bottom and the room between the bands."""
    pos, bands, y = {}, [], 70.0
    for bi, _ in enumerate(sc["bands"]):
        ns = [n for n in sc["nodes"] if n["band"] == bi]
        h = max(n["ry"] + sp["down"].get(n["id"], 0) + L.nh(n) for n in ns)
        for n in ns:
            pos[n["id"]] = (col_x(sp, n["col"]), y + BHEAD + n["ry"] + sp["down"].get(n["id"], 0))
        bh = BHEAD + h + sp["bottom"].get(bi, BOTTOM)
        bands.append((y, bh))
        y += bh + sp["between"].get(bi, BETWEEN)
    return pos, bands, y


class Graph:
    def __init__(self, sc, sp):
        self.sc, self.sp = sc, sp
        self.pos, self.bands, self.h = layout(sc, sp)
        self.byid = {n["id"]: n for n in sc["nodes"]}
        self.ncol = max(n["col"] for n in sc["nodes"]) + 1

    def port(self, nid, out, i):
        x, y = self.pos[nid]
        return (x + (NW if out else 0), y + NH0 + i * PR + PR / 2 + 2)

    def col_x(self, c):
        return col_x(self.sp, c)

    def gap(self, c):
        """The gap right of column c (c = -1: left of the first), as (x0, x1): where lanes may
        run, a lane's clearance off the blocks."""
        jin = getattr(self, "jog_in", {})
        jout = getattr(self, "jog_out", {})
        if c < 0:
            return (44.0 + CLEAR, self.col_x(0) - jin.get(0, 0) - CLEAR)
        return (self.col_x(c) + NW + jout.get(c, 0) + CLEAR, self.col_x(c + 1) - jin.get(c + 1, 0) - CLEAR)

    def blocks(self, band, col):
        return sorted((self.pos[n["id"]][1], self.pos[n["id"]][1] + L.nh(n)) for n in self.sc["nodes"] if n["band"] == band and n["col"] == col)

    def free_rows(self, band, cols):
        """Level stretches of the band clear of every block in these columns: (y0, y1)."""
        top, bh = self.bands[band]
        busy = sorted(r for c in cols for r in self.blocks(band, c))
        out, y = [], top + BHEAD - 4
        for t, b in busy:
            # even a narrow one: the lanes that take it widen it
            if t - CLEAR >= y:
                out.append((y, t - CLEAR))
            y = max(y, b + CLEAR)
        out.append((y, top + bh - 6))
        return out


# ---------------- the links as units: one link, or a stereo pair ----------------

def units(g):
    ls, used, out = g.sc["links"], set(), []
    for k, l in enumerate(ls):
        if k in used:
            continue
        pair = next((j for j, m in enumerate(ls) if j not in used and j != k and m[0] == l[0] and m[2] == l[2] and m[1] == l[1] + 1 and m[3] == l[3] + 1 and m[4] == l[4]), None)
        members = [l] if pair is None else [l, ls[pair]]
        used.add(k)
        if pair is not None:
            used.add(pair)
        out.append(dict(links=members, passive=l[4], src=l[0], dst=l[2], color=L.C["meter"] if g.byid[l[0]]["kind"] == "meter" else L.C[g.byid[l[0]]["kind"]]))
    for u in out:
        u["a"] = [g.port(m[0], True, m[1]) for m in u["links"]]
        u["b"] = [g.port(m[2], False, m[3]) for m in u["links"]]
        w = width(u["passive"])
        u["w"] = w
        u["s"] = 2 * width(False)  # centre to centre: one line's width between them (as for a solid pair)
        u["span"] = (len(u["links"]) - 1) * u["s"] + w  # how wide the unit is
        u["ay"] = sum(p[1] for p in u["a"]) / len(u["a"])
        u["by"] = sum(p[1] for p in u["b"]) / len(u["b"])
        u["a_shared"] = u["b_shared"] = False
        u["ajog"] = u["bjog"] = PR
    # several from the same port, or into the same port: they part (or meet) at the step a
    # port spacing from the block, each at a height of its own a lane's clearance apart, so none
    # runs on another; ordered by where they go (or come from), to cross less
    for end, other in ((("a", "by"), ("b", "ay")) if SHARE_WAYS else ()):
        groups = {}
        for u in out:
            m = u["links"][0]
            k = (m[0], m[1]) if end == "a" else (m[2], m[3])
            groups.setdefault(k, []).append(u)
        for grp in groups.values():
            if len(grp) < 2:
                continue
            # three ways off a port: straight on (the one whose other end is nearest the port's
            # height), up and down (as their other ends lie); one per way in each layer, the
            # first layer turning a port spacing from the block, the second two, the third
            # three. On one side the nearer turn goes further out, so they don't cross
            step = max(u["span"] for u in grp) + CLEAR
            port_y = grp[0][end + "y"]
            anchor = min(grp, key=lambda u: abs(u[other] - port_y))
            ups = sorted((u for u in grp if u is not anchor and u[other] < port_y), key=lambda u: u[other])
            downs = sorted((u for u in grp if u is not anchor and u[other] >= port_y), key=lambda u: -u[other])
            for side, way in ((ups, -1), (downs, 1)):
                for k, u in enumerate(side):
                    u[end + "y"] = port_y + way * (len(side) - k) * step
                    u[end + "_shared"] = True
                    u[end + "jog"] = (k + 1) * PR
    return out


# ---------------- routing: a skeleton of lane requests, then the lanes, then the lines ----------------

def plan(g, u):
    """The unit's way as alternating level and upright pieces: each upright one in a gap lane
    (`("gap", key)`), each level one at a height (a port's, or a row lane `("row", key)`)."""
    ns, nd = g.byid[u["src"]], g.byid[u["dst"]]
    sb, db, sc_, dc = ns["band"], nd["band"], ns["col"], nd["col"]
    if sb == db and dc > sc_:
        between = list(range(sc_ + 1, dc))
        if not between:
            if abs(u["ay"] - u["by"]) < 0.5:
                return [("y", "a")]
            return [("y", "a"), ("gap", (sb, dc - 1)), ("y", "b")]
        # through a row clear in all the columns between, the one nearest the way
        rows = g.free_rows(sb, between)
        want = (u["ay"] + u["by"]) / 2
        row = min(rows, key=lambda r: 0 if r[0] <= want <= r[1] else min(abs(r[0] - want), abs(r[1] - want)))
        return [("y", "a"), ("gap", (sb, sc_)), ("row", (sb, tuple(between), row)), ("gap", (sb, dc - 1)), ("y", "b")]
    # to another band, one column on: straight down (or up) the gap between the two columns
    if sb != db and dc == sc_ + 1:
        return [("y", "a"), ("gap", (sb, sc_)), ("y", "b")]
    # back, or to another band: out to the gap right of its column, along the band's bottom or
    # the gap between the bands, in from the gap left of the target's column
    if sb == db:
        top, bh = g.bands[sb]
        row = ("bottom", sb, (top + bh - g.sp["bottom"].get(sb, BOTTOM) + CLEAR, top + bh - CLEAR))
    else:
        lo, hi = min(sb, db), max(sb, db)
        top, bh = g.bands[lo]
        row = ("between", lo, (top + bh + CLEAR, top + bh + g.sp["between"].get(lo, BETWEEN) - CLEAR))
    return [("y", "a"), ("gap", (sb, sc_)), ("row", row), ("gap", (db, dc - 1)), ("y", "b")]


def assign(g, us):
    """Lanes: each gap's uprights side by side about its middle, each row's levels likewise; a
    lane's clearance between any two, never closer. Returns the room that was short: per gap,
    per row (what it would widen)."""
    short = []
    # the steps off shared ports, by column: the lanes keep clear of them
    g.jog_out, g.jog_in = {}, {}
    for u in us:
        if u["a_shared"] or len(u["links"]) > 1:
            c = g.byid[u["src"]]["col"]
            g.jog_out[c] = max(g.jog_out.get(c, 0), u["ajog"] + 2 * R)
        if u["b_shared"] or len(u["links"]) > 1:
            c = g.byid[u["dst"]]["col"]
            g.jog_in[c] = max(g.jog_in.get(c, 0), u["bjog"] + 2 * R)
    gaps, rows = {}, {}
    for u in us:
        u["plan"] = plan(g, u)
        for i, (k, key) in enumerate(u["plan"]):
            if k == "gap":
                gaps.setdefault(key, []).append((u, i))
            elif k == "row":
                rows.setdefault(key, []).append((u, i))
    for key, items in gaps.items():
        band, c = key
        x0, x1 = g.gap(c)

        # an upright piece: where it comes from and goes to (its neighbours' heights)
        def ends(it):
            u, i = it
            return (level(u, i - 1), level(u, i + 1))
        # which goes right of which: A right of B costs a crossing (or a piece of one on the
        # other) when B leaves rightwards at a height A's upright spans, or A comes in from the
        # left at a height B's spans; each placed by how many it is better right of
        def inside(y, e):
            return min(e) - .5 <= y <= max(e) + .5

        def cost(a, b):
            ea, eb = ends(a), ends(b)
            return inside(eb[1], ea) + inside(ea[0], eb)
        score = {id(it): sum(cost(o, it) - cost(it, o) for o in items if o is not it) for it in items}
        items.sort(key=lambda it: -score[id(it)])
        total = sum(u["span"] + CLEAR for u, _ in items) - CLEAR
        if total > x1 - x0:
            short.append(("gap", c, total - (x1 - x0)))
        x = (x0 + x1) / 2 + total / 2
        for u, i in items:
            x -= u["span"] / 2
            u.setdefault("at", {})[i] = x
            x -= u["span"] / 2 + CLEAR
    for key, items in rows.items():
        y0, y1 = key[-1]

        # where each would rather run: at its target's height if the row has it (no more turns
        # there), else its source's, else as near as the row comes
        def want(u):
            for y in (u["by"], u["ay"]):
                if y0 + u["span"] / 2 <= y <= y1 - u["span"] / 2:
                    return y
            return min(max((u["ay"] + u["by"]) / 2, y0 + u["span"] / 2), y1 - u["span"] / 2)
        items.sort(key=lambda it: want(it[0]))
        # each at its wish, pushed down by the one above it, then all up if they ran out of room
        ys, last = [], None
        for u, _ in items:
            y = want(u) if last is None else max(want(u), last + CLEAR + u["span"] / 2)
            ys.append(y)
            last = y + u["span"] / 2
        over = (last or y1) - y1
        first = ys[0] - items[0][0]["span"] / 2 if ys else y0
        if over > 0 or first < y0:
            short.append(("row", key, max(over, 0) + max(y0 - first, 0)))
        for (u, i), y in zip(items, ys):
            u.setdefault("at", {})[i] = y
    return short


def level(u, i):
    """The height of the unit's level piece i (for sorting before the lanes are all set)."""
    if i < 0 or i >= len(u["plan"]):
        return u["ay"]
    k, key = u["plan"][i]
    if k == "y":
        return u["ay"] if key == "a" else u["by"]
    if k == "row":
        y0, y1 = key[-1]
        return (y0 + y1) / 2
    return u["ay"]


def centreline(u):
    """The unit's middle line: points at its turns."""
    pts = []
    xs = []
    ys = []
    for i, (k, key) in enumerate(u["plan"]):
        if k == "y":
            ys.append(u["ay"] if key == "a" else u["by"])
        elif k == "row":
            ys.append(u["at"][i])
        else:
            xs.append(u["at"][i])
    a = (u["a"][0][0], ys[0])
    b = (u["b"][0][0], ys[-1])
    pts = [a]
    for j, x in enumerate(xs):
        pts += [(x, ys[j]), (x, ys[j + 1])]
    pts.append(b)
    return pts


def line_pts(u, li):
    """Line li of the unit (0: the first channel, FL). A single link: the middle line itself. A
    pair: the middle line moved off itself, left of the way for the first, right for the second,
    pressed to a line's width apart all the way, and split again a step from each block: as far
    from the ports as the ports are from each other."""
    c = centreline(u)
    a, b = u["a"][li], u["b"][li]
    if len(u["links"]) == 1:
        # a step off a shared port to its own height
        head = [a, (a[0] + u["ajog"], a[1]), (a[0] + u["ajog"], c[0][1])] if u["a_shared"] else [a]
        tail = [(b[0] - u["bjog"], c[-1][1]), (b[0] - u["bjog"], b[1]), b] if u["b_shared"] else [b]
        return head + c[1:-1] + tail
    d = u["s"] / 2 if li == 0 else -u["s"] / 2
    # the middle line's level pieces (their heights, which way they run) and upright ones (x)
    levels = [(c[k][1], 1 if c[k + 1][0] >= c[k][0] else -1) for k in range(0, len(c) - 1, 2)]
    ups = [(c[k][0], 1 if c[k + 1][1] >= c[k][1] else -1) for k in range(1, len(c) - 1, 2)]
    # left of the way: up going right, down going left; right going down, left going up
    ys = [y - d * way for y, way in levels]
    xs = [x + d * way for x, way in ups]
    # an end whose lane comes near the block (no room for a turn, a pressed piece and a step):
    # the pair runs pressed up or down the lane and each line turns off it at its own port's
    # height; else it comes level, pressed, and splits a step from the block: as far from the
    # ports as they are from each other
    near = 2 * PR + 2 * R
    if NEAR_TURN and xs and xs[0] - a[0] < near and not u["a_shared"]:
        ys[0] = a[1]
        out = [a]
    else:
        ja = a[0] + u["ajog"]
        out = [a, (ja, a[1]), (ja, ys[0])]
    if NEAR_TURN and xs and b[0] - xs[-1] < near and not u["b_shared"]:
        ys[-1] = b[1]
        tail = [b]
    else:
        jb = b[0] - u["bjog"]
        tail = [(jb, ys[-1]), (jb, b[1]), b]
    for j, x in enumerate(xs):
        out += [(x, ys[j]), (x, ys[j + 1])]
    return out + tail


def rounded(pts, radii):
    s = f"M{pts[0][0]:.2f},{pts[0][1]:.2f}"
    for i in range(1, len(pts) - 1):
        p, c, n = pts[i - 1], pts[i], pts[i + 1]
        din, dout = math.dist(p, c), math.dist(c, n)
        if din < .01 or dout < .01:
            continue
        r = max(0.5, min(radii[i], din / 2, dout / 2))
        a = (c[0] + (p[0] - c[0]) * r / din, c[1] + (p[1] - c[1]) * r / din)
        b = (c[0] + (n[0] - c[0]) * r / dout, c[1] + (n[1] - c[1]) * r / dout)
        # a quarter circle: as a cubic, its handles 0.552 of the radius
        k = 0.552
        h1 = (a[0] + (c[0] - a[0]) * k, a[1] + (c[1] - a[1]) * k)
        h2 = (b[0] + (c[0] - b[0]) * k, b[1] + (c[1] - b[1]) * k)
        s += f" L{a[0]:.2f},{a[1]:.2f} C{h1[0]:.2f},{h1[1]:.2f} {h2[0]:.2f},{h2[1]:.2f} {b[0]:.2f},{b[1]:.2f}"
    s += f" L{pts[-1][0]:.2f},{pts[-1][1]:.2f}"
    return s


def radii(pts, d, s):
    """At each turn: the blocks' radius on the inside of the turn, more by the pair's spacing on
    the outside, so a pair turns side by side."""
    out = [0.0] * len(pts)
    for i in range(1, len(pts) - 1):
        p, c, n = pts[i - 1], pts[i], pts[i + 1]
        din = (c[0] - p[0], c[1] - p[1])
        dout = (n[0] - c[0], n[1] - c[1])
        cross = din[0] * dout[1] - din[1] * dout[0]  # > 0: turning right (y down)
        if d == 0:
            out[i] = R + s / 2 if s else R
        else:
            # left of the way (d > 0) is the inside of a left turn
            inside = (d > 0) == (cross < 0)
            out[i] = R if inside else R + s
    return out


def draw(g, us, o):
    for passive in (1, 0):
        for u in us:
            if u["passive"] != passive:
                continue
            n = len(u["links"])
            for li in range(n):
                pts = line_pts(u, li)
                d = 0.0 if n == 1 else (u["s"] / 2 if li == 0 else -u["s"] / 2)
                rr = radii(pts, d, u["s"] if n > 1 else 0)
                st = f'stroke-width="{u["w"]}" stroke-dasharray="3 4"' if passive else f'stroke-width="{u["w"]}"'
                o.append(f'<path d="{rounded(pts, rr)}" fill="none" stroke="{u["color"]}" {st}/>')


def widen(g, sp, short):
    """More room where the lanes were short of it: a gap wider, the blocks under a row lower, a
    band's bottom or the room between the bands taller."""
    for kind, key, more in short:
        more = math.ceil(more) + 1
        if kind == "gap":
            sp["gap"][key] = sp["gap"].get(key, BPAD if key < 0 else GAP0) + more
        elif key[0] == "bottom":
            sp["bottom"][key[1]] = sp["bottom"].get(key[1], BOTTOM) + more
        elif key[0] == "between":
            sp["between"][key[1]] = sp["between"].get(key[1], BETWEEN) + more
        else:
            band, cols, (y0, y1) = key
            for n in g.sc["nodes"]:
                if n["band"] == band and n["col"] in cols and g.pos[n["id"]][1] >= y1 - 1:
                    sp["down"][n["id"]] = sp["down"].get(n["id"], 0) + more


def picture(key, sc):
    # lay out, route, give the lanes the room they lacked, again until nothing is short
    sp = {"gap": {}, "down": {}, "bottom": {}, "between": {}}
    for _ in range(12):
        g = Graph(sc, sp)
        us = units(g)
        short = assign(g, us)
        if not short:
            break
        widen(g, sp, short)
    W = col_x(sp, g.ncol - 1) + NW + 40 + 40
    H = g.h + 10
    o = [f'<text x="40" y="30" {L.F} font-weight="bold" font-size="16" letter-spacing="1.5" fill="#e58f2c">RIGHT ANGLES, PAIRS PRESSED TOGETHER</text>',
         f'<text x="40" y="52" {L.F} font-size="12.5" fill="#9a9da1">Level and upright only, turns rounded as the blocks (inside 6, outside 6 + the spacing); a stereo pair a line\'s width apart; a lane each.  ·  {key.upper()}: {L.esc(sc["name"].lower())}</text>']
    for bi, (y, bh) in enumerate(g.bands):
        kind, nm, rest = sc["bands"][bi]
        o.append(f'<rect x="40" y="{y}" width="{W-80}" height="{bh}" rx="10" fill="#fff" fill-opacity=".025" stroke="#4f86d9" stroke-opacity=".35" stroke-width="1.2"/>')
        o.append(f'<text x="{40+26}" y="{y+22}" {L.M} font-weight="bold" font-size="12" fill="#c9cbcd" xml:space="preserve"><tspan fill="#e58f2c" letter-spacing="1">{kind}</tspan>  {L.esc(nm)}  <tspan fill="#7d8085" font-weight="normal">{L.esc(rest)}</tspan></text>')
    draw(g, us, o)
    for n in sc["nodes"]:
        x, y = g.pos[n["id"]]
        L.block(n, x, y, o)
    return f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}"><rect width="{W}" height="{H}" fill="#1b1c1e"/>{"".join(o)}</svg>\n'


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "out"
    os.makedirs(out, exist_ok=True)
    for key, sc in L.SCHEMES.items():
        open(os.path.join(out, f"ortho-{key}.svg"), "w").write(picture(key, sc))
