# expect-cleanup: steps 5 and 6 of the no-crash plan

Covers §3.5 and §3.6 of [`no-crash-audit.md`](no-crash-audit.md) and the
non-blocking review notes on #753. Rules are in
[`docs/contributing/panic-safety.md`](../contributing/panic-safety.md).

## What tdesktop does

Nothing here is a tdesktop feature, so there is no behaviour to match. The
only new user-visible path is the alert when the main window cannot open,
which used to be a panic. Its wording and fallback are Quill's own.

## What changed

**Checked `expect`s became types.** The 46 `if self.live.is_some()` blocks
that then called `self.live.as_mut().expect("live")` are now
`if let Some(live) = self.live.as_mut()`. Where a block also read `self`
between the check and the use, the read (all side-effect free: `session()`,
`thread_active()`, `recording_send_reply()`) moved above the `if let`. Files:
`ui/calls`, `ui/chat_list` (mod, toggle_chat_pin), `ui/composer` (mod,
copy_inline_text), `ui/groups_forum/forum_topics_pane`,
`ui/history/load_older_action`, `ui/message_actions/unpin_from_banner`,
`ui/notification_settings` (mute_menu, sounds), `ui/polls`, `ui/privacy`
(mod, privacy_radio_row), `ui/reactions`, `ui/recording`, `ui/secret_chats`,
`ui/share_box_ui`. The rich composer send path, which had no check at all,
now returns when there is no live session.

The other checked sites from §3.6:

| Site | Now |
| --- | --- |
| `ui/group_call_panel.rs` `mute_state` | `let Some(me) = me.filter(..) else { Connecting }` |
| `ui/export_box.rs` | `let Some(draft) = .. else { progress dialog }` |
| `ui/settings_ui.rs` | `else if let Some(settings) = settings` |
| `ui/threads.rs` (2) | `let Some(live) = .. else { return }` |
| `ui/chat_row/with_call_badge.rs` | match on `(badge, emoji)` |
| `ui/group_admin_panels/mod.rs` | empty element without a session |
| `state/session_thread.rs` | `let .. else { return false }` |
| `state/session_calls.rs` (2) | dropped the re-borrow; `get_or_insert_with` already returns `&mut` |
| `ui/history/session_history_row.rs` seek views | `seek_bar.as_ref().map(..)` |
| `ui/media_viewer/video.rs` (3) | `let Some(path) = path else { return }` |
| `ui/selectable_text.rs` element ids (3) | `match global_id`, plus a `retained_cell` helper; a missing id gives a one-frame cell |
| `composer/markup.rs` char boundaries (2) | `text.get(i..)` + `let .. else { break }` |
| `export/json.rs` | builds a `serde_json::Map` directly (same key order) |
| `settings/accounts.rs` | `ok_or_else(io::Error)?` |
| `video_decode/mod.rs` `with_demuxer` | returns `Result` (test-only caller) |
| `sticker_playback.rs` "loaded" | `match &mut *slot { Some(r) => r, empty => empty.insert(..) }` |
| `credentials.rs` "validated" | `api_id?` / `api_hash?` |
| `connect_smoke.rs` | `let Some(..) else { Blocked(MissingCredentials) }` |
| `connect/calls/history.rs`, `connect/group_calls/{mod,controls}.rs` "available engine" (6) | `let Some(engine) = .. else { return }` |

`rg 'expect\('` found a few more in production code that the audit did not
list, fixed the same way: `ui/emoji_sets.rs` (`selected_set_id.unwrap()`
after `is_some()`), and the screenshot demo fixtures `ui/demo/mod.rs`,
`ui/demos/{media,messages,security}.rs`, `ui/message_text/mod.rs`
(`serde_json::to_string(&str)` cannot fail, so `unwrap_or_default`). The demo
self-check `assert!`s in `demos/media.rs` stay: they are the capture's own
test.

