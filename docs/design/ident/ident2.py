# The name and screen block, second step (after ident.py): the kind line under the name gone, the
# lamp before the name, the screen grown up into the room that freed, the name off the screen and
# the device's type on its first line. A sound card's face, 3.6 times its size, sizes as the app's.
#   python3 ident2.py ident2.svg && rsvg-convert ident2.svg -o ident2.png
import sys
import textwrap

from PIL import ImageFont

F = 'font-family="Noto Sans"'
M = 'font-family="Noto Sans Mono"'
S = 3.6
RACK_U = 128.0 * 1.75 / 1.719
H = 128.0
PAD, IDW = 16.0, 190.0
EAR = 17.45 + 7.2 + 10.25 + 1.5 - PAD
X0 = EAR + PAD
SCREW_X, SCREW_Y = 17.45, RACK_U * 0.25 / 1.75 - (RACK_U - H) / 2
TITLE = 14.0
KEY_Y, KEY_H, KEY_W2 = 107.0, 14.0, 84.0
SHOWN = 300.0

# the lamp, before the name, level with its capitals
LAMP_R = 3.2
LAMP_X, LAMP_Y = X0 + LAMP_R, 28.0
NAME_X = X0  # the lamp gone: the name where it was
# the hide key stays where it was
# the hide key: in the row of keys under the screen, always, at its left; its size as before
HIDE_X, HIDE_Y, HIDE_W = X0, 107.0, 22.0
KGAP = 6.0
# the name: the top line to itself now
NAME_W = IDW
# the screen: from just under the name (was 54) down to the keys, as before
LCD_Y, LCD_BOTTOM = 43.0, 100.0
LCD_H = LCD_BOTTOM - LCD_Y
LINE = 13.0

BOX_W = 640
FACE_X, FACE_Y = 820, 360
LEFT_X = 60
RIGHT_X = FACE_X + SHOWN * S + 120
W = RIGHT_X + BOX_W + 60

BLACK = ImageFont.truetype("/usr/share/fonts/noto/NotoSans-Black.ttf", 140)
o = []


def width(t):
    return BLACK.getlength(t) / 10


def fit(name, room):
    """As face.rs fit_title: capitals, cut with “…” to the room."""
    t = name.upper()
    if width(t) <= room:
        return t
    while t and width(t + "…") > room:
        t = t[:-1]
    return t.rstrip() + "…"


def fx(x):
    return FACE_X + x * S


def fy(y):
    return FACE_Y + y * S


