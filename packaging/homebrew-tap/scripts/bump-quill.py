#!/usr/bin/env python3
"""Point Casks/quill-app.rb at Quill's latest GitHub release.

Reads the release's SHA256SUMS.txt (the checksums the release pipeline
publishes) and rewrites the cask's version and sha256 lines. Prints the new
version when the cask changed, nothing when it was already current. Any
failure (network, rate limit, missing asset, a cask line that did not match)
exits non-zero so the workflow fails instead of reporting a silent success.

GITHUB_TOKEN, when set, authenticates the API call: unauthenticated calls
share the runner IP's 60-per-hour limit and get rate limited.
"""
import json
import os
import re
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

REPO = "shinycake/quill"
ASSET = "quill-macos-aarch64.zip"
CASK = Path(__file__).resolve().parent.parent / "Casks" / "quill-app.rb"
ATTEMPTS = 5
RETRY_CODES = {403, 429, 500, 502, 503, 504}


def get(url: str) -> bytes:
    headers = {"User-Agent": "shinycake-homebrew-tap"}
    token = os.environ.get("GITHUB_TOKEN")
    if token and url.startswith("https://api.github.com/"):
        headers["Authorization"] = f"Bearer {token}"
    for attempt in range(1, ATTEMPTS + 1):
        try:
            with urllib.request.urlopen(urllib.request.Request(url, headers=headers), timeout=60) as r:
                return r.read()
        except urllib.error.HTTPError as e:
            if e.code not in RETRY_CODES or attempt == ATTEMPTS:
                sys.exit(f"GET {url} failed: HTTP {e.code} {e.reason}")
            wait = retry_wait(e, attempt)
            print(f"GET {url}: HTTP {e.code}, retry {attempt} in {wait}s", file=sys.stderr)
        except (urllib.error.URLError, TimeoutError) as e:
            if attempt == ATTEMPTS:
                sys.exit(f"GET {url} failed: {e}")
            wait = 2 ** attempt * 5
            print(f"GET {url}: {e}, retry {attempt} in {wait}s", file=sys.stderr)
        time.sleep(wait)
    sys.exit(f"GET {url} failed")


def retry_wait(e: urllib.error.HTTPError, attempt: int) -> int:
    backoff = 2 ** attempt * 5
    try:
        if e.headers.get("Retry-After"):
            return min(int(e.headers["Retry-After"]), 300)
        if e.headers.get("X-RateLimit-Remaining") == "0" and e.headers.get("X-RateLimit-Reset"):
            return max(1, min(int(e.headers["X-RateLimit-Reset"]) - int(time.time()) + 1, 300))
    except ValueError:
        pass
    return backoff


release = json.loads(get(f"https://api.github.com/repos/{REPO}/releases/latest"))
tag = release.get("tag_name", "")
version = tag.removeprefix("v")
if not re.fullmatch(r"\d{4}\.\d{3,4}\.\d+", version):
    sys.exit(f"unexpected tag {tag!r}")
sums = get(f"https://github.com/{REPO}/releases/download/{tag}/SHA256SUMS.txt").decode()
digest = next((line.split()[0] for line in sums.splitlines() if line.endswith(f"  {ASSET}")), None)
if not digest or not re.fullmatch(r"[0-9a-f]{64}", digest):
    sys.exit(f"no checksum for {ASSET} in {tag}")

text = CASK.read_text()
new, versions = re.subn(r'(?m)^  version ".*"$', f'  version "{version}"', text)
new, sha_lines = re.subn(r'(?m)^  sha256 ".*"$', f'  sha256 "{digest}"', new)
if versions != 1 or sha_lines != 1:
    sys.exit(f"cask has {versions} version and {sha_lines} sha256 lines, expected one each")
if new != text:
    CASK.write_text(new)
    print(version)
