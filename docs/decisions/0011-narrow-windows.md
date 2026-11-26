# Narrow windows

Status: accepted · 2026-11-19

## Decision
- The top panel stays one row. A key the screen can't use isn't on it at all (2026-11-25; it was
  greyed, and went first when the panel was too wide): the desk's keys in ENGINE and MIXER, ALIGN
  in the rack. When it is too wide, in this order: the logo goes, then groups fold from the right
  into MORE ▾. MIXER·RACK·ENGINE never fold. A click off MORE's drop-down closes it and does what it does where it lands (a jack heard in
  CUE mode at once, a plug taken): it isn't spent on closing (2026-11-23).
- ENGINE has no strip over its graph (2026-11-26; it showed CLOCK, QUANTUM, DSP LOAD, XRUNS,
  HEALTH first and the rest behind an "i" cell): all of it is DIAGNOSTICS' first tab, SERVER, one
  row each with what it means, so the graph has the whole height. SAVE REPORT is on DIAGNOSTICS'
  tab row.
- When the status bar's two texts meet, the hints on the right go first.

## Links
`docs/design/narrow/`; issue #16; released in 0.5.0.