def face():
    w, h = SHOWN * S, H * S
    o.append('<defs><linearGradient id="fade" x1="0" x2="1"><stop offset=".82" stop-color="#a3a5a8"/><stop offset="1" stop-color="#a3a5a8" stop-opacity="0"/></linearGradient>'
             '<linearGradient id="lcd" x1="0" x2="0" y1="0" y2="1"><stop offset="0" stop-color="#f7bb62"/><stop offset="1" stop-color="#e58f2c"/></linearGradient>'
             '<radialGradient id="lit"><stop offset="0" stop-color="#d6ffe0"/><stop offset=".45" stop-color="#4cff6a"/><stop offset="1" stop-color="#1f8a33"/></radialGradient></defs>')
    o.append(f'<rect x="{FACE_X}" y="{FACE_Y}" width="{w}" height="{h}" rx="{3 * S}" fill="url(#fade)"/>')
    o.append(f'<rect x="{fx(X0 + IDW + 14)}" y="{fy(18)}" width="{(SHOWN - X0 - IDW - 14) * S}" height="{92 * S}" rx="{6 * S}" fill="#000" fill-opacity=".12" stroke="#2e8580" stroke-opacity=".5" stroke-width="4"/>')
    for sx, sy in ((SCREW_X, SCREW_Y), (SCREW_X, H - SCREW_Y)):
        cx, cy = fx(sx), fy(sy)
        o.append(f'<circle cx="{cx}" cy="{cy}" r="{7.2 * S}" fill="#c9ccd0" stroke="#333" stroke-width="3"/><path d="M{cx - 12},{cy - 12} l24,24 M{cx + 12},{cy - 12} l-24,24" stroke="#333" stroke-width="5"/>')
    # 1 the name (4, the lamp, gone)
    o.append(f'<text x="{fx(NAME_X)}" y="{fy(33)}" {F} font-weight="900" font-size="{TITLE * S}" fill="#1d1e20">{fit("Ryzen HD Audio Controller", NAME_W)}</text>')
    # 3 hide, at the left of the keys' row
    o.append(f'<rect x="{fx(HIDE_X)}" y="{fy(HIDE_Y)}" width="{HIDE_W * S}" height="{14 * S}" rx="{2.5 * S}" fill="#1b1c1e"/>')
    ex, ey = fx(HIDE_X + 11), fy(HIDE_Y + 7)
    o.append(f'<path d="M{ex - 18},{ey} q18,-18 36,0 q-18,18 -36,0z" fill="none" stroke="#e6e7e8" stroke-width="3"/><circle cx="{ex}" cy="{ey}" r="5" fill="#e6e7e8"/><path d="M{ex - 16},{ey + 13} L{ex + 16},{ey - 13}" stroke="#e6e7e8" stroke-width="3"/>')
    # 9 the screen, taller: 5 the type, 6 and 7 as before, a free line under them
    o.append(f'<rect x="{fx(X0 - 1.5)}" y="{fy(LCD_Y - 3)}" width="{(IDW + 3) * S}" height="{(LCD_H + 6) * S}" rx="{4 * S}" fill="#111214"/>')
    o.append(f'<rect x="{fx(X0 + 1.5)}" y="{fy(LCD_Y)}" width="{(IDW - 3) * S}" height="{LCD_H * S}" rx="{2 * S}" fill="url(#lcd)"/>')
    for i, (t, bold) in enumerate((("SOUND CARD", True), ("2 LINKS", False), ("2 NODES  6 PORTS", False), ("Analog Stereo Duplex", False))):
        o.append(f'<text x="{fx(X0 + 1.5 + 6)}" y="{fy(LCD_Y + 13 + i * LINE)}" {M} font-size="{10.5 * S}" {"font-weight=\"bold\"" if bold else ""} fill="#3a2408" xml:space="preserve">{t}</text>')
    # the state on line 1, against the screen's right edge
    o.append(f'<text x="{fx(X0 + IDW - 1.5 - 6)}" y="{fy(LCD_Y + 13)}" {M} font-size="{10.5 * S}" font-weight="bold" fill="#3a2408" text-anchor="end">RUNNING</text>')
    # 8 the keys
    # 8 PROFILE and PORT share the rest of the row evenly
    keys = ("PROFILE ▾", "PORT ▾")
    x0 = HIDE_X + HIDE_W + KGAP
    kw = (X0 + IDW - x0 - KGAP * (len(keys) - 1)) / len(keys)
    for i, name in enumerate(keys):
        x = x0 + i * (kw + KGAP)
        o.append(f'<rect x="{fx(x)}" y="{fy(KEY_Y)}" width="{kw * S}" height="{KEY_H * S}" rx="{2.5 * S}" fill="#1b1c1e"/>')
        o.append(f'<text x="{fx(x + kw / 2)}" y="{fy(KEY_Y + 10)}" {F} font-weight="bold" font-size="{7 * S}" letter-spacing="2" fill="#e6e7e8" text-anchor="middle">{name}</text>')


