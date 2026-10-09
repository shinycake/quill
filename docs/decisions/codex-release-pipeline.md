# Release pipeline (tag → draft GitHub release)

## What the owner does

1. Bump `version` in `Cargo.toml` (and `Cargo.lock`) to `X.Y.Z`; optionally add
   `release-notes/vX.Y.Z.md` with curated highlights. Merge.
2. `git tag vX.Y.Z <sha> && git push origin vX.Y.Z`.
3. `.github/workflows/release.yml` builds everything and creates a **draft**
   release `vX.Y.Z`. Review it on GitHub, then click Publish.
4. Publishing fires `.github/workflows/release-published.yml`, which sends the
   `site-refresh` repository dispatch for the Pages site.

Dry run: `gh workflow run release.yml -f version=X.Y.Z --ref <branch>` builds
the same files and uploads them as the `quill-release-vX.Y.Z` workflow
artifact. It never creates a tag or a release. `embed_credentials=true` opts a
dry run into the credential secrets (off by default). GitHub only dispatches a
workflow whose file exists on the default branch, so dry runs work once this PR
is on `main`.

## Workflows

- `package-linux.yml`, `package-windows.yml`, `package-macos.yml`: reusable
  (`workflow_call`) packaging workflows. Linux and Windows are the former
  `ci.yml` jobs moved verbatim (same job ids, so the Rust caches carry over),
  plus the credential input. `ci.yml` calls all three on every PR without
  secrets (none is a required check; the required checks are unchanged).
  `release.yml` calls them with `embed_credentials`.
- `release.yml`: `prepare` (version = tag or input, must equal `Cargo.toml`;
  credential mode), the three package workflows, `lgpl-sources`, and
  `assemble release` (checks each package against its build job's `.sha256`,
  generates notes, writes `SHA256SUMS.txt` and `latest.json`, uploads the
  artifact, and on a tag push only runs `gh release create --draft
  --verify-tag`). Top-level permissions are `contents: read`; only the assemble
  job gets `contents: write` (the generate-notes API and the release).
  Re-running on a tag whose draft already exists fails with a message instead
  of overwriting it.
- `release-published.yml`: on `release: published` (and manual dispatch),
  `POST /repos/{repo}/dispatches` with `event_type=site-refresh` and a payload
  of `tag`, `release_url` and the stable manifest URL. Dispatches sent with
  `GITHUB_TOKEN` do start workflows (it is one of the two documented
  exceptions), so the Pages workflow can listen with
  `on: repository_dispatch: types: [site-refresh]`.

## Release assets

| Asset | What |
|---|---|
| `quill-macos-aarch64.zip` | `Quill.app` (macOS 14+, Apple Silicon), ad-hoc signed, zipped with `ditto` |
| `quill-linux-x86_64-bundle.tar.gz` | relocatable Linux package with `install.sh` (unchanged layout) |
| `quill-windows-x86_64.zip` | flat Windows folder (now without VC++ runtime / OpenSSL / zlib DLLs) |
| `quill-linux-x86_64` | bare executable for the in-app updater |
| `quill-source-*.tar.xz`, `LGPL-SOURCES.txt` | LGPL corresponding source + written offer |
| `latest.json` | machine-readable manifest |
| `SHA256SUMS.txt` | SHA-256 of every other asset |

Names carry no version, so `https://github.com/shinycake/quill/releases/latest/download/<name>`
is a stable link for the site.

### In-app updater compatibility (`src/updater.rs`, `src/update_install.rs`)

The checker reads `releases/latest` (drafts and prereleases are invisible to
it, so a draft never reaches users), compares the tag with `CARGO_PKG_VERSION`,
and looks for an asset named `quill-<os>-<arch>` (`std::env::consts`):
`quill-linux-x86_64`, `quill-macos-aarch64`, `quill-windows-x86_64`, using
GitHub's per-asset `digest` (`sha256:...`) and `size`. Installation is only
possible for a standalone Unix executable outside a `.app`
(`can_install_here`), and the staged binary must print `Quill X.Y.Z` and `ui`.
So:

- Linux: the release uploads the packaged `quill` (RUNPATH `$ORIGIN/lib`) as
  `quill-linux-x86_64`. The updater stages it next to the installed one inside
  the package directory, where `$ORIGIN/lib` resolves, and the version probe
  passes because `prepare` refuses a tag that differs from `Cargo.toml`.
