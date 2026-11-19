<div align="center">

<img src="assets/icons/io.github.bewzaalex.patchcord.svg" width="96" alt="">

# Patchcord

**A PipeWire patchbay that looks like a rack of real gear.**<br>
Every device is a hardware panel with 6.3 mm jacks, and every connection is a patch cord that
hangs, swings and settles like a real one.

[![License: MIT](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
![Rust 2024](https://img.shields.io/badge/Rust-2024_edition-dea584?logo=rust&logoColor=white)
![Linux](https://img.shields.io/badge/Linux-Wayland_%7C_X11-fcc624?logo=linux&logoColor=black)
![PipeWire](https://img.shields.io/badge/PipeWire-1.0+-4a86c5)

<img src="docs/screenshots/demo.gif" width="900" alt="Patching cords between devices on the desk, dragging a device into line, the rack, and ENGINE with an app's details">

<a href="docs/screenshots/rack.png"><img src="docs/screenshots/rack.png" width="440" alt="The devices in a 19-inch rack"></a>
<a href="docs/screenshots/engine.png"><img src="docs/screenshots/engine.png" width="440" alt="ENGINE: PipeWire's real nodes, ports and links, grouped by driver, with an app's details and the TOP table"></a>

</div>

Patchcord shows the live PipeWire graph, audio and MIDI, with signal meters and the state of each
device. You patch it with the mouse, on a desk or in a 19" rack, and keep your setups as presets.
It also does what pavucontrol does, built into the panels: the volume (or recording gain),
balance and mute of each app and each device beside its jacks, a sound card's profile and ports,
and the default output and input on the top panel. ENGINE shows what PipeWire really has under
the panels: every node, port and link, grouped by the driver that clocks them, with timings,
xruns and what looks wrong. What comes next is in the [issues](https://github.com/bewzaalex/patchcord/issues). The design is the
interactive mockup in [docs/design/mockup](docs/design/mockup/index.html).

## Installation

Every [release](https://github.com/bewzaalex/patchcord/releases/latest) has packages for x86_64
PCs. Download the one for your system from the release's **Assets** and install it as below.
Patchcord needs PipeWire running, and a GPU with Vulkan (OpenGL is the fallback).

### Flatpak

Works on any Linux distribution with Flatpak. It fetches the freedesktop runtime from Flathub if
you don't have it yet:

```sh
flatpak install --user patchcord-*-x86_64.flatpak
```

The Flatpak reaches PipeWire through its socket, so it can see and connect every device and
program. To uninstall: `flatpak uninstall --user io.github.bewzaalex.patchcord`.

### Debian and Ubuntu

For Ubuntu 24.04 and Debian 13 or newer (older ones: use the Flatpak).

```sh
sudo apt install ./patchcord_*_amd64.deb
```

To uninstall: `sudo apt remove patchcord`.

### Fedora and openSUSE

For Fedora 40 and openSUSE Leap 16 or newer, and Tumbleweed (older ones: use the Flatpak).

```sh
sudo dnf install ./patchcord-*.x86_64.rpm        # Fedora
sudo zypper install ./patchcord-*.x86_64.rpm     # openSUSE
```

To uninstall: `sudo dnf remove patchcord` or `sudo zypper remove patchcord`.

### Other Linux

The `.tar.gz` holds the program, which runs on any distribution with glibc 2.39 and PipeWire
1.0 or newer (2024 and later), with its icon and desktop entry:

```sh
tar -xzf patchcord-*-x86_64-linux.tar.gz
./patchcord-*-x86_64-linux/patchcord
```

To add it to the application launcher, see [below](#add-it-to-the-application-launcher).

With every package, Patchcord appears in the application launcher with its icon (the `.tar.gz`
once you add it). There are no automatic updates yet: to update, install the package from a newer
release the same way.

### From source

You need [Rust](https://rustup.rs) 1.87 or newer, the PipeWire development files and libclang
(the `pipewire` crate generates its bindings at build time): `pipewire` and `clang` on Arch,
`media-video/pipewire` and `llvm-core/clang` on Gentoo, `libpipewire-0.3-dev` and `libclang-dev`
on Debian and Ubuntu, `pipewire-devel` and `clang-devel` on Fedora.

```sh
git clone https://github.com/bewzaalex/patchcord.git
cd patchcord
cargo run --release
```

The first build downloads the dependencies and takes a minute or two. After that you can start
the program directly with `./target/release/patchcord`.

### Add it to the application launcher

The packages do this for you. For the `.tar.gz` or a build from source, install Patchcord for
your user with its icon and launcher entry (on Wayland the desktop shows an application's icon
only once its desktop entry is installed).

From the unpacked `.tar.gz`:

```sh
cd patchcord-*-x86_64-linux
install -Dm755 patchcord ~/.local/bin/patchcord
install -Dm644 -t ~/.local/share/applications io.github.bewzaalex.patchcord.desktop
install -Dm644 -t ~/.local/share/icons/hicolor/scalable/apps io.github.bewzaalex.patchcord.svg
```

From the source folder:

```sh
cargo build --release
install -Dm755 target/release/patchcord ~/.local/bin/patchcord
install -Dm644 -t ~/.local/share/applications assets/io.github.bewzaalex.patchcord.desktop
install -Dm644 -t ~/.local/share/icons/hicolor/scalable/apps assets/icons/io.github.bewzaalex.patchcord.svg
```

To uninstall, delete those three files.

## Using it

### Devices and their frames

Every device is a panel; each of its inputs and outputs is a frame with jacks, a name and, for
audio, its own SIG meter (what comes in, or what goes out, after its volume or gain).

| | |
| --- | --- |
| Frame names | an app's say which way the sound goes: INPUT, OUTPUT. A sound card's say what it is, from the port in use, and follow it: SPEAKERS, HEADPHONES, HDMI, BUILT-IN MIC, MIC (SOUND OUT / SOUND IN when the card doesn't tell). A Bluetooth headset has HEADPHONES and MIC |
| IN / OUT tags | which way a cord goes at that frame: into it (IN) or out of it (OUT) |
| REC OUT | inside a sound card output's frame, after a dotted rule: a copy of what it plays, to record it (PipeWire's monitor) |
| OUTPUT 1, OUTPUT 2… | an app with several streams of a kind (Teams in a call records on two and plays on six) is one device with a numbered frame for each, each with its own controls and meter |
| Units | up to four frames side by side; more make the device two or three units high, the frames spread evenly (eight: two units of four), inputs first and outputs last, vents where a unit has nothing to show |
| Colours (K) | coloured bands behind the frames' names by kind (outputs, inputs, apps' outputs and inputs, MIDI), or plain frames; the jacks keep their coloured rings either way |
| HIDE key | the struck-through eye right of a device's name (or H over it) hides a device without cords; with cords it is dimmed. Hidden devices stay hidden after a restart and come back by themselves when something connects to them |
| HIDDEN key | the eye on the top panel (or H over the desk), with how many are hidden: click one to show it again. It also lists, dimmed and marked SYSTEM, what Patchcord always leaves out: other programs' level meters and its own, apps' monitors, and the plumbing behind a Bluetooth headset's MIC |

### Patching

| | |
| --- | --- |
| Drag from a jack's free slot | pull out a cord; drop its end on a jack to link (the jacks it fits glow, the others fade; a jack already patched to it takes no second cord) |
| Drag a cord's end | move it to another jack, or drop it in empty space to unlink |
| Double-click a cord or its end | pull it out (unlink) |
| Drag a cord | grab it and swing it |
| SPLIT / JOIN | one stereo jack, or separate L and R jacks, L above R; the stereo jacks its cords lead to follow, and theirs in turn, and so does a new stream that starts playing into a split pair. On a frame with controls it sits beside MUTE |
| FLOW key (W) | which way each cord goes: a light runs inside every plugged-in cord from its output to its input, one at a time; on by default |
| Hover a jack, cord or device | its cords stand out (even with CABLES on fade or hide) and the others fade; after a moment a tooltip gives the full names |
| Ctrl+Z / Ctrl+Shift+Z | undo / redo |

### Volume, balance, mute

| | |
| --- | --- |
| VOLUME / GAIN knob | in each frame with controls: VOLUME on what plays (an app's OUTPUT, a card's SPEAKERS), GAIN on what comes in (an app's INPUT, a MIC). A sound card's are its own hardware volume, as pavucontrol sets it. From silence to 153% (red past 100%: it can clip): drag up or down (Shift: fine), or the wheel (5%, Shift: 1%); it rests at 100% for a moment, and a double-click sets 100%. The screen shows the value in % and dB while it changes |
| BAL knob | on stereo: the balance between L and R. A push-push knob, as on an old car stereo: click to pop it out, then turn it like VOLUME; click again to push it back in, where it can't be turned by accident |
| MUTE key | silences it; its LED lights red |

### Sound cards

| | |
| --- | --- |
| PROFILE ▾ | which inputs and outputs the card has (analog or HDMI, Bluetooth hi-fi or headset) and, for Bluetooth, its codec; profiles with nothing plugged in are marked. The list stays open to try another; a click elsewhere, Esc or the wheel closes it. A card that is off shows too, so it can be switched on |
| PORT ▾ | where an output plays (speakers, headphones) or an input records from (built-in or plugged-in microphone), unplugged ones marked, and each port's latency offset (− and +, 10 ms with Shift) |
| OUT ▸ / IN ▸ | on the top panel's screen: the default output and input, where new apps play and record. Click for the list: click one to make it the default, or ⌖ to glide to it |

### Listening (cue)

| | |
| --- | --- |
| Hold Alt over the jacks | the jack under the pointer plays in the default output, like a test probe, while Alt is held; a cyan ring with headphones marks it |
| CUE key · LEVEL | CUE mode (LED lit): hover a jack to hear it, click it to keep it playing (it glows; as many as you like, mixed); CUE again or Esc stops them all. LEVEL sets how loud, without touching any app's or device's volume. The output you listen on, and its REC OUT, can't be heard (they would feed back); listening only hears what already flows |

### The desk and the rack

| | |
| --- | --- |
| Drag a device | on the desk it stops at other devices and slides along them, and sticks lightly to their edges and centres, with a guide line (G: place freely; ALIGN is off in the rack); in RACK, drag it up or down to change the order |
| R, or DESK / RACK | devices on a desk, or in a 19" rack (each keeps its desk place) |
| MORE ▾ | in a narrow window the top panel makes room: the logo goes first, then the keys the screen can't use, then groups fold into MORE from the right. MORE drops them below the panel as they are |
| A, or ARRANGE | devices with cords in columns by signal flow (sources left), ordered so cords cross less; the unused ones after them, kind by kind (sound cards, apps, MIDI), in rows shaped to the window so the faces come out as big as they can |
| F | fit everything in the window |
| L | lock devices in place |
| C, or CABLES | cords shown, faded or hidden |
| Drag empty space, or right/middle drag · wheel | pan · zoom |
| Ctrl+S / Ctrl+O, or the LCD | save a new preset / pick one from the list |
| Esc · Ctrl+Q | close a list, leave CUE mode · quit |

### ENGINE: what PipeWire really has

ENGINE (E) shows PipeWire's own graph, read-only, for when you want to know what is really going
on: what the panels leave out, which clock drives what, and why something crackles.

| | |
| --- | --- |
| Blocks | a block per node, grouped under the driver that clocks it (NOT RUNNING: nothing clocks it now, no CPU used). Coloured by what it is: device, app, system, MIDI, video, Patchcord's own meters. Its ports are ears on its edges, inputs left, outputs right; a light shows if it runs, idles, sleeps or had xruns |
| Links | in the colour of the block they come out of; faded when paused, dashed when they only listen (meters), red when the ends share no format. A stereo pair runs side by side, its distance kept |
| Click | a node, port or link turns amber with what belongs to it, and its details open on the right: INFO, PROPS, PARAMS, TIMING, where it is on the desk (SHOW ON DESK goes there), its ports and links |
| ALSO SHOW | Patchcord's meters, MIDI, what isn't running, PipeWire's internal drivers, on and off |
| The strip | the clock, the quantum, the DSP load with its last 45 seconds, xruns, then (when there is room) drivers, server, session, allowed rates and objects; in a narrow window those are behind **i**. HEALTH counts errors, warnings and notes |
| DIAGNOSTICS | the tables below: TOP (as `pw-top`: each driver and its nodes, the last half second's timings), ISSUES (what looks wrong, worst first, with what to do), OBJECTS, METADATA, EVENTS. **SAVE REPORT** writes it all to a text file in your home folder for a bug report, without your user and host names, MAC addresses or the titles of what plays |
| ? or LEGEND | how to read the graph: colours, lights, links, tags, groups |
| Drag · wheel · F · Esc | pan · zoom · fit · deselect; E or DESK / RACK goes back |

Settings and presets are plain text files in `~/.config/patchcord`: `settings.txt` (cords, lock,
lining up, colours, the cord lights, desk or rack, window size, where devices sit, the rack's
order, hidden devices) and `presets.txt` (each preset's links, split jacks, layout, volumes, the
cards' profiles and ports, and the default output and input, by PipeWire's names). Loading a
preset takes away the links between devices it knows that it doesn't have; devices started later
keep theirs. The Flatpak keeps them in `~/.var/app/io.github.bewzaalex.patchcord/config/patchcord`
instead.

`patchcord --render out.png [rack] [demo] [split] [hover | held] [save | presets | defaults | hidden] [profile[=NAME]] [port[=NAME]] [only=NAME,…] [size=WxH]`
draws the current graph to a PNG without opening a window, with its live signal levels: in the
rack, with a few cords added to the picture only (`demo`, for screenshots), every stereo jack
split, the pointer on a jack, a cord's end in the hand, a drop-down or a card's list open, only
some devices, or at a given size (`PATCHCORD_SCALE` sets the scale, 1.5 by default).
`patchcord --render out.png engine [select=NODE] [tab=top|issues|objects|metadata|events]
[itab=info|props|params|timing] [shut=bottom] [legend] [info] [size=WxH]` draws ENGINE the same
way, after letting PipeWire's profiler run for two seconds; MAC addresses in names are masked.
`patchcord --record DIR` plays a short scripted session on the current graph and saves it as
frames for the README's animation; neither changes anything in PipeWire. `PATCHCORD_DEBUG=1`
prints which GPU it uses and PipeWire's errors that are otherwise expected noise;
`PATCHCORD_FRAMES=1` prints every slow frame and what took the time (or `PATCHCORD_FRAMES=FILE`
writes it to a file), for reports about the app feeling slow.

### Bluetooth headsets: when the microphone switches on

A Bluetooth headset can't play in high quality and use its microphone at the same time: its
hi-fi profile (A2DP) has no microphone, and its headset profile (HSP/HFP) has one but sounds
like a phone call. Its MIC is there in every profile, and PipeWire's session manager,
WirePlumber, switches the profile by itself: to the headset profile when an app's input gets
connected to MIC, and back to hi-fi when that cord goes. (Behind MIC is WirePlumber's own
plumbing, the real microphone and a capture stream, listed under SYSTEM in the HIDDEN list.) Patchcord leaves this to
WirePlumber, and shows each switch on the card's screen and in its PROFILE list.

WirePlumber looks again only when a cord to a recording app comes or goes, not when the app
starts or stops recording. Many recording apps, DAWs among them, keep their input open between
takes. So if you switch back to hi-fi by hand while such an app is still connected, it stays
in hi-fi the next time you record, and the recording is silent. To get the microphone back,
pick the headset profile with PROFILE ▾, or pull the cord out of the app's INPUT and plug it
back in: WirePlumber then switches again.

## Development

Bugs and plans are [issues on GitHub](https://github.com/bewzaalex/patchcord/issues), the Flatpak build in
[packaging/flatpak](packaging/flatpak/README.md), and how releases are made in
[docs/RELEASING.md](docs/RELEASING.md).

## License

MIT, see [LICENSE](LICENSE). The Noto fonts in `assets/fonts` are under the SIL Open Font License
(`assets/fonts/OFL.txt`).
