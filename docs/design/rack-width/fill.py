# The rack as wide as its widest device (fit.png, from the app's rack_picture at 1200 x 900):
# what goes where a face's frames end, instead of the vents. Five ways, made from fit.png itself
# (the vents found by their slots and painted over with the face's own metal), each a picture
# the size of fit.png:
#   A  plain metal
#   B  the device only as wide as it needs, a blank panel beside it to the rail
#   C  two narrow devices side by side in one unit, a blank panel for what is left
#   D  the frames stretched to the face's width
#   E  plain metal, the kind of device printed small at its right
#   python3 fill.py   (writes fill-a.png ... fill-e.png next to it)
import os
import random

from PIL import Image, ImageDraw, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = Image.open(os.path.join(HERE, "fit.png")).convert("RGB")
ORIGINAL = SRC              # where the vents are is found on it (its slots' colour)
W, H = SRC.size
TOP, U = 62, 110            # the first unit's top, a unit's pitch
L, R = 20, 1180             # a face's left and right edges (the rails outside)
SLOT = (11, 12, 13)         # a vent slot's colour
BOLD = "/usr/share/fonts/noto/NotoSans-Bold.ttf"
# the demo's devices, top to bottom, and what each is (as its screen says)
KINDS = ["APPLICATION", "APPLICATION", "VIRTUAL SOURCE", "APPLICATION", "APPLICATION", "VIRTUAL SINK"]
random.seed(7)


def vents(im, k):
    """Unit k's vents: (x0, y0, x1, y1), or None (its frames take the whole face)."""
    y0, y1 = TOP + k * U, TOP + k * U + U - 2
    im = ORIGINAL
    rows = [y for y in range(y0, y1) if sum(im.getpixel((x, y)) == SLOT for x in range(600, 1100, 10)) > 40]
    if not rows:
        return None
    y = rows[0]
    xs = [x for x in range(L, R) if im.getpixel((x, y)) == SLOT]
    # (a slot's round ends and its light edge below)
    return (xs[0] - 3, rows[0] - 2, xs[-1] + 4, rows[-1] + 4)


def metal(im, k, box, src_x):
    """Paints `box` with unit k's own metal: its shade down the face (from a few columns at
    `src_x` that have nothing on them, smoothed) and its grain."""
    x0, y0, x1, y1 = box
    px = im.load()
    cols = range(src_x, src_x + 4)
    clamp = lambda y: max(0, min(im.height - 1, y))
    means = [[sum(px[x, clamp(y)][i] for x in cols) / len(cols) for i in range(3)] for y in range(y0 - 3, y1 + 3)]
    sd = (sum((px[x, y][0] - means[y - y0 + 3][0]) ** 2 for y in range(y0, y1) for x in cols) / ((y1 - y0) * len(cols))) ** 0.5
    sd = min(sd, 5.0)
    for y in range(y0, y1):
        win = means[y - y0:y - y0 + 7]
        m = [sum(w[i] for w in win) / len(win) for i in range(3)]
        for x in range(x0, x1):
            n = random.gauss(0, sd)
            px[x, y] = tuple(max(0, min(255, int(m[i] + n))) for i in range(3))


def plain(im):
    for k in range(6):
        v = vents(im, k)
        if v:
            metal(im, k, v, v[0] - 5)
    return im


def blank(w, h, ref):
    """A blank panel w x h: black, its rim, screws at its right end (where the rail is)."""
    p = Image.new("RGB", (w, h))
    d = ImageDraw.Draw(p)
    for y in range(h):
        g = int(44 - 14 * y / h)
        d.line([(0, y), (w, y)], fill=(g, g + 1, g + 3))
    for x in range(w):
        for y in range(h):
            if random.random() < .5:
                c = p.getpixel((x, y))
                n = random.randint(-5, 5)
                p.putpixel((x, y), tuple(max(0, c[i] + n) for i in range(3)))
    d.rounded_rectangle([0, 0, w - 1, h - 1], radius=3, outline=(9, 9, 9), width=1)
    d.line([(2, 1), (w - 3, 1)], fill=(80, 81, 84))
    # its screws, at the rail
    for cy in (14, h - 15):
        cx = w - 18
        d.ellipse([cx - 7, cy - 7, cx + 7, cy + 7], fill=(150, 152, 156), outline=(20, 20, 22))
        d.line([(cx - 4, cy), (cx + 4, cy)], fill=(40, 41, 44), width=2)
        d.line([(cx, cy - 4), (cx, cy + 4)], fill=(40, 41, 44), width=2)
    return p


