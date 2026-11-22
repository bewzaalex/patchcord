# Links as railway track: straight, 45° diagonals, turns of 45° only, branches leaving the
# trunk at 45° like points; on the routes the app uses now (ortho.py: lanes of their own, a
# pair pressed together a step from the block, several off one port sharing the pressed stretch).
#   python3 rail.py rail/     writes rail/<n>-<variant>-<scheme>.svg
import math
import os
import sys

import links as L
import ortho as O

KB = 8.0  # how far a 90° turn is cut back each way: its 45° diagonal


def turns_inside(pts, d, s):
    """Which turns a line takes on the inside (as ortho.radii decides)."""
    r = O.radii(pts, d, s)
    return [x <= O.R + 1e-6 for x in r]


def chamfer(pts, d, s):
    """Every right-angle turn cut at 45°: the inside line cut KB back, the outside a little more,
    so a pair's two diagonals stay side by side."""
    inside = turns_inside(pts, d, s)
    out = [pts[0]]
    for i in range(1, len(pts) - 1):
        p, c, n = pts[i - 1], pts[i], pts[i + 1]
        din, dout = math.dist(p, c), math.dist(c, n)
        if din < .01 or dout < .01:
            continue
        ux, uy = (c[0] - p[0]) / din, (c[1] - p[1]) / din
        vx, vy = (n[0] - c[0]) / dout, (n[1] - c[1]) / dout
        if abs(ux * vy - uy * vx) < .1:  # not a right angle: a 45° turn already, or straight
            out.append(c)
            continue
        k = KB + (0 if inside[i] or s == 0 else s * math.tan(math.radians(22.5)))
        k = min(k, din / 2, dout / 2)
        out += [(c[0] - ux * k, c[1] - uy * k), (c[0] + vx * k, c[1] + vy * k)]
    out.append(pts[-1])
    return out


def diagonals(pts):
    """An upright stretch between two level ones eased in and out by 45° diagonals, as far as
    the level stretches either side leave room: level, diagonal, upright (in its own lane),
    diagonal, level, as track bends."""
    out = list(pts)
    i = 1
    while i + 2 < len(out):
        p, c1, c2, n = out[i - 1], out[i], out[i + 1], out[i + 2]
        if abs(p[1] - c1[1]) < .01 and abs(c1[0] - c2[0]) < .01 and abs(c2[1] - n[1]) < .01 and (c1[0] - p[0]) * (n[0] - c2[0]) > 0:
            x, dy = c1[0], abs(c2[1] - c1[1])
            way = 1 if n[0] > x else -1
            sy = 1 if c2[1] > c1[1] else -1
            w1 = max(min(dy / 2, abs(x - p[0]) - KB), 0)
            w2 = max(min(dy / 2, abs(n[0] - x) - KB), 0)
            mid = [(x - way * w1, c1[1]), (x, c1[1] + sy * w1), (x, c2[1] - sy * w2), (x + way * w2, c2[1])]
            # the same point twice (no upright left) once
            mid = [q for k, q in enumerate(mid) if k == 0 or math.dist(q, mid[k - 1]) > .01]
            out[i:i + 2] = mid
            i += len(mid)
            continue
        i += 1
    return out


def sleepers(pts, o, every=7.0, half=4.5):
    """Ties across the line every few units, under it."""
    carry = 0.0
    for p, q in zip(pts, pts[1:]):
        ln = math.dist(p, q)
        if ln < .01:
            continue
        ux, uy = (q[0] - p[0]) / ln, (q[1] - p[1]) / ln
        t = carry
        while t < ln:
            x, y = p[0] + ux * t, p[1] + uy * t
            o.append(f'<path d="M{x - uy*half:.1f},{y + ux*half:.1f} L{x + uy*half:.1f},{y - ux*half:.1f}" stroke="#4a4c50" stroke-width="1.6"/>')
            t += every
        carry = t - ln


def poly_d(pts):
    return "M" + " L".join(f"{x:.2f},{y:.2f}" for x, y in pts)


VARIANTS = [
    ("cut", "CUT CORNERS", "As now, but every right angle cut into two 45° turns; a pair's step a 45° switch."),
    ("diagonal", "DIAGONALS", "Up and down as 45° diagonals where there is room, as track runs; branches leave at 45° like points."),
    ("track", "TRACK", "The diagonals, and sleepers under what plays: a pair is a double track; what only listens stays a dashed line."),
]


def picture(key, sc, vi):
    vname, title, note = VARIANTS[vi]
    sp = {"gap": {}, "down": {}, "bottom": {}, "between": {}}
    for _ in range(12):
        g = O.Graph(sc, sp)
        us = O.units(g)
        short = O.assign(g, us)
        if not short:
            break
        O.widen(g, sp, short)
    W = O.col_x(sp, g.ncol - 1) + O.NW + 80
    H = g.h + 10
    o = [f'<text x="40" y="30" {L.F} font-weight="bold" font-size="16" letter-spacing="1.5" fill="#e58f2c">{vi+1} · {title}</text>',
         f'<text x="40" y="52" {L.F} font-size="12.5" fill="#9a9da1">{L.esc(note)}  ·  {key.upper()}: {L.esc(sc["name"].lower())}</text>']
    for bi, (y, bh) in enumerate(g.bands):
        kind, nm, rest = sc["bands"][bi]
        o.append(f'<rect x="40" y="{y}" width="{W-80}" height="{bh}" rx="10" fill="#fff" fill-opacity=".025" stroke="#4f86d9" stroke-opacity=".35" stroke-width="1.2"/>')
        o.append(f'<text x="66" y="{y+22}" {L.M} font-weight="bold" font-size="12" fill="#c9cbcd" xml:space="preserve"><tspan fill="#e58f2c" letter-spacing="1">{kind}</tspan>  {L.esc(nm)}  <tspan fill="#7d8085" font-weight="normal">{L.esc(rest)}</tspan></text>')
    ties, lines = [], []
    for passive in (1, 0):
        for u in us:
            if u["passive"] != passive:
                continue
            n = len(u["links"])
            for li in range(n):
                pts = O.line_pts(u, li)
                d = 0.0 if n == 1 else (u["s"] / 2 if li == 0 else -u["s"] / 2)
                s = u["s"] if n > 1 else 0
                if vname != "cut":
                    pts = diagonals(pts)
                pts = chamfer(pts, d, s)
                if vname == "track" and not passive:
                    sleepers(pts, ties)
                st = f'stroke-width="{u["w"]}" stroke-dasharray="3 4"' if passive else f'stroke-width="{u["w"]}"'
                lines.append(f'<path d="{poly_d(pts)}" fill="none" stroke="{u["color"]}" {st} stroke-linejoin="round" stroke-linecap="round"/>')
    o += ties + lines
    for nd in sc["nodes"]:
        x, y = g.pos[nd["id"]]
        L.block(nd, x, y, o)
    return f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}"><rect width="{W}" height="{H}" fill="#1b1c1e"/>{"".join(o)}</svg>\n'


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "rail"
    os.makedirs(out, exist_ok=True)
    for vi, (vname, *_) in enumerate(VARIANTS):
        for key, sc in L.SCHEMES.items():
            open(os.path.join(out, f"{vi+1}-{vname}-{key}.svg"), "w").write(picture(key, sc, vi))
