# Panic lints, phases A and B

Branch `panic-lints`. The last step of the no-crash plan
([`no-crash-audit.md`](no-crash-audit.md) §5, step 8). No parity item and no
tdesktop behaviour: this is a build rule, tdesktop has no equivalent.

## What changed

- Root `Cargo.toml` gets `[workspace.lints.clippy]` with `deny` for
  `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented`,
  `unreachable`, `exit` and `undocumented_unsafe_blocks`. The root package and
  `crates/ntgcalls-sys`, `crates/webview-protocol`, `crates/webview-host` opt
  in with `[lints] workspace = true`. The vendored `third_party/` crates are
  `[patch]` paths, not workspace members, so they are unaffected. Clippy's
  default groups and rustc's default lints stay as they were; CI's
  `-D warnings` still turns every other warning into an error.
- New `clippy.toml`: `allow-unwrap-in-tests`, `allow-expect-in-tests`,
  `allow-panic-in-tests`, `allow-indexing-slicing-in-tests`. Clippy 1.99 has
  no `allow-unreachable-in-tests` key (it rejects the file), so test code uses
  `panic!` with a message instead of `unreachable!`.
- Clippy recognises test code only under a plain `#[cfg(test)]`, not
  `#[cfg(all(test, ..))]`. The 59 `cfg(all(test, X))` attributes (mostly
  `feature = "demo-capture"` UI test modules) are split into `#[cfg(test)]`
  plus `#[cfg(X)]`, which is the same condition. That alone cleared about 120
  hits that were all in test code.
- Integration tests (`tests/*.rs`): helpers outside `#[test]` functions are
  not seen as test code, so the 11 files with hits carry a file-level
  `#![allow(clippy::unwrap_used / expect_used, reason = ..)]`.
- Test `unsafe` blocks: 11 got `// SAFETY:` comments (env var guards,
  ntgcalls test calls in `crates/ntgcalls-sys/tests/dlopen_smoke.rs` and
  `src/calls/engine/ntgcalls/native_tests.rs`).
- Two test `unreachable!()` arms became `panic!` with a message.
- Three scoped `#[expect(clippy::exit, reason = ..)]`:
  `parse_screenshot_demo` in `src/main.rs` (unknown demo kind is a CLI usage
  error; the `match` became a `let .. else` so `main.rs` does not grow),
  `startup_failure::window_failed` (no window can open, nothing to keep
  running), and the re-executed child in the `telegram/ffi.rs` log test. The
  `process::exit` calls inside `fn main` itself are not flagged by clippy.
- Three unrelated clippy warnings in `demo-capture` test code
  (`sort_by_key`, `next_back`, redundant closure) fixed so the
  `demo-capture` build is clean under `-D warnings` too.

Production code had no remaining unwrap/expect/panic/unreachable or
undocumented `unsafe` hits: earlier no-crash steps had already converted them.
So this PR fixes 0 production sites and adds 3 `#[expect]`s, all on `exit`.

`docs/contributing/panic-safety.md` marks phases A and B as on and recounts
phases C (140) and D (621).

## How verified

- `cargo clippy --workspace --no-default-features --all-targets -- -D warnings`
- `cargo clippy --workspace --features ui,demo-capture,frame-profiler --all-targets -- -D warnings`
- `cargo clippy --no-default-features --all-targets --target x86_64-pc-windows-msvc`:
  no hits from the new lints.
- `cargo clippy -p quill -p ntgcalls-sys -p quill-webview-protocol --no-default-features --all-targets --target x86_64-unknown-linux-gnu`:
  no hits.
- `gate.sh` (fmt, core and UI clippy with `-D warnings`, core and UI tests).
- cfg(windows) / cfg(target_os = "linux") files read with a script for
  unwrap/expect/panic/unreachable/exit and `unsafe` without a nearby
  `SAFETY` comment, outside test modules. The only hits are `process::exit`
  inside the two `fn main`s and the two `#[expect]`ed functions above.

## Not verified

- The Windows and Linux UI builds (`--features ui`) do not cross-compile here
  (GPUI's build script needs `llvm-rc`; GTK needs a pkg-config sysroot), and
  `crates/webview-host` does not cross-compile to Linux. Those cfgs were read
  by hand only; CI's Linux job runs the core clippy.
- The Windows core build has five older, unrelated clippy warnings
  (`battery_saving.rs`, `local_path.rs`, `spell_win.rs`, `winreg.rs`); no CI
  job runs clippy on Windows, so they are left for a separate change.
