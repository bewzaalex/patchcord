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

<img src="docs/screenshots/desk.png" width="720" alt="Devices on the desk, connected with patch cords">

</div>

Patchcord shows the live PipeWire graph, audio and MIDI, with signal meters and the state of each
device. You patch it with the mouse, on a desk or in a 19" rack, and keep your setups as presets.
Plug sounds and smaller things come next (see [docs/TODO.md](docs/TODO.md)). The design is the
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
| Drag from a jack's free slot | pull out a cord; drop it on a jack to link (the jacks it fits glow, the others fade) |
| Drag a plug | move it to another jack, or drop it in empty space to unlink |
| Double-click a cord or plug | pull it out (unlink) |
| SPLIT / JOIN | one stereo jack, or separate L and R jacks; the stereo jacks at the other ends of its cords follow, and so does a new stream that starts playing into a split pair |
| Ctrl+Z / Ctrl+Shift+Z | undo / redo |
| Drag a device | move it; in RACK, drag it up or down to change the order |
| R, or DESK / RACK | devices on a desk, or in a 19" rack (each keeps its desk place) |
| Hover a jack, plug, cord or device | its cords stand out (even with CABLES on fade or hide) and the others fade; after a moment a tooltip gives the full names |
| Drag a cord | grab it and swing it |
| Drag empty space, or right/middle drag | pan |
| Wheel | zoom |
| Top panel | undo, redo, save and load presets, lock devices (L), the CABLES switch show · fade · hide (drag, wheel, click, or C), the SAG knob (drag or wheel), arrange (A), fit (F) |
| Ctrl+S / Ctrl+O, or the LCD | save a new preset / pick one from the list |
| F | fit everything in the window |
| A | arrange devices by signal flow |
| Ctrl+Q | quit |

Settings and presets are plain text files in `~/.config/patchcord`: `settings.txt` (SAG, cords,
lock, desk or rack, window size, where devices sit, the rack's order) and `presets.txt` (each preset's links, split jacks and
layout, by PipeWire's node and port names). Loading a preset takes away the links between
devices it knows that it doesn't have; devices started later keep theirs. The Flatpak keeps them
in `~/.var/app/io.github.bewzaalex.patchcord/config/patchcord` instead.

`patchcord --render out.png [rack] [hover | held] [save | presets]` draws the current graph to a PNG without
opening a window, optionally in the rack, with the pointer on a jack, with a plug in the hand or with a preset drop-down open (`PATCHCORD_SCALE` sets the scale, 1.5 by default). `PATCHCORD_DEBUG=1` prints which GPU it uses.

## Development

The plan is in [docs/TODO.md](docs/TODO.md), the Flatpak build in
[packaging/flatpak](packaging/flatpak/README.md), and how releases are made in
[docs/RELEASING.md](docs/RELEASING.md).

## License

MIT, see [LICENSE](LICENSE). The Noto fonts in `assets/fonts` are under the SIL Open Font License
(`assets/fonts/OFL.txt`).
