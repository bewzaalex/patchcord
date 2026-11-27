# Vents: what fills a rack face where it has nothing to show. Today each unit gets its own block
# of long horizontal slots, so a 3U device reads as three 1U ones. Ten ways, each one field of
# holes laid on the face's own grid (not each unit's), so where the free space runs from one unit
# into the next the holes run on with it. Only whole holes: a hole the edge would cut is left out.
#   python3 vents.py   (writes vent-01.png ... vent-10.png and all.png next to it; needs rsvg-convert)
import math
import os
import subprocess

HERE = os.path.dirname(os.path.abspath(__file__))
U = 108.0 * 1.75 / 1.719        # a rack unit's pitch (face::RACK_U)
GAP = U - 108.0                  # between two devices
W = 1180.0                       # a face's width
HOLE = "#0b0c0d"                 # a slot's colour today
LIP = "rgba(255,255,255,.12)"    # the light edge under a hole (light from the top left)
PAINTS = {"black": ("#232427", "#d5d7d9"), "navy": ("#2b3950", "#dde5f0"), "olive": ("#45512f", "#ebedd9")}

# The specimen: the screenshot's first two devices, plus a 2U node. Each: paint, units, what is on
# it (name+screen at the top left always; frames as (unit, x0, x1, colour)), and its free space
# as rectangles on the face (x0, y0, x1, y1), the way face.rs finds it, but reaching across the
# boundary between units so a field may run on.
DEVICES = [
    ("FOOYIN", "black", 1, [(0, 420, 740, "#bb5a2a")]),
    ("JLAB GO WORK 2", "navy", 3, [(1, 60, 920, "#1f8a86"), (2, 60, 400, "#2f78b4")]),
    ("EASY EFFECTS", "olive", 2, [(0, 420, 760, "#4f8a32"), (1, 60, 400, "#bb5a2a"), (1, 420, 760, "#bb5a2a")]),
]
EDGE = 22.0        # from the face's top and bottom edges, and from the screws' column
NEAR = 16.0        # from a frame or the screen


def free(units, frames):
    """The free space of a face, as rectangles, one a unit, each reaching to the next unit's
    when the space goes on there."""
    h = U * units - GAP
    x_r = W - 50.0
    rects = []
    for u in range(units):
        y0 = u * U + (EDGE if u == 0 else 0.0)
        y1 = (u + 1) * U if u < units - 1 else h - EDGE
        mine = [f for f in frames if f[0] == u]
        if u == 0:
            left = 420.0 if not mine else max(f[2] for f in mine) + NEAR
        else:
            left = 50.0 if not mine else max(f[2] for f in mine) + NEAR
        rects.append((left, y0, x_r, y1))
    return rects


def inside(rects, x0, y0, x1, y1):
    """Whether a hole's box lies on free space (its corners and middle each on some rectangle)."""
    pts = [(x0, y0), (x1, y0), (x0, y1), (x1, y1), ((x0 + x1) / 2, (y0 + y1) / 2)]
    return all(any(a <= px <= c and b <= py <= d for a, b, c, d in rects) for px, py in pts)


def lip_circle(cx, cy, r):
    a0, a1 = math.radians(20), math.radians(160)
    return (f'<path d="M{cx + r * math.cos(a0) + .3:.1f},{cy + r * math.sin(a0) + .8:.1f} '
            f'A{r},{r} 0 0 1 {cx + r * math.cos(a1) + .3:.1f},{cy + r * math.sin(a1) + .8:.1f}" '
            f'fill="none" stroke="{LIP}" stroke-width="1"/>')


def poly(pts, lip=True):
    d = "M" + " L".join(f"{x:.1f},{y:.1f}" for x, y in pts) + "Z"
    s = f'<path d="{d}" fill="{HOLE}"/>'
    if lip:
        s += f'<path d="{d}" fill="none" stroke="{LIP}" stroke-width="1" transform="translate(.4,.9)"/>'
    return s


def hexagon(cx, cy, r, flat=False):
    a = 0 if flat else 30
    return [(cx + r * math.cos(math.radians(a + 60 * k)), cy + r * math.sin(math.radians(a + 60 * k))) for k in range(6)]


# --- the ten designs: each takes the free rectangles and gives the holes' SVG ---------------

