#!/usr/bin/env python3
"""A report's graph as a picture: every node with its ports, and how they are linked.

Reads a report saved by ENGINE's SAVE REPORT (patchcord-report-*.txt) and draws a schematic:
blocks left to right as sound flows (what only gives sound at the left, what only takes it at the
right), each port a row, each link a line from an output to an input. Only the report is read:
nothing of PipeWire, nothing of this machine.

    python3 tools/report_graph.py REPORT.txt [OUT.svg|OUT.png] [--meters]

OUT defaults to the report's name with .svg; a .png needs rsvg-convert. Patchcord's own meter
nodes (patchcord-meter-*) are left out unless --meters: there is one for every device the
reporter's rack showed, and the header says how many were left out.

On a block: its description, then #id, its class and its state (the light: green running, amber
idle, grey suspended, red error); a red edge where the profiler counted xruns; under it, from the
report's STATE (reports saved after 0.6.0), its volume and mute, its card's profile and port. A link is
solid when active, dashed when passive (it wakes nothing), grey when not active.
"""

import re
import shutil
import subprocess
import sys
from html import escape

W = 280          # a block's width
GAP_X = 150      # between columns
GAP_Y = 22       # between blocks in a column
HEAD = 40        # a block's head: description and #id · class · state
ROW = 16         # a port
NOTE = 15        # a line of state under the head

STATE_LED = {"running": "#3fb950", "idle": "#e5a72c", "suspended": "#5a5d62", "creating": "#5a5d62", "error": "#e5534b"}
KIND = {"audio": "#4fb3c8", "midi": "#c678dd", "video": "#e5c07b"}


def sections(text):
    """The report's sections by name ("GRAPH", "OBJECTS"…): their lines."""
    out, name = {"HEAD": []}, "HEAD"
    for line in text.splitlines():
        m = re.match(r"^==== (\S+)", line)
        if m:
            name = m.group(1)
            out[name] = []
        else:
            out[name].append(line)
    return out


def parse(text):
    s = sections(text)
    r = {"title": next((l for l in s["HEAD"] if l.strip()), "Patchcord report"), "system": [], "issues": [], "nodes": {}, "links": {}, "xruns": {}, "state": {}, "cards": {}}
    r["system"] = [l for l in s.get("SYSTEM", []) if l.strip()]
    r["issues"] = [l for l in s.get("ISSUES", []) if l.startswith("[")]
    for l in s.get("DRIVERS", []):
        m = re.match(r"^\s+(\d+)\s+\S+\s+\S+\s+\S+\s+\S+\s+\S+\s+(\d+)\s", l)
        if m and int(m.group(2)) > 0:
            r["xruns"][int(m.group(1))] = int(m.group(2))
    node = None
    ends = {}  # link id -> {"out": port, "in": port, "state", "passive"}
    for l in s.get("GRAPH", []):
        m = re.match(r'^node (\d+) (.*?) "(.*)" \[([^\]]*)\] (\S+)(.*)$', l)
        if m:
            node = {"id": int(m.group(1)), "name": m.group(2), "desc": m.group(3), "class": m.group(4), "state": m.group(5), "ins": [], "outs": []}
            r["nodes"][node["id"]] = node
            continue
        m = re.match(r"^    (in |out) (\d+) (.*?)(?:  (.*))?$", l)
        if m and node:
            out = m.group(1) == "out"
            port = int(m.group(2))
            node["outs" if out else "ins"].append((port, m.group(3).strip()))
            for lm in re.finditer(r"\(link (\d+) (\S+?)( passive)?\)", m.group(4) or ""):
                e = ends.setdefault(int(lm.group(1)), {"state": lm.group(2), "passive": bool(lm.group(3))})
                e["out" if out else "in"] = port
    port_node, port_kind = {}, {}
    for n in r["nodes"].values():
        for p, _ in n["ins"] + n["outs"]:
            port_node[p] = n["id"]
    # each port's kind, and each node's card, from OBJECTS
    obj, props = None, {}
    for l in s.get("OBJECTS", []):
        m = re.match(r"^(\w+) (\d+)$", l)
        if m:
            obj = (m.group(1), int(m.group(2)))
            props[obj] = {}
            continue
        m = re.match(r"^    (\S+) = (.*)$", l)
        if m and obj:
            props[obj][m.group(1)] = m.group(2)
    for (ty, i), p in props.items():
        if ty == "Port" and p.get("format.dsp"):
            f = p["format.dsp"]
            port_kind[i] = "midi" if "midi" in f else "video" if "video" in f else "audio"
    # a port without format.dsp (a video stream's) is of its node's kind
    for n in r["nodes"].values():
        c = props.get(("Node", n["id"]), {})
        n["device"] = int(c["device.id"]) if c.get("device.id", "").isdigit() else None
        guess = "video" if "Video" in n["class"] else "midi" if "Midi" in n["class"] else "audio"
        for p, _ in n["ins"] + n["outs"]:
            port_kind.setdefault(p, guess)
    for i, e in ends.items():
        if "out" in e and "in" in e and e["out"] in port_node and e["in"] in port_node:
            e["kind"] = port_kind.get(e["out"], "audio")
            r["links"][i] = e
    r["port_node"] = port_node
    r["port_kind"] = port_kind
    # STATE: volumes and mute by node; each card's profile in use
    card = None
    for l in s.get("STATE", []):
        m = re.match(r"^node (\d+) volume (.*?) · mute (\w+)", l)
        if m:
            r["state"][int(m.group(1))] = "volume " + m.group(2) + (" · MUTE" if m.group(3) == "yes" else "")
        m = re.match(r"^device (\d+)$", l)
        if m:
            card = int(m.group(1))
        m = re.match(r'^    profile \*\d+ (\S+) "(.*)"', l)
        if m and card is not None:
            r["cards"].setdefault(card, {})["profile"] = m.group(2)
        m = re.match(r'^    port \*\d+ (out|in ) \S+ "(.*?)"', l)
        if m and card is not None:
            v = re.search(r"volume (.*?) · mute (\w+)", l)
            text = "port " + m.group(2) + (" · volume " + v.group(1) if v else "") + (" · MUTE" if v and v.group(2) == "yes" else "")
            r["cards"].setdefault(card, {}).setdefault("ports", []).append((m.group(1).strip(), text))
    return r


