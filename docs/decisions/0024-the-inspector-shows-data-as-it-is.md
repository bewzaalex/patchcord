# The inspector shows PipeWire's data as it is

Status: accepted · 2026-11-27

## Context
ENGINE's inspector (the panel at the right of the graph) mixed PipeWire's data with Patchcord's
readings of it: "output: makes sound" for `direction`, "yes: it does not keep the nodes running"
for `link.passive`, a port's FORMAT written as "32 bit float mono audio" for every audio port
whatever PipeWire said. The first mockup of a new inspector (`docs/design/inspector/`, its first
version) went further: kinds from the rack (SOURCE, PLAYBACK STREAM) in the head, a summary line
on each param ("F32P · 16000 Hz · MONO", "VOL 77%"), volumes turned into percentages and bars,
ranges rewritten ("1 (1…32)"), descriptions of our own beside the Props, a CONVERTER group
PipeWire doesn't have, the user and host left out on the live screen.

The rack and the desk read the graph for the user: that is what they are for. The inspector is
where the user looks when they want to know what PipeWire actually says, to read it themselves.

## Decision
**The inspector shows what PipeWire gives, as it gives it.**
- Keys and values verbatim, spelt as PipeWire spells them: `media.class = Audio/Source`,
  `direction: output`, `channelVolumes: [ 0.167449, 0.167449 ]`,
  `size: { default: 32768, min: 64, max: 2147483647 }`. No units added, no percentages, no
  renamed values, no summaries, no explanations of our own.
- Descriptions only where PipeWire has them, word for word (PropInfo's `description` beside the
  Props key it describes).
- The data in PipeWire's own structure: an object's info fields (state, error, port counts,
  format, filename, args…), its properties, its params by their ids (EnumFormat, Format, Props,
  PropInfo, PortConfig, Latency, Route…), each entry as it comes. Nothing is regrouped into
  groups PipeWire doesn't have.
- Laid out, never changed: grouping by namespace (`node.*`, `media.*`), search with the match lit,
  folding, a click to copy, monospace for values, an id that names another object (`client.id`,
  `link.output.port`) a link to it.
- What Patchcord adds stands apart, under a heading that says it is Patchcord's: where a node is
  in the rack (and SHOW), the numbers worked out from the profiler's (WAIT, BUSY, B/Q, the chart).
  The profiler's own fields are shown as it gives them.
- **PARAMS as an outline, all folded**: a row a param id (EnumFormat, Props, Route…) with its
  count and its first entry on one line; a click opens it, its entries rows of their own the same
  way, each opened by a click; a search opens what matches. A value on one line where it fits
  (`{ default: 1, min: 1, max: 32 }`, `[ "FL", "FR" ]`), only what doesn't fit nests; a struct of
  key, value pairs (Props' `params`, a route's `info`) a pair a row.
- **One order of tabs for every kind of object: INFO, PROPS, PARAMS, TIMING, CONN**, each only
  where the object has it (a node: INFO, PROPS, PARAMS, TIMING; a port: INFO, PROPS, PARAMS, CONN;
  a link, a client, a module, a factory, the core: INFO, PROPS; a device: INFO, PROPS, PARAMS). A
  metadata's entries are on its INFO.
- Nothing hidden on the live screen. MAC addresses, the user and the host are masked only where
  data leaves the machine: `--render` pictures and SAVE REPORT.

## Rejected
- Summaries and plain-language readings in the inspector: useful, but they are our reading, and
  they hide what PipeWire said behind it. The rack, the desk and MIXER are where Patchcord reads
  the graph for the user.
- Rack kinds (SOURCE, PLAYBACK STREAM) in the inspector's head: `media.class` is what PipeWire says.
- PARAMS one param at a time (its ids as tabs) or a table a param (entries as columns): the first
  shows too little at once, the second is too wide for the panel. Every field a row of its own
  (the first layout): too long to read.

## Links
Code: `Engine::insp_svg` and `IBody` (`ui/engine.rs`), `PVal`, `Event::Fields`, `Event::Object`,
`Event::Param` (`pw.rs`: ports, modules and factories bound for their info); the clipboard through
`smithay-clipboard` on the window's Wayland connection (none under X11).
`docs/design/inspector/` (the mockup, redone to this), `docs/principles.md` (The graph is honest),
ADR 0022 (ISSUES keep PipeWire's own reason).
