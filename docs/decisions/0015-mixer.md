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

**A console, by what things do** (2026-11-24, as a hardware one; it was a module a device, which
put a headset's mic and headphones together and the master anywhere). Left to right:
- **INPUTS**: every source (a microphone, a virtual source), the default input first.
- **APPS**: each program's INPUT and OUTPUT (an INPUT first).
- **OUTPUTS**: every sink but the default one; cards whose profile is Off after them.
- **MASTER**: the default output, always at the window's right edge. While the console is wider
  than the window, the rest scrolls under it (a backdrop and a shadow at its left); while it all
  fits, the room between the outputs and MASTER is empty.
- Within a group, by name, A to Z: static, whatever plays.
- Between two groups a wider gap (22) with a groove down it; between two modules 6. Each group's
  name over its first module.

**A module is one strip**, of one width (112 + 12), under whose it is: its device's (or app's)
name, cut with an ellipsis. A card's PROFILE key is once: on its output (else its input), in
DEVICE SETUP, the profile in use at its right, as PORT's port; a card that is off is its name and DEVICE SETUP with
PROFILE.
No screens. Paint by owner, as on the rack: hardware the top panel's metal, a program black,
PipeWire's own olive. Every module is as tall as the window (no shorter than 550): what it gains
is room under the first group of keys; CHANNEL's keys, the fader and the meter keep their size.

**Every group of keys has its title**, its keys in rows under it (17 tall, 23 apart), so they line
up across the console: OUTPUT (MASTER, PLAY ALL HERE), INPUT (MAIN IN, RECORD ALL HERE), PLAY
ON, RECORD FROM; on a device, under its first group, DEVICE SETUP (PROFILE, PORT): what sets up
the card rather than routes the sound; then CHANNEL (CUE, MUTE, SPLIT).

**Every key is the top panel's** (`panel::cap_svg`, its LED as `panel::led_svg`). A key with a LED
(PLAY ON, RECORD FROM, MASTER, MAIN IN, CUE, MUTE, SPLIT) is in while it is on, pressed into its
hole; held down it sinks deeper, and it does what it does when let go over it, as the rack's BAL
knob. The others (PLAY ALL HERE, PORT, PROFILE) act on the press.

**A strip is a group** (`graph::Group`, `mixer::role_of`): audio, no MONITOR, with a volume, or
on a node whose ports go only one way.
- A DAW's engine and a filter that only plays are strips that say NO VOLUME IN PIPEWIRE.
- A node that takes sound in and gives it out without a volume is no strip: an effect inside
  Easy Effects' chain. The rack has it.
- MIDI and video are not in MIXER. Devices hidden on the rack (HIDE, SYSTEM) are not in it
  either.

**A strip's band** is the block kind's colour, with GRAPH (`#id`, opens ENGINE) and the frame's
name.

**An app's strip**
- **PLAY ON** keys go to every sink shown; **RECORD FROM** keys go to every source shown.
- Each key is a push-push key: in, its red LED lit, the app is linked there; out, it isn't.
- A click pushes it in or out: the app plays (records) there too, L to L and R to R (mono to
  both), or no longer. Several can be in at once (2026-11-24; a click used to move the app, and
  Ctrl+click add one).
- One step to undo (`Scene::reroute`). These are links, as cords on the rack are.

**A device's strip**
- **MASTER** (an output's) / **MAIN IN** (an input's): the default (`default.configured.audio.*`);
  in, its amber LED lit, on the default. (It was MAIN OUT; an amber outline round the strip
  went, 2026-11-24.)
- **PLAY ALL HERE / RECORD ALL HERE**: every app that plays (records) somewhere moves here, and
  this becomes the default.
- **PORT**: the card's port list, when it has more than one port.

**Every strip has:**
- **CUE**: its jacks heard in CUE mode, as on the rack.
- **MUTE**.
- **SPLIT**: a fader for L and one for R, for a stereo pair; it stands for BAL.
- **The fader block** (2026-11-24, from `docs/design/mixer/faders.html`'s A): under SPLIT, the
  scale's numbers at the left (0, 25, 50, 75, 100, 150), a line at every 12.5% left of each slot
  (long at the numbers), the slot red past 100%; each cap has its volume (%) on it (red past
  100%), so there is no number under the fader; under it all its name as on the rack's knob:
  VOLUME on what plays (an output, an app's OUTPUT), GAIN on what comes in (a microphone, an
  app's INPUT). The fader goes from 0 to 150%
  (the rack's VOLUME; 153% until 2026-11-24, as pavucontrol's +11 dB), double-click 100%, the
  wheel steps it (Shift: finer).
- **The meter**: at the right, as tall as the fader's slot and level with it, 12 thin LEDs a channel, green, amber, red. It shows the
  sound after the strip's volume, as every meter of Patchcord does now (2026-11-24; a meter used
  to undo a stream's volume and show what it was sent): a fader turned down, the meter falls.

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