def layout(r, nodes):
    """Columns by how far sound has come: a node one further than the furthest that feeds it."""
    ids = set(nodes)
    edges = {(r["port_node"][e["out"]], r["port_node"][e["in"]]) for e in r["links"].values()}
    edges = {(a, b) for a, b in edges if a in ids and b in ids and a != b}
    linked = {a for a, _ in edges} | {b for _, b in edges}
    rank = {n: 0 for n in linked}
    for _ in range(len(linked)):  # longest path, cycles cut off after as many rounds as nodes
        changed = False
        for a, b in edges:
            if rank[b] < rank[a] + 1 and rank[a] + 1 < len(linked):
                rank[b] = rank[a] + 1
                changed = True
        if not changed:
            break
    cols = {}
    for n in sorted(linked, key=lambda n: (nodes[n]["class"], nodes[n]["desc"].lower())):
        cols.setdefault(rank[n], []).append(n)
    # order each column by where what feeds it sits (a few sweeps)
    pos = {}
    for c in sorted(cols):
        for i, n in enumerate(cols[c]):
            pos[n] = i
    for _ in range(4):
        for c in sorted(cols)[1:]:
            def bary(n):
                feeds = [pos[a] for a, b in edges if b == n and a in pos]
                return sum(feeds) / len(feeds) if feeds else pos[n]
            cols[c].sort(key=bary)
            for i, n in enumerate(cols[c]):
                pos[n] = i
    loose = sorted((n for n in ids if n not in linked), key=lambda n: (nodes[n]["class"], nodes[n]["desc"].lower()))
    return [cols[c] for c in sorted(cols)], loose


def notes(r, n):
    out = []
    if n["id"] in r["state"]:
        out.append(r["state"][n["id"]])
    card = r["cards"].get(n.get("device"))
    if card and card.get("profile"):
        out.append("profile " + card["profile"])
    # the card's port in use on its sink (out) or source (in): where the device's volume is
    way = "out" if "Sink" in n["class"] else "in" if "Source" in n["class"] else None
    for d, t in (card or {}).get("ports", []):
        if d == way:
            out.append(t)
    return out


def height(r, n):
    return HEAD + NOTE * len(notes(r, n)) + ROW * (len(n["ins"]) + len(n["outs"])) + 8


def cut(t, k):
    return t if len(t) <= k else t[: k - 1] + "…"


