#!/usr/bin/env python3
"""Release helper for the release workflow.

    release.py version [TAG]   print the version; with TAG (vX.Y.Z), fail unless the tag,
                               Cargo.toml and the newest metainfo release all agree
    release.py notes VERSION   print the GitHub release notes for VERSION, taken from the
                               metainfo release entry, plus the install instructions
"""

import re
import sys
import tomllib
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
METAINFO = ROOT / "assets" / "io.github.bewzaalex.patchcord.metainfo.xml"


def cargo_version() -> str:
    return tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]


def releases():
    return ET.parse(METAINFO).getroot().findall("./releases/release")


def version(tag: str | None) -> str:
    v = cargo_version()
    problems = []
    newest = releases()[0].get("version") if releases() else None
    if newest != v:
        problems.append(f"newest release in {METAINFO.name} is {newest}, Cargo.toml says {v}")
    if tag is not None and tag != f"v{v}":
        problems.append(f"tag {tag} does not match Cargo.toml version {v} (expected v{v})")
    if problems:
        sys.exit("version check failed:\n  " + "\n  ".join(problems))
    return v


def text(el) -> str:
    return re.sub(r"\s+", " ", "".join(el.itertext())).strip()


def notes(v: str) -> str:
    rel = next((r for r in releases() if r.get("version") == v), None)
    if rel is None:
        sys.exit(f"no release {v} in {METAINFO.name}")
    out = []
    desc = rel.find("description")
    for el in desc if desc is not None else []:
        if el.tag == "p":
            out += [text(el), ""]
        elif el.tag in ("ul", "ol"):
            out += [f"- {text(li)}" for li in el.findall("li")] + [""]
    out += [
        "### Install",
        "",
        "| System | File | Command |",
        "| --- | --- | --- |",
        f"| Any Linux with Flatpak | `patchcord-{v}-x86_64.flatpak` | `flatpak install --user patchcord-{v}-x86_64.flatpak` |",
        f"| Debian, Ubuntu | `patchcord_{v}-1_amd64.deb` | `sudo apt install ./patchcord_{v}-1_amd64.deb` |",
        f"| Fedora | `patchcord-{v}-1.x86_64.rpm` | `sudo dnf install ./patchcord-{v}-1.x86_64.rpm` |",
        f"| openSUSE | `patchcord-{v}-1.x86_64.rpm` | `sudo zypper install ./patchcord-{v}-1.x86_64.rpm` |",
        f"| Other Linux | `patchcord-{v}-x86_64-linux.tar.gz` | unpack, then run `./patchcord` |",
        "",
        "`SHA256SUMS` lists the checksums of all files: `sha256sum -c --ignore-missing SHA256SUMS`.",
        "",
        f"To build from source, see the [README for this version]"
        f"(https://github.com/bewzaalex/patchcord/blob/v{v}/README.md#from-source).",
    ]
    return "\n".join(out) + "\n"


if __name__ == "__main__":
    match sys.argv[1:]:
        case ["version"]:
            print(version(None))
        case ["version", tag]:
            print(version(tag))
        case ["notes", v]:
            print(notes(v), end="")
        case _:
            sys.exit(__doc__)
