CHANGES

# Review of B2, B7, B11, B16, B17, B19, B20 in sweep-all

Reviewed sweep-all at bcf6752e. All the bundle branches are merged into it: claude/sweep-b2, sweep-b7, sweep-b11, sweep-b16, sweep-b17, sweep-b19 and sweep-b20. One exception: claude/fix-b7's last commit, 21035932, is not in sweep-all (see B7-2). No later merge reverted a bundle. Line numbers refer to sweep-all.

"Confirmed" means the bug was traced in the code or reproduced by a test. "Plausible" means the reading is strong but nothing ran to prove it. Under the brief, any item left partly done counts as a defect.

## Required confirmations (B2)

1. **The persistent disk warning does not show in every window.** `disk_low_banner` has one call site, `src/ui/app_render/mod.rs:605`, which is the main window. Chat windows (`src/ui/chat_window/view.rs`), call, voice-chat, PiP and notification windows draw no disk element. See B2-4. In the main window, the bar is derived from the disk state on every render. It cannot be dismissed, is not one-shot, and stays until a reading reaches the warn floor. That part is correct.
2. **The tdesktop cache limits are seeded once per account: yes, with one race.**
   - Values: `storage_max_files_size` = 1048576 KiB (1 GiB), `storage_max_time_from_last_access` = 31 x 86400 s, and `use_storage_optimizer` on (`src/storage_limits.rs:26-27, 91-113`).
   - The `storage_limits_seeded` flag is stored per account in data_storage.json. Background accounts are seeded from the shared driver on Ready (`src/connect/core/mod.rs:699`).
   - A second start sends nothing, and an explicit "no limit" sticks.
   - Race: the flag is written before TDLib confirms the options (B2-12).
   - Not done: the brief also asked for a check of how TDLib's cleanup treats files in the Downloads list, and for a release note. Neither exists.

## B2 disk-edges

**B2-1. Freeing space during a held call ends the call (N14). Confirmed.**
- Where: `src/ui/disk_state.rs:228-239, 253-259` and `src/ui/connect_ui.rs:178-183`.
- Failure: at Full, the client is held open for a call. The user frees space, and the next reading is Low or Healthy. `enter_disk_state` clears `held_for_call`, `(Full, _)` steps to Reconnect, and `start_connection` does `live.take()` and `close_live_later`, which drops the call in progress. With the app locked, the same happens at unlock. The test sets `held_for_call` by hand with no live client, so it cannot catch this.
- Fix: when leaving Full while `self.live` is still open (held), skip the reconnect for the shown client and only restart background accounts. Add a test with a live held client.

**B2-2. Downloads paused at Low never resume after a Full close or a restart (N10). Confirmed.**
- Where: `src/connect/media.rs:393-399`. The `disk_paused` set lives in the session (`src/state/domains/media/state.rs:15`).
- Failure: a download is paused at Low (TDLib keeps the pause). The disk then reaches Full and the client closes. When space returns, a new session starts with an empty set, so nothing resumes. Quitting while Low ends the same way.
- Fix: persist the disk-paused file ids per account, and resume them on the first Healthy reading after Ready.

**B2-3. At Full with a held call, user downloads keep writing. Confirmed.**
- Where: `src/connect/media.rs:401-403`.
- Failure: `note_disk(Full)` pauses nothing and sets `disk_room = None`, so a large download keeps writing below the close floor. TDLib then gets ENOSPC and aborts, which is the crash this guard exists to prevent.
- Fix: treat Full like Low with room 0, pausing every user download while the call holds the client.

**B2-4. The disk bar renders only in the main window. Confirmed.**
- Where: `src/ui/app_render/mod.rs:605` (the only call site).
- Failure: a user who works in a chat window, with the main window hidden in the tray, never sees the Low warning, and never sees "will disconnect when the call ends" during a held call.
- Fix: render `disk_low_banner` at the top of `ChatWindowView` and the call and voice-chat windows, from the same disk state. Add a UI test that opens a chat window at Low and finds the bar.

**B2-5. The open floor can rise above the warn floor, so an open is refused with no warning first. Confirmed.**
- Where: `src/disk_guard/mod.rs:100-112`.
- Failure: the warn floors stay fixed at 1024/1280 MiB, while the open floor grows with database size and account count. With a 1.5 GB database and 1.2 GB free, the state is Healthy and no bar shows, yet the next restart or account switch lands on the full-disk screen. The floor-order test (`src/disk_guard/tests.rs:195`) checks only one empty account.
- Fix: `warn = max(1 GiB, open + 256 MiB)` and `clear = warn + 256 MiB`. Add a test of the floor order for a large database and several accounts.

