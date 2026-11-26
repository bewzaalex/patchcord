# ISSUES: only what is wrong

Status: accepted · 2026-11-26

## Context
ENGINE's ISSUES listed ten kinds of things, all Patchcord's own rules in `Engine::find_issues`:
PipeWire keeps no list of problems. Only three were PipeWire's facts: a node in error, a link in
error, xruns. The rest were our readings of the graph. Some were problems (a cycle close to its
deadline, a stream resampled). Others were not: an app linked to nothing, a small quantum, two
devices on one clock, a card whose profile is Off, Patchcord's own meters. They came as blue
"notes" in one list with the errors, so a card switched off read as something wrong. Their
coloured dots said nothing once HEALTH's lamps moved to SERVER. And when a node or a link was in
error, PipeWire's own reason was dropped: the list guessed one ("may be busy or gone").

## Decision
**ISSUES lists only what is wrong**, worst first:
- ERROR: a node in error, or a link in error, with **PipeWire's own reason** ("PipeWire says:
  …", `Event::Failed`). When it gives none, the list says so.
- ERROR: xruns (the profiler's count since ENGINE opened).
- WARNING: a driver's cycle over 70% busy (xruns come next).
- WARNING: a stream or device resampled to its driver's rate.

**Its level is a word in its colour**: ERROR in red, WARNING in amber. No dots, nothing to
learn. SERVER's HEALTH counts them the same way: "0 ERRORS · 0 WARNINGS".

## Rejected
- Notes (NO LINKS, QUANTUM, ONE CLOCK, PROFILE OFF, PATCHCORD) in ISSUES, or in a tab of their
  own: they describe how PipeWire works, not something to fix. A card Off shows on the rack; what
  Patchcord's meters cost shows in TOP.
- A coloured dot for the level: a legend to learn for two words.

## Links
`Engine::find_issues`, `Graph::failed`, `pw::Event::Failed`, test `issues_are_only_what_is_wrong`.
ADR 0011 (SERVER), ADR 0018 (the report's ISSUES).
