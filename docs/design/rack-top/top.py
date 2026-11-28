# The unit beside a face's name and screen. Today (ADR 0004, variant E) the lower units fill
# first, across the whole width, and the top unit takes only what is left over; so a face of many
# frames can keep its top unit all vents (a Pro Audio card: a MASTER and eight AUX frames, the
# name cut short over an empty unit), and a 2U face is its name over a row of frames, the name
# cut at the screen's width though the unit beside it is empty.
#
# Made from screenshots of the live rack (spec-proaudio.png, spec-2u.png): the frames are the
# program's own pictures, moved; the vents are the screenshot's own hole, stamped on its own grid.
#   A  the top unit takes MASTER, the units under it two AUX each
#   B  the top unit takes what fits (MASTER and AUX0), the rest follow on
#   C  the top unit stays empty: the name runs on over it to the right, no vents under it
#   D  C on a 2U face
#   python3 -I top.py   (writes a.png ... d.png next to it; needs Pillow)
import os
import random

from PIL import Image, ImageDraw, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
FONT = os.path.join(HERE, "..", "..", "..", "assets", "fonts", "NotoSans-Black.ttf")

PITCH = 187          # a unit's pitch in the screenshot
TOP_Y = 17           # a frame's top in the top unit (a lower unit's, a pitch up)
ROOM_X = 465         # where the room beside the name and screen starts
LEFT = 106           # a lower unit's first frame
GAP = 12             # between two frames
RIGHT = 1162         # the vents' right end
BOTTOM = 932         # the last unit shown (the next node's frames start under it)
NEAR = 14            # vents keep this far from what is on the face

# the Pro Audio face's frames in the screenshot, with their outline (x0, y0, x1, y1)
FRAMES = {
    "MASTER": (106, 204, 345, 366),
    "AUX0": (356, 204, 715, 366),
    "AUX1": (727, 204, 1086, 366),
    "AUX2": (106, 391, 465, 553),
    "AUX3": (477, 391, 836, 553),
    "AUX4": (106, 578, 465, 740),
    "AUX5": (477, 578, 836, 740),
    "AUX6": (106, 765, 465, 927),
    "AUX7": (477, 765, 836, 927),
}


def dark(p):
    return p[0] + p[1] + p[2] < 75


def cover(im, rect, metal):
    """Plain metal over `rect`, in pieces cut at random from `metal`, so the grain goes on."""
    rnd = random.Random(rect[0] * 7919 + rect[1])
    x0, y0, x1, y1 = rect
    sw, sh = metal.size
    t = 12
    for y in range(y0, y1, t):
        for x in range(x0, x1, t):
            w, h = min(t, x1 - x), min(t, y1 - y)
            sx, sy = rnd.randrange(0, sw - t), rnd.randrange(0, sh - t)
            im.paste(metal.crop((sx, sy, sx + w, sy + h)), (x, y))


def holes(im, area):
    """The whole holes that start in `area`: each one's box."""
    px = im.load()
    W, H = im.size
    seen = set()
    out = []
    x0, y0, x1, y1 = area
    for y in range(y0, y1):
        for x in range(x0, x1):
            if (x, y) in seen or not dark(px[x, y]):
                continue
            stack = [(x, y)]
            seen.add((x, y))
            bx0, by0, bx1, by1 = x, y, x, y
            while stack:
                cx, cy = stack.pop()
                bx0, by0, bx1, by1 = min(bx0, cx), min(by0, cy), max(bx1, cx), max(by1, cy)
                for nx, ny in ((cx + 1, cy), (cx - 1, cy), (cx, cy + 1), (cx, cy - 1)):
                    if 0 <= nx < W and 0 <= ny < H and (nx, ny) not in seen and dark(px[nx, ny]):
                        seen.add((nx, ny))
                        stack.append((nx, ny))
            if 12 <= bx1 - bx0 <= 34 and 12 <= by1 - by0 <= 34:
                out.append((bx0, by0, bx1 + 1, by1 + 1))
    return out


def near(box, rects):
    x0, y0, x1, y1 = box
    return any(x1 > r[0] - NEAR and x0 < r[2] + NEAR and y1 > r[1] - NEAR and y0 < r[3] + NEAR for r in rects)


