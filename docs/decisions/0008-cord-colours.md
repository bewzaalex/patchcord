# Cord colours

Status: accepted · 2026-11-22

## Decision
- Eight patch-cable colours, every one colourful (no black, no grey):
  `#D4232A` red, `#EC7A1C` orange, `#F0CC1C` yellow, `#14965A` green, `#12A5B8` cyan, `#1752C8` blue,
  `#7B4FB8` purple, `#D63C8C` pink (`graph::PALETTE`).
- A cord takes the colour of the device its sound comes out of. One pulled from an input is its
  own device's colour until it reaches an output.
- Every device gets a colour of its own while there are colours to go round. A device keeps its
  colour while it is there. A new one takes a colour no one has (the one its key hashes to
  first), else the one fewest have.

## Rejected
- A colour by hash alone: three colours for five devices.
- Black and grey cords: they disappear on the desk.

## Open
Colours are given out again at each start: a device may get another one next time.
