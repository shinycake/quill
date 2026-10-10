# GitHub Actions bump (issue #556)

## What the reference does

tdesktop has no equivalent; this is repo maintenance. The deps-watch workflow
(`scripts/deps-watch.py`, source `actions`) flagged eight actions with a newer
major release.

## What changed

Every workflow in `.github/workflows` moved to the latest major. Tag-style refs
stay tags; the two SHA-pinned `actions/checkout` refs
(`quill-ui-build.yml`, `macos-smoke-fallback.yml`) stay pinned, now to the v7.0.1
commit with a `# v7` comment.

| action | from | to | breaking notes that matter here |
| --- | --- | --- | --- |
| `actions/checkout` | v4 | v7 | v5/v6 run on Node 24 (runner >= 2.327.1); v6 stores credentials in a separate file (only affects Docker-container actions); v7 blocks checking out fork PRs under `pull_request_target`/`workflow_run`. Only `dashboard-data.yml` uses those triggers and it only calls a reusable workflow, no checkout. |
| `actions/cache` (+ `/restore`, `/save`) | v4 | v5 | Node 24 runtime. Key and path inputs unchanged. |
| `actions/upload-artifact` | v4 | v7 | v5/v6 Node 24; v7 adds `archive: false` (opt-in, unused). Name, path, `retention-days`, `if-no-files-found` unchanged. |
| `actions/download-artifact` | v4 | v8 | v5 changed the path for single downloads by artifact ID (we download by name, unaffected); v7 Node 24; v8 fails on digest mismatch by default and skips unzipping non-zip files. All our artifacts are zips from upload-artifact. |
| `actions/configure-pages` | v5 | v6 | Node 24 only. |
| `actions/deploy-pages` | v4 | v5 | Node 24 only. |
| `actions/upload-pages-artifact` | v3 | v5 | v4 excludes dotfiles from the artifact (new `include-hidden-files` input in later versions). `site/` has no dotfiles, so no input needed. v5 builds on upload-artifact v7. |
| `gitleaks/gitleaks-action` | v2 | v3 | Node 24 only; inputs and behaviour unchanged. Node 20 leaves hosted runners on 2026-09-16. |

Artifact names and paths were not touched. `package-*.yml` upload and download
the same names (`quill-windows-exe`, `-native`, `-rlottie`, `-ffmpeg`), and
`release.yml` still downloads `quill-macos-aarch64`,
`quill-linux-x86_64-bundle`, `quill-windows-x86_64` and `quill-lgpl-sources`
into `in/*`, so the exact file-list check on `release-out/` sees the same files.

## How verified

- `actionlint` is clean.
- `python3 scripts/deps-watch.py --dry-run --only actions` prints
  "all on the latest major".
- CI run on the PR branch and a `release.yml` dry run (no `release=true`, which
  creates no draft and no tag); results are in the PR.
