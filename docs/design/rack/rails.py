# RACK with rails: the window is the rack. Two rails at the window's sides, from the top panel down
# to the status bar, square holes after the EIA drawing (in proportion to a unit: 1U = 128 of the
# app's units = 1.75"), unit numbers from the top; the devices screwed on, two screws a side, in
# the U's top and bottom holes; no frame round them. Two ways to hold them, both as narrow as the
# holes allow:
#   A  small ears, just over the holes
#   B  no ears: the panel itself runs over the rail and the screws go through it
#   python3 rails.py rails.svg && rsvg-convert -z .4 rails.svg -o rails.png
# The devices are cut from docs/screenshots/rack.png (screen scale 1.5), without their old ears.
import base64
import sys

SHOT = "../../screenshots/rack.png"
S0 = 1.5  # the screenshot's px per unit
WW, WH = 2280, 1857  # the window
TOP, BOT = 96, 1824  # the field between the top panel and the status bar

INCH = 128 / 1.75
U = 128.0
HOLE = .375 * INCH  # 27.4
PITCH = [.25 * INCH, .875 * INCH, 1.5 * INCH]  # hole centres from a unit's top: 18.3, 64, 109.7
HOLE_IN = .281 * INCH  # 20.6: a hole's centre from the opening's edge (18.312" between, 17.75" open)
NUMS = 20.0  # the rail's strip with the numbers, outside the devices
# the panel runs over the rail just enough for its screws: as far from the panel's edge as from the
# LCD (which starts 8 into the face cut here), as far as the edge was in the first try
SCREW_R = 7.2
SCREW_PAD = 38.0 - HOLE_IN - SCREW_R  # 10.2
LCD_IN = 8.0
SCREW_X = LCD_IN - SCREW_PAD - SCREW_R  # the screws' centre from the face: -9.4
EXT = SCREW_PAD + SCREW_R - SCREW_X  # how far the panel runs over the rail: 26.8
HC = NUMS + EXT + SCREW_X  # a hole's centre from the rail's outer edge
RIM = 8.0  # the rail beside its holes, on the inside: thin, as on the photo, but there
RAIL = HC + HOLE / 2 + RIM
GAP = .031 * INCH  # a 1U panel is 1.719" high: 2.3 between two
OPEN = 1000 - 2 * 36  # the faces without their old ears (rack_w 1000, old ear 36)
Z = WW / (OPEN + 2 * (NUMS + EXT))  # the least zoom: the rails at the window's edges
Y0 = TOP + GAP / 2 * Z  # unit 1 at the top of the scroll: as far from the panel as two devices are apart

VARIANTS = {
    "B": ("NO EARS", EXT, False, f"The panel itself runs {EXT:.1f} over the rail (was 38); its screws {SCREW_PAD:.1f} from its edge and from the LCD."),
}

# the screenshot's devices (top px)
ROWS_TOP = [186, 382.5, 579, 775.5, 972, 1168.5, 1365]
HUNG = 5  # devices hung: the rest of the rack empty
F = 'font-family="Noto Sans"'

o = []


def rgb(c):
    return "#%02x%02x%02x" % c


def colours(x):
    """Each device's colours down its height at screenshot px x."""
    from PIL import Image
    im = Image.open(SHOT).convert("RGB")
    return [[rgb(im.getpixel((int(x), int(t + k * S0)))) for k in (6, 40, 64, 90, 122)] for t in ROWS_TOP]