def grid(rects, dx, dy, stagger, hole_box):
    """Points of a lattice on the face's own grid, kept where the hole round them fits."""
    out = []
    x_min, y_min = min(r[0] for r in rects), min(r[1] for r in rects)
    x_max, y_max = max(r[2] for r in rects), max(r[3] for r in rects)
    j = math.floor(y_min / dy)
    while j * dy <= y_max:
        y = j * dy
        off = (dx / 2 if j % 2 else 0.0) if stagger else 0.0
        i = math.floor((x_min - off) / dx)
        while i * dx + off <= x_max:
            x = i * dx + off
            hx, hy = hole_box
            if inside(rects, x - hx, y - hy, x + hx, y + hy):
                out.append((x, y))
            i += 1
        j += 1
    return out


def d01(rects):
    """Honeycomb, fine: small hexagons, thin webs."""
    r = 5.2
    dx = r * math.sqrt(3) + 2.4
    pts = grid(rects, dx, dx * math.sqrt(3) / 2, True, (r, r))
    return "".join(poly(hexagon(x, y, r)) for x, y in pts)


def d02(rects):
    """Honeycomb, coarse: big hexagons, a guard's wide webs."""
    r = 10.0
    dx = r * math.sqrt(3) + 4.0
    pts = grid(rects, dx, dx * math.sqrt(3) / 2, True, (r, r))
    return "".join(poly(hexagon(x, y, r)) for x, y in pts)


