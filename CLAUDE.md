# Patchcord

A PipeWire patchbay for Linux, drawn as a rack of hardware: devices are gear with 6.3 mm jacks,
links are patch cords with physics. Rust (winit, wgpu, resvg, the `pipewire` crate).

## Rules that always hold

- **Everything written into the repository is in English**: code, comments, commit messages,
  docs, ADRs, mockups and their text, issues and release notes. Conversation with the
  maintainer may be in another language; what lands in the repo is not.

@docs/principles.md

## Before you change something

- **Design work** (anything visible): read the ADRs it touches in `docs/decisions/`, then make or
  update a mockup in `docs/design/<topic>/`. Write code only once the mockup is agreed.
- **A new decision**, or a changed one: write an ADR (`docs/decisions/NNNN-*.md` from
  `0000-template.md`) and add it to the index there, in the same commit as the code.
- **Code**: where things are is in `docs/architecture.md`. To know what a frame is, use
  `Group.role`, never its `label`.

## Checking

- `cargo clippy --all-targets` must be clean, and `cargo test` must pass.
- To see the rack without PipeWire: `PATCHCORD_PIC=out.png cargo test rack_picture -- --ignored`.
- `patchcord --render` reads the live graph but starts level meters in it. On someone's own
  machine, ask before running it.
- Tests and checks must not change the live audio graph (no test sinks or links) unless the
  person asks.

## Docs to keep in step

`README.md` (what users see), `docs/decisions/` (why), `docs/architecture.md` (where),
`docs/design/README.md` (the mockups), `docs/RELEASING.md` (how to release).
