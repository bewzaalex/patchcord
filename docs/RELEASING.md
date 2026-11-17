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
