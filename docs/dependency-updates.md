# Dependency updates

Project rule: Quill stays current on TDLib, GPUI Kit and every other dependency.
Updates are found by a weekly watch, filed as issues, and integrated in small PRs.

## Cadence

- `.github/workflows/deps-watch.yml` runs every Monday 08:00 UTC (and on demand:
  Actions, deps-watch, Run workflow, mode `dry-run` prints without filing).
- It runs `scripts/deps-watch.py`, which files **one issue per dependency**, label
  `deps-watch` (TDLib also keeps `tdlib-update`). A later run edits the same issue
  (a newer version changes its title) and closes it once the update has landed. A
  source that fails to fetch is skipped for that week and its issue is left alone.
- **Check the watch issues at the start of every work session**
  (`gh issue list --label deps-watch`). Pick them up before new feature work, or say
  why one is deferred in the issue.
- Run it locally, read-only: `python3 scripts/deps-watch.py --dry-run [--only KEY]`.
  Offline tests: `python3 -m unittest discover -s scripts -p 'test_deps_watch.py'`.

| Issue (marker key) | Watches | Compared with |
| --- | --- | --- |
| `tdlib` | tdlib/td master (`scripts/tdlib-watch.py`) | `src/pins.rs` |
| `gpui-kit` | gpui-kit on crates.io, and `gpui-base` / `gpui-pre-*` | `=` pin in `Cargo.toml`, `third_party/*/Cargo.toml` |
| `crates` | `cargo update --dry-run`, newer majors via the crates.io API | `Cargo.lock`, direct deps |
| `native:ffmpeg` | FFmpeg release tags | `TAG` in `scripts/build-ffmpeg.sh` |
| `native:rlottie` | Samsung/rlottie master (no releases) | `PIN` in `scripts/build-rlottie.sh` |
| `native:ntgcalls` | pytgcalls/ntgcalls releases | `NTGCALLS_VERSION` in `scripts/vendor-ntgcalls.sh` |
| `native:rust` | rust-lang/rust releases | `rust-toolchain.toml` |
| `actions` | latest release of every action in `.github/workflows` | `uses:` refs |

Why a custom check and no Dependabot: one inbox and one format for everything, no PR
noise, and no bot that could bump the pinned or vendored GPUI family on its own.
Do not enable auto-merge, Dependabot or Renovate PRs for `gpui-*`, `third_party/` or
the `=` pinned crates. Updates there are deliberate and visually verified.

## TDLib

1. In a work tree, check out the new commit under `native/td` (read-only toward
   tdlib/td, never push there).
2. `src/pins.rs`: `TDLIB_GIT_COMMIT`, `TDLIB_CMAKE_VERSION`, `TD_API_TL_SHA256`,
   `TD_API_TL_BYTES`, `TD_API_TL_UPSTREAM_URL`.
3. `bash scripts/vendor-td-schema.sh` re-vendors `schema/td_api.tl`; copy the printed
   size and hash into `src/pins.rs`. The `vendored_schema_equals_official_commit` test
   checks them.
4. Regenerate `native/patches/tdlib-quill-takeout-contacts.patch` against the new
   commit and rebuild with `bash scripts/build-tdlib.sh` (and
   `scripts/build-tdlib-windows.ps1`); both read the pin from `src/pins.rs`.
5. Fix every removed or changed constructor the issue lists that Quill uses.
6. `docs/build.md` (version line) and `schema/README.md`, `THIRD_PARTY.md`. The
   README badge is edited by the merge pipeline, not feature PRs: ask for it.
7. Gate, then a decision doc (`docs/decisions/codex-tdlib-X.Y.Z.md`, see 1.8.68).

## GPUI Kit

1. Read the release notes linked in the issue
   (https://github.com/longbridge/gpui-component/releases).
2. Bump the `=` pin of `gpui-kit` in `Cargo.toml`.
3. Re-vendor every crate in `third_party/` (`gpui-base`, `gpui-pre-macos`,
   `gpui-pre-linux`, `gpui-pre-windows`) at the new registry version, each as two
   commits: a **pristine** commit with the unmodified registry copy, then a **patch**
   commit re-applying Quill's changes (`QUILL-CHANGES.md` lists them; the diff
   against the pristine commit is the patch). Update the version comments next to
   the `[patch.crates-io]` entries in `Cargo.toml`.
4. Refresh `.claude/skills/gpui-kit` from the new tag (and the version note in its
   `SKILL.md`), and `GPUI_KIT_VERSION` if `src/pins.rs` still carries it.
5. `bash /Users/idan/Developer/Projects/quill-tools/gate.sh`, fix API changes,
   then a visual check (demo-capture screenshots of the main screens, light and dark).
6. Decision doc with what changed in the patched crates.

## Other crates

- Compatible updates: `cargo update`, gate, commit `Cargo.lock`. Batch them; split
  out a crate that breaks something.
- Newer majors: bump one crate per PR, read its changelog, fix call sites, gate.
- Crates with an `=` pin stay pinned until a reason to move is written down.

## Native dependencies

- **FFmpeg**: change `TAG` and `COMMIT` in `scripts/build-ffmpeg.sh` (the commit is
  the tag's commit), check `licenses/ffmpeg/` and the `windows-ffmpeg` job cache key
  (it hashes the script), rebuild `quillvideo`, run the `video_decode` tests.
  `scripts/release-lgpl-sources.sh` reads the pin for the LGPL source archive.
- **rlottie**: change `PIN` in `scripts/build-rlottie.sh` **and** `$pin` in
  `scripts/build-rlottie-windows.ps1`; run the sticker tests.
- **ntgcalls**: change `NTGCALLS_VERSION` and the per-asset checksums in
  `scripts/vendor-ntgcalls.sh`; check the calls smoke path and
  `scripts/release-lgpl-sources.sh`.
- **Rust toolchain**: `rust-toolchain.toml`, fix new clippy lints, gate.
- **GitHub Actions**: bump the `uses:` ref; for SHA-pinned actions update the SHA and
  its `# vN` comment together. Run `actionlint`.

Every dependency PR: gate (`GATE OK`), one decision doc line on how it was verified,
and close the watch issue with the PR (`Closes #N`) if the watch has not already.
