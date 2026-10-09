#!/usr/bin/env python3
"""Weekly dependency watch for Quill (see docs/dependency-updates.md).

Run by .github/workflows/deps-watch.yml. Each source below produces at most one
issue; the issue is identified by a hidden marker `<!-- deps-watch:KEY -->` in
its body, so a later run updates the same issue (new version in the title)
instead of opening another, and closes it when the update has landed.

Sources (KEY):
  tdlib            TDLib master vs src/pins.rs (scripts/tdlib-watch.py logic)
  gpui-kit         gpui-kit on crates.io vs the =pin in Cargo.toml, plus the
                   vendored gpui-base / gpui-pre-* copies in third_party/
  crates           cargo update --dry-run (compatible) + newer majors (crates.io)
  native:ffmpeg    scripts/build-ffmpeg.sh TAG vs FFmpeg tags
  native:rlottie   scripts/build-rlottie.sh PIN vs Samsung/rlottie master
  native:ntgcalls  scripts/vendor-ntgcalls.sh vs pytgcalls/ntgcalls releases
  native:rust      rust-toolchain.toml vs rust-lang/rust latest release
  actions          `uses:` refs in .github/workflows vs each action's latest release

Usage:
  scripts/deps-watch.py --dry-run [--only KEY ...]   print what would be filed
  scripts/deps-watch.py --apply                      open/update/close issues via gh

--dry-run never writes anything and never calls gh. A source that fails to
fetch is reported and skipped; its issue is neither updated nor closed.
"""
import argparse
import importlib.util
import json
import os
import re
import subprocess
import sys
import time
import urllib.error
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
API = "https://api.github.com"
CRATES = "https://crates.io/api/v1/crates"
LABEL = "deps-watch"
MARKER = "<!-- deps-watch:{} -->"
MARKER_RE = re.compile(r"<!-- deps-watch:([A-Za-z0-9:_.-]+) -->")
DOC = "docs/dependency-updates.md"
FOOTER = (
    f"_Opened by `.github/workflows/deps-watch.yml` (see `{DOC}`). It updates this issue "
    "on later runs and closes it when the update lands._"
)
# Gap between crates.io requests (their crawler policy asks for at most 1/s).
CRATES_DELAY = 1.0


# --------------------------------------------------------------------- http


def fetch(url: str, accept: str = "application/json") -> bytes:
    request = urllib.request.Request(url, headers={"Accept": accept, "User-Agent": "quill-deps-watch"})
    token = os.environ.get("GITHUB_TOKEN")
    if token and url.startswith(API):
        request.add_header("Authorization", f"Bearer {token}")
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read()


def get_json(url: str):
    return json.loads(fetch(url))


def read(rel: str) -> str:
    with open(os.path.join(ROOT, rel), encoding="utf-8") as f:
        return f.read()


# ------------------------------------------------------------- versions


def vtuple(version: str) -> tuple:
    """'n8.1.3' / 'v3.0.0' / '1.8.68' -> (8, 1, 3). Ignores any suffix."""
    return tuple(int(p) for p in re.findall(r"\d+", version.split("-")[0]))


def breaking_key(version: str) -> tuple:
    """Leading components up to the first non-zero one: the semver-breaking position."""
    parts = vtuple(version)
    for i, part in enumerate(parts):
        if part:
            return parts[: i + 1]
    return parts or (0,)


def is_breaking_newer(latest: str, locked: str) -> bool:
    return breaking_key(latest) > breaking_key(locked)


def table(headers: list, rows: list) -> str:
    out = ["| " + " | ".join(headers) + " |", "|" + "|".join(" --- " for _ in headers) + "|"]
    out += ["| " + " | ".join(str(c) for c in row) + " |" for row in rows]
    return "\n".join(out)


class Issue:
    def __init__(self, key: str, title: str, body: str, labels=()):
        self.key = key
        self.title = title
        self.labels = [LABEL, *labels]
        self.body = MARKER.format(key) + "\n" + body.strip() + "\n\n" + FOOTER + "\n"


# --------------------------------------------------------------- sources


