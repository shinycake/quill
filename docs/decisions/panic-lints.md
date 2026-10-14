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

## Phase C: `string_slice` and `unchecked_time_subtraction`

Branch `panic-lints-c`. Both lints are now `deny` in `[workspace.lints.clippy]`.

- String slices: about 120 production sites (the `composer` markup parser,
  `spellcheck`, `suggest`, `text_split`, `composer_doc`, link and deep-link
  parsing, `emoji_replace`, UI helpers) went from `text[a..b]` to
  `text.get(a..b).unwrap_or_default()`. For offsets that come from `find` or
  `char_indices` on the same string the result is identical. For an offset
  that is not a char boundary the code now sees an empty slice instead of
  panicking. `stat_chart` colour parsing and the webview-host hex id decode
  use `get` with `and_then` / `?` instead. No `#[expect]` was needed.
- Time subtraction: `Instant::now() - d` became `checked_sub(..)` with
  `unwrap_or_else(Instant::now)`, and the history jump fade uses
  `Duration::saturating_sub`.
- Test code has no clippy allowance for these lints, so test modules and
  test-only files that slice known literals carry
  `#![allow(clippy::string_slice, reason = ..)]`.
- Vendored: `third_party/gpui-base/src/input/editor/display_map/bidi.rs`
  compiles into the quill crate via `#[path]`; its production slices got the
  same `get` treatment and its test module the allow.
- Two leftovers from phase B surfaced under `demo-capture`:
  `src/ui/sticker_set_box/mod.rs` still used `cfg(all(test, ..))` (split now).
- A Windows-only slice in `src/ui/clipboard_files.rs` was fixed by hand; it
  does not compile on macOS.

Verified: UI, `--no-default-features`, `demo-capture,frame-profiler` clippy
all targets, Windows and Linux core cross clippy (no hits), gate.sh. The
Windows/Linux UI-only files were grepped by hand for slices and `Instant`
subtraction.

## Phase D, batch 1: `indexing_slicing` in four modules

Branch `panic-lints-d1`. `src/telegram/envelope`, `src/state`, `src/composer`
and `src/composer_doc` now start with `#![deny(clippy::indexing_slicing)]`.
Other modules are still open and get their own batches.

- `telegram/envelope` (TDLib JSON parsing) had no production hits left; the
  attribute only locks that in.
- 19 production sites fixed with `get`, `first`, `find` and slice patterns:
  `composer` 6 (`attachments`, `markup`), `composer_doc` 2, `state` 11
  (`chat_activity`, `history_trim`, `session_apply_ok`, `session_chat_search`,
  `profile_music`, `shared_media_*`). On valid input the behaviour is the
  same. The fallbacks only run on states that used to panic and none of them
  loops: byte lookups in the markup parser return "not a line start"/"no
  previous byte" and the loops still advance.
- One `#[expect]`: `SharedMediaState::tab_state`, whose array has one slot per
  `SharedMediaTab` and is indexed by the enum discriminant.

Verified: clippy with `-W clippy::indexing_slicing` has zero hits in the four
modules for `--features ui`, `--no-default-features` and
`ui,demo-capture`; plain clippy with the new `deny` passes in all three;
`gate.sh`.

## Phase D, batch 2: the non-UI library modules

Branch `panic-lints-d2`. 168 non-UI modules now start with
`#![deny(clippy::indexing_slicing)]`. That is every top-level module of
`src/lib.rs` that had no site left, including the ones that never had one
(`rich`, `link_handler`, `link_policy`, `deep_link_types`, `emoji`, ...), so a
new slice in them fails the build.

Sites fixed (clippy lines on macOS `--features ui`, production code):

- Network and file-format parsers first: `telegram` 16 (request builders,
  vCard unfolding, name-accent table, failed-response recovery), `export` 69
  (TDLib answers read through `JsonExt::at`, HTML entity cuts, userpic
  colour), `stat_chart` 11 (JSON columns, xlsx cells, pie shares, day labels),
  `spellcheck` 11 plus `spell_dict` and `spell_download`, `text_split` 3,
  `text` 1, `translate` 3, `proxy` 5 (MTProto secret checks), `credentials`
  2, `passcode` 3 (sealed blob parsing), `start_files` 3, `connect` 17
  (percent decoding, request JSON, paid media zip), `update_install`,
  `single_instance`, `video_decode` 3, `voice_opus` 3.
- Pixel and layout code: `wallpaper_raster` 24 (gradient sampling, blur,
  tiling, blit, soft-light table), `album` 35 (the layout functions take
  `[f64; N]` after one slice-pattern match on the ratios; grouping uses
  `get`), `app_theme` 16 (palette and zip parsing, comment stripping).
- Small leftovers: `advanced`, `earn`, `key_fingerprint`, `local_time`,
  `notify`, `service_text`, `settings`.
- Windows only (not compiled on macOS): `spell_win` 1 and `winreg` 2, found by
  the Windows core cross clippy.

Total 243 sites, 2 `#[expect]` (`Palette::rgba` and `Palette::set`, indexed by
the `PaletteKey` discriminant like `SharedMediaState::tab_state` in batch 1).

New helper `src/json_access.rs`. Clippy flags every index into a
`serde_json::Value`, and `value["k"] = x` really does panic when the value is
an array, string or number. `JsonExt::at` / `nth` read like `value["k"]` /
`value[0]` (a missing entry is `Null`, same as `Index`), and `set` /
`set_path` write only into objects or `Null`, as `IndexMut` does, and skip
anything else instead of panicking. On valid input (objects built by
`json!`) the result is identical; the new unit tests cover the skip cases.

Fallbacks, all on states that used to panic: a missing byte or slice reads as
"no match" or an empty slice; the album layout returns no rectangles; the
MTProto secret check reports `Invalid`; `Raster::pixel` returns transparent
black. None of them loops or feeds an offset forward: loops that used to
index still advance or `break`/`continue`.

Left for the next batch (37 modules, about 105 sites): `video` 15,
`calls` 14, `tray_mac` 14, `profile_photo_gradient` 10, `tray` 8, `suggest`
7, `business_hours_edit` 7, `share_box` 4, `power_saving` 4, `playlist` 4,
`folder_archive` 4, `paid_reaction` 3, `inline_grid` 3,
`chatlist_suggestions` 3, `selection_pin` 2, `privacy` 2, `pin_reorder` 2 and
one site each in `voice`, `schedule`, `profile_photo_sources`, `owned_bots`,
`data_settings`, `contacts_index`, `business_info`, `business_bots`,
`auto_delete`. Not touched on purpose: `media_viewer`, the sticker files,
`video_edit`, `video_timeline` and `profile_photo_video` (other threads), and
the Linux-only `tray_sni`, which does not build here (it stays open with
`tray`). `src/ui/` is a later batch.

Verified: plain clippy with the new `deny` passes for `--features ui`,
`--no-default-features` and `ui,demo-capture` (all targets) and for the
Windows and Linux core cross builds; `-W clippy::indexing_slicing` has zero
hits in the denied modules; gate.sh.
