# MIXER: the graph as a mixing console

Status: accepted · 2026-11-23

## Context
The rack shows how everything is connected, and ENGINE shows all of PipeWire. Someone who only
wants to set volumes, choose where an app plays and pick the default output needs neither cords
nor nodes. KDE's Audio Volume does these jobs as lists. MIXER does them as a console, on the same
graph the rack shows. The mockup is `docs/design/mixer/index.html`; it went through several
rounds (channels and buses, masters, KDE's lists) before this shape.

## Decision
**MIXER is a screen beside DESK, RACK and ENGINE.**
- Its key is on the top panel right of DESK; the keyboard key is M.
- It is remembered across restarts (`mixer = on` in settings.txt).
- E opens ENGINE from it, and E again comes back to MIXER.
- DESK stays for now.

**A module is a device of the rack.** A card, a program (every node it made) or a node of its
own each make one module.
- Its heading: a card's name in the left half and its PROFILE key, named by the profile in use, in
  the right; anything else its name across it. Names are cut with an ellipsis: they never widen a
  module. (A PROFILE row under the heading left the apps an empty one, 2026-11-24.)
- A fixed width: one strip's, two strips', three… (112 each); a card that is off, one strip's.
  SPLIT doesn't widen a strip. One gap (6) between every two modules, devices and apps alike.
- No screens.
- Paint by owner, as on the rack: hardware the top panel's metal, a program black, PipeWire's
  own olive.
- Every module is the full height of the console.

**Every key is the top panel's** (`panel::cap_svg`, its LED as `panel::led_svg`). A key with a LED
(PLAY ON, RECORD FROM, MAIN, CUE, MUTE, SPLIT) is in while it is on, pressed into its hole; held
down it sinks deeper, and it does what it does when let go over it, as the rack's BAL knob. The
others (PLAY ALL HERE, PORT, PROFILE) act on the press.

**Order.** DEVICES first, then APPS (`Finish::Black`), each by name, A to Z. The order is static:
it doesn't follow what plays or which device is the default (2026-11-24; it put the default
output last, which moved modules about). A card whose profile is Off is its heading only.

**A strip is a group** (`graph::Group`, `mixer::role_of`): audio, no MONITOR, with a volume, or
on a node whose ports go only one way.
- A DAW's engine and a filter that only plays are strips that say NO VOLUME IN PIPEWIRE.
- A node that takes sound in and gives it out without a volume is no strip: an effect inside
  Easy Effects' chain. The rack has it.
- MIDI and video are not in MIXER. Devices hidden on the rack (HIDE, SYSTEM) are not in it
  either.

**Inside a module**, what takes sound in is at the left (a sink, an app's INPUT) and what gives it
out is at the right (a source, an app's OUTPUT), as the rack's frames go. The band is the block
kind's colour, with GRAPH (`#id`, opens ENGINE) and the frame's name.

**An app's strip**
- **PLAY ON** keys go to every sink shown; **RECORD FROM** keys go to every source shown.
- Each key is a push-push key: in, its red LED lit, the app is linked there; out, it isn't.
- A click pushes it in or out: the app plays (records) there too, L to L and R to R (mono to
  both), or no longer. Several can be in at once (2026-11-24; a click used to move the app, and
  Ctrl+click add one).
- One step to undo (`Scene::reroute`). These are links, as cords on the rack are.

**A device's strip**
- **MAIN OUT / MAIN IN**: the default (`default.configured.audio.*`); in, its amber LED lit, on
  the default. (An amber outline round the strip went, 2026-11-24.)
- **PLAY ALL HERE / RECORD ALL HERE**: every app that plays (records) somewhere moves here, and
  this becomes the default.
- **PORT**: the card's port list, when it has more than one port.

**Every strip has:**
- **CUE**: its jacks heard in CUE mode, as on the rack.
- **MUTE**.
- **SPLIT**: a fader for L and one for R, for a stereo pair; it stands for BAL.
- **The fader**: 0–153% (the rack's VOLUME), double-click 100%, the wheel steps it (Shift:
  finer). Its scale: a line at every 12.5% (long at the numbers, 100% the strongest), on the pixel
  grid; behind the lines, up to 100% in the print's grey, past it to the top a dull red; narrower
  than the cap.
- **The meter**: the rack's, ten LEDs.

The console scrolls by dragging empty space or with the wheel. It has no zoom and no animation.

## Rejected
- Channels, groups and masters with assign keys: a link has no level, so the assign keys
  pretended to be sends.
- A screen (LCD) on every strip, then one per module: they said what the keys already say.
- Patchcord making virtual sinks as groups or masters: it would add nodes to the graph. That is a
  decision of its own.
- From KDE, for now: renaming devices (WirePlumber rules), volume-control settings, app icons
  (often missing), TEST, MUTE ALL OUT / IN, VIRTUAL, MAX 150%.

## Links
Mockup: `docs/design/mixer/index.html`, an exact port of `mixer.rs` (its look is tuned there
first). Code: `src/ui/mixer.rs`; `Scene::reroute`, `group_links`,
`set_channels`; the MIXER key in `ui/panel.rs`. Picture: `cargo test mixer_picture -- --ignored`,
`patchcord --render out.png mixer`.
