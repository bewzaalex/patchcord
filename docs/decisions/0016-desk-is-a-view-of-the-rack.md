# DESK is a view of the rack

Status: accepted · 2026-11-25

## Context
LAYOUT had four screens: DESK, MIXER, RACK and ENGINE. DESK and RACK show the same thing, the
devices with their jacks and cords, only laid out differently: anywhere on a desk, or screwed into
a 19" rack's rails. As two screens they doubled LAYOUT's keys, and the desk's own keys (LOCK,
ALIGN, CABLES, ARRANGE...) belonged to both. MIXER (ADR 0015) and ENGINE are other things.

## Decision
- **LAYOUT is MIXER · RACK · ENGINE**, MIXER first: what most people want, then the devices,
  then PipeWire's own graph. RACK shows the devices, in one of two views.
- **The view is a switch in VIEW**, after its keys: a two-position knob as CABLES' is, RAILS at
  its left (the devices in the rack's rails, as ADR 0004) and DESK at its right (anywhere on a
  desk). A click on the knob takes the other view, a click on a printed name that one. It is only
  there while RACK shows, as every key the screen can't use (ADR 0011).
- **Keyboard**: M MIXER; R shows RACK in the view it was last in, V takes the other view (in RACK
  only); E ENGINE. D is gone. LAYOUT's keys still go nowhere back and forth (ADR 0015).
- The view is remembered across restarts, as the rack was (`rack = on` in settings.txt).

## Rejected
- Two keys, RAILS and DESK, in VIEW: a switch says better that it is one thing in two positions.
- RACK pressed again switching the view: LAYOUT's keys stay where they are when pressed again.
- D for the desk and R for the rails: chosen against; R is the screen, V its view.

## Links
ADR 0004 (the rack), 0011 (narrow windows: keys a screen can't use aren't there), 0015 (MIXER,
LAYOUT's keys). Code: `ui/panel.rs` (`Key::Views`, `Key::ViewsAt`, `Part::View`), `App::set_view`.
The panel on each screen: `PATCHCORD_MIXER_PANEL=out.png PATCHCORD_MIXER_PANEL_SCREEN=rails|desk|engine
cargo test mixer_panel -- --ignored`.
