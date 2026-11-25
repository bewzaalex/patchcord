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
| `apps/` | a program under one roof (`flat.py`: the rules on a live graph); a frame is a block (`nodes.py`, `progframes.py`, `hwframes.py`, `kinds.py`, `paints.py`, `linkgroup.py`; the whole palette and OUTPUT's brown: `palette.py`, `outcolour.py`) | [0006](../decisions/0006-one-roof.md), [0007](../decisions/0007-midi-bridge-clients.md), [0014](../decisions/0014-a-frame-is-a-block.md) |
| `connect/` | making links in ENGINE, interactive (open `index.html`): drag port to port, a block's head onto another (all its channels), or a port's LINK TO list in its details' CONN tab; the blocks' heads as bands; on a made-up graph in ENGINE's measures and colours | [0017](../decisions/0017-links-in-engine.md) |
| `names/` | a card's frames' names when the port in use doesn't tell them apart or isn't known: the port's word, then `node.nick` (where KDE writes a name given), on a laptop's HiFi card (a node a port) and a desktop's (`names.py`) | [0020](../decisions/0020-a-cards-frames-names.md) |
| `mixer/` | MIXER exactly as `ui/mixer.rs` draws it, interactive (open `index.html`): the code's measures, type and paints, the app's own top panel (`panel.png`, from `cargo test mixer_panel -- --ignored`); the look is tuned here first (ADR 0015); `faders.html`: ten ways to draw the fader block, and A and B, two of them together (A is in the code) | |
