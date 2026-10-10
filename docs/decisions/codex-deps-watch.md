# Weekly dependency watch

The owner made staying current on TDLib, GPUI Kit and every other dependency a project rule.

## What was there

`tdlib-watch.yml` + `scripts/tdlib-watch.py` opened a weekly "TDLib X.Y.Z available" issue
(label `tdlib-update`), updated it instead of duplicating, and never closed it.

## What changed

- `scripts/deps-watch.py` (new) generalises it. Sources: TDLib (reuses `tdlib-watch.py`,
  now exposing `check()`), gpui-kit vs the `=` pin plus the vendored `gpui-base` /
  `gpui-pre-*` copies, `cargo update --dry-run` and newer majors (crates.io API, 1 req/s)
  as one table issue, FFmpeg, rlottie, ntgcalls, the Rust toolchain, and GitHub Actions.
- One issue per source, identified by a hidden `<!-- deps-watch:KEY -->` marker. Existing
  TDLib issues (label only) are adopted. A newer version retitles the same issue; when
  the source is current the issue is closed. A source that errors never closes or edits
  its issue.
- `.github/workflows/deps-watch.yml` replaces `tdlib-watch.yml` (same cron, same
  `contents: read` + `issues: write`). Manual runs offer `dry-run`.
- `docs/dependency-updates.md` is the policy and procedure; the three agent files point to it.
- No Dependabot: a single custom check keeps one inbox and cannot touch the pinned and
  vendored GPUI family; Dependabot for Actions would only add PR noise next to the
  `actions` issue.

## tdesktop

Not applicable (infrastructure). tdesktop tracks its dependencies in `Telegram/build/` and
`cmake/external/` by hand.

## Verified

- `python3 -m unittest discover -s scripts -p 'test_deps_watch.py'`: parsing, version
  ordering, issue sync planning (create, update, close, failed source, legacy TDLib).
- `python3 scripts/deps-watch.py --dry-run` against live upstreams (prints only; no issue
  is opened from a dev machine). `actionlint` on the new workflow.
- Not verified: `--apply` against GitHub (first scheduled or manual CI run).
