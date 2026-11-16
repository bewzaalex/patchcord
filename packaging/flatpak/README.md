# Flatpak

`io.github.bewzaalex.patchcord.yml` builds Patchcord on the freedesktop runtime with the Rust SDK
extension. The sandbox reaches PipeWire through its socket (`--filesystem=xdg-run/pipewire-0`):
Patchcord lists, links and unlinks every node, which PipeWire's portal for sandboxed apps
doesn't allow. Flatpak builds have no network, so `cargo-sources.json` lists every crate from
`Cargo.lock` as a download; cargo then builds offline from them. The manifest builds the source
it sits in: your working copy here, and the release tag in the release workflow
([`docs/RELEASING.md`](../../docs/RELEASING.md)), which makes the bundle for each release.

Patchcord isn't on Flathub. Flathub doesn't accept manifests written with AI help, so a Flathub
submission needs its own manifest, written and submitted by a person.

## Build and test

You need the builder, the freedesktop SDK and its Rust and LLVM extensions as flatpaks (LLVM
brings libclang, which the `pipewire` crate needs at build time; the extensions are big downloads):

```sh
flatpak install --user flathub org.flatpak.Builder org.freedesktop.Sdk//25.08 \
    org.freedesktop.Sdk.Extension.rust-stable//25.08 org.freedesktop.Sdk.Extension.llvm21//25.08
```

Build in `build/flatpak/`, so everything the builder writes stays there, its cache in `.flatpak-builder`
included. The first build downloads all crates in `Cargo.lock` (later builds reuse them):

```sh
mkdir -p build/flatpak && cd build/flatpak
flatpak run org.flatpak.Builder --user --force-clean --repo=repo \
    --compose-url-policy=full --mirror-screenshots-url=https://dl.flathub.org/media/ \
    app ../../packaging/flatpak/io.github.bewzaalex.patchcord.yml
flatpak install --user ./repo io.github.bewzaalex.patchcord
flatpak run io.github.bewzaalex.patchcord
```

Check the manifest and the build against Flathub's rules, still in `build/flatpak/`:

```sh
flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest ../../packaging/flatpak/io.github.bewzaalex.patchcord.yml
flatpak run --command=flatpak-builder-lint org.flatpak.Builder repo repo
```

Both should pass with no errors. The two URL options put the screenshots and icons in the catalog
with their Flathub addresses, as Flathub's own builds do.

To make a bundle file from your build, still in `build/flatpak/`:

```sh
flatpak build-bundle --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
    repo patchcord-dev-$(flatpak --default-arch).flatpak io.github.bewzaalex.patchcord master
```

## Crates

When `Cargo.lock` changes, regenerate the crate list and commit it with the lock file:

```sh
python3 packaging/flatpak/cargo-sources.py
```

## Clean up

From the top folder of the repository:

```sh
flatpak uninstall --user io.github.bewzaalex.patchcord
rm -rf build/flatpak
```
