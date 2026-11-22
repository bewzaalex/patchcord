# The MIDI bridge: a frame for each client

Status: accepted · 2026-11-22

## Context
WirePlumber's MIDI bridge is one node holding every ALSA MIDI client (Midi Through, keyboards,
programs such as Hydrogen). One MIDI frame mixed them all.

## Decision
A node's ports one way are split into frames when they say they belong to different clients:
- each set has its own `port.group` (`client_N`);
- the part of `port.alias` before ':' differs between sets.

The frame is named by that client part. Frames go in ALSA's client order (the number in
`client_N`): Midi Through (14), hardware, then programs (128 and up). The ports stay on the
bridge, not under the program's roof, because PipeWire says they are the bridge's.

Ports that don't say so stay one frame. For example, a JACK app's `out1`, `out2` each have a
`port.group`, but one client in their aliases.

## Known limit
a2jmidid puts every port under one alias client (`a2j:`): one frame.

## Links
`graph::by_client`; `docs/design/apps/flat.png` (section 4).
