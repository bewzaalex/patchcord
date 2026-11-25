# A card's frames: what each is called (issue D). Now a frame is named by the port in use (its
# route: SPEAKERS, HEADPHONES), else SOUND OUT / SOUND IN, else INPUT / OUTPUT. On a laptop where
# every port is a node of its own (UCM, the HiFi profile) that gives three frames called HDMI and
# two called MIC; with no routes (a report saved before STATE) every frame is INPUT or OUTPUT.
# What else PipeWire names a node with: node.nick, node.description, device.profile.description.
# Drawn from two real cards: a Fedora report's Meteor Lake (HiFi), and this machine's Ryzen (ACP).
#   python3 names.py names.svg && rsvg-convert -z .5 names.svg -o names.png
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "apps"))
import nodes as n  # noqa: E402
from hwframes import DEFS  # noqa: E402
from nodes import F, M, N, P, Z, VOL, GAIN, ST, text, o  # noqa: E402

ORANGE, GREY, BODY = "#e58f2c", "#9a9da1", "#c9cbcd"
GOOD, BAD = "#5fb36a", "#d2574a"


def sink(fid, name, nid):
    n.NID[fid] = nid
    return N(fid, name, "Audio/Sink", Z("in", "PLAYBACK", P("st", ST, 1)), Z("mon", "MONITOR", P("st", ST, 1)), mix=VOL(.6))


def source(fid, name, nid, stereo=True):
    n.NID[fid] = nid
    return N(fid, name, "Audio/Source", Z("out", "CAPTURE", P("st", ST if stereo else "MONO", 1)), mix=GAIN(.5, stereo))


def card(name, kind, sinks, sources):
    count = len(sinks) + len(sources)
    return {"name": name, "roof": "HARDWARE", "kind": kind, "res": f"{count} NODES", "c": "#7B4FB8",
            "blocks": [[f] for f in sinks + sources]}


def label(y, t, colour):
    """NOW / A / B at the left of a rack unit, over its screws."""
    o.append(f'<rect x="8" y="{y + 8}" width="64" height="26" rx="4" fill="{colour}"/>')
    text(40, y + 26, t, 13, "#101113", True, "middle", 'letter-spacing="1"')


def facts(x, y, rows):
    """A table of what PipeWire says about each node: a mono font, one row a node."""
    for k, row in enumerate(rows):
        text(x, y + k * 19, row, 12.5, BODY if k else GREY, k == 0, font=M)
    return y + len(rows) * 19


def case(y, title, sub, table, versions):
    text(60, y, title, 22, ORANGE, True, extra='letter-spacing="1.5"')
    for k, s in enumerate(sub):
        text(60, y + 28 + k * 21, s, 15, GREY)
    y = facts(60, y + 28 + len(sub) * 21 + 12, table) + 18
    for tag, colour, why, dev in versions:
        text(n.RX - 34, y + 4, why, 14, colour, True)
        y += 14
        label(y, tag, colour)
        y += n.device2(y, dev) + 26
    return y + 30


# --- 1 · Fedora's laptop, Meteor Lake, the HiFi profile: a node a port --------------------------
# (from a report: node.nick, node.description, the profile's description, the route in use)
ML = "METEOR LAKE-P HD AUDIO CONTROLLER"
ML_KIND = "CARD · HIFI"
ML_TABLE = [
    "NODE  MEDIA.CLASS   ROUTE (PORT IN USE)  NODE.NICK           NODE.DESCRIPTION",
    "62    Audio/Sink    [Out] Headphones     Headphones          Headphones   (renamed in KDE)",
    "63    Audio/Sink    [Out] HDMI3          HDMI 3              Meteor Lake-P HD Audio Controller HDMI / DisplayPort 3 Output (Stereo)",
    "82    Audio/Sink    [Out] HDMI1          HDMI 1              Meteor Lake-P HD Audio Controller HDMI / DisplayPort 1 Output (Stereo)",
    "83    Audio/Sink    [Out] HDMI2          HDMI 2              Meteor Lake-P HD Audio Controller HDMI / DisplayPort 2 Output (Stereo)",
    "80    Audio/Source  [In] Mic1            Digital Microphone  Meteor Lake-P HD Audio Controller Digital Microphone",
    "81    Audio/Source  [In] Mic2            Headphones Mic      Headphones Mic   (renamed in KDE)",
]


def meteor(names):
    s = [sink("ml62", names[0], 62), sink("ml63", names[1], 63), sink("ml82", names[2], 82), sink("ml83", names[3], 83)]
    c = [source("ml80", names[4], 80), source("ml81", names[5], 81)]
    for f in s + c:  # each its own id, so the three drawings don't share jacks
        nid = n.NID.pop(f["id"])
        f["id"] += "_" + names[0][:3] + names[1][:3]
        n.NID[f["id"]] = nid
    return card(ML, ML_KIND, s, c)


# --- 2 · this machine's Ryzen card, the ACP profile: one sink, its ports a choice -------------
RY = "RYZEN HD AUDIO CONTROLLER"
RY_KIND = "CARD · ANALOG STEREO DUPLEX"
RY_TABLE = [
    "NODE  MEDIA.CLASS   ROUTE (PORT IN USE)            NODE.NICK      NODE.DESCRIPTION / PROFILE",
    "51    Audio/Sink    analog-output-speaker (of 2)   ALC287 Analog  Ryzen HD Audio Controller Analog Stereo / Analog Stereo",
    "45    Audio/Source  analog-input-internal-mic      ALC287 Analog  Ryzen HD Audio Controller Analog Stereo / Analog Stereo",
]


