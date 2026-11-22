# A Bluetooth headset's microphone: the virtual source only

Status: accepted · 2026-11-18

## Context
In HSP/HFP the real microphone is an internal node. It feeds WirePlumber's loopback, and the
loopback reaches the virtual source with no PipeWire link. Apps link only to the virtual source,
and that link is what makes WirePlumber switch the profile.

## Decision
Show only the virtual source as the headset's MIC. The internal nodes are listed in HIDDEN as
SYSTEM ("the plumbing behind MIC"), and they are in ENGINE.

## Rejected
A cord to the internal nodes: it would lead nowhere a user can follow.
