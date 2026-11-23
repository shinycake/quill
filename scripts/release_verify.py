#!/usr/bin/env python3
"""Check Quill release files against the release signing key.

Anyone with write access to the repository can replace, delete or add
release files and move the "latest" flag, so nothing that ships Quill to
users trusts a release by itself (docs/decisions/release-secrets.md).
Every file on a release since signing began has `<file>.sig` beside it: an
Ed25519 signature over "quill-update-v1\\n<version>\\n<name>\\n<sha256>\\n"
made by the release workflow. GitHub reports each file's SHA-256 (`digest`),
so a signature is checked without downloading the file; a replaced file has
a new digest and no longer matches.

    release_verify.py pick            JSON of the newest published stable
                                      release whose files all verify
    release_verify.py check <tag> [name ...]
                                      exit 1 unless the named files (all
                                      files by default) verify
    release_verify.py file <version> <path>
                                      exit 1 unless <path>.sig signs the
                                      local file <path>
    release_verify.py audit           exit 1 if any signed release has a
                                      file that fails, an unsigned release
                                      differs from scripts/legacy-releases.txt,
                                      or "latest" is not the newest verified
                                      release
    release_verify.py legacy          print the snapshot of unsigned releases
                                      (to write scripts/legacy-releases.txt
                                      once, when signing began)

Needs GH_TOKEN (or a gh login) and openssl 3.
"""
import base64
import json
import os
import re
import subprocess
import sys
import tempfile
import urllib.request
from pathlib import Path

REPO = os.environ.get("GITHUB_REPOSITORY", "shinycake/quill")
ROOT = Path(__file__).resolve().parent.parent
# DER prefix of an Ed25519 SubjectPublicKeyInfo; the 32 key bytes follow.
ED25519_SPKI = bytes.fromhex("302a300506032b6570032100")


def release_key() -> bytes:
    src = (ROOT / "src/update_install/signature.rs").read_text(encoding="utf-8")
    m = re.search(r'^const RELEASE_KEY: &str = "([A-Za-z0-9+/=]+)";$', src, re.M)
    if not m:
        sys.exit("release_verify: no RELEASE_KEY in signature.rs")
    key = base64.b64decode(m[1])
    if len(key) != 32:
        sys.exit("release_verify: RELEASE_KEY is not 32 bytes")
    return key


def message(version: str, name: str, sha256: str) -> bytes:
    return f"quill-update-v1\n{version}\n{name}\n{sha256.lower()}\n".encode()


def signature_ok(key: bytes, version: str, name: str, sha256: str, sig_text: bytes) -> bool:
    try:
        sig = base64.b64decode(sig_text.strip(), validate=True)
    except ValueError:
        return False
    if len(sig) != 64 or not re.fullmatch(r"[0-9a-f]{64}", sha256.lower()):
        return False
    with tempfile.TemporaryDirectory() as d:
        pub, msg, sigf = Path(d, "pub.der"), Path(d, "msg"), Path(d, "sig")
        pub.write_bytes(ED25519_SPKI + key)
        msg.write_bytes(message(version, name, sha256))
        sigf.write_bytes(sig)
        r = subprocess.run(
            ["openssl", "pkeyutl", "-verify", "-pubin", "-keyform", "DER", "-inkey", str(pub),
             "-rawin", "-in", str(msg), "-sigfile", str(sigf)],
            capture_output=True,
        )
        return r.returncode == 0


def gh_json(path: str):
    r = subprocess.run(["gh", "api", "--paginate", "--slurp", path], capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"release_verify: gh api {path}: {r.stderr.strip()}")
    pages = json.loads(r.stdout)
    if pages and isinstance(pages[0], list):
        return [item for page in pages for item in page]
    return pages[0] if len(pages) == 1 else pages