The composer_doc stack pop listed in the brief was already gone on main
(fixed in #753).

**`unreachable!`** (3): `twofa_status_body.rs` returns an empty row for the
views that draw their own buttons; `giveaway/about.rs` folds the `Ongoing`
match into the `if` with a let chain; `export/worker.rs` returns
`AlreadyExists` after the numbered-name loop.

**`update_install.rs`**: the app-version parse joins the existing `let (..)
else { return false }`, and a missing probe stdout is an `io::Error`.

**Startup**: `main.rs` no longer panics when `open_window` fails (main and
screenshot demo windows). `ui/startup_failure.rs` writes the error to stderr,
shows a native alert (NSAlert on macOS, `MessageBoxW` on Windows, `zenity`
or `kdialog` on Linux when installed) and exits with code 1. main.rs stays
at its file-size baseline; the trailer covers the hotspot rule because the
brief requires this change there. objc2-app-kit now lists the `NSAlert` feature
explicitly (it was already on through feature unification; no new crate,
Cargo.lock unchanged).

**`// SAFETY:` comments** on every undocumented `unsafe` block: `telegram/ffi.rs`
(10), `calls/engine/ntgcalls/{mod,call_engine}.rs` (14),
`calls/ptt_global_mac.rs`, `calls/ptt_global_win.rs`, `spell_win.rs` (4),
`local_time.rs`, `tray/mod.rs`, `main.rs`, `ui/capture_block.rs`,
`ui/video_pip.rs`, `crates/ntgcalls-sys/src/loader.rs:223,238` and
`crates/webview-host/src/capture.rs` (3, not in the audit). Windows and
Linux files are not linted by a macOS clippy run, so a script also checked
every `unsafe {` in `src` and `crates` for a comment above it.

**One unsoundness fixed** (`calls/ptt_global_mac.rs`): `Handle::drop` set
`active = false` and then called `CFRunLoopStop` on the tap thread's run loop.
The thread could see the flag at its 0.5 s timeout, exit, and free its run
loop before `CFRunLoopStop` ran, which is a use-after-free. The tap thread now
stores the run loop with a `CFRetain`, and `Drop` takes it with `swap(0)`,
stops it and releases it. If `start` times out before the handle exists, that
one retained run loop leaks, which is harmless.

**#753 review nits**: `premium_ui/promo.rs` uses `try_borrow`/`try_borrow_mut`
for the open-and-store step too, so a re-entrant call skips the frame;
`history_message_list.rs` pushes the probe row through `try_borrow_mut`;
`composer_doc/tests.rs` asserts that the full plain text round-trips through
`parse_format_markup`; `composer_doc/mod.rs` drops the redundant
`i + 1 < cuts.len()` (the last cut is always `text.len()`, so the loop stops
there anyway).

## Behaviour

Unchanged wherever the old invariant held. A site that used to panic now
returns early, skips a frame, or reports an error.

## Skipped

None. `gh pr list --repo shinycake/quill` showed no open PRs, so no file was
owned by in-flight parity work.

Left for later (not in this brief): the P2 note in §3.5 that `on_event` in
`ptt_global_mac.rs` takes `&mut TapContext` (sound today, single thread); the
Windows PTT `Drop` posting `WM_QUIT` by thread id could reach a reused id
after the hook thread exits (no memory unsafety); `unsafe` blocks inside
`#[cfg(test)]` code.

## How verified

- `cargo clippy --no-default-features --lib` and `cargo clippy --features ui
  --bins`, both with `-W clippy::expect_used -W clippy::unwrap_used -W
  clippy::unreachable -W clippy::panic -W clippy::undocumented_unsafe_blocks`:
  no warnings left (54 core and 83 UI-only sites before).
- A source scan for `unsafe {` without a `SAFETY:` comment over `src` and
  `crates`, covering the Windows and Linux files: none left.
- `gate.sh`: `GATE OK core=4209 0 ui=549 0`.
- `scripts/check-file-size.sh` passes; `check-hotspots.sh` needs the trailer
  for main.rs.

Not verified: the Windows `MessageBoxW` and the Linux `zenity`/`kdialog`
paths were not compiled or run here (macOS host; CI builds Linux and
Windows). The startup alert was not triggered by hand, since that needs a
machine where window creation fails.