def ryzen(out, inp, tag):
    s, c = sink("ry51" + tag, out, 51), source("ry45" + tag, inp, 45)
    return card(RY, RY_KIND, [s], [c])


def main(argv):
    W = 2100
    y = 190
    y = case(y, "1 · ONE NODE A PORT: A LAPTOP'S CARD IN ITS HIFI PROFILE (FEDORA, A REPORT)", [
        "Every port is a node of its own, with one route each. The port's word alone says HDMI three times and MIC twice.",
        "node.nick tells them apart, and it is where KDE writes a name the user gave (System Settings → Sound → rename): 62 and 81 here.",
    ], ML_TABLE, [
        ("NOW", BAD, "NOW, LIVE: THE PORT'S WORD. THREE HDMI, TWO MIC: WHICH IS WHICH?", meteor(["HEADPHONES", "HDMI", "HDMI", "HDMI", "MIC", "MIC"])),
        ("NOW", BAD, "NOW, FROM A REPORT WITHOUT ROUTES (OR NO PROFILES YET): EVERY FRAME INPUT OR OUTPUT", meteor(["INPUT", "INPUT ", "INPUT  ", "INPUT   ", "OUTPUT", "OUTPUT "])),
        ("NEW", GOOD, "NEW, LIVE OR FROM ANY REPORT: THE PORT'S WORD WHEN ONLY ONE FRAME HAS IT; ELSE NODE.NICK, WHEN ONLY ONE NODE HAS IT", meteor(["HEADPHONES", "HDMI 3", "HDMI 1", "HDMI 2", "DIGITAL MICROPHONE", "HEADPHONES MIC"])),
    ])
    y = case(y, "2 · ONE NODE, ITS PORTS A CHOICE: A DESKTOP OR LAPTOP CARD (THIS MACHINE)", [
        "One sink and one source; their ports are a choice (SPEAKERS or HEADPHONES), the route in use names the frame and changes with it.",
        "node.nick is the codec's name, ALC287 ANALOG, the same on the sink and the source: a name two frames share tells nothing, so it is never taken.",
    ], RY_TABLE, [
        ("NEW", GOOD, "NOW AND NEW, THE SAME: THE PORT IN USE", ryzen("SPEAKERS", "BUILT-IN MIC", "a")),
        ("NEW", GOOD, "NOW AND NEW WITHOUT ROUTES: THE CODEC'S NAME IS SHARED, SO WHAT IT IS", ryzen("SOUND OUT", "SOUND IN", "b")),
    ])

    text(60, y, "THE RULE, IN ORDER: THE FIRST THAT GIVES A NAME (ADR 0020)", 22, ORANGE, True, extra='letter-spacing="1.5"')
    rows = [
        "1.  The port in use (its route) as a word: SPEAKERS, HEADPHONES, HDMI, MIC… : when no other frame of the device has the same word.",
        "2.  node.nick, PipeWire's short name for the node (where KDE writes a name the user gives): when no other node of the device has it, and it",
        "     isn't the device's own name. A desktop card's sink and source share the codec's name (ALC287 ANALOG): it tells nothing, it is never taken.",
        "3.  The word after all, or what it is: HEADPHONES / MIC on a headset; SOUND OUT / SOUND IN on a card.",
        "Two frames still of one name (two HDMI with no nick): the second is numbered, HDMI 2. Nothing guessed: only PipeWire's own properties.",
        "",
        "Not taken: node.description and the profile's description (long, the card's name in front); a rename winning over the port's word",
        "(only KDE's renames show, as a node's own device.description): a name given shows when the port's word doesn't tell the frames apart.",
    ]
    for k, t in enumerate(rows):
        text(60, y + 34 + k * 24, t, 15, BODY)
    y += 34 + len(rows) * 24 + 40
    text(60, y, "Not taken (KMix, Plasma's applet): the port's description when a list has one device only, and hiding a device whose one port is unplugged.", 15, GREY)
    text(60, y + 22, "The first changes with the list, not the device; the second takes something real off the rack (principles: the graph is honest).", 15, GREY)
    y += 70

    head = f'{DEFS}<rect width="{W}" height="{y}" fill="#1f2023"/>'
    head += f'<text x="60" y="70" {F} font-weight="bold" font-size="40" letter-spacing="3" fill="{ORANGE}">A CARD\'S FRAMES: THEIR NAMES</text>'
    head += f'<text x="60" y="108" {F} font-size="20" fill="{GREY}">What a frame is called when the port in use doesn\'t say it, or says the same as another frame\'s.</text>'
    head += f'<text x="60" y="136" {M} font-size="14" fill="{GREY}">face::name · the names in capitals, as the face prints them</text>'
    out = argv[1] if len(argv) > 1 else "names.svg"
    open(out, "w").write(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{y}" viewBox="0 0 {W} {y}">{head}{"".join(o)}</svg>\n')


if __name__ == "__main__":
    main(sys.argv)
