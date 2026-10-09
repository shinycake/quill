#!/usr/bin/env python3
"""Check tdlib/td master against the TDLib pin in src/pins.rs.

Used by .github/workflows/tdlib-watch.yml. Reads TDLIB_GIT_COMMIT and
TDLIB_CMAKE_VERSION from src/pins.rs, fetches the HEAD of tdlib/td master and
the version in its CMakeLists.txt, and, when upstream is newer, writes an issue
title and a Markdown body (compare link, commit count, td_api.tl constructor
diff with TON wallet / TON Connect / on-ramp items collapsed into a count).

Usage:
  scripts/tdlib-watch.py --out DIR [--ours-commit SHA --ours-version X.Y.Z]

Writes DIR/newer ("true"/"false"), and when newer DIR/title.txt and
DIR/body.md. GITHUB_TOKEN, when set, authenticates the GitHub API calls.
Read-only toward tdlib/td: it never writes there.
"""
import argparse
import json
import os
import re
import sys
import urllib.request

UPSTREAM = "tdlib/td"
API = "https://api.github.com"
RAW = "https://raw.githubusercontent.com"
SCHEMA = "td/generate/scheme/td_api.tl"


def fetch(url: str, accept: str = "application/vnd.github+json") -> bytes:
    request = urllib.request.Request(url, headers={"Accept": accept, "User-Agent": "quill-tdlib-watch"})
    token = os.environ.get("GITHUB_TOKEN")
    if token and url.startswith(API):
        request.add_header("Authorization", f"Bearer {token}")
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read()


def read_pin(pins_text: str, name: str) -> str:
    match = re.search(rf'pub const {name}: &str = "([^"]+)";', pins_text)
    if not match:
        sys.exit(f"tdlib-watch: {name} not found in src/pins.rs")
    return match.group(1)


def cmake_version(cmake_text: str) -> str:
    match = re.search(r"project\(TDLib VERSION ([0-9]+\.[0-9]+\.[0-9]+)", cmake_text)
    if not match:
        sys.exit("tdlib-watch: no project(TDLib VERSION ...) in upstream CMakeLists.txt")
    return match.group(1)


def version_tuple(version: str) -> tuple:
    return tuple(int(part) for part in version.split("."))


def constructors(schema_text: str) -> dict:
    """name -> normalized declaration, for every type and function line."""
    out = {}
    for line in schema_text.splitlines():
        line = line.strip()
        if not line or line.startswith("//") or line.startswith("---"):
            continue
        match = re.match(r"^([A-Za-z0-9_]+)\b.*=\s*[A-Za-z0-9_<>.]+;$", line)
        if match:
            out[match.group(1)] = " ".join(line.split())
    return out


def is_ton(name: str) -> bool:
    """TON wallet / TON Connect / on-ramp / wallet-bot constructors."""
    tokens = [t.lower() for t in re.findall(r"[A-Z]?[a-z0-9]+|[A-Z]+(?![a-z])", name)]
    if "ton" in tokens or "wallet" in tokens:
        return True
    return any(a == "on" and b == "ramp" for a, b in zip(tokens, tokens[1:]))


def schema_diff(old_text: str, new_text: str) -> dict:
    old, new = constructors(old_text), constructors(new_text)
    return {
        "added": sorted(set(new) - set(old)),
        "removed": sorted(set(old) - set(new)),
        "changed": sorted(n for n in set(old) & set(new) if old[n] != new[n]),
    }


def section(title: str, names: list) -> str:
    kept = [n for n in names if not is_ton(n)]
    ton = len(names) - len(kept)
    lines = [f"**{title}: {len(names)}**"]
    if ton:
        lines.append(f"- {ton} TON wallet / TON Connect / on-ramp item(s), not used by Quill")
    lines += [f"- `{n}`" for n in kept]
    if not names:
        lines.append("- none")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", required=True)
    parser.add_argument("--ours-commit")
    parser.add_argument("--ours-version")
    args = parser.parse_args()
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    with open(os.path.join(root, "src", "pins.rs"), encoding="utf-8") as pins:
        pins_text = pins.read()
    ours_commit = args.ours_commit or read_pin(pins_text, "TDLIB_GIT_COMMIT")
    ours_version = args.ours_version or read_pin(pins_text, "TDLIB_CMAKE_VERSION")

    head = json.loads(fetch(f"{API}/repos/{UPSTREAM}/commits/master"))["sha"]
    upstream_version = cmake_version(fetch(f"{RAW}/{UPSTREAM}/{head}/CMakeLists.txt", "*/*").decode())
    newer = version_tuple(upstream_version) > version_tuple(ours_version)
    print(f"pinned TDLib {ours_version} @ {ours_commit}; upstream master {upstream_version} @ {head}")
    os.makedirs(args.out, exist_ok=True)
    with open(os.path.join(args.out, "newer"), "w") as f:
        f.write("true" if newer else "false")
    if not newer:
        print("up to date")
        return 0

    compare = json.loads(fetch(f"{API}/repos/{UPSTREAM}/compare/{ours_commit}...{head}"))
    diff = schema_diff(
        fetch(f"{RAW}/{UPSTREAM}/{ours_commit}/{SCHEMA}", "*/*").decode(),
        fetch(f"{RAW}/{UPSTREAM}/{head}/{SCHEMA}", "*/*").decode(),
    )
    title = f"TDLib {upstream_version} available"
    body = "\n\n".join(
        [
            f"tdlib/td master is at **{upstream_version}** (`{head[:9]}`); Quill pins "
            f"**{ours_version}** (`{ours_commit[:9]}`, `src/pins.rs`).",
            f"- Compare: https://github.com/{UPSTREAM}/compare/{ours_commit}...{head}\n"
            f"- Commits: {compare.get('total_commits', '?')}",
            "### td_api.tl constructor diff",
            section("Added", diff["added"]),
            section("Removed", diff["removed"]),
            section("Changed signature", diff["changed"]),
            "Bump checklist: update `src/pins.rs` (commit, CMake version, schema SHA-256 and size), "
            "re-vendor with `scripts/vendor-td-schema.sh`, regenerate "
            "`native/patches/tdlib-quill-takeout-contacts.patch` against the new commit, fix every "
            "removed/changed constructor Quill uses, and rebuild TDLib with `scripts/build-tdlib.sh`.",
            "_Opened by `.github/workflows/tdlib-watch.yml`; it updates this issue instead of opening another._",
        ]
    )
    with open(os.path.join(args.out, "title.txt"), "w") as f:
        f.write(title)
    with open(os.path.join(args.out, "body.md"), "w") as f:
        f.write(body + "\n")
    print(title)
    return 0


if __name__ == "__main__":
    sys.exit(main())