class Comb:
    """The screenshot's honeycomb: one hole (with its light lip) and the grid it sits on, read
    from a stretch of the field that is whole (the column right of the AUX frames)."""

    def __init__(self, im, area):
        found = holes(im, area)
        rows = {}
        for b in found:
            rows.setdefault(b[1], []).append(b[0])
        ys = sorted(rows)
        mid = lambda v: sorted(v)[len(v) // 2]
        self.dy = mid([b - a for a, b in zip(ys, ys[1:])])
        xs = sorted(rows[ys[0]])
        self.dx = mid([b - a for a, b in zip(xs, xs[1:])])
        # a row's first hole, two rows: the grid is theirs, on and on by dx and dy
        self.ref = (xs[0], ys[0])
        self.shift = (sorted(rows[ys[1]])[0] - xs[0]) % self.dx
        b = found[0]
        self.w, self.h = b[2] - b[0], b[3] - b[1]
        self.sprite = im.crop((b[0] - 2, b[1] - 2, b[2] + 3, b[3] + 4))

    def stamp(self, im, field, keep_off):
        """Holes over `field` (x0, y0, x1, y1) on the grid, whole ones only, none near `keep_off`."""
        fx0, fy0, fx1, fy1 = field
        rx, ry = self.ref
        k0 = -int((ry - fy0) / self.dy) - 1
        for k in range(k0, k0 + 400):
            y = round(ry + k * self.dy)
            if y + self.h > fy1:
                break
            if y < fy0:
                continue
            x = rx + (self.shift if k % 2 else 0)
            x -= self.dx * int((x - fx0) / self.dx + 1)
            while x < fx0:
                x += self.dx
            while x + self.w <= fx1:
                box = (round(x), y, round(x) + self.w, y + self.h)
                if not near(box, keep_off):
                    im.paste(self.sprite, (box[0] - 2, box[1] - 2))
                x += self.dx


def relay(src, comb, rows):
    """The Pro Audio face with its frames laid as `rows` (a unit's frames, from the left; the top
    unit's from ROOM_X), vents in all the room left over."""
    im = src.copy()
    metal = src.crop((120, 368, 700, 388))
    boxes = []
    for u, names in enumerate(rows):
        x = ROOM_X if u == 0 else LEFT
        top = TOP_Y + u * PITCH
        for n in names:
            r = FRAMES[n]
            boxes.append((x, top, x + r[2] - r[0], top + r[3] - r[1], n))
            x += r[2] - r[0] + GAP
    # everything beside and under the name and screen bare, then the vents, then the frames
    cover(im, (ROOM_X - 4, 0, RIGHT + 6, 186), metal)
    cover(im, (LEFT - 6, 186, RIGHT + 6, BOTTOM + 4), metal)
    name_screen = (0, 0, ROOM_X - NEAR - 1, 186)
    comb.stamp(im, (LEFT, 0, RIGHT, BOTTOM), [b[:4] for b in boxes] + [name_screen])
    for (x0, y0, _, _, n) in boxes:
        im.paste(src.crop(FRAMES[n]), (x0, y0))
    return im


def long_name(src, old, text, colour, plain):
    """The name in full, on to the right over an empty top unit; the holes under it and NEAR
    round it out."""
    im = src.copy()
    metal = src.crop(plain)
    font = ImageFont.truetype(FONT, 23)
    x0, y0, x1, y1 = old
    cover(im, (x0 - 3, y0 - 4, x1 + 4, y1 + 5), metal)
    text_box = (x0, y0, x0 + int(font.getlength(text)), y1)
    for b in holes(im, (x0, 0, im.size[0] - 40, y1 + 40)):
        if near(b, [text_box]):
            cover(im, (b[0] - 3, b[1] - 3, b[2] + 3, b[3] + 4), metal)
    ImageDraw.Draw(im).text((x0, y0 - 9), text, font=font, fill=colour)
    return im


def main():
    pro = Image.open(os.path.join(HERE, "spec-proaudio.png")).convert("RGB")
    two = Image.open(os.path.join(HERE, "spec-2u.png")).convert("RGB")
    comb = Comb(pro, (850, 394, 1150, 700))
    relay(pro, comb, [["MASTER"], ["AUX0", "AUX1"], ["AUX2", "AUX3"], ["AUX4", "AUX5"], ["AUX6", "AUX7"]]).save(os.path.join(HERE, "a.png"))
    relay(pro, comb, [["MASTER", "AUX0"], ["AUX1", "AUX2"], ["AUX3", "AUX4"], ["AUX5", "AUX6"], ["AUX7"]]).save(os.path.join(HERE, "b.png"))
    long_name(pro, (111, 29, 416, 45), "RADEON HIGH DEFINITION AUDIO CONTROLLER", (219, 226, 237), (120, 368, 700, 388)).save(os.path.join(HERE, "c.png"))
    long_name(two, (73, 36, 389, 51), "RYZEN HD AUDIO CONTROLLER", (202, 209, 219), (500, 188, 840, 205)).save(os.path.join(HERE, "d.png"))


if __name__ == "__main__":
    main()
