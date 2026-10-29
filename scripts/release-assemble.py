#!/usr/bin/env python3
"""Assemble a Quill GitHub release: checksums, release notes and latest.json.

    scripts/release-assemble.py --assets DIR --version X.Y.Z --repo OWNER/NAME \
        --changes CHANGES.md [--curated release-notes/vX.Y.Z.md] \
        [--previous-tag vA.B.C] --credentials embedded|none --notes-out NOTES.md
    scripts/release-assemble.py --defuse NOTES.md

--defuse rewrites a hand-written release body in place so that Telegram
@usernames (@gif, @vote) become code spans; run it before `gh release edit
--notes-file`. GitHub turns every bare @name in a release body into a mention
and lists that account under the release's Contributors.

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
        "id": "linux-aarch64",
        "asset": "quill-linux-aarch64-bundle.tar.gz",
        "label": "Linux arm64 (glibc 2.35 or later)",
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
UPDATE_BINARIES = {"linux-x86_64": "quill-linux-x86_64", "linux-aarch64": "quill-linux-aarch64"}

INSTALL = """\
## Installing

**macOS (Apple Silicon, macOS 14+).** Paste this into Terminal to install or
update Quill in Applications:

```sh
curl -fsSL https://raw.githubusercontent.com/shinycake/quill/main/scripts/macos-install.sh | bash
```

It downloads `quill-macos-aarch64.zip`, checks its SHA-256 and signature, and
installs `Quill.app`. Quill opens straight away, with no security warning.

If you download the zip in a browser instead, macOS blocks the first launch,
because Quill is not notarized (it has no paid Apple Developer ID; it is only
ad-hoc signed). Move `Quill.app` to Applications and run
`xattr -dr com.apple.quarantine /Applications/Quill.app` in Terminal, or open
Quill once, then go to System Settings > Privacy & Security, click
**Open Anyway** next to the Quill message, and confirm.

**Linux (x86_64 or arm64, glibc).** Paste this into a terminal to install or
update Quill for your user, with no root needed:

```sh
curl -fsSL https://raw.githubusercontent.com/shinycake/quill/main/scripts/linux-web-install.sh | bash
```

It picks the package for your CPU (`quill-linux-x86_64-bundle.tar.gz` or
`quill-linux-aarch64-bundle.tar.gz`), checks its SHA-256, installs it under
`~/.local` with a desktop entry, and names any system library that is missing.
To do it by hand, unpack the tarball with `tar -xzf`, then run
`quill-linux-<arch>/quill` in place or `quill-linux-<arch>/install.sh` (pass a
prefix to install elsewhere). Quill can update this package in place from
Settings.

**Windows (x86_64, Windows 10+).** Paste this into PowerShell to install or
update Quill for your user, with no administrator rights needed:

```powershell
irm https://raw.githubusercontent.com/shinycake/quill/main/scripts/windows-install.ps1 | iex
```

It downloads `quill-windows-x86_64.zip`, checks its SHA-256, installs it in
`%LOCALAPPDATA%\Programs\Quill` with a Start menu shortcut, and adds Quill to
Settings > Apps so you can remove it there. Quill starts without a SmartScreen
warning (Smart App Control, when turned on, is a separate check and may still
block it). If you download the zip in a browser instead, unzip it anywhere and
run `quill.exe`. The zip is not code-signed, so SmartScreen warns the first
time: choose **More info > Run anyway**. No Visual C++ Redistributable is
needed.
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


# A bare @name that GitHub would link: not part of an email or URL, not already in code.
MENTION = re.compile(r"(?<![\w@/.`-])@([A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?)(?![\w-])")
# GitHub's generated changelog credits authors as "by @login in https://...": keep those.
CREDIT = re.compile(r" by @[A-Za-z0-9-]+ in https://\S+$")
FIRST_CONTRIBUTION = re.compile(r"^\* @[A-Za-z0-9-]+ made their first contribution in https://\S+$")


def defuse_mentions(text: str) -> str:
    """Wrap bare @names outside code in backticks, keeping changelog author credits."""
    out = []
    fenced = False
    for line in text.split("\n"):
        if line.lstrip().startswith("```"):
            fenced = not fenced
            out.append(line)
            continue
        if fenced or FIRST_CONTRIBUTION.match(line):
            out.append(line)
            continue
        credit = CREDIT.search(line)
        head, tail = (line[: credit.start()], line[credit.start():]) if credit else (line, "")
        # Odd segments of a backtick split are inline code.
        parts = head.split("`")
        for i in range(0, len(parts), 2):
            parts[i] = MENTION.sub(r"`@\1`", parts[i])
        out.append("`".join(parts) + tail)
    return "\n".join(out)


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
    whats_new = defuse_mentions(
        "## What's new\n\n" + (curated + "\n\n" if curated else "") + changes
    )

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

`quill-linux-x86_64` and `quill-linux-aarch64` are the bare executables the in-app
updater installs into an existing Linux package. `latest.json` is a machine-readable summary of this release.

{INSTALL}
## Telegram API credentials

{cred}

## Source code and licenses

Quill is MIT-licensed; the source is this tag. Each package lists its
third-party components in `THIRD_PARTY.md`. The LGPL libraries it ships (FFmpeg
and the tgcalls inside libquill_calls) are attached as `quill-source-*`
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
    if len(sys.argv) == 3 and sys.argv[1] == "--defuse":
        path = Path(sys.argv[2])
        path.write_text(defuse_mentions(path.read_text(encoding="utf-8")), encoding="utf-8")
        return
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
