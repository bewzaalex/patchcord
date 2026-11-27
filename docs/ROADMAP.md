# Roadmap

What we would like Patchcord to do next, and what is waiting. GitHub issues are for users' bug
reports and requests; this file is the maintainer's own list. Nothing here is a promise or has a
date. When something is decided it gets an ADR (`docs/decisions/`) and leaves this file once it
ships; what was turned down goes under **Not now** with the ADR that says why.

## Next

- **ENGINE shows all PipeWire has about an object, on request**: now it shows the registry's
  properties of ports, modules and factories, and of a card's params only what the rack reads
  (its routes and profiles aren't in ENGINE at all: "Speakers" is nowhere there). Opening an
  object would ask for its whole info and every param (a node's error text, a link's format, a
  module's arguments, a card's routes and profiles) and show them as they come. Mockup first.
- **The rack's view switch, a new look**: RAILS ⇄ DESK works (ADR 0016), but the two-position
  knob in VIEW isn't liked. Find another form for it. Mockup first.

## Later

- **A printed border round a rack face**: a line through its four screws' middles, as the tags
  are outlined, broken wherever something is on the face (its name, screen, keys, frames, vents,
  the screws), a little room round each: the face reads as one piece of gear. Tried 2026-11-27
  (an SVG mask with a hole round each thing, `face_border`) and put away for now; its sides
  only, a line down each from screw to screw, are in since the same day.
- **One look for RACK and MIXER**: MIXER found its own forms (frames on the module's edge with a
  two-line band, clear lit keys, amber LCDs for choices, titles between rules, keys that don't
  latch); bring the rack's faces to the same, or settle where they differ and why. Mockups first.

Features beyond what pavucontrol does, roughly in the order they fit "a rack of gear".

- **Virtual devices from the rack**: a null sink (a bus apps play into, patched on) and a loopback
  (one device into another, a mic into headphones) first. An "add unit" key drops a new device in.
  Patchcord would create nodes: that needs an ADR of its own (ADR 0015 turned it down for MIXER's
  groups).
  - A combine sink (one output to several) comes last: playing on two outputs is a few links
    already (MIXER's PLAY ON keys). It earns its place only for what links can't do: being the
    default, so every new stream goes to all of them (WirePlumber gives a new stream one target);
    a target WirePlumber remembers; one volume and MUTE for them all; their latencies evened out
    (`combine.latency-compensate`: Bluetooth lags wired speakers by 150–250 ms); several devices as
    the channels of one (two cards as front and rear). Keeping the clocks together is no reason:
    PipeWire resamples a follower on links just the same.
- **Effects as rack units** (filter-chain): a parametric EQ; a convolver (reverb, room
  correction, headphone surround); LADSPA/LV2 plugins (compressor, limiter, noise suppression);
  WebRTC echo cancelling. Each a device with its own knobs, patched between an app and a speaker.
- **The engine's clock**: buffer size and sample rate (`clock.force-quantum`, `clock.force-rate`)
  as a knob; the panel's screen already shows them.
- **Load and xruns per device** (what `pw-top` shows): a light or a counter on each face, to find
  what crackles.
- **Routing rules**: pin an app to a device (WirePlumber rules), so it stays when the default
  changes.
- **MIXER on its side**: a second view of MIXER with the modules lying across, one under another
  (keys and the fader running left to right), for a tall, narrow window. Mockup first.
- **WirePlumber's settings**: what `wpctl settings` changes, through its `sm-settings` metadata
  (live, kept across restarts when saved): a Bluetooth headset switching to its headset profile
  when an app takes its mic, the profile it prefers, remembering a card's profile and port, apps
  following the default output, moving streams, pausing playback when an output goes, new
  streams' volume. Rules in `~/.config/wireplumber` (renaming, disabling, priorities) are
  something else: files beside PipeWire, applied only on a restart, an ADR of their own.
- **Screenshots by release**: a doc of its own with each release's pictures (DESK, RACK, MIXER,
  ENGINE), so how Patchcord looked stays on record when the README moves on. Made with
  `--render` on a graph without private data, as the README's.
- **Hiding the top panel**: fold it away for more room for the desk, the rack, MIXER or ENGINE,
  and bring it back (a key, and the keyboard).
- **Other kinds of devices**: video on the rack (cameras, screen sharing: which app uses the
  camera; only in ENGINE until then, ADR 0019), network
  audio (AirPlay, RTP/AES67, ROC, remote PulseAudio), a Bluetooth headset's battery on its screen.

## Open problems

- **WirePlumber links an app again** that Patchcord unplugged, on its next rescan (any new node):
  an unplug doesn't last.
- **Meters' monitor ports**: WirePlumber gives every audio input node monitor ports
  (`node.features.audio.monitor-ports`), Patchcord's meters too. Hiding them would break "the
  graph is honest"; untried: asking for none on our own streams and seeing whether WirePlumber
  puts them back.
- **The cue**: its clock drifts from the device's over a long listen; its node is found by name
  only, so a second Patchcord clashes.
- **ENGINE**: a link between two bands may pass through a port's dot; a node linked to itself;
  a sink's input port is heard from raw links, where the desk uses its monitor.
- A preset whose name has " | " in it doesn't load back.
- A DIAGNOSTICS hint when a Bluetooth device follows an ALSA clock (xruns).

## Tidying

- Long functions read hard: `frame`, `press`, `insp_svg`, `run`.
- ENGINE's panels build their SVG every frame; a profile event rebuilds all of ENGINE twice a
  second; the cords' mesh is built every frame. All measured as cheap for now
  (`PATCHCORD_FRAMES=1`): only if a slow frame says so.

## Not now

- From KDE's Audio Volume, for MIXER: renaming devices, volume-control settings, app icons, TEST,
  MUTE ALL, VIRTUAL, MAX 150% (ADR 0015).
- MIXER's MASTER module: removed (ADR 0015); commit 7b3f74a has it.
- Routed links in ENGINE (ADR 0002).
