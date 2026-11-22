# The name and screen block of a face: what each place on it shows now, and what it could show.
# The left part of a rack face (a sound card: it has every place, the PROFILE and PORT keys too) in
# the middle, 3.6 times its size, sizes as the app's (face.rs); a callout for each place.
#   python3 ident.py ident.svg && rsvg-convert ident.svg -o ident.png
import sys
import textwrap

F = 'font-family="Noto Sans"'
M = 'font-family="Noto Sans Mono"'
S = 3.6  # the face's scale here
# the app's sizes (face.rs), in face units
RACK_U = 128.0 * 1.75 / 1.719
H = 128.0
PAD, IDW = 16.0, 190.0
EAR = 17.45 + 7.2 + 10.25 + 1.5 - PAD
X0 = EAR + PAD
SCREW_X, SCREW_Y = 17.45, RACK_U * 0.25 / 1.75 - (RACK_U - H) / 2
LCD_Y, LCD_H_CARD = 54.0, 46.0
KEY_Y, KEY_H, KEY_W2 = 107.0, 14.0, 84.0
SHOWN = 300.0  # how much of the face is drawn

BOX_W = 640
FACE_X, FACE_Y = 820, 360
LEFT_X = 60
RIGHT_X = FACE_X + SHOWN * S + 120
W = RIGHT_X + BOX_W + 60

o = []


def fx(x):
    return FACE_X + x * S


def fy(y):
    return FACE_Y + y * S


def face():
    w, h = SHOWN * S, H * S
    o.append('<defs><linearGradient id="fade" x1="0" x2="1"><stop offset=".82" stop-color="#a3a5a8"/><stop offset="1" stop-color="#a3a5a8" stop-opacity="0"/></linearGradient>'
             '<linearGradient id="lcd" x1="0" x2="0" y1="0" y2="1"><stop offset="0" stop-color="#f7bb62"/><stop offset="1" stop-color="#e58f2c"/></linearGradient></defs>')
    o.append(f'<rect x="{FACE_X}" y="{FACE_Y}" width="{w}" height="{h}" rx="{3 * S}" fill="url(#fade)"/>')
    # the first frame, cut off at the right
    o.append(f'<rect x="{fx(X0 + IDW + 14)}" y="{fy(18)}" width="{(SHOWN - X0 - IDW - 14) * S}" height="{92 * S}" rx="{6 * S}" fill="#000" fill-opacity=".12" stroke="#2e8580" stroke-opacity=".5" stroke-width="4"/>')
    for sx, sy in ((SCREW_X, SCREW_Y), (SCREW_X, H - SCREW_Y)):
        cx, cy = fx(sx), fy(sy)
        o.append(f'<circle cx="{cx}" cy="{cy}" r="{7.2 * S}" fill="#c9ccd0" stroke="#333" stroke-width="3"/><path d="M{cx - 12},{cy - 12} l24,24 M{cx + 12},{cy - 12} l-24,24" stroke="#333" stroke-width="5"/>')
    # 1 the name, 2 the kind
    o.append(f'<text x="{fx(X0)}" y="{fy(33)}" {F} font-weight="900" font-size="{14 * S}" fill="#1d1e20">RYZEN HD AUDIO CO…</text>')
    o.append(f'<text x="{fx(X0)}" y="{fy(46)}" {F} font-weight="bold" font-size="{7 * S}" letter-spacing="3" fill="#2c2e31">ALSA · PCI</text>')
    # 3 hide, 4 the lamp
    hx, hy = X0 + IDW - 22, 21
    o.append(f'<rect x="{fx(hx)}" y="{fy(hy)}" width="{22 * S}" height="{14 * S}" rx="{2.5 * S}" fill="#1b1c1e"/>')
    ex, ey = fx(hx + 11), fy(hy + 7)
    o.append(f'<path d="M{ex - 18},{ey} q18,-18 36,0 q-18,18 -36,0z" fill="none" stroke="#e6e7e8" stroke-width="3"/><circle cx="{ex}" cy="{ey}" r="5" fill="#e6e7e8"/><path d="M{ex - 16},{ey + 13} L{ex + 16},{ey - 13}" stroke="#e6e7e8" stroke-width="3"/>')
    o.append(f'<circle cx="{fx(X0 + IDW - 5)}" cy="{fy(43)}" r="{3.2 * S}" fill="#2a1d0a" stroke="#000" stroke-width="2"/>')
    # the screen: its bezel, glass and three lines
    o.append(f'<rect x="{fx(X0 - 1.5)}" y="{fy(LCD_Y - 3)}" width="{(IDW + 3) * S}" height="{(LCD_H_CARD + 6) * S}" rx="{4 * S}" fill="#111214"/>')
    o.append(f'<rect x="{fx(X0 + 1.5)}" y="{fy(LCD_Y)}" width="{(IDW - 3) * S}" height="{LCD_H_CARD * S}" rx="{2 * S}" fill="url(#lcd)"/>')
    for i, (t, bold) in enumerate((("Ryzen HD Audio Controller", True), ("SUSPENDED  NO LINKS", False), ("Analog Stereo Duplex", False))):
        o.append(f'<text x="{fx(X0 + 1.5 + 6)}" y="{fy(LCD_Y + 13 + i * 13)}" {M} font-size="{10.5 * S}" {"font-weight=\"bold\"" if bold else ""} fill="#3a2408" xml:space="preserve">{t}</text>')
    # 8 the keys
    kx = X0 + (IDW - 2 * KEY_W2 - 6) / 2
    for i, name in enumerate(("PROFILE ▾", "PORT ▾")):
        x = kx + i * (KEY_W2 + 6)
        o.append(f'<rect x="{fx(x)}" y="{fy(KEY_Y)}" width="{KEY_W2 * S}" height="{KEY_H * S}" rx="{2.5 * S}" fill="#1b1c1e"/>')
        o.append(f'<text x="{fx(x + KEY_W2 / 2)}" y="{fy(KEY_Y + 10)}" {F} font-weight="bold" font-size="{7 * S}" letter-spacing="2" fill="#e6e7e8" text-anchor="middle">{name}</text>')


