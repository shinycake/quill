#!/usr/bin/env python3
"""Assemble a Quill GitHub release: checksums, release notes and latest.json.

    scripts/release-assemble.py --assets DIR --version X.Y.Z --repo OWNER/NAME \
        --changes CHANGES.md [--curated release-notes/vX.Y.Z.md] \
        [--previous-tag vA.B.C] --credentials embedded|none --notes-out NOTES.md

DIR holds every file that will be uploaded to the release. This script adds
two more to it: latest.json (the machine-readable manifest the product page
reads) and SHA256SUMS.txt. NOTES.md is the release body. Nothing here talks to
the network; release.yml fetches the generated changelog and uploads the files.
See docs/decisions/codex-release-pipeline.md for the manifest schema.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import re
import sys
from pathlib import Path

# Matches src/about.rs DISCLAIMER (shortened to one line for the release body).
DISCLAIMER = (
    "Quill is an independent, unofficial Telegram client. It is not affiliated with, "
    "endorsed by, or sponsored by Telegram. It is early, experimental software provided "
    "under the MIT License, without warranty of any kind."
)

# Release packages, keyed by the platform id the manifest exposes.
PACKAGES = [
    {
        "id": "macos-aarch64",
        "asset": "quill-macos-aarch64.zip",
        "label": "macOS 14 or later, Apple Silicon",
        "kind": "app-zip",
    },
    {
        "id": "linux-x86_64",
        "asset": "quill-linux-x86_64-bundle.tar.gz",
        "label": "Linux x86_64 (glibc 2.35 or later)",
        "kind": "tarball",
    },
    {
        "id": "windows-x86_64",
        "asset": "quill-windows-x86_64.zip",
        "label": "Windows 10 or later, x86_64",
        "kind": "zip",
    },
]
# Bare executables the in-app updater swaps in place (src/updater.rs
# binary_asset_name: quill-<os>-<arch>). Only the Linux package can self-update.
UPDATE_BINARIES = {"linux-x86_64": "quill-linux-x86_64"}

INSTALL = """\
## Installing

**macOS (Apple Silicon, macOS 14+).** Unzip `quill-macos-aarch64.zip` and move
`Quill.app` to Applications. The app is not notarized (Quill has no Apple
Developer ID; it is only ad-hoc signed), so macOS blocks the first launch:
open Quill once, then go to System Settings > Privacy & Security and click
**Open Anyway** next to the Quill message, and confirm. On macOS 14 you can
instead Control-click Quill.app in Finder and choose **Open**. Alternatively
run `xattr -dr com.apple.quarantine /Applications/Quill.app` in Terminal.

**Linux (x86_64).** `tar -xzf quill-linux-x86_64-bundle.tar.gz`, then either run
`quill-linux-x86_64/quill` in place or `quill-linux-x86_64/install.sh` to install
under `~/.local` (pass a prefix to install elsewhere). Quill can update this
package in place from Settings.

