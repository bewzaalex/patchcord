# The frames' band colours

Status: accepted · 2026-11-23

## Context
An app's (and a program's) INPUT band was ochre (`#a8862c`) and its OUTPUT rust (`#b45a2e`): 42° of
hue apart and alike, so on Easy Effects, where every node has both, inputs and outputs ran
together. The ochre also read poorly under the cream lettering (3.0:1). VIDEO had no colour of its
own.

## Decision
Seven colours of one lightness and one chroma (OKLCH L .52, C .115), the hues spread round the
circle, so none shouts over another and each is told apart (`face::ACCENTS`):

| Role | Colour | Hue |
|---|---|---|
| MIDI | `#934C7E` | 340° |
| MONITOR | `#695CA6` | 290° |
| PLAYBACK (a device's output) | `#007C7D` | 195° |
| CAPTURE (a device's input) | `#206EA6` | 245° |
| OUTPUT (an app's) | `#9F4F32` | 40° |
| INPUT (an app's) | `#427935` | 140° |
| VIDEO | `#876200` | 85° |

The cream lettering reads at 4.4–5.1:1 on all of them. The cords keep their own palette
([0008](0008-cord-colours.md)).

## Links
`docs/design/apps/block3.py` (the palette beside the old one).