def rail(x0, mirror, n_units):
    """A rail: x0 its outer edge in window px; numbers on its outer strip."""
    w = RAIL * Z
    xs = (x0 - w, x0) if mirror else (x0, x0 + w)
    h = BOT - TOP
    o.append(f'<rect x="{xs[0]:.1f}" y="{TOP}" width="{w:.1f}" height="{h}" fill="url(#steel)"/>')
    o.append(f'<rect x="{xs[0]:.1f}" y="{TOP}" width="{w:.1f}" height="{h}" fill="url(#grain)" opacity=".5"/>')
    # the folded edges: a light one outside, a dark fold inside where the rail turns back
    outer = xs[1] - 1.5 if mirror else xs[0] + .5
    inner = xs[0] if mirror else xs[1] - .8 * Z
    o.append(f'<rect x="{outer:.1f}" y="{TOP}" width="1.5" height="{h}" fill="#6b6e72" opacity=".7"/>')
    o.append(f'<rect x="{inner:.1f}" y="{TOP}" width="{.8 * Z:.1f}" height="{h}" fill="#0d0d0e" opacity=".7"/>')
    # and the edge catches the light just before it
    o.append(f'<rect x="{inner - (-.8 * Z if mirror else 1.2):.1f}" y="{TOP}" width="1.2" height="{h}" fill="#55585c" opacity=".6"/>')
    sgn = -1 if mirror else 1
    hx = x0 + sgn * HC * Z  # hole centre
    nx = x0 + sgn * NUMS * Z * .5  # numbers
    hs = HOLE * Z
    for u in range(n_units):
        top = Y0 + u * U * Z
        if top > BOT:
            break
        for k, p in enumerate(PITCH):
            cy = top + p * Z
            o.append(f'<rect x="{hx - hs / 2:.1f}" y="{cy - hs / 2:.1f}" width="{hs:.1f}" height="{hs:.1f}" rx="{1.2 * Z:.1f}" fill="#050506" stroke="#000" stroke-width="1"/>')
            o.append(f'<path d="M{hx - hs / 2 + 2:.1f},{cy + hs / 2:.1f} h{hs - 4:.1f}" stroke="#5a5d61" stroke-width="1.4" opacity=".8"/>')
            if k == 1:  # the little notch by the middle hole, as on the photo
                o.append(f'<circle cx="{hx - sgn * hs / 2:.1f}" cy="{cy:.1f}" r="{2.6 * Z:.1f}" fill="#050506"/>')
        # the unit's line (not over the first: the top panel is there) and number
        if u > 0:
            o.append(f'<path d="M{x0 + sgn * 3 * Z:.1f},{top:.1f} H{hx - sgn * hs / 2:.1f}" stroke="#e6e6e6" stroke-width="{1.3 * Z:.1f}" opacity=".9"/>')
        o.append(f'<text x="{nx:.1f}" y="{top + U * Z / 2 + 4 * Z:.1f}" {F} font-weight="bold" font-size="{11 * Z:.1f}" fill="#e8e8e8" text-anchor="middle">{u + 1}</text>')


def screw(cx, cy):
    r = SCREW_R * Z
    o.append(f'<circle cx="{cx:.1f}" cy="{cy + 1.2 * Z:.1f}" r="{r + 2.5 * Z:.1f}" fill="#000" opacity=".45"/>')
    o.append(f'<circle cx="{cx:.1f}" cy="{cy:.1f}" r="{r:.1f}" fill="url(#head)" stroke="#1a1b1c" stroke-width="1.2"/>')
    a = r * .55
    for d in (f"M{cx - a:.1f},{cy:.1f} H{cx + a:.1f}", f"M{cx:.1f},{cy - a:.1f} V{cy + a:.1f}"):
        o.append(f'<path d="{d}" stroke="#151617" stroke-width="{2.2 * Z:.1f}" stroke-linecap="round"/>')
    o.append(f'<path d="M{cx - a:.1f},{cy - r * .25:.1f} A{r * .8:.1f},{r * .8:.1f} 0 0 1 {cx + a * .2:.1f},{cy - r * .78:.1f}" stroke="#fff" stroke-opacity=".45" stroke-width="1.3" fill="none"/>')


