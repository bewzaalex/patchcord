# Cue: listening through streams of Patchcord's own

Status: accepted · 2026-11-20

## Context
Listening to any jack (cue) first used module-loopback. Its shared `node.group`/`node.link-group`
woke devices and pushed DSP over 100%.

## Decision
- The cue is two streams of Patchcord's own (`pw.rs`): a passive capture and a playback into the
  default output, joined by a buffer. The playback waits for 2 cycles of cushion, plays silence
  when dry, and catches up beyond 6.
- Alt + hover is a momentary probe. The CUE key is a mode with a steady LED: hover listens, and a
  click pins a jack. Turning CUE off (or Esc) stops all of them. LEVEL is the cue's volume, saved.
- Only sources whose node is running are heard. The default output and its monitor are refused,
  because they would feed back.
- The pointer stays the system's own; a headphones badge drawn by Patchcord sits beside it.

## Known limits
Clock drift between the two streams; the cue node is found by its name (a second Patchcord
instance clashes).

## Links
`docs/design/cue/`.
