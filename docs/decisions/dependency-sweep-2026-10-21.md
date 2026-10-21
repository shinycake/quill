# Dependency sweep, October 21 2026: small bumps

## What tdesktop does

Nothing relevant. This is Quill's own dependency policy (`docs/dependency-updates.md`).

## What changed

**Watch.** `scripts/deps-watch.py` reported a direct dependency as current whenever any
crate in the lockfile had a newer copy, so `rand` 0.8 (0.10 via `tungstenite`) and
`base64` 0.22 never showed up as majors. It now reads each workspace member's resolved
dependencies from `cargo metadata`, which also covers `crates/webview-host` and
`crates/ntgcalls-sys`.

**Advisories in CI.** The `licenses` job also runs `cargo deny check advisories`, so a
RustSec vulnerability or a yanked crate fails the next PR. `deny.toml` lists the
unmaintained notices that only come in through GPUI (`instant`, `paste`, `rustybuzz`,
`ttf-parser`) or the held zbus 5 (`zbus_names`, `zvariant`, `zvariant_derive`).

**rand 0.8 to 0.10.** `RngCore` became `rand::Rng`, `thread_rng()` became `rand::rng()`,
and `OsRng` became the fallible `SysRng`. The hybrid passkey environment keeps an OS RNG
that is `Send` through `UnwrapErr(SysRng)`, which behaves as 0.8's `OsRng` did. Other
call sites use the thread RNG, a ChaCha CSPRNG seeded from the OS, as before. The 0.8 and
0.9 copies stay in the lockfile through GPUI's dependencies.

**base64 0.22 to 0.23.** No API change for the engines Quill uses. Default features are
off, so the new `simd-unsafe` engines are not compiled; Quill encodes little data. 0.22
stays in the lockfile through GPUI.

**ntgcalls v3.0.3 to v3.0.5.** Upstream fixes: incoming group call video is no longer
requested without a receiver, the first audio chunks no longer play out of order, and
audio no longer plays fast after a slow stream start. `ntgcalls.h` and the bundled
component versions (WebRTC, FFmpeg, GLib build files) are unchanged between the tags.
Checksums come from the release asset digests.

## How verified

- `python3 -m unittest discover -s scripts -p 'test_deps_watch.py'`, and the dry run lists
  `rand` and `base64`.
- `cargo deny --locked check advisories`: ok.
- `scripts/vendor-ntgcalls.sh` downloads v3.0.5 for macOS arm64, the checksum matches, and
  all 77 `ntg_*` symbols export. Calls cannot be placed on this machine; the C API is
  unchanged, so the bindings in `crates/ntgcalls-sys` stay as they are.
- Gate: fmt, both clippies, core and UI tests, license list.