def load_tdlib_watch():
    spec = importlib.util.spec_from_file_location("tdlib_watch", os.path.join(ROOT, "scripts", "tdlib-watch.py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def src_tdlib():
    watch = load_tdlib_watch()
    result = watch.check(*watch.pins(ROOT))
    if not result:
        return None
    title, body = result
    return Issue("tdlib", title, body, ["tdlib-update"])


def kit_pin(cargo_toml: str) -> str:
    match = re.search(r'\[dependencies\.gpui-kit\][^\[]*?version\s*=\s*"=?([0-9][^"]*)"', cargo_toml)
    if not match:
        raise RuntimeError("gpui-kit version pin not found in Cargo.toml")
    return match.group(1)


def vendored_versions(root: str = ROOT) -> dict:
    """third_party/<crate>/Cargo.toml package versions (name -> version)."""
    out = {}
    base = os.path.join(root, "third_party")
    if not os.path.isdir(base):
        return out
    for name in sorted(os.listdir(base)):
        path = os.path.join(base, name, "Cargo.toml")
        if os.path.isfile(path):
            with open(path, encoding="utf-8") as f:
                text = f.read()
            match = re.search(r'\[package\][^\[]*?\nversion\s*=\s*"([^"]+)"', text)
            if match:
                out[name] = match.group(1)
    return out


def crate_latest(name: str) -> str:
    time.sleep(CRATES_DELAY)
    data = get_json(f"{CRATES}/{name}")["crate"]
    return data.get("max_stable_version") or data["max_version"]


def src_gpui_kit():
    pinned = kit_pin(read("Cargo.toml"))
    latest = crate_latest("gpui-kit")
    kit_newer = vtuple(latest) > vtuple(pinned)
    behind = []
    for name, ours in vendored_versions().items():
        theirs = crate_latest(name)
        if vtuple(theirs) > vtuple(ours):
            behind.append((name, ours, theirs))
    if not kit_newer and not behind:
        print(f"gpui-kit: pinned {pinned}, latest {latest}; vendored crates current")
        return None
    parts = []
    if kit_newer:
        title = f"gpui-kit {latest} available"
        parts.append(
            f"`gpui-kit` **{latest}** is on crates.io; Quill pins `=` **{pinned}** in `Cargo.toml`.\n\n"
            f"- Release notes: https://github.com/longbridge/gpui-component/releases/tag/v{latest} "
            "(all releases: https://github.com/longbridge/gpui-component/releases)\n"
            f"- Crate: https://crates.io/crates/gpui-kit/{latest}"
        )
    else:
        title = "GPUI vendored crates behind crates.io"
        parts.append(f"`gpui-kit` is current (`={pinned}`), but vendored copies in `third_party/` are behind.")
    if behind:
        parts.append(
            "### Vendored crates (patched copies in `third_party/`)\n\n"
            + table(["crate", "vendored", "crates.io"], [(f"`{n}`", o, t) for n, o, t in behind])
            + "\n\nThese carry Quill patches (pristine commit + patch commit); a Kit bump must re-vendor "
            "them, so Dependabot/auto-merge are deliberately not used for them."
        )
    parts.append(f"Procedure: the gpui-kit section of `{DOC}`.")
    return Issue("gpui-kit", title, "\n\n".join(parts))


def parse_cargo_update(text: str) -> list:
    """Lines like `Updating serde v1.0.1 -> v1.0.2` -> [(name, old, new)]."""
    return [
        (m.group(1), m.group(2), m.group(3))
        for m in re.finditer(r"^\s*Updating (\S+) v(\S+) -> v(\S+)", text, re.M)
    ]


def direct_registry_deps(metadata: dict) -> dict:
    """name -> highest locked version, for the root package's crates.io dependencies."""
    root_id = metadata["resolve"]["root"]
    root = next(p for p in metadata["packages"] if p["id"] == root_id)
    locked = {}
    for pkg in metadata["packages"]:
        if (pkg.get("source") or "").startswith("registry+"):
            cur = locked.get(pkg["name"])
            if cur is None or vtuple(pkg["version"]) > vtuple(cur):
                locked[pkg["name"]] = pkg["version"]
    out = {}
    for dep in root["dependencies"]:
        if (dep.get("source") or "").startswith("registry+") and dep["name"] in locked:
            out[dep["name"]] = locked[dep["name"]]
    return out


def find_majors(deps: dict, latest_of) -> list:
    rows = []
    for name, locked in sorted(deps.items()):
        if name == "gpui-kit":
            continue  # own source
        latest = latest_of(name)
        if is_breaking_newer(latest, locked):
            rows.append((name, locked, latest))
    return rows


def src_crates():
    proc = subprocess.run(
        ["cargo", "update", "--dry-run"], cwd=ROOT, capture_output=True, text=True, check=True
    )
    compat = [c for c in parse_cargo_update(proc.stderr + proc.stdout)]
    meta = subprocess.run(
        ["cargo", "metadata", "--format-version", "1"], cwd=ROOT, capture_output=True, text=True, check=True
    )
    majors = find_majors(direct_registry_deps(json.loads(meta.stdout)), crate_latest)
    if not compat and not majors:
        print("crates: nothing to update")
        return None
    parts = [f"{len(compat)} semver-compatible update(s) and {len(majors)} newer major(s) among direct dependencies."]
    if compat:
        parts.append(
            "### Compatible (`cargo update`)\n\nRun `cargo update`, gate, commit `Cargo.lock`.\n\n"
            + table(["crate", "locked", "available"], [(f"`{n}`", o, t) for n, o, t in compat])
        )
    if majors:
        parts.append(
            "### Newer major (needs a `Cargo.toml` change)\n\nDirect dependencies only; check each changelog for "
            "breaking changes.\n\n"
            + table(
                ["crate", "locked", "latest", "link"],
                [(f"`{n}`", o, t, f"https://crates.io/crates/{n}/{t}") for n, o, t in majors],
            )
        )
    title = f"Cargo dependency updates available ({len(compat)} compatible, {len(majors)} major)"
    return Issue("crates", title, "\n\n".join(parts))


def newest_release(repo: str):
    """Newest non-draft, non-prerelease release tag of a GitHub repo, or None."""
    for rel in get_json(f"{API}/repos/{repo}/releases?per_page=15"):
        if not rel.get("draft") and not rel.get("prerelease"):
            return rel["tag_name"], rel["html_url"]
    return None


def native_issue(key, label, pinned, latest, url, where, extra=""):
    title = f"{label} {latest} available (pinned {pinned})"
    body = (
        f"{label} **{latest}** is available upstream; Quill pins **{pinned}** in `{where}`.\n\n"
        f"- Upstream: {url}\n"
        f"- Procedure: the native dependencies section of `{DOC}`.\n{extra}"
    )
    return Issue(key, title, body)


def ffmpeg_tags(pages=3) -> list:
    tags = []
    for page in range(1, pages + 1):
        tags += [t["name"] for t in get_json(f"{API}/repos/FFmpeg/FFmpeg/tags?per_page=100&page={page}")]
    return [t for t in tags if re.fullmatch(r"n\d+\.\d+(\.\d+)?", t)]


def src_ffmpeg():
    pinned = re.search(r"^TAG=(n[0-9.]+)$", read("scripts/build-ffmpeg.sh"), re.M).group(1)
    latest = max(ffmpeg_tags(), key=vtuple)
    if vtuple(latest) <= vtuple(pinned):
        print(f"ffmpeg: pinned {pinned}, latest {latest}")
        return None
    return native_issue(
        "native:ffmpeg", "FFmpeg", pinned, latest,
        f"https://github.com/FFmpeg/FFmpeg/tree/{latest}", "scripts/build-ffmpeg.sh (TAG and COMMIT)",
        "- Also check the Windows `windows-ffmpeg` job cache key and `licenses/ffmpeg/`.\n",
    )


def src_rlottie():
    pin = re.search(r"^PIN=([0-9a-f]{40})$", read("scripts/build-rlottie.sh"), re.M).group(1)
    cmp = get_json(f"{API}/repos/Samsung/rlottie/compare/{pin}...master")
    ahead = cmp.get("ahead_by", 0)
    if not ahead:
        print(f"rlottie: pin {pin[:9]} is master")
        return None
    head = cmp["commits"][-1]["sha"] if cmp.get("commits") else "master"
    title = f"rlottie has {ahead} new commit(s) on master"
    body = (
        f"Samsung/rlottie master is {ahead} commit(s) ahead of the pin `{pin[:9]}` "
        "(`scripts/build-rlottie.sh` and `scripts/build-rlottie-windows.ps1`, which must stay in sync).\n\n"
        f"- Compare: https://github.com/Samsung/rlottie/compare/{pin}...{head}\n"
        f"- Procedure: the native dependencies section of `{DOC}`.\n"
        "- rlottie has no releases; only bump for fixes that matter and re-run the sticker smoke tests."
    )
    return Issue("native:rlottie", title, body)


def src_ntgcalls():
    pinned = re.search(r'^NTGCALLS_VERSION="(v[^"]+)"', read("scripts/vendor-ntgcalls.sh"), re.M).group(1)
    rel = newest_release("pytgcalls/ntgcalls")
    if not rel or vtuple(rel[0]) <= vtuple(pinned):
        print(f"ntgcalls: pinned {pinned}, latest {rel[0] if rel else 'none'}")
        return None
    return native_issue(
        "native:ntgcalls", "ntgcalls", pinned, rel[0], rel[1], "scripts/vendor-ntgcalls.sh",
        "- Update the per-asset SHA-256 values in the same script and the LGPL source notes check "
        "(`scripts/release-lgpl-sources.sh`).\n",
    )


def src_rust():
    pinned = re.search(r'channel\s*=\s*"([0-9.]+)"', read("rust-toolchain.toml")).group(1)
    rel = newest_release("rust-lang/rust")
    if not rel or vtuple(rel[0]) <= vtuple(pinned):
        print(f"rust: pinned {pinned}, latest {rel[0] if rel else 'none'}")
        return None
    return native_issue("native:rust", "Rust", pinned, rel[0], rel[1], "rust-toolchain.toml")


USES_RE = re.compile(r"^\s*-?\s*uses:\s*([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)((?:/[^@\s]+)?)@(\S+)(?:\s*#\s*(.*))?$")


def parse_uses(workflow_text: str) -> list:
    """-> [(repo, ref, comment)] for each remote `uses:` (skips local and reusable workflows)."""
    out = []
    for line in workflow_text.splitlines():
        m = USES_RE.match(line)
        if m and not m.group(2).startswith("/.github/"):
            out.append((m.group(1), m.group(3), (m.group(4) or "").strip()))
    return out


def used_major(ref: str, comment: str):
    """Major the workflow is on: from `v4`, or a `# v4` comment on a SHA pin; else None."""
    for text in (ref, comment):
        m = re.match(r"v(\d+)(?:\.\d+)*\b", text)
        if m:
            return int(m.group(1))
    return None


def src_actions():
    seen = {}
    wf_dir = os.path.join(ROOT, ".github", "workflows")
    for name in sorted(os.listdir(wf_dir)):
        if name.endswith((".yml", ".yaml")):
            with open(os.path.join(wf_dir, name), encoding="utf-8") as f:
                for repo, ref, comment in parse_uses(f.read()):
                    major = used_major(ref, comment)
                    if major is not None:
                        seen.setdefault(repo, major)
    rows = []
    for repo, major in sorted(seen.items()):
        rel = newest_release(repo)
        if rel and vtuple(rel[0]) and vtuple(rel[0])[0] > major:
            rows.append((f"`{repo}`", f"v{major}", rel[0], rel[1]))
    if not rows:
        print("actions: all on the latest major")
        return None
    body = (
        "Actions used in `.github/workflows/` with a newer major release. SHA-pinned actions "
        "(`# vN` comment) must have their SHA updated along with the comment.\n\n"
        + table(["action", "used", "latest", "release"], rows)
    )
    return Issue("actions", f"GitHub Actions updates available ({len(rows)})", body)


SOURCES = {
    "tdlib": src_tdlib,
    "gpui-kit": src_gpui_kit,
    "crates": src_crates,
    "native:ffmpeg": src_ffmpeg,
    "native:rlottie": src_rlottie,
    "native:ntgcalls": src_ntgcalls,
    "native:rust": src_rust,
    "actions": src_actions,
}


# ------------------------------------------------------------------ sync


def plan_sync(existing: list, desired: dict, ok_keys: set) -> list:
    """existing: open issues [{number,title,body,labels}] carrying the watch label.
    desired: key -> Issue. ok_keys: sources that ran successfully.
    Returns [("create", Issue) | ("update", number, Issue) | ("close", number, key)]."""
    by_key = {}
    for issue in existing:
        m = MARKER_RE.search(issue.get("body") or "")
        key = m.group(1) if m else ("tdlib" if "tdlib-update" in issue.get("labels", []) else None)
        if key and key not in by_key:  # first (lowest-numbered after sort) wins; extras are left alone
            by_key[key] = issue
    actions = []
    for key, issue in desired.items():
        cur = by_key.get(key)
        if cur is None:
            actions.append(("create", issue))
        elif cur["title"] != issue.title or (cur.get("body") or "").strip() != issue.body.strip():
            actions.append(("update", cur["number"], issue))
    for key, cur in by_key.items():
        if key in ok_keys and key not in desired:
            actions.append(("close", cur["number"], key))
    return actions


def gh(*args, check=True):
    return subprocess.run(["gh", *args], capture_output=True, text=True, check=check)


def list_open():
    seen = {}
    for label in (LABEL, "tdlib-update"):
        out = gh("issue", "list", "--state", "open", "--label", label, "--limit", "100",
                 "--json", "number,title,body,labels").stdout
        for issue in json.loads(out or "[]"):
            issue["labels"] = [l["name"] for l in issue["labels"]]
            seen[issue["number"]] = issue
    return [seen[n] for n in sorted(seen)]


def apply(actions: list):
    gh("label", "create", LABEL, "--color", "0e8a16", "--description", "Dependency update available", check=False)
    gh("label", "create", "tdlib-update", "--color", "1d76db", "--description", "A newer TDLib is available upstream", check=False)
    for act in actions:
        if act[0] == "create":
            issue = act[1]
            args = ["issue", "create", "--title", issue.title, "--body", issue.body]
            for label in issue.labels:
                args += ["--label", label]
            print(gh(*args).stdout.strip())
        elif act[0] == "update":
            _, number, issue = act
            args = ["issue", "edit", str(number), "--title", issue.title, "--body", issue.body]
            for label in issue.labels:
                args += ["--add-label", label]
            gh(*args)
            print(f"updated #{number}: {issue.title}")
        else:
            _, number, key = act
            gh("issue", "close", str(number), "--comment", f"`{key}` is up to date now; closing.")
            print(f"closed #{number} ({key})")


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--dry-run", action="store_true")
    mode.add_argument("--apply", action="store_true")
    parser.add_argument("--only", nargs="*", choices=sorted(SOURCES), help="run only these sources")
    args = parser.parse_args()

    desired, ok, failed = {}, set(), []
    for key, func in SOURCES.items():
        if args.only and key not in args.only:
            continue
        try:
            issue = func()
        except (urllib.error.URLError, subprocess.CalledProcessError, RuntimeError, OSError, AttributeError, ValueError, KeyError) as e:
            detail = getattr(e, "stderr", "") or e
            print(f"deps-watch: source {key} failed: {str(detail).strip()[:300]}", file=sys.stderr)
            failed.append(key)
            continue
        ok.add(key)
        if issue:
            desired[key] = issue

    if args.dry_run:
        print("\n=== DRY RUN: nothing is filed ===")
        for issue in desired.values():
            print(f"\n--- would open/update [{', '.join(issue.labels)}] {issue.title}\n{issue.body}")
        print(f"\nsummary: {len(desired)} issue(s) would be open/updated; "
              f"{len(ok) - len(desired)} source(s) current; failed: {failed or 'none'}")
    else:
        actions = plan_sync(list_open(), desired, ok)
        apply(actions)
        print(f"deps-watch: {len(actions)} action(s); failed sources: {failed or 'none'}")
    return 1 if failed and len(failed) == len(ok) + len(failed) else 0


if __name__ == "__main__":
    sys.exit(main())
