# Cords like Reason's, calm physics

Status: accepted · 2026-11-17

## Context
Links are patch cords with physics. The first versions had plugs standing out of the panel,
stacked plugs on one port, friction, and steering forces.

## Decision
- A cord has no plugs and no end caps: its ends go straight into the jacks.
- A cord is about 1.3× the distance between its jacks (`target_len`), the same length in the
  hand. There is no SAG knob.
- A jack takes one plug. A port is a "mult": parallel jacks in one outline, always one free.
- A stereo L/R pair is one stereo jack with a SPLIT/JOIN switch.
- Cords appear at rest: they never jump or swing into place.

## Rejected
- Stacked plugs: hard to grab, and a real jack doesn't take two plugs.
- Perspective-tilted plugs (45°, 25°).
- Friction, cords reacting to jacks, steering forces: they trembled.
- Downward-pull hacks: they made hooks.

## Links
`src/ui/cord.rs`, `docs/design/mockup/`.
