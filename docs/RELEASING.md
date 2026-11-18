# Releasing

Releases are built by GitHub Actions, never on a developer machine. Pushing a tag `vX.Y.Z` runs
[`.github/workflows/release.yml`](../.github/workflows/release.yml), which builds every package
from that tag in a clean environment and puts them in a **draft** release. Nothing is public until
you publish the draft.

| File | Built on | For |
| --- | --- | --- |
| `patchcord-X.Y.Z-x86_64.flatpak` | the freedesktop 25.08 SDK, with the manifest in [`packaging/flatpak/`](../packaging/flatpak/README.md) | any Linux with Flatpak |
| `patchcord_X.Y.Z-1_amd64.deb` | Ubuntu 24.04 ([cargo-deb](https://github.com/kornelski/cargo-deb)) | Debian, Ubuntu and derivatives |
| `patchcord-X.Y.Z-1.x86_64.rpm` | Ubuntu 24.04 ([cargo-generate-rpm](https://github.com/cat-in-136/cargo-generate-rpm)) | Fedora, openSUSE and derivatives |
| `patchcord-X.Y.Z-x86_64-linux.tar.gz` | Ubuntu 24.04 | any other Linux with glibc 2.39 and PipeWire 1.0 or newer |
| `SHA256SUMS` | | checksums of all the files above |

The `.deb`, `.rpm` and `.tar.gz` share one build. It's made on Ubuntu 24.04, the oldest
whose PipeWire headers the `pipewire` crate builds with, so it runs on systems with the same or a
newer C library and PipeWire. Their contents are set in `Cargo.toml` under
`[package.metadata.deb]` and `[package.metadata.generate-rpm]`.

## Making a release

1. **Version**: set the new version in `Cargo.toml` and run `cargo build`, so `Cargo.lock` follows.
2. **Release notes**: add a `<release>` at the top of `<releases>` in
   `assets/io.github.bewzaalex.patchcord.metainfo.xml`, with the version, today's date and a short
   description. The GitHub release notes are made from it, with the install table added.
   Point the screenshot URLs in the same file at the new tag (`.../patchcord/vX.Y.Z/docs/...`).
3. **Screenshots**, if the look changed. They show your own PipeWire graph, so patch a few cords
   first (or add `demo`, which adds a few cords to the picture only, between devices named in
   `src/main.rs`). Each starts from fresh settings in a temporary folder, so your own settings
   stay as they are:

   ```sh
   cfg=$(mktemp -d)
   XDG_CONFIG_HOME=$cfg PATCHCORD_SCALE=1.5 cargo run --release -- --render docs/screenshots/desk.png
   XDG_CONFIG_HOME=$cfg PATCHCORD_SCALE=1.5 cargo run --release -- --render docs/screenshots/rack.png rack
   rm -rf $cfg
   for f in docs/screenshots/*.png; do magick $f -strip -define png:compression-level=9 $f; done
   ```

   The README's animation is a short scripted session, played on your live graph (nothing is
   linked in PipeWire; the meters show what really plays): an app patched into speakers or headphones it isn't patched to yet, MIDI
   from Midi-Bridge to BLE MIDI, a device dragged into line, the rack and back. So start an app
   playing first; a step whose devices aren't there is skipped:

   ```sh
   cfg=$(mktemp -d) frames=$(mktemp -d)
   XDG_CONFIG_HOME=$cfg cargo run --release -- --record $frames
   ffmpeg -framerate 25 -i $frames/f%04d.png -vf "fps=20,scale=900:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=192:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle" -y docs/screenshots/demo.gif
   rm -rf $cfg $frames
   ```

   The repository's social preview (**Settings** → **General** → **Social preview**, uploaded by
   hand: GitHub has no API for it) is `docs/screenshots/social-preview.png`, 1280×640: the icon,
   the name, what it is and a few features on the left, kept 40 px from the edges, and the window
   with three devices in the rack and their cords on the right (the rack stacks them, so they
   come out big). With some music playing, so the meters show (pick devices that are there, an
   app playing first):

   ```sh
   cfg=$(mktemp -d) t=$(mktemp -d) f=assets/fonts
   XDG_CONFIG_HOME=$cfg PATCHCORD_SCALE=1.15 cargo run --release -- --render $t/win.png demo rack \
     "only=fooyin,JLab GO Work 2,Ryzen" size=1520x1120
   magick $t/win.png -resize 700x \( +clone -alpha extract -fill black -colorize 100 -fill white -draw "roundrectangle 0,0,699,515,14,14" \) \
     -alpha off -compose CopyOpacity -composite -fill none -stroke '#45484d' -strokewidth 2 -draw "roundrectangle 1,1,698,514,14,14" \
     \( +clone -background black -shadow 75x22+0+14 \) +swap -background none -compose over -layers merge +repage $t/window.png
   rsvg-convert -w 92 -h 92 assets/icons/io.github.bewzaalex.patchcord.svg -o $t/icon.png
   magick -size 1280x640 radial-gradient:'#26282c'-'#121314' $t/window.png -geometry +496+38 -composite $t/icon.png -geometry +52+84 -composite \
     -font $f/NotoSans-Black.ttf -fill '#f3f3f3' -pointsize 64 -annotate +54+250 'Patchcord' \
     -font $f/NotoSans-Bold.ttf -fill '#c9ccd1' -pointsize 25 -annotate +56+300 'A PipeWire patchbay that looks' -annotate +56+332 'like a rack of real gear' \
     -font $f/NotoSans-Bold.ttf -fill '#9a9da2' -pointsize 19 -annotate +76+392 'Patch cords that hang and swing' \
     -annotate +76+424 'Live meters, volume, cue' -annotate +76+456 'Desk or 19" rack, presets, undo' \
     -fill '#f0a64a' -draw "circle 61,386 65,386" -draw "circle 61,418 65,418" -draw "circle 61,450 65,450" \
     -font $f/NotoSansMono-Bold.ttf -fill '#f0a64a' -pointsize 16 -annotate +56+540 'LINUX · PIPEWIRE · OPEN SOURCE' \
     -font $f/NotoSansMono-Regular.ttf -fill '#7d8085' -pointsize 15 -annotate +56+568 'github.com/bewzaalex/patchcord' \
     -depth 8 -strip docs/screenshots/social-preview.png
   rm -rf $cfg $t
   ```

4. **Crates**, if `Cargo.lock` changed: `python3 packaging/flatpak/cargo-sources.py`.
5. **Check** locally: `python3 .github/scripts/release.py version` must print the new version.
6. **Commit, tag and push**:

   ```sh
   git commit -am "chore: release X.Y.Z"
   git tag vX.Y.Z
   git push origin main vX.Y.Z
   ```

7. **Wait** for the *Release* workflow under the repository's **Actions** tab (about 15 minutes,
   mostly the Flatpak). If the tag, `Cargo.toml` and the metainfo disagree, it stops at the first
   step and builds nothing.
8. **Review** the draft under **Releases**: the notes, and that all five files are attached.
   Download a package and try it if you like.
9. **Publish** it: **Edit** → **Publish release**.

## Trying the workflow without releasing

**Actions** → **Release** → **Run workflow** runs the checks and builds every package from the
chosen branch, without creating a release. The packages are attached to the run as artifacts
(*linux* and *flatpak*), for testing.

## When something goes wrong

- **The version check fails**: fix the version or the metainfo, then move the tag to the fixed
  commit: `git tag -f vX.Y.Z && git push -f origin vX.Y.Z`. This is only safe while the release is
  still a draft.
- **A build fails after the draft exists**, or you want to rebuild: re-run the workflow from the
  **Actions** tab. The release step then replaces the files in the existing draft.
- **A published release has a bad file**: don't move the tag of a published release; make a new
  patch release instead.
