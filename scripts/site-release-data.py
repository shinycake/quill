#!/usr/bin/env python3
"""Write site/data/release.json from the latest published GitHub release.

Run in the Pages workflow with GH_TOKEN set. The release body is rendered
by GitHub's Markdown API (which sanitizes it), so the page can insert the
HTML as is. With no published release the file says {"release": null}.
"""
import json
import os
import subprocess
import sys

REPO = os.environ.get("GITHUB_REPOSITORY", "shinycake/quill")
OUT = sys.argv[1] if len(sys.argv) > 1 else "site/data/release.json"


def gh(*args, stdin=None):
    return subprocess.run(
        ["gh", "api", *args], input=stdin, capture_output=True, text=True
    )


def main():
    res = gh(f"repos/{REPO}/releases/latest")
    release = None
    if res.returncode == 0:
        data = json.loads(res.stdout)
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
    elif "Not Found" not in res.stderr:
        sys.exit(f"release lookup failed: {res.stderr.strip()}")
    with open(OUT, "w") as f:
        json.dump({"release": release}, f, indent=2)
        f.write("\n")
    print(f"wrote {OUT}: {release['tag'] if release else 'no release'}")


if __name__ == "__main__":
    main()