# (number, title, what it shows, still open, where on the face (face units), side)
CALLOUTS = [
    (1, "NAME · printed on the plate",
     ["As before (left as it is), where it was", f"The lamp and the hide key gone from its line: room {NAME_W:.0f} (was 160): {fit('Ryzen HD Audio Controller', NAME_W)}"],
     [],
     (NAME_X + 40, 37), "left"),
    (5, "SCREEN · LINE 1 · THE TYPE AND THE STATE",
     ["The device's type, one word or two, nothing after it: what it is, not how it is connected",
      "Hardware: SOUND CARD, HDMI AUDIO, HEADSET, HEADPHONES, SPEAKER, MICROPHONE",
      "Made in software: VIRTUAL SINK, VIRTUAL SOURCE, FILTER, LOOPBACK, NETWORK",
      "And: APP, MIDI, CAMERA",
      "The state against the right edge: RUNNING, IDLE, SUSPENDED, ERROR (PipeWire's node state; a device of several nodes: the busiest)",
      "The longest fits: VIRTUAL SOURCE … SUSPENDED is 23 of 27 characters",
      "From the Device (device.form-factor, else SOUND CARD; HDMI from the profile); with none, from the nodes (media.class, factory.name, node.link-group)"],
     [],
     (X0 + 4, LCD_Y + 9), "left"),
    (6, "SCREEN · LINE 2 · the links",
     ["The links only: 2 LINKS, NO LINKS (the state moved up to line 1)", "For a moment after a change: VOLUME 80 %, BAL L20, the new profile, LATENCY +10 ms, DEFAULT OUTPUT"],
     ["Rate · quantum · latency: 48k · 1024 · 21 ms", "Xruns since it started", "How busy its driver is (CPU %)"],
     (X0 + 4, LCD_Y + 22), "left"),
    (10, "SCREEN · LINE 3 · what it takes of PipeWire",
     ["On every device: how many PipeWire nodes and ports are behind its face", "A card: 2 NODES  6 PORTS (a sink with its monitor, a source)",
      "An app of one stream: 1 NODE  2 PORTS; Easy Effects under one roof: 7 NODES  26 PORTS"],
     [],
     (X0 + 4, LCD_Y + 35), "left"),
    (7, "SCREEN · LINE 4 · what was line 3",
     ["A card: its profile · codec (· UNPLUGGED), scrolling when long", "Anything else: MIDI, STEREO or MONO"],
     ["The channels: 2.0, 5.1, 7.1", "A Bluetooth device's battery", "The clock it follows (its driver)"],
     (X0 + 4, LCD_Y + 48), "left"),
    (3, "HIDE key · moved into the keys' row",
     ["An eye struck through: takes the device off the desk and the rack", "Dimmed while it has cords",
      "Always there, on every face, always at the left of the row; the same size (22 × 14)"],
     [],
     (HIDE_X + 11, KEY_Y + KEY_H + 2.5), "right"),
    (9, "THE SCREEN · grown up",
     [f"From y {LCD_Y:.0f} (was 54): {LCD_H:.0f} high (was 46 on a card, 57 elsewhere)",
      "The same on every face: every face has the keys' row now", "Four lines (were three); still 187 wide, 27 characters"],
     ["Narrower, say 150 (21 characters): about 40 more for frames on every face"],
     (X0 + IDW + 1.5, LCD_Y + 30), "right"),
    (8, "PROFILE · PORT keys (cards only)",
     ["PROFILE on every card with profiles; PORT beside it when an output or input has more than one port",
      "They share what the hide key leaves of the row evenly: 78 each, or one key 162 wide",
      "A face without them (an app, a virtual device): the hide key alone"],
     ["No keys: a click on line 3 opens the profiles; the screen full height", "One SETUP key for both"],
     (X0 + IDW - 9, KEY_Y + 7), "right"),
    ("4", "STATUS LAMP · GONE",
     ["Green, amber or dark beside the hide key: the same as the state word", "The state is on the screen's line 1 now (5)"],
     [],
     None, "right"),
    ("2", "KIND under the name · GONE",
     ["ALSA · PCI, APP · PLAYBACK… were here, between the name and the screen", "The screen took the room; the type moved onto it (5)", "The model stays in the app: the status bar on hover, the HIDDEN list"],
     [],
     None, "right"),
]