**B2-6. A symlinked data folder on a pulled drive is skipped instead of read as unwritable (N08/N13). Confirmed.**
- Where: `src/disk_guard/folders.rs:250-253`.
- Failure: `accounts/<a>/files` links to an external drive, and the drive is pulled. `canonicalize` fails, the folder drops out of the reading, and the internal disk reads Healthy.
- Fix: count a symlink that no longer resolves as `Volume::Unwritable`.

**B2-7. Database size and the cached-files link are stale after an account switch (N108). Confirmed.**
- Where: `src/ui/disk_state.rs:112-154`.
- Failure: only the watch loop refreshes these values, and on a healthy disk it can wait up to 30 s. Manage local storage then shows the previous account's database size, and a reading taken before the switch can land after it.
- Fix: clear the fields on switch and read again at once. Tag each result with its account id and drop it if the account differs.

**B2-8. An update download already running is not held (N04). Confirmed.**
- Where: `src/ui/updates.rs:79-89, 123-147`.
- Failure: the hold is checked only before the download starts. A download already in flight keeps writing at Low and at Full.
- Fix: give the fetch a cancel flag, set it on the Low or Full edge, and restart the download on the Healthy edge.

**B2-9. The full-disk notification carries Reply and Mark as read actions aimed at chat 0 (N15). Confirmed for the buttons.**
- Where: `src/ui/disk_state.rs:301-308`, `src/ui/notifications/spawn_call_notification.rs:248` and `src/notify/mod.rs:497`.
- Failure: the system notice offers an inline reply and Mark as read for chat 0, with no client open.
- Fix: post the notice with no actions.

**B2-10. Reading Linux quota usage over the soft limit as Full closes clients that can still write (N07). Plausible.**
- Where: `src/disk_guard/linux.rs:137-141` takes the smaller of the soft and hard limits.
- Failure: during the grace period, writes still succeed up to the hard limit, but Quill closes every client.
- Fix: use the hard limit, and use the soft limit only once its grace time has expired.

**B2-11. A zero "important usage" reading on macOS reads as a full disk (N06). Plausible.**
- Where: `src/disk_guard/probe.rs:61`.
- Failure: a volume that answers `volumeAvailableCapacityForImportantUsageKey` with 0 overrides a positive statfs reading and closes everything.
- Fix: use the key only when it is above 0. Otherwise fall back to statfs.

**B2-12. The `storage_limits_seeded` flag is saved before TDLib accepts the options (growth-2). Plausible.**
- Where: `src/connect/settings/storage.rs:212-223`.
- Failure: if the process dies after the flag is written but before TDLib stores the options, or TDLib rejects one option, the account stays unlimited for good.
- Fix: set the flag when the `updateOption` echo for `use_storage_optimizer` arrives. Also do the Downloads-list cleanup check and write the release note the brief asks for.

**B2-13. The "Show cached files" button on the full-disk screen ignores a failed reveal (regresses B19 N93). Confirmed.**
- Where: `src/ui/disk_state.rs:381`: `let _ = quill::platform::reveal_in_file_manager(&folder);`.
- Failure: the cache folder is missing, or xdg-open is missing. The click does nothing and no note appears.
- Fix: route the call through `reveal_in_folder` / `report_failed_open`. Add the raw `quill::platform::{reveal_in_file_manager, open_external_url, open_local_file, open_mailto*}` functions to clippy `disallowed-methods`, allowed only in `src/ui/os_open.rs`.

## B7 video-nv12-linux

**B7-1. More than 16 NV12 surfaces in one frame draw the wrong pictures. Confirmed by reading the code.**
- Where: `third_party/gpui-pre-wgpu/src/yuv_surfaces.rs:239-256`, used by `:145` and `:161-171`.
- Failure: `prepare` records a slot index per surface, and the draws run later. Once the 16 slots are full, `slot_for` does `swap_remove(oldest)`, then `push`. The last slot moves into `oldest`, so indices already recorded this frame point at another clip's textures. The surfaces this affects are a GIF panel or chat with 17+ playing GIFs or round videos, or a large group call. On top of the wrong pictures, texture pairs are evicted and recreated every frame. The Windows code this was ported from is safe only because it draws immediately.
- Fix: never evict a slot drawn in the current frame. If nothing can be evicted, return `None` so the frame falls back to an image. Keep indices stable (`Vec<Option<Slot>>`, or compact only in `end_frame`). Consider raising `MAX_SLOTS` to `MAX_DRAWS` (64). Add a headless test that draws 17 surfaces, each a different colour, and checks every tile's colour.