**Windows (x86_64, Windows 10+).** Unzip `quill-windows-x86_64.zip` anywhere and
run `quill.exe`. The zip is not code-signed, so SmartScreen may warn: choose
**More info > Run anyway**. No Visual C++ Redistributable is needed.
"""


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def human(size: int) -> str:
    for unit in ("B", "KB", "MB", "GB"):
        if size < 1024 or unit == "GB":
            return f"{size:.0f} {unit}" if unit == "B" else f"{size:.1f} {unit}"
        size /= 1024
    return str(size)


def changes_section(changes: str, previous_tag: str | None) -> str:
    text = changes.strip()
    # GitHub's generated notes start with "## What's Changed"; we supply our own heading.
    text = re.sub(r"^##\s+What's Changed\s*\n", "", text)
    if not text:
        text = "No merged pull requests since the previous release."
    if previous_tag is None:
        # First release: the list holds every PR ever merged; keep it collapsed.
        count = len(re.findall(r"^\* ", text, flags=re.M))
        text = (
            f"<details><summary>All merged pull requests ({count})</summary>\n\n"
            f"{text}\n\n</details>"
        )
    return text


def build(args: argparse.Namespace) -> None:
    assets = Path(args.assets)
    version = args.version
    tag = f"v{version}"
    repo = args.repo
    base = f"https://github.com/{repo}/releases/download/{tag}"
    date = args.date or dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")

    for name in ("latest.json", "SHA256SUMS.txt"):
        (assets / name).unlink(missing_ok=True)
    files = sorted(p for p in assets.iterdir() if p.is_file())
    info = {p.name: {"size": p.stat().st_size, "sha256": sha256(p)} for p in files}

    missing = [p["asset"] for p in PACKAGES if p["asset"] not in info]
    missing += [n for n in UPDATE_BINARIES.values() if n not in info]
    if missing:
        sys.exit(f"release-assemble: missing assets: {', '.join(missing)}")

    def entry(name: str) -> dict:
        return {"asset": name, "url": f"{base}/{name}", **info[name]}

    curated = ""
    if args.curated and Path(args.curated).is_file():
        curated = Path(args.curated).read_text(encoding="utf-8").strip()
    changes = changes_section(
        Path(args.changes).read_text(encoding="utf-8"), args.previous_tag
    )
    whats_new = "## What's new\n\n" + (curated + "\n\n" if curated else "") + changes

    embedded = args.credentials == "embedded"
    manifest = {
        "schema": 1,
        "name": "Quill",
        "version": version,
        "tag": tag,
        # When the draft was assembled; the GitHub release's published_at is the publish date.
        "date": date,
        "release_url": f"https://github.com/{repo}/releases/tag/{tag}",
        "notes_markdown": whats_new,
        "credentials_embedded": embedded,
        "platforms": {
            p["id"]: {"label": p["label"], "kind": p["kind"], **entry(p["asset"])}
            for p in PACKAGES
        },
        "update_binaries": {k: entry(v) for k, v in UPDATE_BINARIES.items()},
        "sources": [
            entry(n) for n in sorted(info) if n.startswith("quill-source-") or n == "LGPL-SOURCES.txt"
        ],
        "disclaimer": DISCLAIMER,
    }
    manifest_path = assets / "latest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    info["latest.json"] = {"size": manifest_path.stat().st_size, "sha256": sha256(manifest_path)}

    sums = "".join(f"{info[n]['sha256']}  {n}\n" for n in sorted(info))
    sums_path = assets / "SHA256SUMS.txt"
    sums_path.write_text(sums, encoding="ascii")
    info["SHA256SUMS.txt"] = {"size": sums_path.stat().st_size, "sha256": sha256(sums_path)}

    if embedded:
        cred = (
            "This build includes Quill's own Telegram API credentials, so it connects "
            "without any setup. `TELEGRAM_API_ID` / `TELEGRAM_API_HASH` in the environment "
            "(or a `quill.local.env` file) still take precedence."
        )
    else:
        cred = (
            "**This is a credential-free build.** It does not include Telegram API "
            "credentials: set `TELEGRAM_API_ID` and `TELEGRAM_API_HASH` (from "
            "https://my.telegram.org) in the environment, or put them in a "
            "`quill.local.env` file in the working directory, before starting Quill."
        )

    rows = "\n".join(
        f"| {p['label']} | [`{p['asset']}`]({base}/{p['asset']}) | {human(info[p['asset']]['size'])} |"
        for p in PACKAGES
    )
    checksum_lines = "\n".join(f"{info[n]['sha256']}  {n}" for n in sorted(info))
    body = f"""{whats_new}

## Downloads

| Platform | File | Size |
|---|---|---|
{rows}

`quill-linux-x86_64` is the bare executable the in-app updater installs into an
existing Linux package. `latest.json` is a machine-readable summary of this release.

{INSTALL}
## Telegram API credentials

{cred}

## Source code and licenses

Quill is MIT-licensed; the source is this tag. Each package lists its
third-party components in `THIRD_PARTY.md`. The LGPL libraries it ships (FFmpeg,
ntgcalls and the FFmpeg/GLib inside ntgcalls) are attached as `quill-source-*`
archives; `LGPL-SOURCES.txt` has the exact upstream URLs and commits and a
written offer for the corresponding source.

## Checksums (SHA-256)

```
{checksum_lines}
```

---

{DISCLAIMER}
"""
    Path(args.notes_out).write_text(body, encoding="utf-8")
    if len(body) > 125_000:
        sys.exit(f"release-assemble: release body is {len(body)} characters (GitHub allows 125000)")
    print(f"release-assemble: {len(info)} assets, notes {len(body)} chars, credentials {args.credentials}")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--assets", required=True)
    ap.add_argument("--version", required=True)
    ap.add_argument("--repo", required=True)
    ap.add_argument("--changes", required=True)
    ap.add_argument("--curated")
    ap.add_argument("--previous-tag")
    ap.add_argument("--credentials", choices=("embedded", "none"), required=True)
    ap.add_argument("--date")
    ap.add_argument("--notes-out", required=True)
    args = ap.parse_args()
    if not re.fullmatch(r"\d+\.\d+\.\d+", args.version):
        sys.exit(f"release-assemble: version must be X.Y.Z, got {args.version!r}")
    build(args)


if __name__ == "__main__":
    main()