def narrow(im, k):
    """Unit k's face only as wide as it needs: its left part and its right ear."""
    v = vents(SRC, k)
    y0 = TOP + k * U
    face = im.crop((L, y0, R, y0 + U - 2))
    if not v:
        return face
    left = face.crop((0, 0, v[0] - L - 4, U - 2))
    ear = face.crop((R - L - 34, 0, R - L, U - 2))
    out = Image.new("RGB", (left.width + ear.width, U - 2))
    out.paste(left, (0, 0))
    out.paste(ear, (left.width, 0))
    return out


def rails_only():
    """fit.png with no faces on it: the rails and the rack's back."""
    im = SRC.copy()
    back = SRC.crop((L, TOP + 6 * U + 4, R, TOP + 6 * U + 6)).resize((R - L, 1))
    for y in range(TOP, H):
        if y >= TOP + 6 * U:
            break
        im.paste(back, (L, y))
    return im


def a():
    return plain(SRC.copy())


def b():
    im = plain(SRC.copy())
    for k in range(6):
        f = narrow(im, k)
        if f.width == R - L:
            continue
        y0 = TOP + k * U
        im.paste(rails_only().crop((L, y0, R, y0 + U - 2)), (L, y0))
        im.paste(f, (L, y0))
        im.paste(blank(R - L - f.width - 2, U - 2, SRC), (L + f.width + 2, y0))
    return im


def c():
    im = rails_only()
    faces = [narrow(SRC, k) for k in range(6)]
    # in the rack's order, each unit taking as many as fit side by side
    rows, row = [], []
    for f in faces:
        if row and sum(g.width + 2 for g in row) + f.width > R - L:
            rows.append(row)
            row = []
        row.append(f)
    rows.append(row)
    for k, row in enumerate(rows):
        x, y0 = L, TOP + k * U
        for f in row:
            im.paste(f, (x, y0))
            x += f.width + 2
        if R - x > 40:
            im.paste(blank(R - x, U - 2, SRC), (x, y0))
    return im


def d():
    im = SRC.copy()
    for k in range(6):
        v = vents(SRC, k)
        if not v:
            continue
        y0 = TOP + k * U
        fe = v[0] - 10                      # the frame's right edge
        cut = fe - 29                       # a column between its labels and its meter
        face = im.crop((L, y0, R, y0 + U - 2))
        grow = (R - 44) - fe                # to the right ear
        out = Image.new("RGB", face.size)
        out.paste(face.crop((0, 0, cut - L, U - 2)), (0, 0))
        col = face.crop((cut - L, 0, cut - L + 1, U - 2))
        for i in range(grow):
            out.paste(col, (cut - L + i, 0))
        out.paste(face.crop((cut - L, 0, fe - L + 2, U - 2)), (cut - L + grow, 0))
        out.paste(face.crop((R - 34 - L, 0, R - L, U - 2)), (R - 34 - L, 0))
        metal(out, k, (R - 44 - L + 2, 0, R - 34 - L, U - 2), R - 32 - L)
        im.paste(out, (L, y0))
    return im


def e():
    im = plain(SRC.copy())
    d_ = ImageDraw.Draw(im)
    font = ImageFont.truetype(BOLD, 11)
    for k in range(6):
        v = vents(SRC, k)
        if not v:
            continue
        text = " ".join(KINDS[k])
        w = d_.textlength(text, font=font)
        y = TOP + k * U + 10
        d_.text((R - 48 - w, y), text, font=font, fill=(222, 223, 218))
    return im


for name, f in [("a", a), ("b", b), ("c", c), ("d", d), ("e", e)]:
    f().save(os.path.join(HERE, f"fill-{name}.png"))
