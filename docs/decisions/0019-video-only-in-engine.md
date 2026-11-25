# Video only in ENGINE, for now

Status: accepted · 2026-11-25

## Context
Patchcord has shown video nodes since its first version, without ever deciding to: a camera
(`Video/Source`), a screen the compositor shares (`kwin_wayland`'s `Stream/Output/Video`), an
app watching it (`plasmashell`, a video call's `Stream/Input/Video`). They came out wrong on the
rack: a video port has no `format.dsp` (its format is agreed on linking), so its jack was OTHER;
an app's video stream made a device of its own, named after its client, "APP · PLAYBACK"; a
camera was a unit with one OTHER jack. A report from a laptop on Fedora (screen sharing in a call)
filled half the rack with such units. The rack and MIXER are about sound.

## Decision
**Video shows only in ENGINE.** A node whose `media.class` is video (`Video/*`, `Stream/*/Video`:
`block_kind` VIDEO) is left out of the desk, the rack and MIXER (`Graph::hidden`), so a camera
with no other node makes no device either. ENGINE shows them all, as it shows every node, port
and link: the graph stays honest (principles: what isn't on a face is reachable in ENGINE). Links
between video nodes are links in ENGINE, never cords: no cord leads nowhere.

Video on the rack is for later, with an ADR of its own and a mockup (`docs/ROADMAP.md`, "Other
kinds of devices").

## Rejected
- Showing it right on the rack now (VIDEO jacks in ochre, "APP · VIDEO"): a kind of device of its
  own, needing a design; until then, a half-done one misleads.

## Links
`Graph::hidden`, the test `video_is_left_to_engine`. ADR 0014 (its VIDEO colours stay, for
ENGINE).
