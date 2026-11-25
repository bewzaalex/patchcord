# Reports: the whole state, and a picture of the graph from one

Status: accepted · 2026-11-25

## Context
SAVE REPORT (ENGINE's DIAGNOSTICS) writes what PipeWire has to a text file for a bug ticket:
the system, the clock, the profiler's timings, ISSUES, the graph, metadata and every object's
properties. What PipeWire gives as params, not properties, was missing: volumes and mute, a
card's profile and its port (Headphones or Speakers) and that port's volume. A report could not
say that an output was at 0% or a card's profile Off. And a report of 36 nodes and 42 links is
hard to picture from its text; the maintainer reads many of them.

## Decision
**A report keeps the whole state**, in a section of its own, STATE, after GRAPH
(`Graph::state_text`):
- every node's volume and mute (`node 106 volume 85% 85% · mute no`);
- every card's profiles, the one it is in marked `*`, with its availability and priority;
- every card's ports, the ones in use marked `*`, with the device they are in use for, the
  card's volume, mute and latency offset there.

**Nothing personal goes in**, as before (principles: private by default): the whole text
passes through the same redaction as the rest (user and host names, the home folder, MAC
addresses to their last two bytes); profiles' and ports' names are the hardware's.

**A picture from a report is a tool, not a feature**: `tools/report_graph.py`, a script beside
the code (Python, nothing to install; a PNG needs `rsvg-convert`). It reads only the report and
draws a schematic: every node a block (its description, `#id`, class and state, a red edge
where the profiler counted xruns, and from STATE its volume, its card's profile and port), every
port a row, every link a line from an output to an input (dashed when passive, grey when not
active), columns as sound flows, the nodes with no link at the bottom; the report's system and
ISSUES over it. Patchcord's own meter nodes are left out unless `--meters`, and the picture says
how many.

**The reporter's rack and MIXER from a report** (2026-11-25, the schematic wasn't enough):
`patchcord --report REPORT.txt` reads the report back as the events PipeWire would have sent
(`report::events`: objects and properties, nodes' and links' states, metadata, STATE) and draws
the rack and MIXER with the app's own code to `REPORT-rack.png` and `REPORT-mixer.png` beside it.
Pictures only, no window: no read-only mode through the app. The LCD and the status bar say
REPORT and its date. What a report doesn't keep isn't drawn: levels, and the reporter's choices on
the desk (hidden devices, places, presets): every device shows, in the rack's own order. A report
saved before STATE has no volumes, profiles or ports: those show at their defaults.

## Rejected
- Opening a report in Patchcord's window, read-only: a read-only mode through the whole app,
  for the maintainer only. Pictures give the same view.
- An HTML page drawing the report as ENGINE: a second ENGINE to keep in step.

## Links
`Engine::report`, `Graph::state_text`, `tools/report_graph.py`, `report.rs`, `shot::from_report`.