- macOS (inside `Quill.app`) and Windows cannot self-install; the app shows
  "Install the release package from GitHub", which links to the release. No
  bare binary is uploaded for them (it would be unusable).
- Known limit, unchanged: a Linux self-update replaces only the executable, not
  `lib/` (TDLib, ntgcalls, FFmpeg). A release that bumps a native library needs
  the user to install the new tarball; the curated notes should say so.

### `latest.json` (schema 1)

```json
{
  "schema": 1, "name": "Quill", "version": "X.Y.Z", "tag": "vX.Y.Z",
  "date": "<UTC time the draft was assembled; the GitHub release's published_at is the publish time>",
  "release_url": "https://github.com/shinycake/quill/releases/tag/vX.Y.Z",
  "notes_markdown": "## What's new ... (curated + generated, no checksums)",
  "credentials_embedded": true,
  "platforms": {
    "macos-aarch64":  {"label", "kind": "app-zip", "asset", "url", "size", "sha256"},
    "linux-x86_64":   {"label", "kind": "tarball", ...},
    "windows-x86_64": {"label", "kind": "zip", ...}
  },
  "update_binaries": {"linux-x86_64": {"asset", "url", "size", "sha256"}},
  "sources": [{"asset", "url", "size", "sha256"}, ...],
  "disclaimer": "..."
}
```

`latest.json` cannot contain its own checksum, so `notes_markdown` holds only
the "What's new" part; the release body (which lists every checksum, including
`latest.json`'s and `SHA256SUMS.txt`'s) is built after it. The manifest is not
rewritten on publish (that would change its checksum after the fact, and
immutable releases would refuse it); the site should take the publish date from
the API.

## Release notes

`scripts/release-assemble.py` writes the body: "What's new" = the optional
`release-notes/vX.Y.Z.md` followed by GitHub's generated list of merged PRs
(`POST /repos/{repo}/releases/generate-notes` with `.github/release.yml`
categories by label and `previous_tag_name` = newest `vX.Y.Z` tag reachable
from the release commit; the very first release has no previous tag, so its
full PR list is collapsed in a `<details>` block), then downloads with sizes,
install instructions (including how to open the unsigned macOS app and get past
SmartScreen), the credential status, the LGPL source paragraph, a SHA-256 block
for every asset, and the disclaimer (same wording as `src/about.rs`). It fails
if the body exceeds GitHub's 125,000-character limit.

macOS opening instructions: the app is ad-hoc signed only (no Developer ID, no
notarization, as decided). On macOS 15+ the Control-click → Open bypass is gone;
the notes describe System Settings → Privacy & Security → Open Anyway, the
macOS 14 Control-click path, and `xattr -dr com.apple.quarantine`.

## Telegram credentials (compile-time fallback)

`src/credentials.rs`: `load()` = runtime lookup (env `TELEGRAM_*`/`QUILL_*`,
then `.env`/`quill.local.env`) and, only if that yields no complete pair,
`embedded()`, which parses `option_env!("QUILL_BUILD_TELEGRAM_API_ID")` /
`option_env!("QUILL_BUILD_TELEGRAM_API_HASH")`. The pair is used whole (an env
`api_id` is never mixed with an embedded hash). Unset or empty variables embed
nothing, so every dev, test and PR build behaves exactly as before.
`quill --embedded-credentials` prints `embedded` or `none`; each package job
asserts it matches what was requested, so a release cannot silently ship
without (or with) credentials.

Secret handling:
- The secrets are passed only to the reusable workflows' `cargo build` step,
  through `${{ inputs.embed_credentials && secrets.X || '' }}`; `prepare`
  only exports `true`/`false`. Nothing echoes them, and GitHub masks them.
- rustc records `option_env!` values in dep-info files under `target/`, so
  credential builds run `Swatinem/rust-cache` with `save-if: false`: the cache
  shared with PR builds never contains them.
- They end up only in the compiled binaries (where every desktop Telegram
  client keeps its api_id). No source, script, log or non-binary artifact
  contains them; gitleaks scans the tree, which holds only the variable names.