**B7-2. media-7 "reuse the rotate buffer" is not done in sweep-all. Confirmed.**
- Where: `src/video_decode/mod.rs:296-303` (`rotate_bgra` allocates `vec![0; w*h*4]` per picture) and `src/video_decode/nv12.rs:269` (`nv12_to_bgra`, same).
- Failure: on Linux, any stream that stays on BGRA allocates a fresh 8 MB `Vec` per 1080p picture. That covers 10-bit, 4:2:2, BT.2020 and odd-sized streams, and the pause after a refused surface.
- Fix: commit 21035932 on claude/fix-b7 does this but was never merged. Merge it after fixing B7-3.

**B7-3. In 21035932, the BGRA pool keeps up to about 66 MB per player after output returns to NV12. Plausible.**
- Where: `src/video_decode/bgra.rs`, `KEPT = 8`.
- Failure: wrong-size buffers are dropped only inside `take`. After output goes back to NV12, retired images keep returning 8 MB buffers to a pool that is never drained, until the `Player` drops. The bytes are also missing from `INLINE_DECODE_BUDGET` (`src/ui/inline_players.rs:89`).
- Fix: cap the pool at about 3 buffers, clear it when output switches to NV12, and count pooled bytes in the budget.

**B7-4. Freed video textures are aged by rendered frames, not time. Plausible, low severity.**
- Where: `third_party/gpui-pre-wgpu/src/yuv_surfaces.rs:178-187` (`IDLE_FRAMES = 2`) and the atlas `SPARE_FRAMES = 8`.
- Failure: a closed viewer's texture pair (about 3 MB), and up to 16 MB of spare atlas textures, stay on the GPU for as long as the window renders no new frames.
- Fix: free a slot as soon as its surface leaves the scene in `end_frame`, or age slots by the frame clock's time.

## B11 audio-video-edges

**B11-1. N32 is half fixed: after a seek or at open, sound across a gap still plays early. Confirmed.**
- Where: `src/video_decode/mod.rs:548` (`end_pts = None` on a seek) and `src/video_decode/decode_loop.rs:124` (`end_pts: None` at open).
- Failure: a clip has no sound from 1.0 s to 2.0 s, and the user seeks to 1.5 s. The 2.0 s chunk plays at once and the picture jumps 0.5 s ahead. A clip whose sound starts late plays that sound from t=0.
- Fix: on a seek, set `end_pts` to the seek target. At open, start it at `Some(0.0)`. Add tests for both.

**B11-2. The silence-gap fill trusts the timestamp (panic in debug, wrap or stall in release). Confirmed.**
- Where: `src/video_decode/mod.rs:606`, `(gap * rate).round() as usize * channels`.
- Failure: a bogus far-future pts overflows the multiply. That panics in debug and wraps in release, which breaks panic-safety rule 5. A large but finite jump queues hours of silence instead. The audio queue reaches AUDIO_HIGH_SECS, `next_step` returns Wait, and the pictures freeze for good.
- Fix: cap the gap (for example 5 s) and treat a larger jump as a discontinuity, resetting the clock. Use `saturating_mul`, and check `is_finite`.

**B11-3. N30: a clip seeked while hidden never ends. Plausible.**
- Where: `src/video_decode/mod.rs:948` requires `!self.first`.
- Failure: a round video is seeked from the player bar while nothing paints it. It never reports its end after the sound runs out. The test paints once first, so it misses this.
- Fix: base the hidden end on the clock passing the duration, whatever `first` holds after a seek. Add a test that seeks, then never paints.

**B11-4. N29: `keep_skipped` is never cleared. Plausible.**
- Where: `native/quillvideo/quillvideo.c:963`.
- Failure: after the first seek, the flag stays set for the life of the decoder. A later hardware-decode fallback unrelated to any seek can then show the last picture again.
- Fix: clear it where the skip resolves (`quillvideo.c:558, 803`). This path has not run here, since the container has no shim (see Tests).

