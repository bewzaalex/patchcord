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
- Every LAYOUT key shows its own screen and has its own keyboard key: D DESK, M MIXER, R RACK,
  E ENGINE. Pressed again it stays there: no key goes back and forth between two screens.
- DESK stays for now.

**A console, by what things do** (2026-11-24, as a hardware one; it was a module a device, which
put a headset's mic and headphones together and the master anywhere). Left to right:
- **INPUTS**: every source (a microphone, a virtual source), A to Z like the rest: the default
  input isn't put first, so nothing jumps when the default changes.
- **APPS**: each program's INPUT and OUTPUT (an INPUT first).
- **OUTPUTS**: every sink, the default one too (its MAIN lit), and the cards whose profile is
  Off, all A to Z: a card doesn't jump when it goes on or off.
- **No MASTER** (2026-11-25). There was one: a control of its own for the default output, in dark
  steel, held at the window's right edge, the rest scrolling under it (before that, the default
  output's own module moved there, so a device jumped when it became the default). It only
  repeated the default output's module, which is among the OUTPUTS with MAIN lit, and it cost a
  pinned module, a backdrop, a shadow and presses kept from going through it. Commit 7b3f74a
  has it, should it come back.
- Within a group, by name, A to Z: static, whatever plays.
- Between two groups a wider gap (22) with a groove down it; between two modules 3. Each group's
  name over its first module.

**A module is one strip**, of one width (112 + 12), in a frame as the rack's (2026-11-24): on the
module's very edge, its corners the module's, its edge and its band in the block kind's colour.
The band has two lines: the device's (or app's) name, then the frame's, cut with an ellipsis, and
GRAPH (`#id`, opens ENGINE) at the second line's right (it was under the band, and before that on
it beside one line). A card's PROFILE is once: on its output (else its input). A card that is off
has a plain grey band, OFF on its second line (no GRAPH: it has no node), and SETUP with PROFILE.
No screens. Paint by owner, as on the rack: hardware the top panel's metal, a program black,
PipeWire's own olive.

**Every module is as tall as the tallest** (2026-11-25; they were as tall as the window, the room
under the first keys growing with it, which left empty room). The VOLUME (GAIN) block goes right
under the lowest key of any module (every PLAY ON key shows: there is no limit of six): the faders
while the window has room for them, else the rack's knobs; lower still, the console scrolls.

**Every group of keys has its title**, between two thin rules, its keys in rows under it (17 tall,
23 apart), so they line up across the console: on an app PLAY ON or RECORD FROM; on a device SETUP
(it was OUTPUT or INPUT, then DEVICE SETUP under them), its MAIN and ALL HERE in the first row,
then its card's PROFILE and its PORT; then the VOLUME (GAIN) block, one block, its name on top (it
was CHANNEL, with the name under the fader).

**Every function has a form of its own** (2026-11-25; they were all the top panel's key, which
made a muddle of a state, an action and a choice):
- A state, lit (MAIN, CUE, MUTE, SPLIT): a clear key lit from inside, as a DJ mixer's CUE. Off,
  frosted milky plastic; on, the whole cap glows in its colour (MAIN and SPLIT amber, CUE blue,
  MUTE red), its side too, its light on the panel round it; its name black. No blur: fills and
  gradients.
- Where an app goes (PLAY ON, RECORD FROM): the top panel's key (`panel::cap_svg`) with a red LED.
- An action (ALL HERE): the top panel's key, no light.
- A choice with a list (PROFILE, PORT): a small amber LCD, as the top panel's, its name over it,
  the value in it at full width and ▾ at its right; a click opens the list.
- No key latches (2026-11-25; the lit ones used to stay in): a key goes down only while held, does
  what it does when let go over it, and comes back up; its light says whether it is on. A choice
  opens on the press.
