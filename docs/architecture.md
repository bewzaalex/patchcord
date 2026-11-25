# Architecture

Patchcord is one Rust binary: winit for the window, wgpu for the screen, resvg/tiny-skia for the
pictures, and the `pipewire` crate on a thread of its own.

## The flow

```
PipeWire ──(pw.rs thread)── Event ──► Graph (graph.rs) ──devices()──► Scene (ui/scene.rs) ──► App (ui/app.rs) ──► GPU (ui/gpu.rs)
             ▲                                                                                       │
             └──────────────────────────────── Cmd (link, unlink, volume, profile…) ◄──────────────────┘
```

- **`pw.rs`** owns the PipeWire connection. It runs PipeWire's loop on its own thread, sends what
  it sees as `Event`s and carries out `Cmd`s. Patchcord's own streams live here too: the level
  meters (`patchcord-meter-N`) and the cue (two streams of its own). A stream or proxy drops its
  listener first.
- **`graph.rs`** keeps the raw objects and turns them into panel **devices**. A device is a card
  (nodes sharing `device.id`), a program (every node it made, see `Graph::program`), or a system
  node of its own. A device's ports are **groups**: a node's ports one way (and monitor or not;
  on a MIDI bridge, of one `client`). Each group has a `label` (its frame's name), a `role`
  (`port_role`: PLAYBACK, CAPTURE, MONITOR, INPUT, OUTPUT, MIDI, VIDEO: its jacks' colour) and a
  `kind` (`block_kind`: its node's kind, its frame's colour). Code that needs to know what a
  group *is* looks at `role` or `kind`, never at `label`. ENGINE colours its blocks and ports by
  the same two functions (ADR 0014).
- **`ui/scene.rs`** is everything on the field without a window: devices and their places, cords
  and their physics, meters, cue sources, and the layout of the desk and the rack. `--render`
  uses it too.
- **`ui/face.rs`** draws a device's face as SVG: plate, name, LCD, keys, frames, jacks. A frame
  is a node (`face::sections`: its groups side by side, a MONITOR inside the input it copies).
  It computes the face's `Geometry` (where jacks, keys, knobs and each frame's GRAPH key are)
  for hit-testing.
- **`ui/app.rs`** is the window: input, the top panel, drop-down lists (PROFILE, PORT),
  and switching between MIXER, RACK (its two views, rails and desk) and ENGINE.
- **`ui/engine.rs`** is ENGINE: the real graph (nodes, ports, links by driver), the inspector,
  and the diagnostics tables.
- **`ui/mixer.rs`** is MIXER: a module for each strip of the scene (a device's input or output,
  an app's; `mixer::role_of` says which groups are strips), drawn as one picture each on the
  painter's threads, the faders' caps, knobs, lit LEDs and numbers as small pictures over it. Its keys go through `Scene` (`reroute` for
  PLAY ON, `set_default`, `set_volume`, `set_channels`, `toggle_mute`).
- **`ui/panel.rs`** holds the top panel, the status bar and the amber lists.
- **`ui/cord.rs`** is the cords' physics (Verlet). **`ui/mesh.rs`** draws cords as GPU
  triangles, and **`ui/paint.rs`** draws them on the CPU for pictures only.
- **`ui/painter.rs`** draws pictures on threads, so a frame never waits. **`ui/gpu.rs`** puts
  textures and meshes on screen.
- **`config.rs`** holds settings and presets as text files in `~/.config/patchcord`.
- **`shot.rs`** makes pictures without a window (`--render`, `--record`).
- **`report.rs`** reads a SAVE REPORT file back as PipeWire's events; `shot::from_report` draws
  its rack and MIXER (`--report`). **`tools/report_graph.py`** draws one as a schematic (ADR
  0018). Both read only the report.

## Checking a change

| What | How |
|---|---|
| Lint | `cargo clippy --all-targets` (must be clean) |
| Tests | `cargo test` |
| The rack offline, no PipeWire | `PATCHCORD_PIC=out.png [PATCHCORD_PIC_SIZE=WxH] cargo test rack_picture -- --ignored` |
| The live graph as a picture | `patchcord --render out.png [rack] …` (see `shot.rs`). It starts level meters in PipeWire. |
| A user's report as pictures | `patchcord --report REPORT.txt` (rack and MIXER), `python3 tools/report_graph.py REPORT.txt [OUT.svg\|OUT.png] [--meters]` (the graph) |
| Slow frames | `PATCHCORD_FRAMES=1 target/release/patchcord` |

Releases: see [RELEASING.md](RELEASING.md).
