# A program under one roof

Status: accepted · 2026-11-22

## Context
A program with several nodes spread over the rack: Easy Effects made 7–9 devices, and Bitwig
showed as a VIRTUAL DEVICE named after its engine. A DAW would make a mess.

## Decision
- **Who a program is.** It is known by `application.name` + `application.process.binary` +
  `application.process.id`, each from the node, else from its client. The pid alone is not
  enough: every Flatpak is pid 2 inside its sandbox, and an app using PulseAudio comes through
  pipewire-pulse's `pipewire.sec.pid`.
- **Not a program.** A node with `device.id` (hardware), and PipeWire's own processes: the
  daemon, pipewire-pulse and filter-chain (binary `pipewire`), and WirePlumber
  (`wireplumber.daemon`). Their nodes are the system's and keep their own rules.
- **All its nodes, one device**, whatever their `media.class`: streams, its own sinks and
  sources, effects with no class, a JACK-style `Audio/Duplex` node. The kind is APP, the name is
  `application.name`, and the finish is black.
- **Two programs of one name**: the first (by its client's serial) keeps the name, and the next
  get 2, 3. When a name is cut, it is cut before the number.
- **Frame names**:
  - one node on the face: its role, as before (INPUT, OUTPUT);
  - several nodes: `node.description`, as the program named it, unless that is the program's
    own name again; else the role, numbered (OUTPUT 1, 2…);
  - two frames of one name stay so (two SPECTRUMs), and the INSIDE list tells them apart by
    `node.name`;
  - a JACK node's ports one way are one frame, its jacks named by `port.name`.
- **On the face only the edges** (a program with several nodes):
  - its streams;
  - its sink's input and its source's output, which other programs pick by name;
  - any port with a cord to another device (Patchcord's meters don't count).

  A frame comes onto the face when its port gets a cord from outside, and goes back when the cord
  goes. A stream's monitors stay off the face (listed in HIDDEN), as before.
- **INSIDE**, a key under the LCD, shows only when the face doesn't show all the nodes. It opens
  an amber list (like PROFILE) of all the program's nodes, in the order it made them: where each
  sends sound among them, and which are on the face. It is read only. A click on a node shows it
  in ENGINE.
- Device keys: a program with one node keeps `node:<node.name>`; one with several is
  `app:<name>` (`#pid` when two share a name).

## Rejected
- Looking up `.desktop` files or `/proc` for nicer names: only PipeWire data (see principles).
- Grouping by pid alone (Flatpak) or by `pipewire.sec.pid` (pulse).
- Numbering frames the program named alike.
- A push-down tray under the face (mockup B): the list drops over, as PROFILE does, so the rack
  layout doesn't move. This can be revisited.

## Links
`docs/design/apps/` (roof = variants, edges = B up close, flat = variant 1 and the rules on a
live graph); `Graph::program`, `Device::inside`, `panel::inside_menu`.
