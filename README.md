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

<img src="docs/screenshots/demo.gif" width="900" alt="Patching cords between devices on the desk, dragging a device into line, and switching to the rack and back">

</div>

Patchcord shows the live PipeWire graph, audio and MIDI, with signal meters and the state of each
device. You patch it with the mouse, on a desk or in a 19" rack, and keep your setups as presets.
It also does what pavucontrol does, built into the panels: the volume (or recording gain),
balance and mute of each app and each device beside its jacks, a sound card's profile and ports,
and the default output and input on the top panel. Plug sounds and smaller things come next (see the [issues](https://github.com/bewzaalex/patchcord/issues)). The design is the
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

| | |
| --- | --- |
| Drag from a jack's free slot | pull out a cord; drop its end on a jack to link (the jacks it fits glow, the others fade; a jack already patched to it takes no second cord) |
| Drag a cord's end | move it to another jack, or drop it in empty space to unlink |
| Double-click a cord or its end | pull it out (unlink) |
| SPLIT / JOIN | one stereo jack, or separate L and R jacks, L above R; the stereo jacks its cords lead to follow, and theirs in turn, and so does a new stream that starts playing into a split pair |
| VOLUME / GAIN knob beside the jacks | each app stream and each device's output and input has its controls in the same frame as its jacks: VOLUME on what plays (an app's OUTPUT, a device's PLAYBACK), GAIN on what comes in (an app's INPUT, a microphone's CAPTURE). A sound card's are its own hardware volume, as pavucontrol sets it. From silence to 153% as in pavucontrol (red past 100%: it can clip): drag up or down (Shift: fine), or the wheel (5%, Shift: 1%); it rests at 100% for a moment, and a double-click sets 100%. The screen shows the value in % and dB while it changes |
| BAL knob on a stereo stream | the balance between L and R. A push-push knob, as on an old car stereo: click to pop it out, then turn it like VOLUME; click again to push it back in, where it can't be turned by accident |
| MUTE key | silences the stream; its LED lights red |
| A device with two sets of controls | an app that records and plays (as Audacity) is one device with INPUT and OUTPUT; a sound card has PLAYBACK and CAPTURE. Each frame then has its own meter, after its volume or gain |
| PORT ▾ beside PROFILE | on a sound card whose output or input has several ports: where it plays (speakers, headphones) or records from (internal or plugged-in microphone), unplugged ones marked, and each port's latency offset (− and +, 10 ms with Shift) |
| OUT ▸ / IN ▸ screen on the top panel | the default output and input, where new apps play and record. Click it for the list of outputs and inputs: click one to make it the default, or ⌖ to glide to it on the desk |
| PROFILE ▾ under a card's screen | pick the sound card's profile: which inputs and outputs it has (analog or HDMI, Bluetooth hi-fi or headset) and, for Bluetooth, its codec; profiles with nothing plugged in are marked. The list stays open to try another; a click elsewhere, Escape or the wheel closes it. A card that is off shows too, so it can be switched on |
| Ctrl+Z / Ctrl+Shift+Z | undo / redo |
| Drag a device | move it; on the desk it stops at other devices and slides along them, and its edges and centre stick lightly to those of the others, with a guide line, to line devices up (also right beside, above or below one); in RACK, drag it up or down to change the order |
| R, or DESK / RACK | devices on a desk, or in a 19" rack (each keeps its desk place) |
| Hover a jack, cord or device | its cords stand out (even with CABLES on fade or hide) and the others fade; after a moment a tooltip gives the full names |
| Drag a cord | grab it and swing it |
| Drag empty space, or right/middle drag | pan |
| Wheel | zoom |
| Top panel | undo, redo, save and load presets, lock devices (L), line devices up on the desk or place them freely (G), the CABLES switch show · fade · hide (drag, wheel, click, or C), arrange (A), fit (F) |
| Ctrl+S / Ctrl+O, or the LCD | save a new preset / pick one from the list |
| F | fit everything in the window |
| K | colour each group of jacks by its kind (PLAYBACK, CAPTURE, MONITOR, an app's OUTPUT and INPUT, MIDI), or plain frames |
| A | arrange devices by signal flow |
| Ctrl+Q | quit |

Settings and presets are plain text files in `~/.config/patchcord`: `settings.txt` (cords,
lock, lining up, group colours, desk or rack, window size, where devices sit, the rack's order) and `presets.txt` (each preset's links, split jacks,
layout, volumes, the cards' profiles and ports, and the default output and input, by PipeWire's names). Loading a preset takes away the links between
devices it knows that it doesn't have; devices started later keep theirs. The Flatpak keeps them
in `~/.var/app/io.github.bewzaalex.patchcord/config/patchcord` instead.

`patchcord --render out.png [rack] [demo] [split] [hover | held] [save | presets] [profile[=NAME]] [only=NAME,…] [size=WxH]` draws the current graph to a PNG without
opening a window, with its live signal levels, optionally in the rack, with a few cords added to the picture only (`demo`, for screenshots), with every stereo jack split, with the pointer on a jack, with a cord's end in the hand, with a preset drop-down or a card's profile list open, with only some devices or at a given size (`PATCHCORD_SCALE` sets the scale, 1.5 by default).
`patchcord --record DIR` plays a short scripted session on the current graph (cords patched, a device dragged into line, the rack and back) and saves it as frames for the README's animation; neither changes anything in PipeWire. `PATCHCORD_DEBUG=1` prints which GPU it uses.

### Bluetooth headsets: when the microphone switches on

A Bluetooth headset can't play in high quality and use its microphone at the same time: its
hi-fi profile (A2DP) has no microphone, and its headset profile (HSP/HFP) has one but sounds
like a phone call. Its CAPTURE jack is there in every profile, and PipeWire's session manager,
WirePlumber, switches the profile by itself: to the headset profile when an app's input gets
connected to CAPTURE, and back to hi-fi when that cord goes. Patchcord leaves this to
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
