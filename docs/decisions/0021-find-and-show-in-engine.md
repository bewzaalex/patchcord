# FIND · SHOW in ENGINE; its keys along the graph's top

Status: accepted · 2026-11-26

## Context
ENGINE's graph had a row of ALSO SHOW chips over its top (PATCHCORD, MIDI, NOT RUNNING,
INTERNAL), and the LEGEND and DIAGNOSTICS keys at its bottom. A graph of forty nodes had no way to
find one: Teams' six streams, a device by its id from a report. The chips didn't say what they
showed or how many, and three places over the graph held keys.

## Decision
**One row of keys over the graph's top left**: FIND · SHOW, LEGEND, DIAGNOSTICS. Nothing at its
bottom.
- FIND · SHOW says how many blocks its switches hide (`· 6 HIDDEN`).
- LEGEND (?) opens the legend under it.
- DIAGNOSTICS (D) shows the tables below or puts them away. It counts the issues, and its arrow
  says which way a click moves them: down while they show, up while they are away. No light. The
  tab row's own arrow is gone.
- One card is open at a time: FIND's or the legend's.

**FIND · SHOW's card** (the key, or Ctrl+F; Esc closes it):
- A field, ready to type. A block matches by its name, description, id, class or a port's name,
  whatever the case.
- The blocks that match turn amber, as chosen ones, and the rest of the graph fades.
- Enter or ↓ goes to the next one, top to bottom and round; Shift+Enter or ↑ to the previous one.
  It is chosen, its details open and the graph moves to show it.
- The field says how many match and which one is chosen ("2 of 6").
- A match a switch hides is said ("1 more hidden by NOT RUNNING"), with a key to show it: FIND
  never says "none" of what is there.
- ALSO SHOW's four switches are in the card, a row each: its light, its name, what it shows, and
  how many nodes. A click turns it on or off.
- While the card is open, what is typed goes to its field: ENGINE's letter keys (R, M, F, D) wait.

## Rejected
- The chips over the graph as they were, and a field beside them: two places for one question
  ("where is it?").
- Filters by kind (devices, apps, the system) or by media: FIND finds by name already; not wanted
  yet.

## Links
`docs/design/find/` (the mockup), `Engine::overlay_svg`, `Engine::find_svg`, `Engine::found`,
`Engine::find_step`, test `find_picks_out_goes_round_and_says_what_is_hidden`. ADR 0002 (ENGINE).
