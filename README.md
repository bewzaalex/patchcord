# Patchcord

A PipeWire patchbay that looks like a rack of real gear. Every device is a hardware panel with
6.3 mm jacks, and connections are patch cords that hang, swing and settle like real ones, with
right-angle plugs drawn to scale.

Work in progress: it shows the live PipeWire graph with signal meters, and you patch it with the
mouse, on a desk or in a rack, with presets. Plug sounds and smaller things come next (see [docs/TODO.md](docs/TODO.md)).
The design is the interactive mockup in [docs/design/mockup](docs/design/mockup/index.html).

## Building

Needs Rust, the PipeWire development files (`libpipewire-0.3`) and libclang, because the
`pipewire` crate generates its bindings at build time. At run time it needs PipeWire and a GPU
with Vulkan (OpenGL is the fallback).

```sh
cargo build --release
./target/release/patchcord
```

## Using it

| | |
| --- | --- |
| Drag from a jack's free slot | pull out a cord; drop it on a jack to link (the jacks it fits glow, the others fade) |
| Drag a plug | move it to another jack, or drop it in empty space to unlink |
| Double-click a cord or plug | pull it out (unlink) |
| SPLIT / JOIN | one stereo jack, or separate L and R jacks; the stereo jacks at the other ends of its cords follow, and so does a new stream that starts playing into a split pair |
| Ctrl+Z / Ctrl+Shift+Z | undo / redo |
| Drag a device | move it; in RACK, drag it up or down to change the order |
| R, or DESK / RACK | devices on a desk, or in a 19" rack (each keeps its desk place) |
| Drag a cord | grab it and swing it |
| Drag empty space, or right/middle drag | pan |
| Wheel | zoom |
| Top panel | undo, redo, save and load presets, lock devices (L), the CABLES switch show · fade · hide (drag, wheel, click, or C), the SAG knob (drag or wheel), arrange (A), fit (F) |
| Ctrl+S / Ctrl+O, or the LCD | save a new preset / pick one from the list |
| F | fit everything in the window |
| A | arrange devices by signal flow |
| Ctrl+Q | quit |

Settings and presets are plain text files in `~/.config/patchcord`: `settings.txt` (SAG, cords,
lock, desk or rack, window size, where devices sit, the rack's order) and `presets.txt` (each preset's links, split jacks and
layout, by PipeWire's node and port names). Loading a preset takes away the links between
devices it knows that it doesn't have; devices started later keep theirs.

`patchcord --render out.png [rack] [held] [save | presets]` draws the current graph to a PNG without
opening a window, optionally in the rack, with a plug in the hand or with a preset drop-down open (`PATCHCORD_SCALE` sets the scale, 1.5 by default). `PATCHCORD_DEBUG=1` prints which GPU it uses.

## License

MIT, see [LICENSE](LICENSE). The Noto fonts in `assets/fonts` are under the SIL Open Font License
(`assets/fonts/OFL.txt`).
