# ENGINE: plain links, layout by links

Status: accepted · 2026-11-20

## Context
With many links ENGINE was hard to read. Routed styles were tried to clean it up.

## Decision
- Each link is one cubic curve from port to port (`link_curve`), with no routing and no lanes.
- Blocks are coloured by type: device blue, app purple, system teal, MIDI orange, video raspberry,
  Patchcord green, cue cyan. Links take the colour of the block they come out of; error links are
  red. The selection is a yellow glow behind it, and colours stay.
- Layout by links, per driver band, on one grid: no links / only out / in+out / only in. No link
  passes within 6 px of a port dot that isn't its own.

## Rejected
Ten routed styles (orthogonal, rails, 45° cuts, lanes): all unreadable with many links.

## Links
`docs/design/links/`, `docs/design/layout/`, `docs/design/engine/`.
