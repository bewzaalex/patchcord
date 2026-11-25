# Making links in ENGINE

Status: accepted · 2026-11-25

## Context
ENGINE showed PipeWire's graph read-only: to link two ports there, you went back to the rack and
plugged a cord, or used another tool. Some ports have no jack on a face at all (a node the rack
leaves out, a monitor that isn't shown), and a block may be far from the one it should go to.
ENGINE stays the plain truth (principles): a link made there is a real PipeWire link, shown once
PipeWire has it, and on the desk it is a cord like any other.

## Decision
**Three ways to make a link** (mockup `docs/design/connect/`):
- **Port to port**: drag from a port, either way (an output to an input, an input to an output).
  A dashed link in the port's colour follows the pointer; the ports that take it get a ring and
  a stronger ear, the rest of the graph fades. Near one that takes it, the link holds to it and
  shows as it will be; let go there, it is made.
- **Head to block**: drag a block's head (its band) onto another block: all its channels at once,
  its outputs to the other's inputs (an input-only block: its inputs from the other's outputs),
  FL to FL, FR to FR, mono to all and all to mono; with no channel in common, in order. The block
  it would go to turns the amber of what is chosen, and the links show before letting go.
- **LINK TO (LINK FROM)**: a port's details have a tab of their own for it, CONN (INFO · CONN ·
  PROPS; 2026-11-25, it was at the top of INFO): every port it fits, block by block, a row
  each; a linked one in the amber of what is chosen (its dot still in its port's colour), with
  LINKED. A click makes the link, or takes it away.

**What takes a link**: one port gives out, the other takes in, both of one kind (audio, MIDI,
video), on two blocks, neither Patchcord's own (its meters, the cue). A port linked to it already
takes no part while one is carried.

**Taking a link away**: its row in LINK TO, or the link chosen and Delete (Backspace).

- Ctrl+Z undoes it, Ctrl+Shift+Z does it again: one step with the cords' (`Scene::link_ports`).
- Carried near the graph's edge, the link scrolls the graph that way, as a plug does on the desk:
  the other block may be out of sight.
- An empty place still pans the graph.
- The link in the hand is drawn by the GPU over the graph (as the cords are), not into the
  graph's picture: the picture is drawn again only when what takes the link changes.

**A block's head is a band** (2026-11-25, as the frames' on the rack and in MIXER, ADR 0015): its
kind's colour across its top, the name on it in capitals in the band's lettering, the light at
its left, the time on a dark key at its right; under it, on the block, its id and class and the
tags. It was a thin strip of colour over the name.

## Rejected
- A link dropped on a port it has already taking that link away: too easy to do by mistake. A
  linked port takes no part instead.
- A red light for a linked row: red says something is wrong in ENGINE. Then a light in the
  link's colour: a linked row in amber says it plainly.
- The whole band (name, id and class) on the colour: only its first line is.

## Links
Mockup `docs/design/connect/index.html`. Code: `ui/engine.rs` (`Model::fits`, `takes`, `pairs`;
`Carry`, `Engine::carried`, `edge_scroll`, `Hit::LinkTo`), `Scene::link_ports`,
`mesh::lines`. ADR 0002 (ENGINE's links), 0014 (colours).
