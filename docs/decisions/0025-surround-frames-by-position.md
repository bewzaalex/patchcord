# Surround: a node's channels in frames by their positions

Status: accepted · 2026-11-27 · changes "one frame a node" of [0014](0014-a-frame-is-a-block.md)
for nodes of more than two audio channels

## Context
Issue #14. A node with more than two audio channels (an HDMI output in Digital Surround 5.1, a
7.1 card, a 5.1 stream) is one frame today, with one VOLUME knob that sets every channel alike
and no BAL: its channels can't be set one by one, as pavucontrol can with its lock open.

What PipeWire has (a pw-dump of the Radeon HDMI sink in 5.1, 2026-11-27): one node, a port a
channel for each direction (`playback_FL…LFE`, `monitor_FL…LFE`). The ports have no volume of
their own. The node's `Props` has `volume` (one for the node), `mute` (one for the node: there is
no mute a channel) and `channelVolumes` (one a channel, in the order of `channelMap`); a
channel sounds at `volume × channelVolumes[i]`. Every audio port says which channel it carries
in `audio.channel`, from SPA's fixed list of positions (`spa/param/audio/raw.h`: FL, FR, FC, LFE,
RL, RR, SL, SR, FLC, FRC, RC, TFL, TFR… AUX0…AUX63, MONO), the same on every device and stream;
the node lists them in `audio.position`. A professional interface often has no positions, only
AUX0…; one 5.1 device says RL/RR where another says SL/SR.

Seven ways were drawn in `docs/design/channels/` (a tray of trims, a second unit of trims, an AV
receiver's BAL/FADE/CTR/LFE, the meter as faders, MIXER's faders, a block a channel, pairs by
position).

## Decision
**A node of more than two audio channels, with a volume a channel (`channelVolumes`), is shown
as frames by its channels' positions** (variant G), on the rack and on the desk alike (the same
faces):
- **MASTER**: the node's own `volume` (VOLUME) and `mute` (MUTE), and its meter (one column: the
  loudest channel, as a stereo frame's one column is the louder of L and R). It has no jacks, so
  no rule before its knob and no VOLUME heading over it: its name says what it is and runs on
  over the knob; the frame is no wider than its name needs (2026-11-28: it was as wide as its
  name plus a heading, empty at its left).
- **A stereo frame a pair of positions** whose both channels the node has, by a fixed table:
  FL + FR FRONT, RL + RR REAR, SL + SR SIDE, FLC + FRC, TFL + TFR, TRL + TRR, FLW + FRW (each its
  name). Each is a stereo frame as any other: a stereo jack for each of the node's directions on
  those positions (a sink's IN and MON, a source's OUT, a filter's IN and OUT), VOLUME and BAL
  (they set the pair's two `channelVolumes`), SPLIT, a meter column each for L and R. No MUTE:
  PipeWire mutes only the node.
- **The directions go together by position**: a node with inputs and outputs (a filter, a
  virtual 5.1 device) has its FRONT frame hold both its FL/FR inputs and its FL/FR outputs; a
  position only one direction has (an upmix's FC out) is in its frame on that side only.
- **A mono frame a channel without a partner**: FC CENTER, LFE, RC, TC…, AUX0…AUX63, a pair's
  channel whose partner the node hasn't: its jacks, VOLUME (its `channelVolumes`), one column.
- The frames are named by the node's frame name and the part: `HDMI · MASTER`, `HDMI · FRONT`,
  `HDMI · REAR`, `HDMI · CENTER`, `HDMI · LFE`; on a node without positions, `· AUX0` and so on.
- A node of one or two channels stays one frame, as now; so does a node of more channels without
  a volume a channel (a JACK client, a DAW, an interface in its Pro Audio profile without a
  mixer): frames with nothing to set would only be clutter.
- A node whose channels have no positions (AUX0…AUX17 on a big interface) and a volume a channel:
  a mono frame a channel, as for any channel without a partner, however many. Ports that say the
  same position (UNK, or none) are a frame each, numbered: `UNK 1`, `UNK 2`, `MONO 1`… (2026-11-28:
  only the first of them had a frame, the others were left off the face).
- A card's channel volumes are on its route, as for stereo now: a frame's VOLUME and BAL write the
  route's `channelVolumes` (`Cmd::SetRoute`), MASTER's MUTE the route's `mute`; the route has no
  `volume` of its own, so MASTER's VOLUME is the node's `volume`. A stream's and a virtual
  device's are all the node's `Props`.
- Every frame of the node is still that node: a click on a name opens its block in ENGINE, the
  frames keep the node's kind colour, and its ports are all on the device (nothing hidden).
- Nothing saved is lost: cords and presets keep links by node and port name, not by jack, so
  they find the new frames' jacks themselves. A part's SPLIT and volume are kept by its own
  label (PLAYBACK FRONT…), the node's by its MASTER's.
- They wrap onto more units as any frames do (the HDMI 5.1 sink: MASTER, FRONT, REAR in one unit,
  CENTER and LFE in the next), a frame at a time, not the node whole (2026-11-28: a Pro Audio
  node's nine frames, too wide for a unit together, went one a unit). A node so split starts a
  unit of its own, with its MASTER (beside the name and screen, if it is the face's first); a node
  of one or two channels stays beside the frame before it (2026-11-28).

Not decided here: MIXER's own way (variant E, a fader a channel, stays a mockup): it is ADR 0026
(2026-11-28), one strip, its MASTER, whose SPLIT opens the parts beside it.

## Rejected
- One knob for all (today): a channel can't be set alone.
- A tray of trims under the face (A), a second unit of trims under the jacks (B), the meter as
  faders (D): a channel's knob away from its jacks, or too small to hit.
- An AV receiver's BAL, FADE, CTR and LFE (C): five knobs for six channels, each channel's volume
  worked out by Patchcord, not set.
- A block a channel (F): six frames where three pairs read as three stereo pairs; a pair's BAL and
  SPLIT come with G for free.
- MUTE on a pair or a channel: PipeWire has none; it would be Patchcord's own (a volume set to 0).
- Carrying cords over a card's profile change (5.1 to stereo and back): the card's output node is
  made anew (another `node.name`, new ports), and its links go with the old one. Which links a
  new node gets is the session manager's to decide (WirePlumber), not Patchcord's (2026-11-27).

## Links
Code: `Graph::by_position`, `parts_of` and `channel_keys` (`graph.rs`), `Group.part`, `Mix.chans`/`all`/`master`,
`Event::NodeVolume` and `Cmd::SetMix`'s `master` (`pw.rs`), `Scene::set_mix`, `face::sections`
and `host` by node and part; tests `a_surround_node_is_framed_by_position`, `surround_layouts_by_position`, `surround_streams_sources_and_both_ways`, `a_surround_nodes_frames_share_units`, `a_master_starts_a_unit` (`face.rs`); the rack's picture
with `PATCHCORD_PIC_SURROUND=1`.
`docs/design/channels/` (channels.py, variant G), issue #14, ADR 0014 (one frame a node), ADR 0020
(a card's frames' names).