def draw(r, meters):
    nodes = {i: n for i, n in r["nodes"].items() if meters or not n["name"].startswith("patchcord-meter")}
    left_out = len(r["nodes"]) - len(nodes)
    cols, loose = layout(r, nodes)
    # the head of the picture: the report, its system, its issues
    head = [r["title"]] + r["system"] + [""] + (r["issues"] or ["no issues"])
    if left_out:
        head.append(f"({left_out} Patchcord meter nodes left out: --meters shows them)")
    top = 20 + 16 * len(head) + 24
    box, port_xy = {}, {}
    x = 30
    bottom = top
    for col in cols:
        y = top
        for n in col:
            box[n] = (x, y)
            y += height(r, nodes[n]) + GAP_Y
        bottom = max(bottom, y)
        x += W + GAP_X
    width = max(x, 900)
    # the nodes without a link: rows at the bottom
    if loose:
        y0 = bottom + 40
        per = max(1, (width - 30) // (W + 30))
        rows = [loose[i:i + per] for i in range(0, len(loose), per)]
        y = y0 + 24
        for row in rows:
            h = 0
            for i, n in enumerate(row):
                box[n] = (30 + i * (W + 30), y)
                h = max(h, height(r, nodes[n]))
            y += h + GAP_Y
        bottom = y
    o = []
    for n, (bx, by) in box.items():
        nd = nodes[n]
        y = by + HEAD + NOTE * len(notes(r, nd))
        for p, _ in nd["ins"]:
            port_xy[p] = (bx, y + ROW / 2)
            y += ROW
        for p, _ in nd["outs"]:
            port_xy[p] = (bx + W, y + ROW / 2)
            y += ROW
    # the links under the blocks
    for i, e in sorted(r["links"].items()):
        if e["out"] not in port_xy or e["in"] not in port_xy:
            continue
        (ax, ay), (bx, by) = port_xy[e["out"]], port_xy[e["in"]]
        d = max(abs(bx - ax) / 2, 60)
        colour = KIND[e["kind"]] if e["state"] == "active" else "#6b6e73"
        dash = ' stroke-dasharray="5 4"' if e["passive"] else ""
        o.append(f'<path d="M{ax},{ay} C{ax + d},{ay} {bx - d},{by} {bx},{by}" fill="none" stroke="{colour}" stroke-width="1.6"{dash} opacity=".85"><title>link {i} {e["state"]}{" passive" if e["passive"] else ""}</title></path>')
    for n, (bx, by) in box.items():
        nd = nodes[n]
        h = height(r, nd)
        xr = r["xruns"].get(n)
        o.append(f'<rect x="{bx}" y="{by}" width="{W}" height="{h}" rx="5" fill="#232427" stroke="{"#e5534b" if xr else "#000"}" stroke-width="{2 if xr else 1}"/>')
        o.append(f'<rect x="{bx}" y="{by}" width="{W}" height="{HEAD - 4}" rx="5" fill="#2e3034"/>')
        o.append(f'<circle cx="{bx + 12}" cy="{by + 13}" r="4" fill="{STATE_LED.get(nd["state"], "#5a5d62")}"/>')
        o.append(f'<text x="{bx + 22}" y="{by + 17}" font-weight="bold" font-size="12" fill="#e6e7e8">{escape(cut(nd["desc"], 34))}</text>')
        sub = f'#{n} · {nd["class"] or "—"} · {nd["state"]}' + (f" · {xr} XRUNS" if xr else "")
        o.append(f'<text x="{bx + 10}" y="{by + 31}" font-family="Noto Sans Mono, monospace" font-size="9.5" fill="{"#e5534b" if xr else "#8a8d91"}">{escape(cut(sub, 46))}</text>')
        y = by + HEAD
        for t in notes(r, nd):
            o.append(f'<text x="{bx + 10}" y="{y + 11}" font-family="Noto Sans Mono, monospace" font-size="9.5" fill="#e58f2c">{escape(cut(t, 46))}</text>')
            y += NOTE
        for out, ps in ((False, nd["ins"]), (True, nd["outs"])):
            for p, name in ps:
                cy = y + ROW / 2
                kind = r["port_kind"].get(p, "audio")
                px = bx + W if out else bx
                o.append(f'<circle cx="{px}" cy="{cy}" r="3.5" fill="{KIND[kind]}" stroke="#0f1011" stroke-width="1.2"/>')
                label = f"{name}  {p}" if not out else f"{p}  {name}"
                anchor, tx = ("end", bx + W - 10) if out else ("start", bx + 10)
                o.append(f'<text x="{tx}" y="{cy + 3.5}" text-anchor="{anchor}" font-family="Noto Sans Mono, monospace" font-size="10" fill="#c9cbcd">{escape(cut(label, 40))}</text>')
                y += ROW
    hd = []
    for i, t in enumerate(head):
        colour = "#e6e7e8" if i == 0 else "#e5534b" if t.startswith("[ERROR]") else "#e5a72c" if t.startswith(("[warn]", "[WARNING]")) else "#a8abae"
        hd.append(f'<text x="30" y="{30 + 16 * i}" font-family="Noto Sans Mono, monospace" font-size="{13 if i == 0 else 11}" fill="{colour}" xml:space="preserve">{escape(t)}</text>')
    if loose:
        hd.append(f'<text x="30" y="{box[loose[0]][1] - 10}" font-family="Noto Sans Mono, monospace" font-size="11" fill="#7d8085">NOT LINKED</text>')
    height_px = bottom + 20
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height_px}" font-family="Noto Sans, sans-serif">'
            f'<rect width="100%" height="100%" fill="#17181a"/>{"".join(hd)}{"".join(o)}</svg>')


def main(argv):
    args = [a for a in argv[1:] if not a.startswith("--")]
    if not args or "-h" in argv or "--help" in argv:
        print(__doc__.strip())
        return 0 if args else 2
    src = args[0]
    out = args[1] if len(args) > 1 else re.sub(r"\.txt$", "", src) + ".svg"
    with open(src, encoding="utf-8") as f:
        svg = draw(parse(f.read()), "--meters" in argv)
    if out.endswith(".png"):
        if not shutil.which("rsvg-convert"):
            print("a .png needs rsvg-convert; write .svg instead", file=sys.stderr)
            return 1
        subprocess.run(["rsvg-convert", "-o", out], input=svg.encode(), check=True)
    else:
        with open(out, "w", encoding="utf-8") as f:
            f.write(svg)
    print(out)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
