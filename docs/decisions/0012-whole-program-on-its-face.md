# A program's whole self on its face, along its chains

Status: accepted · 2026-11-23 · supersedes the face of [0006](0006-one-roof.md) (edges only, INSIDE);
its half-and-half band and IN/OUT tags superseded by [0014](0014-a-frame-is-a-block.md)

## Context
0006 put only a program's edges on its face and the rest behind INSIDE: a port came onto the face
when it got a cord from outside and went back when the cord went. So a user who unplugged Easy
Effects' input by mistake saw it vanish, and had no way to plug it back on the desk or in the
rack. The graph was honest in ENGINE, but not where people patch.

## Decision
Variant A of `docs/design/apps/roof.py`:
- A program with several nodes shows **all of them** on its face, always. Nothing comes and goes
  with cords.
- When the program links its own nodes (Easy Effects), its frames go **along its chains**: each
  chain from where it starts (a node none of its own feeds), on along its own links, the chains in
  the order the program made their first nodes, a node in a loop after them, nodes with no links
  of their own last. Each node: its inputs, then its outputs. Its own cords run between
  neighbouring frames on the face. This is still "sound flows left to right": through the chain.
- When its nodes aren't linked among themselves (an app's streams: Teams, Carla, Audacity), the
  face is as every face: inputs at the left, outputs at the right.
- A node's frames (its IN and its OUT) stay together on one unit as **one frame** (variant A of
  `docs/design/apps/block3.py`): one object in PipeWire, as in ENGINE. Its name is printed once
  across a band half the input's colour, half the output's, with IN at the band's left end and OUT
  at its right, and a dotted rule between the halves. Each half keeps its own jacks, meter and SPLIT
  (no SIG on the band: the name is there). This saves the room of a second name and frame, so
  Easy Effects' chain fits fewer units.
- Jacks of different nodes whose ports are named alike (every effect has `input_FL`) are told
  apart by their node, so each of the program's own links is a cord of its own.
- A monitor's jack is labelled MONITOR (it was REC OUT): PipeWire's word, and not only for
  recording.
- The INSIDE key and its list are gone: there is nothing left to show there. ENGINE shows the rest.
- Frame names, numbering, the screen, the device's key: as in 0006.

## Rejected
- Keeping INSIDE as a list beside a full face: it would only repeat the face.
- All inputs, then all outputs, for a chained program: its own cords would cross the whole face.

## Links
`docs/design/apps/roof.png` (variant A); `graph::chain_order`, `Device::chain`, `face::sides`.