# (number, title, now, could be, where on the face (face units), side)
CALLOUTS = [
    (1, "NAME · printed on the plate",
     ["The device's name in capitals, cut with “…” where it doesn't fit (160 wide): RYZEN HD AUDIO CO…", "An app: the app's name. A filter or virtual device: its description (SPECTRUM, EASY EFFECTS SINK)."],
     ["The whole name, on two lines of smaller print", "Scrolling, as on the screen", "Shortened: without “Controller”, “High Definition”, “Audio”", "A name of your own (renamed in Patchcord)", "The only place for the name (the screen's line 1 freed)"],
     (X0 - 4, 28), "left"),
    (2, "KIND · small print under the name",
     ["A device: its API · bus, its form if not built in: ALSA · PCI, ALSA · USB, BLUETOOTH HEADSET", "APP · PLAYBACK, APP · RECORDING, APP · RECORD & PLAY", "VIRTUAL DEVICE, INTERNAL STREAM, MIDI, VIDEO; empty when PipeWire says nothing (SPECTRUM)"],
     ["Gone: the finish already tells (metal: hardware, black: app, cream: virtual, navy: MIDI)", "Moved onto the screen", "A small icon beside the name"],
     (X0 - 4, 43), "left"),
    (5, "SCREEN · LINE 1 (bold)",
     ["The name again, scrolling when long: the same as (1)"],
     ["The state and links (from line 2)", "The format: 48 kHz · F32 · 2 CH", "The active port: SPEAKERS, HEADPHONES", "DEFAULT OUTPUT, DEFAULT INPUT", "The name, if the plate shows only the kind"],
     (X0 + 4, LCD_Y + 9), "left"),
    (6, "SCREEN · LINE 2",
     ["The state and links: RUNNING  2 LINKS, SUSPENDED  NO LINKS; a card with nothing to show: NO INPUTS OR OUTPUTS", "For a moment after a change: VOLUME 80 %, BAL L20, the new profile, LATENCY +10 ms, DEFAULT OUTPUT"],
     ["Links only (the state on the lamp)", "Rate · quantum · latency: 48k · 1024 · 21 ms", "Xruns since it started", "How busy its driver is (CPU %)"],
     (X0 + 4, LCD_Y + 22), "left"),
    (7, "SCREEN · LINE 3",
     ["A card: its profile · codec (· UNPLUGGED), scrolling when long: A2DP Sink · AAC · High Fidelity", "Anything else: MIDI, STEREO or MONO"],
     ["The channels: 2.0, 5.1, 7.1", "A Bluetooth device's battery", "The clock it follows (its driver)"],
     (X0 + 4, LCD_Y + 35), "left"),
    (3, "HIDE key",
     ["An eye struck through: takes the device off the desk and the rack (MORE shows it again)", "Dimmed while it has cords: only an unused device can be hidden"],
     ["Smaller, beside the top screw", "Shown only while the pointer is over the face", "No key: in a right-click menu"],
     (X0 + IDW + 1, 28), "right"),
    (4, "STATUS LAMP",
     ["Green: running, amber: idle, dark: suspended", "The same word is on the screen's line 2"],
     ["The only place for the state (line 2 freed)", "Red after an xrun, for a while", "Flickering with the sound, as a signal LED"],
     (X0 + IDW - 1.5, 43), "right"),
    (9, "THE SCREEN as a whole",
     ["187 × 57, three lines of 27 characters; 46 high on a card (its keys under it)", "The name block is 190 wide on every face"],
     ["Narrower, say 150 (21 characters): about 40 more for frames on every face", "Two lines: the keys move up, beside the screen", "As wide as it is now on cards only"],
     (X0 + IDW + 1.5, LCD_Y + 30), "right"),
    (8, "PROFILE · PORT keys (cards only)",
     ["PROFILE on every card with profiles; PORT beside it when an output or input has more than one port", "Each opens a list; the screen is shorter to make room for them"],
     ["No keys: a click on line 3 opens the profiles, on the port's line the ports; the screen full height", "One SETUP key for both", "Beside the screen, as small keys"],
     (X0 + IDW - 9, KEY_Y + 7), "right"),
]


