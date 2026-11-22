# A device's screen says what it is

Status: accepted · 2026-11-22

## Context
Under the name a model line repeated what it was; an LED nobody looked at showed the state.

## Decision
- The model line under the name and the LED are gone. The name has the room.
- The LCD has four lines:
  1. the kind (SOUND CARD, HEADSET, HDMI AUDIO, VIRTUAL SINK, APP, MIDI…), from PipeWire only,
     with the state at the right;
  2. its links (or a volume note while it changes);
  3. what it takes of PipeWire: `N NODES  M PORTS`;
  4. a card's profile (scrolling when long), else what it carries (STEREO, MONO, MIDI).
- The key row under the LCD: HIDE at the left on every face, then PROFILE and PORT (a card) or
  INSIDE (a program).

## Open
Line 2 as a "profile" for non-cards (channel layout in IN/OUT words): to think about.

## Links
`docs/design/ident/` (ident = the survey, ident2 = the chosen screen); `graph::kind`.