def device(v, i, u_at, cols):
    """Device i of the screenshot hung at unit u_at (0-based), held as variant v."""
    _, ear, seam, _ = VARIANTS[v]
    top = Y0 + u_at * U * Z
    fy, fh = top + GAP / 2 * Z, (U - GAP) * Z
    fx, fw = (NUMS + EXT) * Z, OPEN * Z
    sy = ROWS_TOP[i]
    sx = 391.5 + 36 * S0
    gid = f"side{v}{i}"
    stops = "".join(f'<stop offset="{k / 4}" stop-color="{c}"/>' for k, c in enumerate(cols))
    o.append(f'<defs><linearGradient id="{gid}" x1="0" y1="0" x2="0" y2="1">{stops}</linearGradient></defs>')
    if not seam:
        # the panel, as wide as the rails' holes allow, under the face: one piece with it
        o.append(f'<rect x="{fx - ear * Z:.1f}" y="{fy:.1f}" width="{fw + 2 * ear * Z:.1f}" height="{fh:.1f}" rx="{3 * Z:.1f}" fill="url(#{gid})" filter="url(#drop)"/>')
    # the face, cut from the screenshot
    o.append(f'<svg x="{fx:.1f}" y="{fy:.1f}" width="{fw:.1f}" height="{fh:.1f}" viewBox="{sx} {sy} {OPEN * S0} {U * S0}" preserveAspectRatio="none"><use href="#shot"/></svg>')
    if seam:
        for left in (True, False):
            ex = fx - ear * Z if left else fx + fw - 6 * Z
            o.append(f'<rect x="{ex:.1f}" y="{fy:.1f}" width="{(ear + 6) * Z:.1f}" height="{fh:.1f}" rx="{3 * Z:.1f}" fill="url(#{gid})" stroke="#000" stroke-opacity=".6" filter="url(#drop)"/>')
    else:
        # the panel's own edge, over the cut face
        o.append(f'<rect x="{fx - ear * Z:.1f}" y="{fy:.1f}" width="{fw + 2 * ear * Z:.1f}" height="{fh:.1f}" rx="{3 * Z:.1f}" fill="none" stroke="#000" stroke-opacity=".6"/>')
    for hx in (fx + SCREW_X * Z, fx + fw - SCREW_X * Z):
        for p in (PITCH[0], PITCH[2]):
            screw(hx, top + p * Z)


def window(v):
    o.append(f'<rect width="{WW}" height="{WH}" fill="#17181a"/>')
    n = int((BOT - Y0) / (U * Z)) + 2
    rail(0, False, n)
    rail(WW, True, n)
    # A: the ears' colour; B: the face's own, just inside its old ear, so the two are one
    cols = colours(418 if VARIANTS[v][2] else 391.5 + 40 * S0)
    for i in range(HUNG):
        device(v, i, i, cols[i])
    o.append(f'<svg x="0" y="0" width="{WW}" height="{TOP}" viewBox="0 0 {WW} {TOP}"><use href="#shot"/></svg>')
    o.append(f'<svg x="0" y="{BOT}" width="{WW}" height="{WH - BOT}" viewBox="0 {BOT} {WW} {WH - BOT}"><use href="#shot"/></svg>')


def text(x, y, s, size=26, fill="#9a9da1", bold=False, ls=0, anchor="start"):
    o.append(f'<text x="{x}" y="{y}" {F} font-size="{size}" fill="{fill}"{" font-weight=\"bold\"" if bold else ""}{f" letter-spacing=\"{ls}\"" if ls else ""} text-anchor="{anchor}" xml:space="preserve">{s}</text>')


