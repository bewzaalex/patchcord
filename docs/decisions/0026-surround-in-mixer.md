# Surround in MIXER: SPLIT on MASTER opens the parts

Status: accepted · 2026-11-28 · completes [0025](0025-surround-frames-by-position.md) for MIXER

## Context
ADR 0025 split a node of more than two audio channels, with a volume a channel, into frames by
its channels' positions on the rack and the desk (MASTER, FRONT, REAR, CENTER, LFE…) and left
MIXER's way open: MIXER showed such a node as one strip, its MASTER (the node's volume and
mute), its channels out of reach. Five ways were drawn in `docs/design/mixer-surround/` on the
Radeon HDMI card in 5.1: now (one strip); A SPLIT, a fader a channel beside MASTER's; B a strip
a part, always; C B folded, SPLIT on MASTER opening the parts; D one strip two wide, a fader a
part and BAL over a pair's.

## Decision
**Variant C.** The node is one strip, its MASTER, as before, with SPLIT in its row of keys
(CUE, MUTE, SPLIT), as a stereo strip has:
- **Closed** (SPLIT unlit): its block is VOLUME (GAIN on what comes in), its fader the node's
  `volume`, MUTE the node's `mute`, CUE all its channels. The module is one strip wide.
- **Open** (SPLIT lit): its block is MASTER, and its parts' strips stand beside it in the same
  module, in the order the rack has them (FRONT, REAR, SIDE…, CENTER, LFE, AUX0…), the module as
  wide as its strips. Each part's block is named by the part; it has CUE (its own channels) and
  its fader (its channels' volumes); a pair has SPLIT, its L and R faders, as any stereo strip
  (BAL under the knobs); no MUTE on a part: PipeWire mutes only the node.
- **One band for the node**: its device's name, the frame's (`HDMI`) and its #id, across MASTER
  and the open parts; SETUP (MAIN, ALL HERE, PROFILE, PORT) is the node's, over them all. The
  parts' dashed dividers start under SETUP, at the blocks' titles.
- Under the knobs (a low window) the same: MASTER's VOLUME knob, CUE, MUTE and SPLIT; a part's
  VOLUME knob, a pair's BAL, CUE.
- Which nodes are open is kept while Patchcord runs, as a stereo strip's SPLIT in MIXER (not
  across a restart: neither is that one's).

## Rejected
- Now, one strip only: the channels can't be set in MIXER.
- A, a fader a channel: six faders where the rack has three parts; it is not the rack's way, and
  a pair's balance is two faders to move.
- B, a strip a part always: five strips wide for every surround node, whether its channels are
  ever set or not.
- D, one strip, a fader a part: cramped, and BAL over a fader is found nowhere else in MIXER.
- MUTE on a part: PipeWire has none (ADR 0025).

## Links
`docs/design/mixer-surround/` (surround.py, variant C), issue #14, ADR 0025, ADR 0015 (MIXER).
Code: `Strip::surround`, `open`, `part` and `parts_of` in `Mixer::layout`, `row_keys`,
`block_name`, `module_svg` (`ui/mixer.rs`); test `a_surround_node_in_mixer_opens_its_parts`;
`PATCHCORD_PIC_SURROUND_OPEN=1` for `mixer_picture`.
