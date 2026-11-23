#!/usr/bin/env python3
"""Write site/data/release.json from the newest verified GitHub release.

The release is the newest published stable release whose files all carry
a valid signature (scripts/release_verify.py), not GitHub's "latest": anyone
with write access can move that flag or replace an unsigned release's
files. Beside it go data/SHA256SUMS.txt and data/release-tag.txt, and
data/sums/<tag>.txt for every verified release, which the install scripts
trust instead of the release itself. Before the first
signed release the page falls back to "latest" and writes neither file.

Run in the Pages workflow with GH_TOKEN set. The release body is rendered
by GitHub's Markdown API (which sanitizes it), so the page can insert the
HTML as is. With no published release the file says {"release": null}.
"""
import json
import os
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import release_verify  # noqa: E402

REPO = os.environ.get("GITHUB_REPOSITORY", "shinycake/quill")
OUT = sys.argv[1] if len(sys.argv) > 1 else "site/data/release.json"


def gh(*args, stdin=None):
    return subprocess.run(
        ["gh", "api", *args], input=stdin, capture_output=True, text=True
    )


def main():
    key = release_verify.release_key()
    releases = release_verify.gh_json(f"repos/{REPO}/releases?per_page=100")
    data = release_verify.pick(key, releases)
    verified = data is not None
    if any(release_verify.signed(r) for r in release_verify.stable(releases)) and not verified:
        sys.exit("site-release-data: there are signed releases but none verifies; not updating the page")
    res = None
    if data is None:
        res = gh(f"repos/{REPO}/releases/latest")
        if res.returncode == 0:
            data = json.loads(res.stdout)
    release = None
    if data is not None:
        body_html = ""
        if data.get("body"):
            rendered = gh(
                "markdown",
                "--input",
                "-",
                stdin=json.dumps({"text": data["body"], "mode": "gfm", "context": REPO}),
            )
            if rendered.returncode == 0:
                body_html = rendered.stdout
        release = {
            "tag": data["tag_name"],
            "name": data.get("name") or data["tag_name"],
            "published_at": data.get("published_at"),
            "html_url": data["html_url"],
            "verified": verified,
            "body_html": body_html,
            "assets": [
                {
                    "name": a["name"],
                    "size": a["size"],
                    "url": a["browser_download_url"],
                    "sha256": (a.get("digest") or "").removeprefix("sha256:"),
                }
                for a in data.get("assets", [])
            ],
        }
    elif res is not None and res.returncode != 0 and "Not Found" not in res.stderr:
        sys.exit(f"release lookup failed: {res.stderr.strip()}")
    out_dir = Path(OUT).parent
    if verified:
        sums = "".join(
            f"{a['sha256']}  {a['name']}\n" for a in release["assets"] if not a["name"].endswith(".sig")
        )
        (out_dir / "SHA256SUMS.txt").write_text(sums)
        (out_dir / "release-tag.txt").write_text(release["tag"] + "\n")
    # Every verified release's sums, for installs of a named version.
    sums_dir = out_dir / "sums"
    sums_dir.mkdir(parents=True, exist_ok=True)
    for r in releases:
        if r.get("draft") or not release_verify.signed(r) or release_verify.failures(key, r):
            continue
        (sums_dir / f"{r['tag_name']}.txt").write_text("".join(
            f"{a['digest'].removeprefix('sha256:')}  {a['name']}\n"
            for a in r.get("assets", []) if not a["name"].endswith(".sig")
        ))
    with open(OUT, "w") as f:
        json.dump({"release": release}, f, indent=2)
        f.write("\n")
    print(f"wrote {OUT}: {release['tag'] if release else 'no release'}")


if __name__ == "__main__":
    main()