if __name__ == "__main__":
    PAD = 60
    CU = 4.0  # the close-ups: px a unit
    cw, ch = (RAIL + 100) * CU, (1.75 * U) * CU  # a close-up's size
    total_w = PAD + len(VARIANTS) * (WW + PAD) + 1400
    y_win = 260
    y_cu = y_win + WH - 20
    total_h = y_cu + 200
    o.append(f'<rect width="{total_w}" height="{total_h}" fill="#1f2023"/>')
    text(PAD, 70, "RACK · RAILS", 40, "#e58f2c", True, 3)
    text(PAD, 115, "The window is the rack: rails at its sides from the panel to the status bar, the least zoom puts them at the edges. No frame;")
    text(PAD, 150, f"devices screwed on, 5 of them here and the units below empty. Rails {RAIL:.1f} wide each, {NUMS:.0f} of it for the numbers; {NUMS + EXT:.1f} of it beside a panel.")
    for k, v in enumerate(VARIANTS):
        title, ear, seam, note = VARIANTS[v]
        x = PAD + k * (WW + PAD)
        win_start = len(o)
        window(v)
        win = "".join(o[win_start:])
        del o[win_start:]
        text(x, y_win - 50, f"{v} · {title}", 34, "#e58f2c", True, 3)
        text(x, y_win - 14, note)
        o.append(f'<rect x="{x - 2}" y="{y_win - 2}" width="{WW + 4}" height="{WH + 4}" fill="none" stroke="#3a3c40" stroke-width="2"/>')
        o.append(f'<svg x="{x}" y="{y_win}" width="{WW}" height="{WH}" viewBox="0 0 {WW} {WH}">{win}</svg>')
        # close-ups: the last device and the empty unit under it, left and right
        cx0 = x + WW + PAD
        text(cx0, y_win - 14, "CLOSE UP, 4 px A UNIT", 26, "#c9cbcd", True, 2)
        vw, vh = cw / CU * Z, ch / CU * Z
        spots = [(0.0, TOP), (0.0, Y0 + (HUNG - 1 + .45) * U * Z), (WW - vw, Y0 + (HUNG - 1 + .45) * U * Z)]
        for j, (vx, vy) in enumerate(spots):
            # the top left corner above; the last device and an empty unit, left and right, below
            cx = cx0 if j < 2 else cx0 + cw + 60
            y_c = y_win + (0 if j == 0 else ch + 60)
            o.append(f'<svg x="{cx}" y="{y_c}" width="{cw:.0f}" height="{ch:.0f}" viewBox="{vx:.1f} {vy:.1f} {vw:.1f} {vh:.1f}">{win}</svg>')
            o.append(f'<rect x="{cx}" y="{y_c}" width="{cw:.0f}" height="{ch:.0f}" fill="none" stroke="#3a3c40" stroke-width="2"/>')
    notes = [
        "1U = 128 units (DEV_H) = 1.75″, the rest in proportion: holes 27.4 square (⅜″) at 18.3 · 64 · 109.7 from a unit's top; screws in the top and bottom one.",
        f"A 1U face 125.7 high (1.719″): 2.3 between two, on the unit grid. Least zoom here {Z / S0:.2f}× the screenshot's: {(BOT - TOP) / (U * Z):.1f} units in sight.",
        "Units numbered from the top on both rails, all the way down. Zoom: never less than rails at the window's edges; more is fine (then it scrolls sideways too).",
    ]
    for i, line in enumerate(notes):
        text(PAD, y_cu + 70 + i * 40, line, 25, "#c9cbcd")
    href = "data:image/png;base64," + base64.b64encode(open(SHOT, "rb").read()).decode()
    defs = f'''<defs>
<image id="shot" href="{href}" width="{WW}" height="{WH}"/>
<linearGradient id="steel" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#2c2d30"/><stop offset=".5" stop-color="#36373a"/><stop offset="1" stop-color="#28292b"/></linearGradient>
<filter id="noise" x="0" y="0" width="100%" height="100%"><feTurbulence type="fractalNoise" baseFrequency=".9" numOctaves="2" seed="3"/><feColorMatrix values="0 0 0 0 .6  0 0 0 0 .6  0 0 0 0 .62  0 0 0 .35 0"/></filter>
<pattern id="grain" width="300" height="300" patternUnits="userSpaceOnUse"><rect width="300" height="300" filter="url(#noise)"/></pattern>
<radialGradient id="head" cx=".38" cy=".32" r=".8"><stop offset="0" stop-color="#e4e6e8"/><stop offset=".55" stop-color="#9b9ea2"/><stop offset="1" stop-color="#55585c"/></radialGradient>
<filter id="drop" x="-30%" y="-10%" width="160%" height="130%"><feGaussianBlur in="SourceAlpha" stdDeviation="{2.5 * Z:.1f}"/><feOffset dx="0" dy="{2 * Z:.1f}"/><feComponentTransfer><feFuncA type="linear" slope=".7"/></feComponentTransfer><feMerge><feMergeNode/><feMergeNode in="SourceGraphic"/></feMerge></filter>
</defs>'''
    svg = f'<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}">{defs}{"".join(o)}</svg>\n'
    open(sys.argv[1] if len(sys.argv) > 1 else "rails.svg", "w").write(svg)