**B11-5. L21 is fixed only in the tag: the picture still clips on macOS, and the Windows tag still says HDR. Plausible.**
- Where: `src/ui/hdr_image/viewer.rs:161` and `src/ui/hdr_image/mod.rs:192`.
- Failure on macOS: the HDR frame is still drawn into the 8-bit layer, so highlights clip while the tag says standard range.
- Failure on Windows: `renderer_left_edr` is always false. When the DirectX renderer falls back after `hdr_failed`, the tag still says HDR, which is the L21 bug itself.
- Fix: on fallback, tone-map to SDR. On Windows, wire `hdr_failed` into `renderer_left_edr`.

**B11-6. Missed wake when the sound tap drops. Plausible.**
- Where: `src/video_decode/mod.rs:647-651`.
- Failure: the tap's `Drop` notifies the condvar without holding the state lock. This is the lost-wakeup race that d649d3e6 fixed on the normal path. When the audio output lets go, pictures can stall for up to the new 1 s wait.
- Fix: take the state lock, set the flag, and notify while holding the lock, as the fixed path does.

**B11-7. N35: stopping and replaying a GIF quickly loses the early start. Plausible.**
- Where: `src/ui/inline_playback.rs:190-205`.
- Failure: the reused decode has already sent its first picture, so the row waits for the whole file again. The 165 to 265 ms regression also has no before-and-after measurement anywhere, and there is no PR text, so the item is still unverified.
- Fix: keep the first picture with the reused decode, or restart the decode on replay. Measure the row gap before and after, and record it.

**B11-8. A GIF row jumps back when decoding runs slower than playback. Plausible.**
- Where: `src/ui/gif_clip.rs:332`.
- Failure: `frame_at` uses elapsed time, so during a slow decode the row skips ahead to a later frame, then jumps back once decoding finishes.
- Fix: clamp the frame index to the last decoded picture until decoding completes.

## B16 topic-windows

**B16-1. A topic page in flight lands after a window reset, leaving a gap the reader can never scroll past. Confirmed (two probe tests fail on sweep-all).**
- Where: `src/state/topic_window.rs:114-127` (`note_live` resets on an own send at :117), `src/state/history_prune.rs:205-210` (`trim_to_tail` on close) and `apply_topic_page` at `topic_window.rs:180`. Only `Session::reset_topic_window` calls `drop_topic_pages_past`.
- Failure, scenario A:
  1. The user reads back past the cap, and an older page is in flight.
  2. The user sends a message, which resets the window to that send.
  3. The old page lands. The window now holds messages 96 900 to 96 999 plus the send, with `has_newer=false`.
  4. The reader can never reach the latest messages, and live messages append after the gap.