def box(x, y, c):
    n, title, now, could, *_ = c
    lines = []
    for label, items, col in (("NOW", now, "#c9cbcd"), ("COULD BE", could, "#8fd0a0")):
        lines.append(("label", label, col))
        for it in items:
            for k, part in enumerate(textwrap.wrap(it, 70)):
                lines.append(("item", ("· " if k == 0 else "  ") + part, col))
    h = 56 + len(lines) * 24 + 16
    o.append(f'<rect x="{x}" y="{y}" width="{BOX_W}" height="{h}" rx="8" fill="#26282b" stroke="#3a3c40"/>')
    o.append(f'<circle cx="{x + 30}" cy="{y + 30}" r="16" fill="#e58f2c"/><text x="{x + 30}" y="{y + 37}" {F} font-weight="bold" font-size="20" fill="#1f2023" text-anchor="middle">{n}</text>')
    o.append(f'<text x="{x + 56}" y="{y + 37}" {F} font-weight="bold" font-size="20" letter-spacing="1" fill="#e58f2c">{title}</text>')
    yy = y + 70
    for kind, t, col in lines:
        if kind == "label":
            o.append(f'<text x="{x + 20}" y="{yy}" {F} font-weight="bold" font-size="13" letter-spacing="2" fill="{col}" opacity=".7">{t}</text>')
        else:
            o.append(f'<text x="{x + 20}" y="{yy}" {F} font-size="16" fill="{col}" xml:space="preserve">{t.replace("&", "&amp;")}</text>')
        yy += 24
    return h


def column(cs, x, side, y0):
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
    le, ly = column(left, LEFT_X, "left", 150)
    re, ry = column(right, RIGHT_X, "right", 150)
    # the leaders: from a box to its place, a dot and the number on the face
    for ends, side in ((le, "left"), (re, "right")):
        for c, by in ends:
            n, *_, (px, py), _ = c
            tx, ty = fx(px), fy(py)
            # from the box out to beside the face, then level with the place, in to it; the number
            # beside the face, the dot at the place
            bx = LEFT_X + BOX_W if side == "left" else RIGHT_X
            mid = FACE_X - 50 if side == "left" else FACE_X + SHOWN * S + 50
            o.append(f'<path d="M{bx},{by} L{(bx + mid) / 2},{by} L{mid},{ty} H{tx}" fill="none" stroke="#e58f2c" stroke-width="2" opacity=".75"/>')
            o.append(f'<circle cx="{tx}" cy="{ty}" r="5" fill="#e58f2c" stroke="#1f2023" stroke-width="1.5"/>')
            o.append(f'<circle cx="{mid}" cy="{ty}" r="14" fill="#e58f2c"/><text x="{mid}" y="{ty + 5}" {F} font-weight="bold" font-size="15" fill="#1f2023" text-anchor="middle">{n}</text>')
    Ht = max(ly, ry, fy(H) + 60) + 40
    head = f'<rect width="{W}" height="{Ht}" fill="#1f2023"/>'
    head += f'<text x="{LEFT_X}" y="62" {F} font-weight="bold" font-size="40" letter-spacing="3" fill="#e58f2c">THE NAME AND SCREEN</text>'
    head += f'<text x="{LEFT_X}" y="104" {F} font-size="22" fill="#9a9da1">What each place on a face shows now, and what it could show instead. A sound card\'s face (it has every place), 3.6 times its size.</text>'
    note = f'<text x="{FACE_X}" y="{fy(H) + 50}" {F} font-size="18" fill="#9a9da1">The name is on the plate (1) and again on the screen (5).</text>'
    open(sys.argv[1] if len(sys.argv) > 1 else "ident.svg", "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{Ht}" viewBox="0 0 {W} {Ht}">{head}{"".join(o)}{note}</svg>\n')