def fetch(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": "quill-release-verify"})
    with urllib.request.urlopen(req, timeout=60) as res:
        return res.read(4096)


def version_key(tag: str):
    m = re.fullmatch(r"v(\d+)\.(\d+)\.(\d+)", tag)
    return tuple(int(x) for x in m.groups()) if m else None


def failures(key: bytes, release: dict, names=None) -> list:
    """Problems with a release's files: [] when every file (or every named
    file) has a valid signature and nothing unsigned is on it."""
    tag = release["tag_name"]
    version = tag.removeprefix("v")
    assets = {a["name"]: a for a in release.get("assets", [])}
    wanted = names or [n for n in assets if not n.endswith(".sig")]
    problems = []
    for name in wanted:
        a, sig = assets.get(name), assets.get(name + ".sig")
        if not a:
            problems.append(f"{tag}: {name} is missing")
            continue
        if not sig:
            problems.append(f"{tag}: {name} has no signature")
            continue
        digest = (a.get("digest") or "").removeprefix("sha256:")
        try:
            text = fetch(sig["browser_download_url"])
        except OSError as e:
            problems.append(f"{tag}: {name}.sig could not be read ({e})")
            continue
        if not signature_ok(key, version, name, digest, text):
            problems.append(f"{tag}: {name} does not match its signature")
    if not names:
        for name in assets:
            if name.endswith(".sig") and name.removesuffix(".sig") not in assets:
                problems.append(f"{tag}: {name} signs no file")
    return problems


def stable(releases):
    return [
        r for r in releases
        if not r.get("draft") and not r.get("prerelease") and version_key(r["tag_name"])
    ]


def signed(release) -> bool:
    return any(a["name"] == "SHA256SUMS.txt.sig" for a in release.get("assets", []))


def pick(key, releases):
    for r in sorted(stable(releases), key=lambda r: version_key(r["tag_name"]), reverse=True):
        if signed(r) and not failures(key, r):
            return r
    return None


LEGACY = ROOT / "scripts/legacy-releases.txt"


def legacy_lines(releases) -> list:
    """`<tag> <file> <digest>` for every file on every unsigned release."""
    return sorted(
        f"{r['tag_name']} {a['name']} {a.get('digest') or '-'}"
        for r in releases
        if not r.get("draft") and not signed(r)
        for a in r.get("assets", [])
    ) + sorted(f"{r['tag_name']} (release)" for r in releases if not r.get("draft") and not signed(r))


def main(argv):
    if not argv:
        sys.exit(__doc__)
    key = release_key()
    cmd = argv[0]
    if cmd == "file" and len(argv) == 3:
        import hashlib
        path = Path(argv[2])
        sha = hashlib.sha256(path.read_bytes()).hexdigest()
        sig = Path(str(path) + ".sig")
        ok = sig.is_file() and signature_ok(key, argv[1], path.name, sha, sig.read_bytes())
        if not ok:
            print(f"::error::{path.name} does not match its signature")
        return 0 if ok else 1
    if cmd == "check" and len(argv) >= 2:
        rel = gh_json(f"repos/{REPO}/releases/tags/{argv[1]}")
        problems = failures(key, rel, argv[2:] or None)
        for p in problems:
            print(f"::error::{p}")
        return 1 if problems else 0
    releases = gh_json(f"repos/{REPO}/releases?per_page=100")
    if cmd == "pick":
        r = pick(key, releases)
        print(json.dumps(r))
        return 0
    if cmd == "legacy":
        print("\n".join(legacy_lines(releases)))
        return 0
    if cmd == "audit":
        problems = []
        if LEGACY.is_file():
            known = set(LEGACY.read_text(encoding="utf-8").split("\n")) - {""}
            now = set(legacy_lines(releases))
            for line in sorted(now - known):
                problems.append(f"unsigned release file not in the snapshot: {line}")
            for line in sorted(known - now):
                problems.append(f"unsigned release file changed or removed: {line}")
        for r in stable(releases):
            if signed(r):
                problems += failures(key, r)
        newest = pick(key, releases)
        latest = gh_json(f"repos/{REPO}/releases/latest").get("tag_name")
        if newest and latest != newest["tag_name"]:
            problems.append(f"'latest' is {latest}, but the newest verified release is {newest['tag_name']}")
        for p in problems:
            print(f"::error::{p}")
        print(f"audit: {len(problems)} problem(s); newest verified release: {newest['tag_name'] if newest else 'none'}")
        return 1 if problems else 0
    sys.exit(__doc__)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
