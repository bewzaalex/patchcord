# TODO

The mockup in `docs/design/mockup` is the target. Done so far: the live PipeWire graph as device
panels (desk layout), its links as cords with physics, pan, zoom, dragging devices and cords,
signal meters and node state, the top panel, DESK and RACK layouts, settings and presets in
`~/.config/patchcord` (settings.txt, presets.txt), patching (pull a cord from a free jack, move or drop plugs,
double-click to pull out, SPLIT/JOIN, undo/redo). Other programs' peak-meter streams are hidden.

## Next

- Shift + drag for both cords of a split pair.
- While dragging a plug, mark clearly the jacks it can go into (now only a thin pulsing green
  ring around each free slot, easy to miss): for example light them up and dim the jacks it
  doesn't fit.
- Hover: highlight a device's or jack's cords, tooltips with the full port names.
- Align devices: select several and line them up by their left or right sides, or centres
  (with snapping guides while dragging).

## Then

- Plug sounds (the SOUND key shows a "not implemented yet" notice).

## Later

- Meters for MIDI (activity blink) and, as an option, for inputs (recording a microphone keeps
  it on and switches Bluetooth headsets to their low-quality mode, so not by default).
- A setting to show other programs' peak-meter streams (hidden now).
- Preset options: overwrite or rename a preset; load a preset at start; an "exclusive" mode that
  also takes away links of devices the preset doesn't know.
- Flatpak (`--filesystem=xdg-run/pipewire-0`), .deb, .rpm, like Kadence's release workflow.
- A JACK backend for macOS and Windows, behind a small backend trait.
