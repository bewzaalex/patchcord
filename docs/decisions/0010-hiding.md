# Hiding devices, HIDDEN and SYSTEM

Status: accepted · 2026-11-18

## Decision
- The HIDE key on every face (or H) hides a device without cords. With cords it is only dimmed.
  Hidden devices stay hidden after a restart and come back by themselves when they get a cord.
- The HIDDEN key lists them, to show again. It also lists, dimmed and marked SYSTEM, what
  Patchcord always leaves off the desk: other programs' level meters, its own meters and cue,
  apps' recording monitors, and the plumbing behind a Bluetooth MIC. (Recording monitors are on
  their frames since [0014](0014-a-frame-is-a-block.md).)

## Rejected
- Right-click to hide: people expect a menu there.
- A Show filter (All / Applications / Virtual), as in pavucontrol: cords would lead nowhere.
