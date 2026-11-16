#!/usr/bin/env python3
"""Writes cargo-sources.json: every crate in Cargo.lock as a Flatpak source.

Flatpak builds have no network, so the manifest downloads the crates from crates.io as sources
and cargo builds offline from them. Run this again whenever Cargo.lock changes:

    python3 packaging/flatpak/cargo-sources.py
"""

import json
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent / "cargo-sources.json"

lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
sources = []
for pkg in lock["package"]:
    source = pkg.get("source", "")
    if not source:
        continue  # the app itself
    if not source.startswith("registry+https://github.com/rust-lang/crates.io-index"):
        raise SystemExit(f"unsupported source for {pkg['name']}: {source}")
    name, version, checksum = pkg["name"], pkg["version"], pkg["checksum"]
    dest = f"cargo/vendor/{name}-{version}"
    sources.append({
        "type": "archive",
        "archive-type": "tar-gzip",
        "url": f"https://static.crates.io/crates/{name}/{name}-{version}.crate",
        "sha256": checksum,
        "dest": dest,
    })
    sources.append({
        "type": "inline",
        "contents": json.dumps({"package": checksum, "files": {}}),
        "dest": dest,
        "dest-filename": ".cargo-checksum.json",
    })

# CARGO_HOME is cargo/ in the build directory; point crates.io at the unpacked crates
sources.append({
    "type": "inline",
    "contents": '[source.vendored-sources]\ndirectory = "cargo/vendor"\n\n'
                '[source.crates-io]\nreplace-with = "vendored-sources"\n',
    "dest": "cargo",
    "dest-filename": "config.toml",
})

OUT.write_text(json.dumps(sources, indent=2) + "\n")
print(f"{OUT.relative_to(ROOT)}: {len(sources) // 2} crates")