- Missing secrets: the build still succeeds, `prepare` warns, the binaries
  report `none`, `latest.json` has `credentials_embedded: false`, and the notes
  open the credential section with "This is a credential-free build".
- Dry runs default to no credentials, so routine dry-run artifacts (readable by
  anyone with access to the run) carry no secrets.

## Windows: static C runtime

Before (main @ 63d99db, CI run 37849702698): the zip shipped `vcruntime140.dll`,
`vcruntime140_1.dll`, `msvcp140.dll`, plus `libssl-3-x64.dll`,
`libcrypto-3-x64.dll` and `z.dll`; importers of the VC++ runtime were
`quill.exe`, `tdjson.dll`, `rlottie.dll`, `libssl`, `libcrypto` and `z.dll`.
`ntgcalls.dll` (upstream prebuilt) already used the static runtime; the
MinGW-built FFmpeg/quillvideo DLLs import only the Universal CRT
(`api-ms-win-crt-*`), an OS component on Windows 10+.

Change:
- `quill.exe`: `.cargo/config.toml` sets `-C target-feature=+crt-static` for
  `x86_64-pc-windows-msvc` (applies to local Windows builds too, so they match
  the release). `cc`-built C code (ring, etc.) follows `crt-static`
  automatically.
- `tdjson.dll`: vcpkg triplet `x64-windows-static` (OpenSSL, zlib and gperf
  as static `/MT` libraries) and `CMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded`
  with `CMAKE_POLICY_DEFAULT_CMP0091=NEW` (TDLib declares CMake 3.10, below the
  version where that policy defaults on). OpenSSL and zlib are therefore inside
  `tdjson.dll`, so `libssl`/`libcrypto`/`z.dll` no longer ship either. The
  build script fails if `tdjson.dll` still imports the VC++ runtime, OpenSSL
  or zlib. The TDLib cache keys gained a `-mt-` prefix so the old dynamic-CRT
  build tree (with cached OpenSSL import-library paths) is never restored.
- `rlottie.dll`: same CMake runtime settings; the script fails on a VC++
  runtime import.
- `windows-package.ps1` no longer copies anything from the VC++ redistributable
  directory (`Get-VcRedistDir` is gone); `check-bundle-pe.ps1` now fails on any
  `vcruntime*`/`msvcp*`/`concrt*` import and no longer requires the OpenSSL
  DLLs; `check-bundle-load.ps1` fails if an OpenSSL/zlib DLL gets loaded at all.
  The CI negative test now deletes `avutil-*.dll` (a real import of
  `quillvideo.dll`) instead of the OpenSSL DLL that is no longer shipped.
- Licenses: `THIRD_PARTY.md` (OpenSSL/zlib rows, VC++ runtime row),
  `licenses/msvc-runtime.txt` and the Windows README describe static linking.

Cross-CRT safety: each DLL now has its own static CRT heap. The DLL interfaces
Quill uses never free memory across the boundary (tdjson returns
library-owned `const char*`; rlottie and ntgcalls have explicit
create/destroy calls; FFmpeg goes through the quillvideo shim), which is the
condition for mixing per-module CRTs safely.

Measurements: see "Dry-run results" below (16 PE files before, 10 after).

## LGPL corresponding source

`scripts/release-lgpl-sources.sh` (job `lgpl-sources`) shallow-clones each
component at its pin, checks the commit, and writes `git archive` tarballs:
FFmpeg `n8.1.3` (Quill's build, commit checked against `build-ffmpeg.sh`),
ntgcalls `v3.0.0`, and the versions ntgcalls' `version.properties` pins for its
statically linked LGPL parts (FFmpeg `n9.0.1`, GLib `2.89.4`), plus pytgcalls'
FFmpeg/GLib build-script repositories at the matching tags. `LGPL-SOURCES.txt`
lists exact URLs and commits, the configure-line location, and a three-year
written offer. Local run (2026-10-08): about 50 s and 40 MB of archives
(FFmpeg n8.1.3 11.5 MB, ntgcalls 10.4 MB, FFmpeg n9.0.1 11.8 MB, GLib 5.9 MB,
build scripts 7 KB). Not included: ntgcalls' git submodules (pybind11, oboe:
Python/Android only, not LGPL) and WebRTC's own source (BSD; the URLs are in
`licenses/ntgcalls-components.txt`).

