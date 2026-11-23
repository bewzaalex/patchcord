# A frame is a block: one frame a node, one table of colours with ENGINE

Status: accepted · 2026-11-23 · supersedes [0007](0007-midi-bridge-clients.md), the colours of
[0013](0013-band-colours.md), the half-and-half band of [0012](0012-whole-program-on-its-face.md),
the finishes of [0006](0006-one-roof.md), and the recording monitors of [0010](0010-hiding.md)

## Context
The rack should show how the graph is plugged, without new things to learn: the graph's blocks
are its frames. But a node could be spread over several frames (a sink and its MONITOR, a MIDI
bridge's clients), and colours meant different things in the two views: ENGINE coloured a block
by the first word of its `media.class` (every `Audio/*` one blue DEVICE, a classless effect
SYSTEM) and gave its ports the block's colour, while the rack coloured a frame by its ports' role.
A card's sink and source were one colour in ENGINE and two on the rack. And four finishes stood
for three kinds of device: metal and cream looked alike, navy passed for black.

## Decision
`docs/design/apps/nodes.py` (the rack), `progframes.py`, `hwframes.py` (a frame beside its
ENGINE block), `kinds.py` (variant 4), `paints.py` (variant D).

**One frame a node.** Every port of a node is in its one frame, on its one device: its inputs,
its outputs, its monitor. A frame never splits. A device gathers frames: a card's or headset's
nodes (`device.id`), a program's, or one node of PipeWire's own. `node.link-group` makes no roof
(`linkgroup.py`, not now).
- A monitor's jack sits beside the input it copies, as a sink's does now. A stream's monitor is on
  its frame too: the rack can patch whatever PipeWire can link.
- The MIDI bridge is one frame: each client's ports are jacks in it, named by the client (the part
  of `port.alias` before ':'), in ALSA's client order; one colour for all of them.

**A frame's colour is its block's kind**, by `media.class`, whoever owns it (a program's virtual
sink is a sink). The frame's outline and band on the rack, the block's outline and cap in ENGINE:

| Kind | `media.class` | Colour |
|---|---|---|
| SINK | `Audio/Sink` | `#007C7D` |
| SOURCE | `Audio/Source`, `Audio/Source/Virtual` | `#206EA6` |
| PLAYBACK STREAM | `Stream/Output/Audio` | `#7E5A42` |
| RECORDING STREAM | `Stream/Input/Audio` | `#427935` |
| MIDI | `Midi/*` | `#934C7E` |
| VIDEO | `Video/*`, `Stream/*/Video` | `#876200` |
| DUPLEX | `Audio/Duplex` | `#73348D` |
| NO CLASS | none | `#CBA553`, lettered dark (`#1D1E20`) |

One outline, never dashed. Where a kind has a main group of ports, its colour is that group's.

**A port's colour is its group's**: the ports of one direction and one kind (monitor or not,
audio, MIDI, video). Not `port.group`: Easy Effects gives each channel its own. The jack's ring on
the rack, the port's dot and pill in ENGINE (which stops giving ports the block's colour):

| Group | Ports | Colour |
|---|---|---|
| PLAYBACK | a sink's inputs | `#007C7D` |
| CAPTURE | a source's outputs (a virtual source's too, though `port.monitor`) | `#206EA6` |
| INPUT | anything else's inputs | `#427935` |
| OUTPUT | anything else's outputs | `#7E5A42` |
| MONITOR | outputs with `port.monitor` | `#695CA6` |
| MIDI | MIDI ports | `#934C7E` |
| VIDEO | video ports | `#876200` |
| OTHER | ports of a format Patchcord doesn't know, or of none | `#CBA553` (NO CLASS's) |

ENGINE's SYSTEM, PATCHCORD and CUE colours go: Patchcord's meters and its cue's two streams are
blocks of their kind like any (their PATCHCORD tag says whose). The cue's blue stays on the ports
kept on. Dummy-Driver and Freewheel-Driver have no class: NO CLASS, like any (no grey of their own).

**A device's paint is who owns it**, one paint a kind, lightness first so none passes for another:
HARDWARE aluminium `#C3C6CA` (print `#1D1E20`), PROGRAM black `#232427` (print `#D5D7D9`), a NODE
of PipeWire's or WirePlumber's olive `#66683F` (print `#F1F1E6`).

**The frame**, as the face draws it, with these changes:
- The band: GRAPH, then the name; VOLUME · BAL (GAIN on a source or a recording stream) and SIG
  where there are. No IN or OUT tag on the band.
- GRAPH: a small key with the node's number, `#180` (the id ENGINE's block shows). It opens
  ENGINE with that block chosen and in view. No animation.
- Under each port's jacks, what it is: IN, OUT or MONITOR, all outlined in the paint's print (a
  filled tag passed for a key, as SPLIT and MUTE), in one row with the keys; IN L / IN R when
  split; a bridge's client in its tag (IN · HYDROGEN). STEREO and MONO go: SPLIT says it.
- SPLIT as now: a key beside MUTE for a stereo group with a volume, a little switch under its
  jacks for one without; none for mono, MIDI and video. A frame's SPLIT splits its input and the
  MONITOR inside it together (MONITOR L over MONITOR R, beside IN L over IN R).

**Still off the rack** (HIDDEN, marked SYSTEM; in ENGINE): Patchcord's own meters and cue, other
programs' level-meter streams, and a Bluetooth headset's internal capture stream (in A2DP the real
mic is no node yet: its input waits for a switch to HSP/HFP, see [0003](0003-bluetooth-mic.md)).

## Rejected
- A band half one group's colour, half the other's, for a block with no main group (DUPLEX, NO
  CLASS), or a dashed outline for NO CLASS: every kind gets a colour of its own instead.
- Grey and slate for DUPLEX and NO CLASS: too alike. The mid tones are taken; magenta is the hue
  left, and sand differs by its lightness.
- Rust (`#9F4F32`) for OUTPUT and PLAYBACK STREAM: in ENGINE it read as the error red of a link
  or an xrun light. A warm brown instead (`docs/design/apps/palette.py`, variant B; `outcolour.py`).
- A lighter step for a second group of one kind (a bridge's second client): names tell them apart.
- Keeping four finishes: two kinds of device would share three paints' worth of look.
- `node.link-group` as a roof for a loopback's or filter-chain's two nodes, and as rows inside a
  program: not now.
- Showing the headset's internal capture stream: before the switch there is nothing it is linked
  to, and its free input reads as the mic's.

## Links
`docs/design/apps/nodes.png` (`nodes-rack.png`, `nodes-types.png`), `progframes.png`,
`hwframes.png`, `kinds.png`, `paints.png`, `linkgroup.png`. Code: `graph::block_kind`,
`graph::port_role`, `Finish`; `face::sections`, `face::kind_colour`, `face::role_colour`,
`Geometry::graph`; `Engine::show_node`; the legend `docs/design/engine/legend2.py`.
