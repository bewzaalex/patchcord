# Research: talking to PipeWire from Rust

Patchcord needs to see the PipeWire graph live (devices, nodes, ports, links) and to make and break
links. This spike checks whether the [`pipewire`](https://crates.io/crates/pipewire) crate (the
official freedesktop.org bindings to libpipewire) can do that, built the way the app would be:

- PipeWire runs its own loop on a thread of its own.
- It sends what it sees to the main thread over a `std::sync::mpsc` channel (objects added and
  removed, "everything that existed is reported", errors).
- The main thread sends commands back over `pipewire::channel` (link, unlink, quit).
- The main thread keeps the graph and groups nodes into panel boxes by `device.id`.

## Building

Needs the PipeWire development files (`libpipewire-0.3`, `libspa-0.2`) and libclang, because the
crate generates its C bindings with bindgen at build time. On Gentoo libclang comes with
`llvm-core/clang`. At run time it only needs `libpipewire-0.3.so`, which every PipeWire system has.

```sh
cargo build --release
```

## Running

```sh
./target/release/pw-spike list                 # the graph once, grouped like the panel
./target/release/pw-spike watch                # the graph, then every change live
./target/release/pw-spike link OUT IN          # connect two ports
./target/release/pw-spike unlink OUT IN        # disconnect them
```

A port is `NODE:PORT` (node name, description or nick, then the port name) or a port id, for
example `"Midi-Bridge:Midi Through: Port-0 (capture)"` or `44`.

## Results (2026-11-16, Gentoo, PipeWire 1.6.9, pipewire crate 0.10.1)

| | |
| --- | --- |
| Builds | yes, first try, no warnings (needs libclang) |
| Whole graph after connecting | 1.5–2.3 ms (3 devices, 9 nodes, 17 ports) |
| Create a link until it shows in the graph | 0.5 ms |
| Remove a link until it is gone | 0.2 ms |
| Link outlives the program (`object.linger`) | yes, `pw-link -l` still shows it |
| Live changes (`watch`) | links appearing and going away arrive at once |
| Memory | 5.5 MB RSS, 2 threads |
| Binary | 0.8 MB, links `libpipewire-0.3.so.0` |

The link test used a silent MIDI pair (Midi-Bridge out → BLE MIDI 1 in) and removed it again.

## What we learned for the app

- **Device boxes work by `device.id`.** A card's `Audio/Sink` and `Audio/Source` nodes share it
  (JLab GO Work 2: nodes 90 and 113; Ryzen: nodes 50 and 51). Monitor ports belong to the sink
  node and carry `port.monitor = true`, so the MONITOR group is easy to find.
- **Names:** the device has `device.description` ("Ryzen HD Audio Controller"), nodes have
  `node.description` and sometimes a short `node.nick` ("ALC287 Analog"). Streams and apps have no
  device, so they become a box of their own named by `node.description` or `application.name`.
  The Bluetooth stream's full name is "Bluetooth internal capture stream for JLab GO Work 2".
- **Port types** come from `format.dsp`: "32 bit float mono audio" or "8 bit raw midi".
  `port.direction` is `in` or `out`.
- **Hide driver nodes.** `Dummy-Driver` and `Freewheel-Driver` are nodes with no ports and no
  `media.class`.
- **The ALSA "14:Midi Through" box in qpwgraph is not a PipeWire node.** qpwgraph also shows the ALSA
  sequencer graph; through PipeWire the same ports appear in `Midi-Bridge`, named
  "Midi Through: Port-0 (playback)" and "(capture)". Showing ALSA sequencer clients would be a
  separate backend, if we ever want it.
- **Port names can contain `:`**, so parsing a "node:port" string has to try every split.
- **Keep link proxies** while the app runs; with `object.linger` the links stay after Patchcord
  quits, which is what a patchbay should do.
- The registry also reports clients, modules, factories and more; the app keeps only devices,
  nodes, ports and links, and ignores removals of anything else.

Not tested yet: a device or app appearing while watching (plugging headphones in, starting a
player). The registry delivers those the same way as links, but it is worth seeing once.
