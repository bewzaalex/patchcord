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
| `jacks/` | a jack without its hex nut, as a small amp's panel-mount jacks (`jacks.py`): every state a jack has (empty mono, stereo, MIDI; a plug in it; a mult's free one; a cord's targets and the faded; CUE's ring, disc, half, a split pair), now and three ways (A a printed ring in its colour, B a white ring and a coloured bushing, C a coloured bushing), on aluminium and black; C chosen. `midi.py`: MIDI's jack, six ways (the DIN socket's small pins read as dust); none: MIDI is a jack as the others, in its colour | [0023](../decisions/0023-jacks-without-the-nut.md) |
| `layout/` | ENGINE's layout by links | [0002](../decisions/0002-engine-plain-links.md) |
| `narrow/` | narrow windows: MORE, the strip | [0011](../decisions/0011-narrow-windows.md) |
| `rack-width/` | the rack in a wide window, interactive (open `index.html`): now, as wide as the window; and as wide as its widest device needs while every device fits one unit, in the middle of the window (`now.png`, `fit.png` from `cargo test rack_picture -- --ignored`); and five ways to fill where a face's frames end instead of vents (`fill.py`, from `fit.png`): A plain metal, B the device as narrow as it needs and a blank panel, C two narrow devices in a unit, D the frames stretched, E plain with its kind printed | |
| `rack/` | the rack as a real one: rails, 100%, wrapping | [0004](../decisions/0004-rack.md) |
| `ident/` | a device's name and screen | [0005](../decisions/0005-device-screen.md) |
| `appicon/` | the app's own icon (the system's), twenty to choose from (`appicon.py`), each at 128, 48 and 24 px, the small ones on a light ground too; `appicon-grounds.png`: six of them on other grounds than the panel's grey (aluminium, black, olive, amber, night blue, none). Chosen (2026-11-26): 01, the P as it was, on black, as a program's face on the rack | |
| `apps/` | a program under one roof (`flat.py`: the rules on a live graph); a frame is a block (`nodes.py`, `progframes.py`, `hwframes.py`, `kinds.py`, `paints.py`, `linkgroup.py`; the whole palette and OUTPUT's brown: `palette.py`, `outcolour.py`) | [0006](../decisions/0006-one-roof.md), [0007](../decisions/0007-midi-bridge-clients.md), [0014](../decisions/0014-a-frame-is-a-block.md) |
| `diagnostics/` | ENGINE's DIAGNOSTICS in three looks, on SERVER and the tab row, interactive (open `index.html`): A an instrument panel (the top panel's metal, LCD windows, keys with a light), B a quiet console (cards), C a unit in the rack; what a value is goes to the status bar on hover. All three rejected (2026-11-26): the tables' look stays; only the tabs changed, to real tabs joined to what they show | |
| `find/` | ENGINE's ALSO SHOW chips as one key, FIND · SHOW, and its card: a search field (blocks that match picked out, Enter to the next, a match a switch hides said with a key to show it) and the four switches with what each shows and how many; beside it LEGEND and DIAGNOSTICS, moved up from the graph's bottom; interactive (open `index.html`), on a made-up graph | [0021](../decisions/0021-find-and-show-in-engine.md) |
| `connect/` | making links in ENGINE, interactive (open `index.html`): drag port to port, a block's head onto another (all its channels), or a port's LINK TO list in its details' CONN tab; the blocks' heads as bands; on a made-up graph in ENGINE's measures and colours | [0017](../decisions/0017-links-in-engine.md) |
| `panel/` | hiding the top panel, interactive (open `index.html`): A an edge the panel shows over, B a slim bar with the LCD's lines, C nothing but P; on the app's own panel (`panel.png`) | |
| `names/` | a card's frames' names when the port in use doesn't tell them apart or isn't known: the port's word, then `node.nick` (where KDE writes a name given), on a laptop's HiFi card (a node a port) and a desktop's (`names.py`) | [0020](../decisions/0020-a-cards-frames-names.md) |
| `mixer/` | MIXER exactly as `ui/mixer.rs` draws it, interactive (open `index.html`): the code's measures, type and paints, the app's own top panel (`panel.png`, from `cargo test mixer_panel -- --ignored`); the look is tuned here first (ADR 0015); `faders.html`: ten ways to draw the fader block, and A and B, two of them together (A is in the code) | |
