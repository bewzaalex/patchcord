# A card's frames: their names

Status: accepted · 2026-11-25

## Context
A card's frame was named by the port in use (its route, a device param, not a property): a word
from the port's name, SPEAKERS, HEADPHONES, HDMI, MIC. Else SOUND OUT / SOUND IN, and INPUT /
OUTPUT when no profiles were known either.

A laptop on Fedora (Meteor Lake, SOF, the HiFi profile from ALSA UCM) has a node per port: three
HDMI sinks and two microphones. The port's word named three frames HDMI and two MIC, with no way to
tell which is which. A report saved before STATE had no routes or profiles, and every frame there
was INPUT or OUTPUT.

PipeWire names things after what they are made of, not what they do:
- `device.description` comes from the PCI ids database: "Ryzen HD Audio Controller".
- `node.nick` on an HDA card is the kernel's PCM name: "ALC287 Analog", the codec's.
- Only the ports (from the firmware's pin configuration, or UCM) say Speakers or Headphones.

But on a UCM card `node.nick` is what each port is: HDMI 1, Digital Microphone. KDE's own rename
(System Settings, Sound) writes the given name into `node.nick` too, and into `node.description`,
`device.description`, `device.name` and `alsa.card_name` on the node. KMix shows
`node.description`. Plasma's applet shows the port's description when it lists one device, and
`node.description` when it lists several.

## Decision
**A card's frame (`face::name`): the first of these that gives a name.**
1. The port in use as a word (SPEAKERS, HEADPHONES, HDMI, MIC…), when no other frame of the device
   has the same word.
2. `node.nick`, when no other node of the device has the same nick and it isn't the device's own
   name. A name two frames share tells them nothing: a desktop card's sink and source both say
   ALC287 ANALOG, so a codec's name is never taken, with no list of codecs to guess by.
3. The word after all, or what it is: HEADPHONES / MIC on a headset; SOUND OUT / SOUND IN on a card;
   INPUT / OUTPUT with nothing known.

Two frames still of one name: the second and on are numbered (HDMI, HDMI 2).

Only hardware's frames take a nick (the aluminium paint). A program's and the system's are named as
before: what its program named the node, or which way its sound goes.

The name is the same wherever a frame is named: on its band on the rack and the desk, in MIXER's
strip, in the status bar.

## Rejected
- `node.description` with the card's name taken off its front, and the profile's description:
  long ("HDMI / DisplayPort 3 Output (Stereo)"), and the nick already says it.
- A rename winning over the port's word (KDE's rename shows: the node gets a `device.description`
  of its own). It works for KDE only. A given name shows anyway where the port's word doesn't tell
  the frames apart.
- The port's description when a list has one device only (Plasma's applet): the name would change
  with the list, not the device.
- Hiding a device whose only port is unplugged (Plasma's applet): it takes something real off the
  rack (principles: the graph is honest).
- Guessing from ALSA element names (KMix with ALSA alone): a list of words to keep, not what
  PipeWire says.

## Links
`docs/design/names/` (the mockup, on Fedora's report and this machine's card), `face::name`,
`Group::nick`, test `a_cards_frames_are_named_by_port_then_nick`. ADR 0014 (a frame is a block).
