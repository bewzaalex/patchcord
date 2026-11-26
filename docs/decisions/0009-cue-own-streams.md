# Cue: listening through streams of Patchcord's own

Status: accepted · 2026-11-20

## Context
Listening to any jack (cue) first used module-loopback. Its shared `node.group`/`node.link-group`
woke devices and pushed DSP over 100%.

## Decision
- The cue is two streams of Patchcord's own (`pw.rs`): a capture and a playback into the
  default output, joined by a buffer. The playback waits for 2 cycles of cushion, plays silence
  when dry, and catches up beyond 6.
- The CUE key is a mode with a steady LED: a click on a jack listens to it and keeps it on, again
  stops it; the jack under the pointer only gets a thin ring (2026-11-26: hovering listened, and
  Alt + hover was a momentary probe; both went: listening to whatever the pointer crossed). Turning CUE off (or Esc) stops all of them. LEVEL is the cue's volume, saved.
- What is kept on is heard asleep or not: the cue's links into it are active, so it wakes (a mic
  asleep) as a cord into it would, and sleeps again when it is let go (2026-11-26; the links were
  passive and only sources already running were heard: a sleeping mic stayed silent). The cue's
  capture is not `node.passive` either: PipeWire makes every link into a passive node passive. Only what
  is clicked wakes, never what the pointer crosses. The default output and its monitor are
  refused, because they would feed back.
- The pointer stays the system's own; a headphones badge drawn by Patchcord sits beside it.

## Known limits
Clock drift between the two streams; the cue node is found by its name (a second Patchcord
instance clashes).

## Links
`docs/design/cue/`.