## macOS packaging on a hosted runner

`package-macos.yml` runs on `macos-14` (arm64, Sonoma), never on the personal
self-hosted Mac: tag pushes and PRs must not execute on it. TDLib is built from
source by `scripts/build-tdlib.sh` (targets `tdjson tdjson_static`, Homebrew
`openssl@3` via `OPENSSL_ROOT_DIR`, cached like Linux), rlottie by
`build-rlottie.sh`, ntgcalls from the pinned upstream zip.
`MACOSX_DEPLOYMENT_TARGET=14.0` matches `LSMinimumSystemVersion`, and Sonoma
Homebrew bottles keep the bundled OpenSSL at 14.0 as well.
`scripts/macos-package-smoke.sh` is reused unchanged except that the Info.plist
version now comes from `quill --version` (it was hard-coded to 0.1.0),
`CFBundleShortVersionString` is set, and an ERR trap names the failing command
(the first hosted run exited 1 with no message).

QR scanner self-test on the hosted VM: `quill-qr-scanner --self-test` generates
a QR code with CoreImage and decodes it with Vision. On the hosted macOS VM
Vision finds no barcode ("FAIL: Vision decoded no QR code instead of the
fixture", run 37877194739), while it passes on real Macs. The self-test now
prints why it failed, and `scripts/build-qr-scanner.sh` honours
`QUILL_QR_SELF_TEST=warn`, which only `package-macos.yml` sets: the hosted
release build warns, `quill-ui-build.yml` on the owner's Mac still fails hard.
The owner considered moving the macOS release job to the self-hosted Mac and
decided to keep the hosted runner once it was green (no personal machine in the
release path, no fork-safety questions). Note that a self-hosted build would
also need its own macOS 14 OpenSSL/rlottie builds: that Mac's Homebrew and
developer builds target its own, much newer macOS. The job then requires tdjson, ntgcalls and
rlottie in `Contents/Frameworks`, runs `codesign --verify --deep --strict`,
checks the extracted zip with `check-bundle-macho.sh`, and runs the bundled
binary. macOS ships no FFmpeg (AVFoundation), so there is no macOS source
archive beyond ntgcalls'.

## Not done / follow-ups

- No notarization or Developer ID (owner decision); Gatekeeper will warn.
- No Windows code signing or installer; SmartScreen will warn.
- No in-app self-update on macOS/Windows (unchanged).
- No Linux arm64 or Windows arm64 package.
- The site under `/site` and its Pages workflow are separate work; they should
  listen for `repository_dispatch: site-refresh` and read `latest.json` or the
  releases API.

## Dry-run results

`release.yml` itself cannot be dispatched before it is on `main`, so the
release pipeline was exercised piecewise on 2026-10-08 (the real
`workflow_dispatch` dry run follows the merge):

- PR CI run 37877194739 (head 697620d) ran the three reusable package
  workflows exactly as `release.yml` calls them (without credentials): all
  green, including `windows-package` and `macos-package`.
- The `assemble release` job's commands were replayed locally against that
  run's artifacts plus `scripts/release-lgpl-sources.sh` output: the per-package
  `.sha256` checks passed (including the CRLF Windows file), generate-notes
  returned 469 lines (no previous tag yet, so every merged PR, collapsed), and
  `release-assemble.py` wrote a 59,543-character body, `latest.json` and
  `SHA256SUMS.txt` for 12 assets.

Release assets from that replay (credential-free, version 0.1.0):

| Asset | Bytes |
|---|---|
| `quill-macos-aarch64.zip` | 46,975,645 (`Quill.app` 124 MB unpacked) |
| `quill-linux-x86_64-bundle.tar.gz` | 84,891,626 |
| `quill-windows-x86_64.zip` | 48,104,889 |
| `quill-linux-x86_64` (updater binary) | 75,563,920 |
| `quill-source-ffmpeg-n8.1.3.tar.xz` | 11,528,644 |
| `quill-source-ntgcalls-v3.0.0.tar.xz` | 10,449,608 |
| `quill-source-ntgcalls-ffmpeg-n9.0.1.tar.xz` | 11,811,376 |
| `quill-source-ntgcalls-glib-2.89.4.tar.xz` | 5,856,164 |
| `quill-source-ntgcalls-build-scripts.tar.xz` | 6,692 |
| `LGPL-SOURCES.txt` | 5,118 |
| `latest.json` | 59,854 |
| `SHA256SUMS.txt` | 1,053 |

macOS (hosted `macos-14`): every Mach-O in the app is `minos 14.0` (`quill`
44.9 MB, `libtdjson` 41.8 MB, `libntgcalls` 35.0 MB, `libcrypto.3` 4.9 MB,
`libssl.3` 0.9 MB, `librlottie` 0.6 MB, `quill-qr-scanner`), `codesign --verify
--deep --strict` passes on the extracted zip, `check-bundle-macho.sh` reports 7
Mach-O files depending only on the OS and the bundle, `--embedded-credentials`
prints `none`.

Windows, before (main 63d99db, run 37849702698) vs after (run 37877194739):

| | Before | After |
|---|---|---|
| PE files in the package | 16 (quill.exe + 15 DLLs) | 10 (quill.exe + 9 DLLs) |
| VC++ runtime / OpenSSL / zlib DLLs | vcruntime140, vcruntime140_1, msvcp140, libssl-3-x64, libcrypto-3-x64, z | none |
| `tdjson.dll` imports | CRT API sets, msvcp140, vcruntime140(_1), libssl, libcrypto, z, system DLLs | advapi32, crypt32, kernel32, normaliz, user32, ws2_32 |
| `rlottie.dll` imports | CRT API sets, msvcp140, vcruntime140(_1), kernel32, shlwapi | kernel32, shlwapi |
| zip (packager's report) | 45.9 MB | 45.9 MB |
| zip artifact bytes (zip + .sha256, compressed) | 48,084,540 | 48,010,358 |
| `quill.exe` artifact bytes | 18,482,143 | 18,814,557 |

The static runtime adds roughly what the removed DLLs weighed, so the
download size is unchanged; the gain is no app-local Microsoft runtime to
service or license-track and no OpenSSL DLL-search exposure. The package
check, the negative test (missing `avutil` must fail), the load check
(no OpenSSL/zlib module loaded) and both video probes (10 frames each) pass.

Linux: unchanged package (81 MB tarball as reported by the packager); the CI
artifact grew from 83,847,674 to 110,757,298 bytes because it now also
carries the updater binary `quill-linux-x86_64`.

Local gate before the first push: `GATE OK core=2044 0 ui=126 0`. actionlint
and shellcheck are clean on every new or changed workflow and script
(`.github/actionlint.yaml` declares the self-hosted `quill-ui-build` label so
the existing `quill-ui-build.yml` lints clean too).

## Versioning: computed from the commit (2026-10-09)

The owner asked for versions based on the date, with nothing to choose or bump.

- **Format:** the version is `YEAR.MONTHDAY.COMMITS` for the commit being built, for example `2026.1009.1187`: committed on 2026-10-09 (UTC), the 1187th commit on main.
- **Why it's valid semver:** each part is a plain number. January 9 is `109`, which sorts before `1009`. A suffix like `2026.10.9-2` would be a pre-release that sorts before `2026.10.9`, and build metadata (`+2`) is ignored in comparisons. That is why the build number lives in the patch field.
- **Ordering:** two builds on the same day differ by their commit count, and later commits always compare higher, so the in-app updater (semver precedence) offers them.
- **Repeatable:** the same commit always gets the same version. The workflow refuses to create a second release for a version that already exists.
- **Stamping:** `release.yml`'s prepare step computes the version and passes it to the packaging workflows as the `version` input. They set `QUILL_RELEASE_VERSION` for the release build, and `src/version.rs` (`quill::version::APP`) uses it instead of `Cargo.toml`'s version. `--version`, the macOS bundle version, the updater, the User-Agent and the version reported to Telegram all read `APP`. Local and CI builds keep `Cargo.toml`'s version, which never changes for a release.
- **Cutting a release:** `gh workflow run release.yml -f release=true` (main only) creates a draft. Publishing the draft creates the tag `vYEAR.MONTHDAY.COMMITS` on the built commit. A dry run (no `release`) builds the same version and only uploads artifacts. A hand-pushed tag in this format still works, and its format and date are validated.
