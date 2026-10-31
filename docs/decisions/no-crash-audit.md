# No-crash audit

> Status (2026-10-14): all four lint phases (A to D) are on for the whole
> workspace. PRs: #763, #767, #768, #776, #780, #783, #785, #788, #790, #792
> and `panic-lints-d-final` (this note's PR, which turns on `indexing_slicing`
> workspace-wide). See `docs/decisions/panic-lints.md`.

Branch `no-crash-audit`. Audit and policy, no parity item. Started after the
emoji and sticker panel crash fixed in #747.

The goal: Quill never crashes unless there is no other choice. This document
lists every place in the codebase that can end the process, ranks them, and
gives a fix for each. The rules that keep new code clean are in
[`docs/contributing/panic-safety.md`](../contributing/panic-safety.md).

Scanned at `b9c1bf83` (main after #747), 2026-10-13.

## 1. Why one panic ends the whole app

Rust panics unwind by default (Quill's release profile does not set
`panic = "abort"`). That does not help us, because almost all UI code runs
under a native callback that cannot unwind:

| Platform | What runs our code | What a panic does |
| --- | --- | --- |
| macOS | AppKit calls GPUI's `extern "C"` methods (`step`, `display_layer`, `handle_view_event`, key handlers; `gpui-pre-macos/src/window.rs`). Main-thread and background tasks run through GCD's `extern "C" fn trampoline` (`gpui-pre-apple` 0.3.8 from crates.io, not vendored: `src/dispatcher.rs:192`). | Unwinding out of an `extern "C"` function aborts the process. This is exactly what #747 hit. |
| Windows | `WndProc` is `extern "system"` (`gpui-pre-windows/src/platform.rs:1818`). Debug builds wrap it in `catch_unwind` and then call `process::abort()` on purpose. | Abort in both debug and release. |
| Linux | GPUI's calloop event loop is plain Rust. | The panic unwinds out of `Application::run`, `main` returns, the process exits with status 101. Same result for the user. |

So on all three platforms, a panic anywhere on the UI thread or in a GPUI
background task closes Quill. Threads we spawn ourselves are the exception:
a panic there only ends that thread, which is quieter but can leave a feature
silently dead (a decoder that stops, a receive loop that never returns).

Catching panics on the UI thread is not a way out. GPUI moves an entity out
of its map while `update` runs (`gpui-pre/src/app/entity_map.rs`); if a panic
unwinds through that, the entity never goes back and every later access
panics with "cannot update X while it is already being updated". Zed (GPUI's
origin) ships with `panic = "abort"` and a crash reporter for this reason.
The only real defence on the UI thread is code that does not panic.

There is also no panic hook in Quill today (`rg set_hook src` finds only the
ntgcalls helper of the same name). A crash leaves no Quill log at all; on
macOS the user sees the system crash reporter and we learn nothing.

## 2. Inventory

Counts are for first-party production code: `src/` and `crates/`, without
`#[cfg(test)]` modules, `*_tests.rs`, `tests/` and the demo fixtures. Clippy
counts come from running these restriction lints as warnings on the core
library (`--no-default-features --lib`) and the UI binary (`--features ui
--bin quill`, macOS cfg). Windows-only and Linux-only files were counted with
`rg` and read by hand.

| Category | Sites | Real risk after review | Notes |
| --- | --- | --- | --- |
| `.unwrap()` | 13 | 1 | 2 in `update_install.rs` (one on a probe's piped stdout, one on the app's own version string), 11 in demo fixtures. |
| `.expect()` | 205 | ~40 | 54 in the mock call engine, ~46 `self.live.as_mut().expect("live")` right after an `is_some()` check, ~60 `Mutex::lock().expect(..)`. |
| `panic!` / `todo!` / `unimplemented!` | 0 | 0 | None in production code. |
| `unreachable!` | 3 | 0 | All three are on enum arms the caller rules out; still worth turning into early returns. |
| Indexing `x[i]`, `map[&k]` | 640 | see §3.4 | Clippy `indexing_slicing`: 464 core + 176 UI. |
| String slicing `s[a..b]` | 152 | see §3.4 | Clippy `string_slice`: 141 core + 11 UI. Panics on a non-char boundary. |
| Integer division / modulo | 297 + 52 | see §3.4 | Only division by zero panics in release. |
| Other arithmetic | 1,781 | 0 crashes | Release builds wrap on overflow, they do not panic. Wrong values can still feed an index later. |
| `RefCell::borrow_mut` / `borrow` | 246 + 73 | see §3.2 | 76 guards are held in a `let` binding across other calls; the rest are one-expression borrows. |
| GPUI re-entrancy (`ListState`, entity leases) | 3 `ListState` queries, 168 `Entity<QuillApp>` handles | see §3.2 | The #747 class. |
| `unsafe` blocks / fns | 211 (`src`) + 21 (`crates`) | see §3.5 | 40 without a `// SAFETY:` comment. The 156 hits in `ntgcalls-sys/src/loader.rs` are function-pointer types, not blocks. |
| `extern "C"` callbacks we implement | 6 | 4 | ntgcalls trampolines and the macOS push-to-talk event tap. |
| `process::exit` / `abort` | 6 | 0 | All in `main.rs` CLI paths (`--apply-update`, `--video-probe`, `--connect-smoke`, `--check-updates`, unknown demo kind). Fine. |
| `Mutex` poisoning | 62 lock sites | ~35 | Half already recover with `unwrap_or_else(PoisonError::into_inner)`; the rest `expect` and would turn one panic into a second one. |
| Threads we spawn | 34-35 | see §3.6 | No panic isolation or reporting. |
| Native code in process | TDLib, ntgcalls/WebRTC, rlottie, quillvideo (FFmpeg / AVFoundation), Objective-C calls | see §3.7 | Can crash without any Rust panic. |

Vendored code (`third_party/`: gpui-base and the three GPUI platform crates)
is listed separately in §4. GPUI core (`gpui-pre` 0.3.8) comes from crates.io
unmodified and is not counted.

## 3. Risk ranking and fixes

Severity:

- **P0**: reachable from data we receive (TDLib updates, files, other users'
  messages) or from a common UI path, on a thread where a panic aborts.
- **P1**: reachable from an uncommon user action or uncommon data, or a P0
  pattern that is currently guarded but by a check far away.
- **P2**: needs an invariant to break first (refactor risk), or only affects
  startup, a CLI path, or a background thread.

### 3.1 No crash log and no thread isolation (P0, infrastructure)

| Fix | Where |
| --- | --- |
| Install a panic hook first thing in `main`: write message, location, thread name, app version, OS and a `std::backtrace::Backtrace::force_capture()` to `<data dir>/logs/crash-<timestamp>.txt`, then call the previous hook. No message text from chats goes in it. | `src/main.rs`, new `src/crash_log.rs` |
| On the next launch, if a crash file is newer than the last clean exit, show a one-time "Quill closed unexpectedly" notice with "Show log" (opens the folder). | UI startup |
| Add `spawn_guarded(name, f)` (a `thread::Builder` wrapper that runs `f` under `catch_unwind`, logs through the hook, and reports the death to its owner through a channel or flag) and move all 34 `thread::spawn` / `Builder::spawn` sites to it. The TDLib receive bridge already detects an unexpected stop (`connect/live.rs:78`); the others do not. | `src/` (34 sites, list in §6.1) |
| Keep `panic = "unwind"`. Thread isolation and the test suite depend on it; `abort` would make the background threads in the row above crash the app too. | `Cargo.toml` |

### 3.2 RefCell and GPUI re-entrancy (P0 class, the #747 bug)

The #747 crash: a list row closure asked the `ListState` whether it was on
screen while the list was laying rows out, holding its own state
`borrow_mut()`. The fix moved the query out of the row closure.

What the audit found:

- **`ListState` queries**: after #747 the only three calls
  (`media_panel/mod.rs:444`, `:929`, `:932`) run outside the list's layout.
  The chat list uses kit `VirtualList` and its row closure does not touch the
  scroll handle (`chat_list/toggle_chat_pin.rs:625`). The history list is a
  custom element. No other instance of the bug.
- **Long-held `RefCell` guards**: 76 sites bind a guard with `let` and keep it
  while calling other code. I read all of them. None calls back into code that
  borrows the same cell today. Six hold the guard across a call into a native
  player or other module, which is where a future change would most likely
  re-enter:
  - `ui/story_more.rs:160` and `:793` (`stories.native`, calls `NativeVideo::play` / `pause` / `frame`)
  - `ui/video_editor_tools.rs:161` (`editor.player`, seeks and plays while borrowed)
  - `ui/premium_ui/promo.rs:488` (`premium_preview.video`)
  - `ui/inline_video.rs:645` (`inline_videos`, calls `request_media_tick` while borrowed)
  - `ui/conversation/history_message_list.rs:774` (`probe` inside a `canvas` prepaint, calls `reveal_probe.note`)

  Fix: copy what is needed out of the cell and drop the guard before calling
  anything else (`let frame = slot.borrow_mut().as_mut().and_then(..);`), and
  use `try_borrow_mut()` with an early return in paint/prepaint closures, so a
  re-entrant call skips one frame instead of aborting.
- **Entity leases**: no `cx.entity().update(..)` inside a `QuillApp` method,
  and GPUI defers `emit`/`notify`/`observe` callbacks until the update ends, so
  subscriptions cannot re-enter. The remaining risk is a child entity calling a
  stored `Entity<QuillApp>` callback synchronously while `QuillApp` is updating
  it (`app.update → child.update → callback → app.update`). Not found by
  search, but not provable by search either; a GPUI test that drives every
  dialog would catch it.

### 3.3 Native callbacks that can unwind into C (P0)

Rust code called from C must never panic. Six such functions exist:

| Site | Can panic on | Fix |
| --- | --- | --- |
| `calls/engine/native.rs:40` `signaling_trampoline` | `lock().expect(..)` on two mutexes (poisoned if any holder panicked), plus whatever the stored hook does | Wrap the body in a shared `ffi_boundary(|| ..)` helper (`catch_unwind`, log, return a default). Replace `expect` with a poison-tolerant lock. |
| `calls/engine/native.rs:73` `connection_trampoline` | same | same |
| `calls/engine/native.rs:132` `frames_trampoline` | same, plus 4 locks; frame conversion is checked (`i420_to_rgba` returns `None` on bad sizes) | same |
| `calls/engine/native.rs:253` `remote_source_trampoline` | same | same |
| `calls/ptt_global_mac.rs:88` `on_event` (CGEventTap) | the `sink` closure | same |
| `telegram/ffi.rs:212` `redacted_native_log` | nothing (empty body) | none needed |

`ntgcalls/call_engine.rs` and `ntgcalls/mod.rs` hold another 15 `expect`s on
the same mutexes from ordinary Rust; switch them to the same lock helper.

### 3.4 Indexing, slicing and division

See the per-site triage in §6.2. Summary of what matters:

A helper and I went through all 864 production sites the scan found (348
indexing, 265 slicing, 251 division) plus map lookups. About 250 were read in
context; the rest share a shape with ones that were (pixel chunk indexes,
radio-group `LIST[ix]` closures, float division, `serde_json::Value[key]`).

The news is better than the raw clippy counts suggest: 841 are safe. Every
offset that comes from TDLib (entity offsets, mp4 boxes, Opus packets, chart
JSON, vCards, wallpaper masks) is range- or `is_char_boundary`-checked before
use, and no production code indexes a `HashMap` with `[&key]`. Every integer
divisor is guarded.

The 24 that can panic:

- **P0** `app_theme/png.rs:47-51`: the PNG `IHDR` chunk body is indexed
  (`data[0..4]`, `data[8]`, `data[9]`) without checking it holds 10 bytes. A
  truncated theme or wallpaper file, which anyone can send in a chat, panics.
  Fix: `if data.len() < 10 { return None; }`.
- **P1** `composer_doc/mod.rs:333,337` (`block_range`): clamps a selection to
  the text length but not to a char boundary. Fix: snap with the existing
  `snap_range` (`composer/markup.rs:176`) or use `text.get(..)`.
- **P1** `ui/vanish.rs:196-197`: `picture.width() - 1` on a zero-sized picture
  wraps to `u32::MAX` and `get_pixel` panics. Fix: `saturating_sub(1)`.
- **P2** 18 sites that rely on an invariant a later change could break:
  composer_doc span ranges (9), a stale code-fence range
  (`code_language.rs:104,106`), an index held across a call
  (`ui/emoji_effect.rs:578`), `rotate_bgra` trusting FFmpeg's stride
  (`video_decode/mod.rs:221,245`), chart slices whose start can pass the end
  (`stat_chart/range.rs:135`, `ui/stat_chart/elements.rs:170`), and a fixed
  `[bool; 9]` indexed by `CALL_PROBLEMS` (`ui/calls/mod.rs:368`,
  `ui/call_ui.rs:872`).

Not covered by this pass: unsigned subtraction underflow and `Duration`
subtraction across the codebase (clippy found one `Duration` site,
`ui/history_fx.rs:37`, which is guarded).

### 3.5 `unsafe` and FFI

All `unsafe` is FFI or platform API: TDLib's JSON interface
(`telegram/ffi.rs`), ntgcalls (`calls/engine/ntgcalls/*`, `crates/ntgcalls-sys`),
the video decoder (`video_decode/ffi.rs`), rlottie (`sticker_playback.rs`),
AppKit/AVFoundation/CoreVideo (`ui/native_video*.rs`, `media_session/mac.rs`,
`calls/ptt_global_mac.rs`), Win32 (`spell_win.rs`, `winreg.rs`,
`calls/ptt_global_win.rs`, `icon_badge.rs`) and the Keychain
(`credentials.rs`, `platform.rs`). No `transmute`, no `get_unchecked`, no
`from_utf8_unchecked` in first-party code.

What I checked in the TDLib boundary (`telegram/ffi.rs`): requests with an
interior NUL are rejected (`CString::new` → `InvalidRequest`) instead of
panicking, `td_json_client_receive`'s pointer is null-checked and copied
before the next call, and the log callback is empty. It is sound.

Fixes:

- **P1**: add `// SAFETY:` to the 40 undocumented blocks (37 core, 3 UI:
  `ui/capture_block.rs:33`, `ui/video_pip.rs:177`, `main.rs:596`). Writing the
  comment is the review: each one states what the foreign side guarantees.
  Biggest batches: `telegram/ffi.rs` (10), `calls/engine/ntgcalls/mod.rs` (8),
  `calls/ptt_global_mac.rs` (7), `calls/engine/ntgcalls/call_engine.rs` (6).
- **P1**: `crates/ntgcalls-sys/src/loader.rs:223,238` resolve symbols with no
  comment; same treatment.
- **P2**: `calls/ptt_global_mac.rs:88` turns `info` into `&mut TapContext`
  while `run_tap` still owns the box; fine today because only the tap thread
  touches it, but it should be `&TapContext` with `Cell`s.

### 3.6 Mutexes, `live` checks and other `expect`s (P1)

- **Poisoned mutexes**: `platform.rs:112-147` (credential store, 8 sites),
  `passcode/mod.rs:120-133` (unlock key, 4), `diagnostics.rs:48,62` (the
  diagnostic sink, 2), `connect/sender.rs:89-135` (retry stash, 5),
  `connect/calls/mod.rs` and `connect/group_calls/*` (call outboxes and frame
  slots, 26), `connect/call_audio.rs` (4). A panic on any thread that holds one
  of these turns every later lock into a second panic on the UI thread. Fix:
  one `lock_or_recover()` helper (the pattern `link_handler.rs:396` and
  `spellcheck/mod.rs:398` already use) and use it everywhere.
- **`self.live.as_mut().expect("live")` after `if self.live.is_some()`**: 46
  sites across `ui/calls`, `ui/composer`, `ui/chat_list`, `ui/privacy`,
  `ui/notification_settings`, `ui/secret_chats`, `ui/reactions`,
  `ui/history/load_older_action.rs` and others. Safe today, but the check and
  the use are separate statements. Fix: `if let Some(live) = self.live.as_mut()`.
  Mechanical.
- **Other `expect("checked")`**: `ui/group_call_panel.rs:109`,
  `ui/export_box.rs:168`, `ui/settings_ui.rs:174`, `ui/threads.rs:217,239`,
  `ui/chat_row/with_call_badge.rs:172`, `ui/group_admin_panels/mod.rs:15`
  (guarded by the caller at `:435`), `state/session_thread.rs:232`,
  `state/session_calls.rs:193,235`, `ui/history/session_history_row.rs:435,447`,
  `ui/media_viewer/video.rs:40-55`, `ui/selectable_text.rs:419,682,733`. Same
  fix: restructure so the type proves it (`let Some(x) = .. else { return }`).
- **Startup**: `main.rs:427` `open_window(..).expect("failed to open window")`.
  On a machine without a usable GPU this is the user's whole experience. Fix:
  log the error and show a native alert ("Quill could not open a window: …").
- **`update_install.rs:119`** parses the app's own version with `unwrap()` and
  **`:152`** unwraps a piped stdout that is always set. Both are safe; turn
  them into `?` so the update path has no panics at all.
- **Background threads**: `video_decode/mod.rs:586`
  `expect("spawn the decode thread")` fails only when the OS refuses a thread;
  return the error instead.
- **`unreachable!`** in `ui/security/twofa_status_body.rs:309`,
  `giveaway/about.rs:193`, `export/worker.rs:436`: replace with a default
  button / early return.

### 3.7 Native code (out of Rust's reach)

These can end the process without a Rust panic, so lints do not see them.

- **TDLib** aborts on fatal internal errors (for example a corrupted
  database). That is by design and we cannot catch it. Mitigation: keep TDLib
  current (dependency rule), and make the crash log in §3.1 note when the last
  TDLib log line before exit was fatal (TDLib passes verbosity 0 to the log
  callback first).
- **rlottie** runs in process on sticker files from any chat. Quill bounds the
  input (8 MiB, 120 frames, 30 fps; `sticker_playback.rs:11-49`) and holds a
  global lock. Malformed animations have crashed rlottie in other clients.
  Options, in order of cost: fuzz the loader with `cargo fuzz` on real `.tgs`
  corpora; move rendering into a helper process the way `webview-host`
  already isolates mini apps.
- **quillvideo / FFmpeg / AVFoundation** decode files from chats in process.
  Same options as rlottie.
- **Objective-C exceptions**: an `NSException` thrown inside an `objc2` call
  aborts the process. Calls with ranges or indices are the usual source
  (`NSRangeException`). The one range call (`ui/spellcheck_mac.rs:194`) builds
  the range from the string's own UTF-16 length, so it is safe. Rule for new
  code: compute every `NSRange` from the same `NSString`.
- **Stack overflow** from deep recursion on received data (nested entities,
  rich page blocks). `serde_json` stops at 128 levels; our own recursive
  walkers should carry a depth limit. Not audited site by site yet.

## 4. Vendored code

`third_party/` is our fork of gpui-base 0.7.1 and the GPUI platform crates
0.3.8. They are in the same process, so their panics are our crashes.
Production-code counts: 142 `unwrap`, 57 `expect`, 20 `panic!`, 11
`unreachable!`, 2 `unimplemented!`, 855 `unsafe`, 8 `transmute`.

The ones a user can hit:

| Site | Trigger | Severity | Fix |
| --- | --- | --- | --- |
| `gpui-pre-windows/src/platform.rs:488`, `src/events.rs:1321` `panic!("Device lost: ..")` | GPU driver reset or update while Quill runs, if the renderer cannot be recreated | P0 (Windows) | Retry recreation with backoff; if it still fails, log and close the window cleanly instead of panicking. Patch in our fork and record it in `QUILL-CHANGES.md`. |
| `gpui-pre-windows/src/platform.rs:410` `panic!("failed to initialize DirectX")` | No usable GPU / remote desktop session | P1 (Windows) | Return the error up to `main.rs` so §3.6's startup alert shows it. |
| `gpui-pre-windows/src/vsync.rs` `retrieve_duration` divides by `ticks_per_second / 1_000_000` | DWM reports a refresh period under 1 ms and a refresh rate numerator under 1,000,000, or a QPC clock under 1 MHz | P0 (Windows) | The vsync thread dies and no window repaints. Fixed in the fork: `refresh_interval` is total, recorded in `QUILL-CHANGES.md`. |
| `gpui-pre-windows/src/platform.rs:749,754` `unimplemented!()` in `hide_other_apps` / `unhide_other_apps` | Only if something calls them; Quill does not today (`src/ui/interface_zoom.rs:390` just forwards) | P2 | Make them no-ops in the fork. |
| `gpui-pre-macos/src/display.rs:63` `panic!` when `CGGetActiveDisplayList` fails | Display reconfiguration races (sleep/wake, monitor unplug) | P1 (macOS) | Return an empty list. |
| `gpui-pre-macos/src/text_system.rs:580-605` four `unwrap`s on CoreText run attributes | Malformed font or unusual glyph run | P1 (macOS) | Skip the run. |
| `gpui-pre-macos/src/keyboard.rs:63,70` `to_str().unwrap()` on input source ids | Non-UTF-8 input source name | P2 | `to_string_lossy()`. |
| `gpui-pre-linux/src/linux/x11/client.rs:2707,2715` `panic!` on a bad `GPUI_X11_SCALE_FACTOR` | User sets the env var wrong | P2 | Ignore the value with a warning. |
| `gpui-pre-linux/src/linux/x11/window.rs:523,528` `panic!` on X11 connection errors | X server gone | P2 | Already fatal; fine. |
| `gpui-base/src/resizable/panel.rs`, `dock/layout/tree.rs`, `input/base/*` `expect`s | Internal invariants of kit components | P2 | Leave unless one shows up in a crash log; report upstream. |

Our own changes to these crates follow the same rules as first-party code.
Fixes stay in our fork; only the RTL input work goes upstream.

GPUI core itself (`gpui-pre` 0.3.8, unpatched) panics on entity re-entrancy
by design (§3.2); that is a contract to respect, not a bug to patch.

## 5. Plan

Small PRs, highest risk first, each through the normal gate and review. None
of these touch the files the parity workers currently own (composer send
menu, TON wallet rendering, revenue screens); where one would, it waits.

1. Crash log and guarded threads (§3.1). Gives us data for everything else.
2. FFI boundary helper and poison-tolerant locks (§3.3, mutex part of §3.6).
3. The PNG `IHDR` check, then the P1 and P2 sites from §3.4.
4. RefCell guard narrowing for the six sites in §3.2.
5. `live` / `checked` `expect`s → `if let` (§3.6), and the startup alert.
6. `// SAFETY:` comments (§3.5).
7. Vendored fixes (§4), one PR per platform crate.
8. Turn on the lints in phases (`panic-safety.md`), each phase after its sites
   reach zero.
9. Fuzz targets for the envelope parser, entity offset conversion, composer
   markup and the rlottie loader.

## 6. Site lists

### 6.1 Threads we spawn

- `crates/webview-host/src/main.rs:356`
- `src/single_instance.rs:150`
- `src/single_instance.rs:265`
- `src/single_instance.rs:272`
- `src/single_instance.rs:319`
- `src/video_decode/mod.rs:602`
- `src/profile_photo_camera.rs:94`
- `src/spell_dict.rs:370`
- `src/voice.rs:256`
- `src/calls/ptt_global_mac.rs:179`
- `src/calls/ptt_global_x11.rs:126`
- `src/calls/level_tap.rs:57`
- `src/video/mod.rs:710`
- `src/calls/ptt_global_win.rs:125`
- `src/media_session/linux.rs:229`
- `src/media_session/windows_smtc.rs:31`
- `src/ui/notifications/spawn_call_notification.rs:55`
- `src/ui/notifications/spawn_call_notification.rs:143`
- `src/ui/notifications/spawn_call_notification.rs:282`
- `src/notify_focus.rs:38`
- `src/ui/notifications/linux_dbus.rs:123`
- `src/tray_sni.rs:232`
- `src/system_accent.rs:123`
- `src/spell_win.rs:132`
- `src/ui/wallpaper_cache.rs:147`
- `src/export/account/handle.rs:34`
- `src/export/handle.rs:137`
- `src/icon_badge.rs:330`
- `src/icon_badge.rs:381`
- `src/passcode/mod.rs:859`
- `src/telegram/client.rs:68`
- `src/telegram/hub.rs:184`
- `src/ui/web_app_ui/spawn_mini_app_window.rs:48`
- `src/ui/spellcheck_mac.rs:40`

### 6.2 Indexing, slicing and division triage

The helper's full triage, including the fix for each risky site and per-file
counts of safe sites.

#### Summary

| kind | total | SAFE | RISK-HIGH | RISK-MED | RISK-LOW |
|---|---|---|---|---|---|
| index_lit | 348 | 337 | 0 | 2 | 9 |
| slice_range | 265 | 253 | 0 | 3 | 9 |
| div | 251 | 251 | 0 | 0 | 0 |
| map_index | 0 prod | 0 | 0 | 0 | 0 |
| **all** | 864 | 841 | 0 | 5 | 18 |

map_index: `rg '\w\[&' src` gives 133 hits; all are in `#[cfg(test)]` modules or tests dirs except src/ui/demos/media.rs:204 (`media.files[&82]` on a demo session, demo-only, excluded). So there are no production HashMap/BTreeMap Index sites.

Div: all integer divisors were either guarded (`.max(1)`, `> 0` filter, `is_empty` early return, `months > 0` / `winner_count > 0` filters, `total == 0` check) or are float/Pixels divisions (cannot panic; inf/NaN at worst).

#### RISK list

##### HIGH
None.

##### MED

1. **src/app_theme/png.rs:47-51** (slice_range 47, index_lit 48, 51). Source (a): theme/wallpaper PNG from a theme file or download. The `IHDR` chunk data is indexed `data[0..4]`, `data[4..8]`, `data[8]`, `data[9]` without checking `data.len() >= 10`; a truncated or malformed IHDR chunk panics. Chunk bounds themselves are checked (`png.get(at+8..at+8+len)?`). Fix: `if data.len() < 10 { return None; }` at the top of the IHDR arm, or use `data.get(..)`.

2. **src/composer_doc/mod.rs:333, 337** (slice_range, 2 sites; `block_range`). Source (b): selection range from the composer. `text[..range.start.min(text.len())]` and `text[end_from..]` clamp to length but not to a char boundary, so a range starting or ending inside a multi-byte char panics. Unverified whether every caller passes boundary-aligned offsets (the input widget normally does). Fix: snap offsets to char boundaries (the `snap_range` helper in src/composer/markup.rs:176 already does this) or use `text.get(..)`.

##### LOW

3. **src/composer_doc/mod.rs:660, 677, 714, 727, 744** (index_lit `self.inline[ix]`) and **687, 691, 746, 747** (slice_range `text[a..b]`, `text[..range.start]`, `text[range.end..]`). Source (c): inline span ranges and emoji ranges must stay in sync with `doc.text` after every edit (`cuts` are only filtered to `<= text.len()`, not to char boundaries). Safe today as long as edit paths adjust ranges; a bug there becomes a panic on the serialize/render path. Fix: `debug_assert` plus `text.get(a..b).unwrap_or_default()` in the serializer.

4. **src/code_language.rs:104, 106** (slice_range). Source (c): `with_language(text, fence, ..)` slices by `fence.language` computed by an earlier `fence_at` call; if the draft changed between the two calls the range is stale. Fix: re-run `fence_at` inside, or `text.get(range)` returning an error.

5. **src/ui/emoji_effect.rs:578** (index_lit `active[index]`). Source (c): `index` was captured from `ACTIVE` before `self.effect_clip(..)` ran; fine unless `effect_clip` ever removes entries from the thread-local `ACTIVE`. Fix: `let Some(entry) = active.get_mut(index) else { continue };`.

6. **src/video_decode/mod.rs:221, 245** (index_lit `src[at]`, slice `src[y*stride..y*stride+row]` in `rotate_bgra`). Source (c): trusts `src.len() >= stride * height` from the FFmpeg frame. Fix: early return when `src.len() < stride * h`.

7. **src/ui/stat_chart/elements.rs:170** (slice_range `line.y[first..=last.min(len-1)]`). Source (c): panics if `first > min(last, len-1)`; indices come from `local.indices`, normally ordered and in range. Fix: `line.y.get(first..=end).unwrap_or(&[])`.

8. **src/stat_chart/range.rs:135** (slice_range `line.y[start..=end]`). Source (c): `start` is `indices.0` unclamped while `end` is clamped to `len-1`; panics if a caller passes `indices.0 > len-1`. Current callers use `visible_indices`, which clamps. Fix: also clamp `start` to `end`.

9. **src/ui/calls/mod.rs:368** and **src/ui/call_ui.rs:872** (index_lit `detail.problems[index]`). Source (c): `problems` is a hard-coded `[bool; 9]` while the loop runs over `CALL_PROBLEMS` (currently also 9 entries). Adding a problem to the list without growing the array panics on the render thread. Fix: type the array from `CALL_PROBLEMS.len()`.

##### Extra (not in the input list)

- **src/ui/vanish.rs:196-197** `picture.width() - 1` / `picture.height() - 1`: unsigned underflow (debug panic; in release wraps and then `.min()` keeps a huge value, so `get_pixel` panics). LOW (c): needs a zero-sized decoded picture; line 182 already uses `.max(1)` on the width, so the author considered it. Fix: `saturating_sub(1)`.

#### Notable SAFE-by-construction patterns (spot-checked)

- Text entity offsets: `styled_runs` (src/text/mod.rs:317-325) filters by `utf8_end <= len` and `is_char_boundary` before every slice at lines 356/363/407. rich/input.rs, export/text.rs, bot_stream.rs use converted/clamped offsets.
- Composer parser (src/composer/markup.rs), suggest.rs, emoji_replace.rs, spellcheck/mod.rs, composer_suggest.rs: all slice at ASCII delimiter positions or after `is_char_boundary` / `str::get`.
- mp4 probing (src/video/mod.rs), Opus (src/ui/audio/opus.rs), PNG chunk walker, zip reader, vCard unfolding: every offset checked against length first (the PNG IHDR body is the exception above).
- Stat charts: parsing rejects mismatched column lengths, so `line.y[i]` with `i < x.len()` holds.
- RadioGroup `on_click(|ix| LIST[ix])` closures (settings_ui, call_ui, payments, proxy, appearance, story_*, ttl_picker, promote dialog, export boxes): the closure captures the same list used to build the radios, so `ix` is in range.
- Division: album.rs, bubble_layout.rs, inline_grid.rs, theme_colorize.rs, bubble_theme.rs / backgrounds.rs / soft_light.rs (`n` after a non-empty check or `count.max(1)`), giveaway/award.rs, premium_hub/promo.rs, spell_catalog.rs, video_edit.rs.
- Album layout: for 2 to 4 items, every `ratios[i]` index exists; for 5+ items `layout_complex` returns an empty vec when no split fits rather than indexing.

#### Per-file SAFE counts (sites classified SAFE)

| file | SAFE |
|---|---|
| src/album.rs | 52 |
| src/composer/markup.rs | 43 |
| src/wallpaper_raster/mod.rs | 41 |
| src/spellcheck/mod.rs | 25 |
| src/video/mod.rs | 22 |
| src/ui/stat_chart/paint.rs | 21 |
| src/ui/photo_editor.rs | 18 |
| src/profile_photo_gradient.rs | 16 |
| src/stat_chart/range.rs | 16 |
| src/suggest.rs | 16 |
| src/ui/vanish.rs | 13 |
| src/app_theme/file.rs | 12 |
| src/tray_mac.rs | 11 |
| src/stat_chart/mod.rs | 10 |
| src/ui/inline_video.rs | 10 |
| src/stat_chart/axis.rs | 9 |
| src/text_split.rs | 9 |
| src/ui/message_text/rich_text_reserving.rs | 9 |
| src/composer_doc/mod.rs | 8 |
| src/ui/audio/opus.rs | 8 |
| src/ui/audio/tempo.rs | 8 |
| src/ui/photo_edit.rs | 8 |
| src/ui/spoiler_fx.rs | 8 |
| src/business_hours_edit.rs | 7 |
| src/theme_colorize.rs | 7 |
| src/voice_opus.rs | 7 |
| src/calls/engine/types.rs | 6 |
| src/sticker_playback.rs | 6 |
| src/tray/monochrome.rs | 6 |
| src/ui/anim_layer.rs | 6 |
| src/ui/image_budget.rs | 6 |
| src/ui/round_seek.rs | 6 |
| src/app_theme/png.rs | 5 |
| src/bubble_layout.rs | 5 |
| src/calls/engine/native.rs | 5 |
| src/composer/commands.rs | 5 |
| src/deep_link_inbox.rs | 5 |
| src/export/account/sections.rs | 5 |
| src/folder_archive.rs | 5 |
| src/inline_grid.rs | 5 |
| src/start_files.rs | 5 |
| src/stat_chart/pie.rs | 5 |
| src/ui/composer_suggest.rs | 5 |
| src/ui/demo/mod.rs | 5 |
| src/ui/history/bubble_width.rs | 5 |
| src/ui/stat_chart/pie_paint.rs | 5 |
| src/ui/sticker_playback.rs | 5 |
| src/connect/deep_links.rs | 4 |
| src/connect/messages/media_options.rs | 4 |
| src/export/model.rs | 4 |
| src/media_tools.rs | 4 |
| src/media_viewer/transform.rs | 4 |
| src/passcode/mod.rs | 4 |
| src/proxy.rs | 4 |
| src/share_box.rs | 4 |
| src/spell_win.rs | 4 |
| src/text/mod.rs | 4 |
| src/ui/bubble_theme.rs | 4 |
| src/ui/clipboard_files.rs | 4 |
| src/ui/editor_art.rs | 4 |
| src/ui/story_composer.rs | 4 |
| src/code_language.rs | 3 |
| src/credentials.rs | 3 |
| src/emoji_replace.rs | 3 |
| src/export/text.rs | 3 |
| src/link_policy.rs | 3 |
| src/main.rs | 3 |
| src/paid_reaction.rs | 3 |
| src/power_saving.rs | 3 |
| src/rich/input.rs | 3 |
| src/stat_chart/xlsx.rs | 3 |
| src/translate/mod.rs | 3 |
| src/ui/audio/mod.rs | 3 |
| src/ui/call_panel_backdrop.rs | 3 |
| src/ui/call_tones.rs | 3 |
| src/ui/chatlist_global.rs | 3 |
| src/ui/composer/mod.rs | 3 |
| src/ui/group_panels/mod.rs | 3 |
| src/ui/inline_playback.rs | 3 |
| src/ui/message_actions/unpin_from_banner.rs | 3 |
| src/ui/message_media/visual.rs | 3 |
| src/ui/profile_photo_sources/gradient_editor.rs | 3 |
| src/ui/stat_chart/details.rs | 3 |
| src/ui/stat_chart/state.rs | 3 |
| src/ui/wallpaper_source.rs | 3 |
| src/video_decode/mod.rs | 3 |
| src/voice.rs | 3 |
| src/advanced/codes.rs | 2 |
| src/battery_saving.rs | 2 |
| src/calls/audio_level.rs | 2 |
| src/calls/ptt_global_x11.rs | 2 |
| src/composer/attachments.rs | 2 |
| src/contacts_index.rs | 2 |
| src/local_time.rs | 2 |
| src/owned_bots.rs | 2 |
| src/pin_reorder.rs | 2 |
| src/reply_options.rs | 2 |
| src/rich/markup.rs | 2 |
| src/selection_pin.rs | 2 |
| src/stat_chart/format.rs | 2 |
| src/state/chat_activity.rs | 2 |
| src/story_ring.rs | 2 |
| src/telegram/requests/contacts.rs | 2 |
| src/tray/mod.rs | 2 |
| src/ui/account_lifecycle.rs | 2 |
| src/ui/call_ui.rs | 2 |
| src/ui/emoji_sets.rs | 2 |
| src/ui/export_box.rs | 2 |
| src/ui/frame_clock.rs | 2 |
| src/ui/group_panels/user_info_panel.rs | 2 |
| src/ui/history/mod.rs | 2 |
| src/ui/message_actions/message_menu_overlay.rs | 2 |
| src/ui/message_media/sticker.rs | 2 |
| src/ui/payments/mod.rs | 2 |
| src/ui/premium_ui/promo.rs | 2 |
| src/ui/reply_pattern.rs | 2 |
| src/ui/settings_ui.rs | 2 |
| src/ui/sticker_mirror.rs | 2 |
| src/ui/stickers_ui.rs | 2 |
| src/ui/story_viewer/ensure_story_download.rs | 2 |
| src/ui/wallpaper_cache.rs | 2 |
| src/video_decode/ffi.rs | 2 |
| src/winreg.rs | 2 |
| src/auto_delete.rs | 1 |
| src/boost_box.rs | 1 |
| src/business_bots.rs | 1 |
| src/business_info.rs | 1 |
| src/chat_swipe.rs | 1 |
| src/composer/media_options.rs | 1 |
| src/composer/reply.rs | 1 |
| src/connect/media.rs | 1 |
| src/deep_link_types/link_info_text.rs | 1 |
| src/export/account/worker.rs | 1 |
| src/export/assets.rs | 1 |
| src/export/html_media.rs | 1 |
| src/export/worker.rs | 1 |
| src/giveaway/award.rs | 1 |
| src/link_handler.rs | 1 |
| src/media_viewer/mod.rs | 1 |
| src/media_viewer/ttl.rs | 1 |
| src/notify_prefs.rs | 1 |
| src/phone.rs | 1 |
| src/playback.rs | 1 |
| src/playlist.rs | 1 |
| src/premium_hub/promo.rs | 1 |
| src/premium_limits.rs | 1 |
| src/profile_photo_video.rs | 1 |
| src/pull_next/layout.rs | 1 |
| src/pull_next/mod.rs | 1 |
| src/rich/parse.rs | 1 |
| src/service_text/actions.rs | 1 |
| src/service_text/mod.rs | 1 |
| src/settings/accounts.rs | 1 |
| src/single_instance.rs | 1 |
| src/spell_catalog.rs | 1 |
| src/spell_dict.rs | 1 |
| src/spell_download.rs | 1 |
| src/state/account_notices.rs | 1 |
| src/state/history_trim.rs | 1 |
| src/state/profile_music.rs | 1 |
| src/state/session_apply_ok.rs | 1 |
| src/state/session_chat_search.rs | 1 |
| src/story_extras.rs | 1 |
| src/story_viewer.rs | 1 |
| src/system_accent.rs | 1 |
| src/telegram/envelope/backgrounds.rs | 1 |
| src/telegram/envelope/boost_features.rs | 1 |
| src/telegram/envelope/message_media.rs | 1 |
| src/telegram/requests/polls.rs | 1 |
| src/ui/account_export_box.rs | 1 |
| src/ui/appearance/mod.rs | 1 |
| src/ui/attach_bots.rs | 1 |
| src/ui/bot_stream.rs | 1 |
| src/ui/bots.rs | 1 |
| src/ui/bots_extras.rs | 1 |
| src/ui/business/start_page.rs | 1 |
| src/ui/call_panel.rs | 1 |
| src/ui/chat_list/toggle_chat_pin.rs | 1 |
| src/ui/composer/toggle_composer_silent.rs | 1 |
| src/ui/conversation/history_message_list.rs | 1 |
| src/ui/demos/composer.rs | 1 |
| src/ui/demos/media.rs | 1 |
| src/ui/gifs_ui.rs | 1 |
| src/ui/group_call_extras.rs | 1 |
| src/ui/group_moderation/promote_dialog_panel.rs | 1 |
| src/ui/history_fx.rs | 1 |
| src/ui/inline_mode.rs | 1 |
| src/ui/interface_zoom.rs | 1 |
| src/ui/jump_date.rs | 1 |
| src/ui/media_panel/mod.rs | 1 |
| src/ui/media_viewer/navigation.rs | 1 |
| src/ui/media_viewer/overlay.rs | 1 |
| src/ui/media_viewer/ttl_ui.rs | 1 |
| src/ui/media_viewer/video.rs | 1 |
| src/ui/message_checklist.rs | 1 |
| src/ui/message_media/audio.rs | 1 |
| src/ui/message_media/surfaces.rs | 1 |
| src/ui/message_text/link_preview_tile.rs | 1 |
| src/ui/message_text/mod.rs | 1 |
| src/ui/message_text/rich_block_element.rs | 1 |
| src/ui/motion.rs | 1 |
| src/ui/native_video_rotate.rs | 1 |
| src/ui/notification_settings/ttl_picker.rs | 1 |
| src/ui/playback.rs | 1 |
| src/ui/player_bar.rs | 1 |
| src/ui/polls/option_thumb.rs | 1 |
| src/ui/profile_business.rs | 1 |
| src/ui/profile_photo_sources/camera.rs | 1 |
| src/ui/proxy/build_proxy_edit_dialog.rs | 1 |
| src/ui/proxy/mod.rs | 1 |
| src/ui/pull_next_ui.rs | 1 |
| src/ui/reactions.rs | 1 |
| src/ui/round_record.rs | 1 |
| src/ui/selection_drag.rs | 1 |
| src/ui/send_fly.rs | 1 |
| src/ui/share_box_ui.rs | 1 |
| src/ui/shortcut_pack.rs | 1 |
| src/ui/stat_chart/elements.rs | 1 |
| src/ui/stat_chart/mod.rs | 1 |
| src/ui/stat_chart/state_zoom.rs | 1 |
| src/ui/stat_chart/tween.rs | 1 |
| src/ui/story_albums/mod.rs | 1 |
| src/ui/story_ring.rs | 1 |
| src/ui/story_viewer/story_viewer_overlay.rs | 1 |
| src/ui/video_editor.rs | 1 |
| src/ui/video_editor_tools.rs | 1 |
| src/ui/video_pip.rs | 1 |
| src/ui/wallpaper.rs | 1 |
| src/ui/web_app_ui/mod.rs | 1 |
| src/update_install.rs | 1 |
| src/video_edit.rs | 1 |
| src/voice_input.rs | 1 |
| src/wallpaper_raster/soft_light.rs | 1 |

Total SAFE: 841
