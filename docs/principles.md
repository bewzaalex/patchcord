# Principles

The rules every change to Patchcord keeps. They come from decisions taken while building it (see
[decisions](decisions/README.md)). Changing one is a decision of its own: it gets an ADR first.

## The graph is honest, always

Patchcord shows what PipeWire has. It never hides a real object, port or link to make the picture
tidier.

- A device may group nodes (a card's sink and source, a program's nodes). Whatever it doesn't show
  on its face must still be reachable: on another device, in the HIDDEN list (marked SYSTEM), or
  in ENGINE. Nothing a user can unplug may vanish so it can't be plugged back.
- A cord must never lead nowhere. A device with a cord is never hidden. A hidden device that gets
  a cord comes back by itself.
- ENGINE is the plain truth: every node, port and link as PipeWire reports it.

## Only what PipeWire says

Names, kinds, grouping and roles come from PipeWire's own properties: `node.*`, `device.*`,
`application.*`, `port.*`, the client's properties, links. Patchcord doesn't look at the system
beside it: no `.desktop` files, no `/proc`, no guessing from process trees. Where PipeWire can't
tell two things apart, Patchcord says so rather than guessing.

## Sound flows left to right

On every face, on the desk and in the rack, frames that take sound in (IN) are at the left, and
frames that send it out (OUT) are at the right. On the desk a cord runs from one device's right
side to the next device's left.

## Mockup first, code after

A visible change starts as a mockup in `docs/design/<topic>/` (a Python script writing SVG and
PNG, or the HTML mockup). It is discussed and settled there. Code follows only once the mockup is
agreed. The mockup stays in the repo as the record of why things look the way they do.

## Hardware, not a diagram

Devices are gear: painted metal, screws, an amber LCD, 6.3 mm jacks. Cords are patch cables with
weight and sag. A jack takes one plug, and a port is a "mult" with one free jack always.
Controls are knobs, keys and switches, not web widgets.

- No glows on hover or on a found device.
- No animation when switching screens.
- No cords that jump or swing into place: cords appear at rest.
- No custom pointer cursors: they break the system's cursor theme.

## Private by default

Pictures made for the README, releases or reports never show private data. MAC addresses are
masked in `--render` output and in ENGINE's log, properties and metadata. SAVE REPORT collects no
confidential information.

## Measure before optimising

Slow frames are found with `PATCHCORD_FRAMES=1`, not by guessing. Big SVGs are never rasterized
in the frame: they go to the painter threads (`ui/painter.rs`). There is no blur filter in
anything redrawn often.
