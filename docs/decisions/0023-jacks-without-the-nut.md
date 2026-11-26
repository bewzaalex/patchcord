# Jacks without the nut

Status: accepted · 2026-11-26

## Context
A jack on a face was a chassis jack seen with its hex nut: a nut, a dark washer with the group's
colour in a thin ring (two for stereo), a chrome bushing and the hole. MIDI was the same jack with
a red washer. Next to the panel's flat keys and screens the nuts looked fussy, and the group's
colour, a ring a pixel wide, didn't read from a distance. The maintainer's reference: a small
headphone amp, its 6.3 mm jacks flush with the face.

## Decision
**A jack is flush with the face, no nut** (`face::jack_pic_ring`, variant C of the mockup):
- a wide bushing in its group's colour (PLAYBACK, OUTPUT, MONITOR…), a thin dark line round it
  on the paint (no light edge: 2026-11-26);
- inside it a black bezel and the jack's nickel round the deep hole, **the same on every jack**
  (2026-11-26: stereo was brass, mono nickel; two metals said nothing the tags don't);
- **MIDI is a jack as the others**, only its bushing in MIDI's colour (2026-11-26; a DIN socket
  was tried: its small pins read as dust, a cord going into it looked wrong; `docs/design/jacks/midi.py`).

Nothing else about a jack changes: the plus badge on a mult's free jack, a cord's targets (green,
gold) and the faded ones, CUE's ring, disc, half disc and a split pair's mark, the mult's outline,
the split pair's tags. They lie over or under the jack as before, at the same sizes.

## Rejected
- A: a ring printed on the face in the group's colour round the jack (as the reference's white
  rings): read best, but more drawing round every jack.
- B: a white printed ring and the colour on the bushing: closest to the reference, but the
  colour was small and lost, on MIDI most.
- Keeping the nut.

## Links
`docs/design/jacks/` (every state, now and three ways), `face::jack_pic_ring`, `face::jack_parts`.
ADR 0014 (a frame's colours).