- Failure, scenario B: a newer page lands after the user closes a short topic. On reopen, the topic shows a middle chunk with a gap before the latest page.
- Fix: mark every in-flight topic page stale on every reset and tail trim (`drop_topic_pages_past(.., &[Oldest, Newest])` from `note_live`'s reset and both `trim_to_tail` branches). Also drop a newer page that lands when `!has_newer`. Add the two probes as regression tests: `own_send_reset_drops_older_page_in_flight` and `closing_short_topic_drops_newer_page_in_flight`.

**B16-2. A failed send that completes late wipes the window. Plausible.**
- Where: `src/state/topic_window.rs:115-117`. `is_local` also matches failed sends.
- Failure: while the user reads back in a short window, a late send failure jumps the window to the latest messages. It can also trigger B16-1.
- Fix: reset only for a new pending send.

**B16-3. Jump to latest after a failed older page shows an empty view with Retry. Plausible.**
- Where: `src/state/topic_window.rs:226` and `src/connect/groups/forum_window.rs:59-72`.
- Failure: a failed older page sets `topic_load_failed`. `topic_jump_to_latest` then empties the window, `fetch_topic_history` returns `Ok(None)`, and no request is sent.
- Fix: `reset_topic_window` clears `topic_load_failed` for the topic.

## B17 engine-small

**B17-1. `ChatId::from_supergroup` returns the wrong id (N104). Confirmed.**
- Where: `src/ids.rs:47-49`, used as the fallback at `src/state/session_forum.rs:119`.
- Failure: `-1_000_000_000_000_i64.saturating_sub(id)` parses as `-(1e12 - id)`, because a method call binds tighter than unary minus. Supergroup 1234567890 gives -998765432110 instead of -1001234567890. The fallback for a chat that no updateNewChat has named yet never finds that chat, so `is_forum` and the member status are lost on updateSupergroup.
- Fix: `Self((-1_000_000_000_000_i64).saturating_sub(supergroup_id))`. Add a unit test, and reuse the function at `connect/subsection_tabs.rs:64` and `connect/deep_links.rs:434`.

**B17-2. N99 is half done: an edited cross-chat original whose history is not loaded keeps a stale strip. Plausible.**
- Where: `src/state/session_reply.rs:650-694` and `src/state/session_apply_messages.rs:391`.
- Failure: chat 7 replies to a text message in chat 8, and chat 8's history is not loaded. The original was fetched with getRepliedMessage and lives in `reply_targets`. It is then edited to add a photo. The thumbnail pass runs, but `edit_loaded_message` never updates the `ReplyTarget::Loaded` copy, so no thumbnail is downloaded and the strip keeps the old text.
- Fix: in the content, edited and ephemeral-content handlers, also update a matching `ReplyTarget::Loaded`. Add a test that delivers the original through getRepliedMessage.

**B17-3. Unread topics of a forum the user left stay in the main badge. Plausible.**
- Where: `src/tray/mod.rs:135` and `:176` (`chat.in_archive == archive`).
- Failure: after the user leaves a forum, the chat drops to order 0, but its `forum_topics` entry stays. The tray, Dock and title badges count its unread topics forever.
- Fix: count only chats with `in_main_list && !in_archive` for the main badge, or drop `forum_topics` for a chat that leaves.

**B17-4. A new per-update scan of every shown message. Plausible, performance.**
- Where: `src/connect/core/mod.rs:96-105` (`shows_cross_chat_reply_to`).
- Failure: every updateMessageContent from a background chat, such as polls or live locations, walks all messages of all shown chats. That is the kind of per-update walk this bundle set out to remove.
- Fix: keep a set of cross-chat reply targets per shown history, rebuilt when its ids revision changes.

## B19 feedback-in-every-window

**B19-1. L42 is not done: a t.me link still says "Opened". Confirmed.**
- Where: `src/ui/deep_links.rs:266`, `StatusNote::info(tr(keys::Q_MISC_OPENED_LINK))`.
- Failure: every successful t.me open shows an "Opened link" toast for 3 s.
- Fix: set no note on `Ok`, and remove `qll_misc_opened_link` from `lang/en/misc.strings:428`.

**B19-2. On macOS and Linux, "the helper started" counts as success (N92/N93). Confirmed.**
- Where: `src/platform.rs:573-581` (`os_open_now`), `:652-660` (`os_spawn`) and `:743-770` (`reveal_now`).
- Failure: the exit status is thrown away, and only a missing binary is reported. These stay silent:
  - xdg-open with no handler (exit 3 or 4)
  - `open -R` on a moved file (exit 1)
  - an xdg-open reveal of a missing folder (exit 2)
- Fix: on unix, return `OsOpen::Pending` and answer with `status.success()` from the reaper thread, as Windows does. For reveal, check `path.exists()` first on every OS, since explorer always exits 1. Then update `crash_notice.rs:126`, which matches only `Answered(false)`.

**B19-3. A narrow forum chat window never draws the toast (N91). Confirmed.**
- Where: `src/ui/chat_window/body.rs:68-76, 94`.
- Failure: the toast is placed inside `conversation`, which is `None` when `topics_only` is true. The note is handed to this window and drawn nowhere.
- Fix: when `topics_only`, draw it in the window body, as `viewer_toast` does.

**B19-4. The toast's window is picked at render time from `windows.last_input`. Plausible.**
- Where: `src/ui/app_render/status_toast.rs:89-96` and `src/ui/chat_window/slot.rs:233-236, 268-272`.
- Failure: a link fails in chat window A, and the user moves the pointer over the main window without clicking. The toast moves there and can show in both windows until A redraws. On Windows, a `Pending` answer lands in whichever window had the last input when the shell replied.
- Fix: capture the host window when the note is queued (in `report_failed_open`, before the open runs), and move a closing chat window's toasts to the main window.

**B19-5. The toast's hover pause can stick. Plausible.**
- Where: `src/ui/app_render/status_toast.rs:212-217` and `toast_queue.rs:114-129, 158-162`.
- Failure: if the hovered toast stops being drawn (its window closes, or the host changes as in B19-4), no hover-off arrives. That toast never expires, and every later toast starts paused.
- Fix: call `set_hovered(false)` in `forget_chat_window` and whenever the host window changes.

**B19-6. Two macOS System Settings opens skip the helper. Confirmed, low impact.**
- Where: `src/calls/capture/screen_mac.rs:76-81` and `src/calls/ptt_global_mac.rs:244-249` use `Command::new("open")` and drop a spawn error.
- Fix: route both through a platform function that returns `OsOpen`, then through `report_failed_open`.

## B20 tests-and-docs

**B20-1. L13: the helper live test can still hang. Plausible.**
- Where: `src/proxy_carrier/helper_live_tests.rs:63-78`.
- Failure: a helper that keeps sending `Alive` while the page never relays still blocks `next()` with no bound. That is the original hang.
- Fix: count the pokes that arrive, and fail once the expected relay has not appeared after N keep-alives. That gives a bounded wait with no wall clock.

**B20-2. L14: the test would pass with the fix reverted. Plausible.**
- Where: `badge_cache_sees_a_folder_read_locally` in `src/tray/tests.rs`.
- Failure: the test calls `mark_chats_read_locally` directly. Reverting the fixed call site at `src/ui/folder_extras.rs:114` leaves it green.
- Fix: drive the demo folder mark-read action through the UI path and assert the badge.

**B20-3. N33: the stale-shim hint can still be lost. Plausible.**
- Where: `src/video_decode/ffi.rs:279, 288, 295`.
- Failure: `last` is overwritten by any later candidate that exists but fails to load. The stale-shim reason is lost, the assert at `src/video_decode/tests.rs:473` still passes, and the decoder tests skip silently again.
- Fix: once a stale reason is found, keep it, and prefer it in the error.

## Tests run (Linux, as root)

- `cargo test --lib` with default features, filtered to these bundles' modules. All passed, 0 failed:
  - B2: disk_guard, crash_log, storage_limits, account_hygiene, closing, downloads, data_settings and others, 156 tests.
  - B7, B11 and B20: video_decode, 81 tests (1 ignored).
  - B11: ui::audio, gif_clip, hdr_image and inline_playback (lib side), 81 tests.
  - B17: avatar, folder_order, forum, supergroup, tray, notify_accounts, pinned and others, more than 300 tests.
  - B19: platform, toast and status_note, 37 tests.
  - B16 and B20: topic_window_cap, threads, history_* and wake, plus tray and stream, more than 150 tests.
- `cargo test --features demo-capture --bin quill`, filtered to these bundles' UI tests. All passed, 0 failed:

  | Filter | Passed |
  | --- | --- |
  | disk_state | 22 |
  | disk_tests | 7 |
  | chat_window | 73 |
  | os_open | 13 |
  | status_toast | 2 |
  | pump_tests | 4 |
  | group_thumbs | 9 |
  | deleting_tests (passes as root now) | 10 |
  | gif_clip | 13 |
  | inline_playback | 2 |
  | video_player | 191 |
  | hdr_image | 20 |
  | ui::audio | 93 |
  | topic | 39 |
  | local_storage | 1 |
  | advanced_ui | 11 |

- Probes for B16-1: two scratch tests (scenarios A and B) failed on sweep-all, which reproduces the bug. They are not committed.
- naga 30.0.1 validated the new WGSL (storage and webgl builds). The color matrices, the range handling and the 80-byte `SurfaceParams` layout match.

Not run here:
- The `gpui-pre-wgpu` `yuv_surfaces` GPU tests. The crate's test target needs font files and `test-support`.
- Every FFmpeg-backed decoder test, including the N29 hardware path. They skip because this container has no quillvideo shim.
- The helper_live tests, which need a desktop session.
- macOS-only code (the important-usage probe, the EDR fallback, the System Settings opens), which can only be read here.
- A Windows build: the `x86_64-pc-windows-gnu` target is not installed.

Side note, outside these bundles: `cargo test --bin quill` with default features does not compile. `src/ui/video_player/decoder_load.rs:61-105` (from fix-b14) uses `#[gpui_kit::test]` and `TestAppContext` under plain `#[cfg(test)]`. CI uses `--features demo-capture`, so CI is unaffected.
