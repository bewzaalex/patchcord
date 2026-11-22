# Mockups

Every visible change starts here (see [principles](../principles.md)). Most mockups are Python
scripts that write an SVG; turn it into a PNG with `rsvg-convert -z .5 x.svg -o x.png`.

| Folder | What | Decision |
|---|---|---|
| `mockup/` | the first interactive HTML mockup: desk, rack, cords, ENGINE key | [0001](../decisions/0001-cords-like-reason.md) |
| `streams/` | an app with several streams: a frame each | |
| `cue/` | listening to a jack (CUE, LEVEL, probe) | [0009](../decisions/0009-cue-own-streams.md) |
| `engine/` | ENGINE's colours, legend and specimen | [0002](../decisions/0002-engine-plain-links.md) |
| `links/` | routed link styles for ENGINE (all rejected) | [0002](../decisions/0002-engine-plain-links.md) |
| `layout/` | ENGINE's layout by links | [0002](../decisions/0002-engine-plain-links.md) |
| `narrow/` | narrow windows: MORE, the strip | [0011](../decisions/0011-narrow-windows.md) |
| `rack/` | the rack as a real one: rails, 100%, wrapping | [0004](../decisions/0004-rack.md) |
| `ident/` | a device's name and screen | [0005](../decisions/0005-device-screen.md) |
| `apps/` | a program under one roof (`flat.py`: the rules on a live graph) | [0006](../decisions/0006-one-roof.md), [0007](../decisions/0007-midi-bridge-clients.md) |
