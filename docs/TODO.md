# TODO

The mockup in `docs/design/mockup` is the target. Done so far: the live PipeWire graph as device
panels (desk layout), its links as cords with physics, pan, zoom, dragging devices and cords,
signal meters and node state, the top panel, DESK and RACK layouts, settings and presets in
`~/.config/patchcord` (settings.txt, presets.txt), patching (pull a cord from a free jack, move or drop plugs,
double-click to pull out, SPLIT/JOIN, undo/redo; while a plug is in the hand the jacks it fits
glow and the others fade). Other programs' peak-meter streams are hidden. Plugs are right-angle
6.3 mm plugs drawn to scale; SPLIT carries over to the stereo jacks at the other ends of the cords.

## Next

- Shift + drag for both cords of a split pair.
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
- Cheaper cords: the cord layer is drawn on the CPU every animated frame (5–8 ms for two cords
  on a 2560×1440 window, the whole window while a plug is in the hand).
- A JACK backend for macOS and Windows, behind a small backend trait.
