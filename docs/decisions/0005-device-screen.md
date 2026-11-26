# A device's screen says what it is

Status: accepted · 2026-11-22

## Context
Under the name a model line repeated what it was; an LED nobody looked at showed the state.

## Decision
- The model line under the name and the LED are gone. The name has the room.
- The LCD has three lines:
  1. the kind (SOUND CARD, HEADSET, HDMI AUDIO, VIRTUAL SINK, APP, MIDI…), from PipeWire only,
     with the state at the right;
  2. a card's profile (scrolling when long), else what it carries (STEREO, MONO, MIDI, or `N CH`
     when its channels don't say they're L and R, as a JACK app's out1, out2);
  3. what it takes of PipeWire and its links: `N NODES  M PORTS  K LINKS` (one space between
     when that's too long for the line), or a volume note while it changes.

  (2026-11-23: lines 2 and 4 swapped, so what a device is and carries come first, its cords last.
  2026-11-26: the links joined line 3 and the screen lost its fourth line, so faces are lower.)
- The key row under the LCD: HIDE at the left on every face, then PROFILE and PORT (a card) or
  nothing more (INSIDE went with [0012](0012-whole-program-on-its-face.md)). Its bottom is level
  with the frames' bottom edge (2026-11-26; it was under them).
- A face is a unit 108 high (2026-11-26; it was 128): its frames with 8 above and below them, the
  name, the screen and its keys beside them in the same height.

## Open
The profile line (now line 2) for non-cards (channel layout in IN/OUT words): to think about.

## Links
`docs/design/ident/` (ident = the survey, ident2 = the chosen screen); `graph::kind`.