def box(x, y, c):
    n, title, now, could, *_ = c
    gone = c[4] is None
    lines = []
    for label, items, col in (("SHOWS", now, "#c9cbcd"), ("STILL OPEN", could, "#8fd0a0")):
        if not items:
            continue
        lines.append(("label", label, col))
        for it in items:
            for k, part in enumerate(textwrap.wrap(it, 70)):
                lines.append(("item", ("· " if k == 0 else "  ") + part, col))
    h = 56 + len(lines) * 24 + 16
    o.append(f'<rect x="{x}" y="{y}" width="{BOX_W}" height="{h}" rx="8" fill="#26282b" stroke="#3a3c40"{" stroke-dasharray=\"8 6\" opacity=\".75\"" if gone else ""}/>')
    o.append(f'<circle cx="{x + 30}" cy="{y + 30}" r="16" fill="{"#6a6c70" if gone else "#e58f2c"}"/><text x="{x + 30}" y="{y + 37}" {F} font-weight="bold" font-size="{17 if len(str(n)) > 1 else 20}" fill="#1f2023" text-anchor="middle">{n}</text>')
    o.append(f'<text x="{x + 56}" y="{y + 37}" {F} font-weight="bold" font-size="20" letter-spacing="1" fill="{"#9a9da1" if gone else "#e58f2c"}">{title}</text>')
    yy = y + 70
    for kind, t, col in lines:
        if kind == "label":
            o.append(f'<text x="{x + 20}" y="{yy}" {F} font-weight="bold" font-size="13" letter-spacing="2" fill="{col}" opacity=".7">{t}</text>')
        else:
            o.append(f'<text x="{x + 20}" y="{yy}" {F} font-size="16" fill="{col}" xml:space="preserve">{t.replace("&", "&amp;")}</text>')
        yy += 24
    return h


def column(cs, x, y0):
    y = y0
    ends = []
    for c in cs:
        h = box(x, y, c)
        ends.append((c, y + 30))
        y += h + 24
    return ends, y


if __name__ == "__main__":
    face()
    left = [c for c in CALLOUTS if c[5] == "left"]
    right = [c for c in CALLOUTS if c[5] == "right"]
    le, ly = column(left, LEFT_X, 150)
    re, ry = column(right, RIGHT_X, 150)
    for ends, side in ((le, "left"), (re, "right")):
        for c, by in ends:
            n, *_, at, _ = c
            if at is None:
                continue
            tx, ty = fx(at[0]), fy(at[1])
            bx = LEFT_X + BOX_W if side == "left" else RIGHT_X
            mid = FACE_X - 50 if side == "left" else FACE_X + SHOWN * S + 50
            o.append(f'<path d="M{bx},{by} L{(bx + mid) / 2},{by} L{mid},{ty} H{tx}" fill="none" stroke="#e58f2c" stroke-width="2" opacity=".75"/>')
            o.append(f'<circle cx="{tx}" cy="{ty}" r="5" fill="#e58f2c" stroke="#1f2023" stroke-width="1.5"/>')
            o.append(f'<circle cx="{mid}" cy="{ty}" r="14" fill="#e58f2c"/><text x="{mid}" y="{ty + 5}" {F} font-weight="bold" font-size="{13 if len(str(n)) > 1 else 15}" fill="#1f2023" text-anchor="middle">{n}</text>')
    Ht = max(ly, ry, fy(H) + 60) + 40
    head = f'<rect width="{W}" height="{Ht}" fill="#1f2023"/>'
    head += f'<text x="{LEFT_X}" y="62" {F} font-weight="bold" font-size="40" letter-spacing="3" fill="#e58f2c">THE NAME AND SCREEN · STEP 2</text>'
    head += f'<text x="{LEFT_X}" y="104" {F} font-size="22" fill="#9a9da1">The kind line (2) and the lamp (4) gone, the screen grown up (9), the type and the state on its first line (5), the hide key into the keys\' row (3), PipeWire\'s nodes and ports on line 3 (10). A sound card\'s face, 3.6 times its size.</text>'
    note = f'<text x="{FACE_X}" y="{fy(H) + 50}" {F} font-size="18" fill="#9a9da1">The name is only on the plate; the screen starts with the type and the state.</text>'
    open(sys.argv[1] if len(sys.argv) > 1 else "ident2.svg", "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{Ht}" viewBox="0 0 {W} {Ht}">{head}{"".join(o)}{note}</svg>\n')