- Every key has its own place, always the same: MAIN at the left half of SETUP's first row, ALL
  HERE at the right; CUE, MUTE, SPLIT in three equal places (CUE, MUTE under the knobs). A key
  that would do nothing isn't there, its place empty.

**A strip is a group** (`graph::Group`, `mixer::role_of`): audio, no MONITOR, with a volume, or
on a node whose ports go only one way.
- A DAW's engine and a filter that only plays are strips that say NO VOLUME IN PIPEWIRE.
- A node that takes sound in and gives it out without a volume is no strip: an effect inside
  Easy Effects' chain. The rack has it.
- MIDI and video are not in MIXER. Devices hidden on the rack (HIDE, SYSTEM) are not in it
  either.

**An app's strip**
- **PLAY ON** keys go to every sink shown; **RECORD FROM** keys go to every source shown.
- Each key's red LED is lit where the app is linked; out, it isn't.
- A click links it or unlinks it: the app plays (records) there too, L to L and R to R (mono to
  both), or no longer. Several can be in at once (2026-11-24; a click used to move the app, and
  Ctrl+click add one).
- One step to undo (`Scene::reroute`). These are links, as cords on the rack are.

**A device's strip**, under SETUP
- **MAIN**: the default output (input) (`default.configured.audio.*`), lit amber on the default;
  (It was MAIN OUT, then MASTER and MAIN IN, which mixed up with the MASTER module, 2026-11-25.)
- **ALL HERE**: every app that plays (records) somewhere moves here, and this becomes the
  default. (It was PLAY ALL HERE / RECORD ALL HERE.)
- **PROFILE**: the card's profile list, once a card. **PORT**: its port list, when it has more
  than one port.

**Every strip has the VOLUME (GAIN) block**, named as on the rack's knob: VOLUME on what plays
(an output, an app's OUTPUT), GAIN on what comes in (a microphone, an app's INPUT).
- **Its keys** in one row, each in its place: **CUE** (its jacks heard in CUE mode, as on the rack),
  **MUTE**, **SPLIT** (a fader for L and one for R, for a stereo pair; it stands for BAL). A key
  that would do nothing isn't there (2026-11-24; they were greyed): no MUTE without a volume in
  PipeWire, no SPLIT on a mono strip, no CUE on the default output (what CUE plays in: it would
  feed back, and can't go on).
- **The faders** (2026-11-24, from `docs/design/mixer/faders.html`'s A): under the keys, the
  scale's numbers at the left (0, 25, 50, 75, 100, 150), a line at every 12.5% left of each slot
  (long at the numbers), the slot red past 100%; each cap has its volume (%) on it (red past
  100%), so there is no number under the fader. The fader goes from 0 to 150%
  (the rack's VOLUME; 153% until 2026-11-24, as pavucontrol's +11 dB), double-click 100%, the
  wheel steps it (Shift: finer).
- **The knobs**, when the window is too low for the faders under every key (2026-11-24): the
  rack's VOLUME (GAIN) knob and, on a stereo strip, its BAL knob (push-push, the same knob as on
  the rack: in, flush and still; out, it turns); the name says VOLUME · BAL, across the strip
  over the meter; CUE and MUTE under them; no SPLIT. A knob turns with the pointer going up and down or with the wheel;
  double-click: 100%.
- **The meter**: at the right, 12 thin LEDs a channel, green, amber, red; as tall as the fader's
  slot and level with it, or beside the knobs under the name, its bottom level with the keys'. It shows the
  sound after the strip's volume, as every meter of Patchcord does now (2026-11-24; a meter used
  to undo a stream's volume and show what it was sent): a fader turned down, the meter falls.

The console scrolls by dragging empty space or with the wheel, and has scroll bars where it goes on
beyond the window (2026-11-24): thin ones, as the rack's, but each in a dark lane of its own at
the window's right edge or bottom, which the console stands back from (over the modules, a bar
cut across them). A thumb is carried; a press beside it brings it there. It has no zoom and no animation.

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