def d03(rects):
    """Perforated sheet: round holes, staggered (60°)."""
    r = 3.4
    pts = grid(rects, 10.0, 10.0 * math.sqrt(3) / 2, True, (r, r))
    return "".join(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="{r}" fill="{HOLE}"/>' + lip_circle(x, y, r) for x, y in pts)


def d04(rects):
    """Square mesh: square holes in rows and columns."""
    s = 4.2
    pts = grid(rects, 10.0, 10.0, False, (s, s))
    return "".join(poly([(x - s, y - s), (x + s, y - s), (x + s, y + s), (x - s, y + s)]) for x, y in pts)


def d05(rects):
    """Diamonds: square holes turned 45°, a tight lattice."""
    s = 6.0
    pts = grid(rects, 14.0, 7.0, True, (s, s))
    return "".join(poly([(x, y - s), (x + s, y), (x, y + s), (x - s, y)]) for x, y in pts)


def d06(rects):
    """Short slots, staggered like bricks: an amplifier's top."""
    w, h = 22.0, 4.2
    out = []
    for x, y in grid(rects, 30.0, 9.5, True, (w / 2, h / 2)):
        out.append(f'<rect x="{x - w / 2:.1f}" y="{y - h / 2:.1f}" width="{w}" height="{h}" rx="2.1" fill="{HOLE}"/>'
                   f'<path d="M{x - w / 2 + 2:.1f},{y + h / 2 + .9:.1f} h{w - 4}" stroke="{LIP}"/>')
    return "".join(out)


def d07(rects):
    """Expanded metal: long flat diamonds, staggered."""
    a, b = 11.0, 4.6
    pts = grid(rects, 26.0, 10.5, True, (a, b))
    return "".join(poly([(x - a, y), (x, y - b), (x + a, y), (x, y + b)]) for x, y in pts)


def d08(rects):
    """Vertical slots: tall pills, each as tall as its field allows, so no row of them follows a
    unit."""
    w, pitch = 4.2, 9.5
    out = []
    x_min, x_max = min(r[0] for r in rects), max(r[2] for r in rects)
    i = math.ceil(x_min / pitch)
    while i * pitch <= x_max:
        x = i * pitch
        # the runs of free space down this column
        ys = sorted({v for r in rects if r[0] <= x - w / 2 and x + w / 2 <= r[2] for v in (r[1], r[3])})
        spans = [r for r in sorted(rects, key=lambda r: r[1]) if r[0] <= x - w / 2 and x + w / 2 <= r[2]]
        runs = []
        for r in spans:
            if runs and abs(runs[-1][1] - r[1]) < 0.5:
                runs[-1][1] = r[3]
            else:
                runs.append([r[1], r[3]])
        for y0, y1 in runs:
            y0, y1 = y0 + 6, y1 - 6
            if y1 - y0 > 20:
                out.append(f'<rect x="{x - w / 2:.1f}" y="{y0:.1f}" width="{w}" height="{y1 - y0:.1f}" rx="2.1" fill="{HOLE}"/>'
                           f'<path d="M{x + w / 2 + .7:.1f},{y0 + 2:.1f} v{y1 - y0 - 4:.1f}" stroke="{LIP}"/>')
        i += 1
    return "".join(out)


def d09(rects):
    """Round holes, square grid, bigger: a speaker grille."""
    r = 4.6
    pts = grid(rects, 13.0, 13.0, False, (r, r))
    return "".join(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="{r}" fill="{HOLE}"/>' + lip_circle(x, y, r) for x, y in pts)


def d10(rects):
    """Honeycomb with a bezel: one recessed plate over the whole free space (its outline drawn
    once round the union), fine hexagons in it."""
    out = []
    # the plate: each rectangle filled a shade darker, the joins between them not drawn
    for a, b, c, d in rects:
        out.append(f'<rect x="{a + 4:.1f}" y="{b + (0 if b % U < 1 and b > 1 else 4):.1f}" width="{c - a - 8:.1f}" height="{d - b - (0 if d % U < 1 and d < 400 else 4) - (0 if b % U < 1 and b > 1 else 4):.1f}" fill="rgba(0,0,0,.22)"/>')
    inner = [(a + 10, b + (0 if b % U < 1 and b > 1 else 10), c - 10, d - (0 if d % U < 1 else 10)) for a, b, c, d in rects]
    r = 4.0
    dx = r * math.sqrt(3) + 1.6
    pts = grid(inner, dx, dx * math.sqrt(3) / 2, True, (r, r))
    out += [poly(hexagon(x, y, r), lip=False) for x, y in pts]
    return "".join(out)


DESIGNS = [d01, d02, d03, d04, d05, d06, d07, d08, d09, d10]
NAMES = ["01 honeycomb, fine", "02 honeycomb, coarse", "03 perforated, round", "04 square mesh",
         "05 diamonds", "06 short slots, bricks", "07 expanded metal", "08 vertical slots",
         "09 round grille", "10 honeycomb in a plate"]


def face(name, paint, units, frames, design, y):
    base, print_ = PAINTS[paint]
    h = U * units - GAP
    s = [f'<g transform="translate(40,{y:.1f})">',
         f'<rect width="{W}" height="{h:.1f}" rx="3" fill="{base}"/>',
         f'<rect width="{W}" height="{h:.1f}" rx="3" fill="url(#light)"/>']
    s.append(design(free(units, frames)))
    # name and screen
    s.append(f'<text x="60" y="36" fill="{print_}" font-family="Noto Sans" font-weight="bold" font-size="21">{name}</text>')
    s.append('<rect x="60" y="44" width="320" height="57" rx="3" fill="#f3a53c" stroke="#111" stroke-width="2"/>')
    # frames
    for u, x0, x1, c in frames:
        fy = u * U + 14
        s.append(f'<rect x="{x0}" y="{fy:.1f}" width="{x1 - x0}" height="80" rx="5" fill="rgba(0,0,0,.18)" stroke="{c}" stroke-width="2"/>')
        s.append(f'<path d="M{x0},{fy + 18:.1f} v-13 a5,5 0 0 1 5,-5 h{x1 - x0 - 10} a5,5 0 0 1 5,5 v13 z" fill="{c}"/>')
    # screws
    for sx in (26, W - 26):
        for sy in (16, h - 16):
            s.append(f'<circle cx="{sx}" cy="{sy:.1f}" r="8" fill="#cfd2d6" stroke="#555" stroke-width=".8"/>'
                     f'<path d="M{sx - 4},{sy:.1f} h8 M{sx},{sy - 4:.1f} v8" stroke="#555" stroke-width="2"/>')
    s.append("</g>")
    return "".join(s), h


def picture(i):
    body, y = [], 50.0
    for name, paint, units, frames in DEVICES:
        g, h = face(name, paint, units, frames, DESIGNS[i], y)
        body.append(g)
        y += h + GAP
    hgt = y + 20
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{W + 80:.0f}" height="{hgt:.0f}">'
            '<defs><linearGradient id="light" x1="0" y1="0" x2="1" y2="1">'
            '<stop offset="0" stop-color="#000" stop-opacity="0"/><stop offset="1" stop-color="#000" stop-opacity=".26"/>'
            '</linearGradient></defs>'
            f'<rect width="100%" height="100%" fill="#16171a"/>'
            f'<text x="40" y="32" fill="#cfd2d6" font-family="Noto Sans" font-weight="bold" font-size="20">{NAMES[i]}</text>'
            + "".join(body) + "</svg>")


if __name__ == "__main__":
    outs = []
    for i in range(len(DESIGNS)):
        svg = os.path.join(HERE, f"vent-{i + 1:02}.svg")
        png = os.path.join(HERE, f"vent-{i + 1:02}.png")
        with open(svg, "w") as f:
            f.write(picture(i))
        subprocess.run(["rsvg-convert", "-o", png, svg], check=True)
        os.remove(svg)
        outs.append(png)
    subprocess.run(["magick", "montage", *outs, "-tile", "2x", "-geometry", "+6+6", "-background", "#0d0d0f",
                    "-resize", "50%", os.path.join(HERE, "all.png")], check=True)
