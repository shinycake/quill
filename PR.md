Quill keeps decoding, file writes and shader setup off the UI thread, and the tray badge stops recounting on every update batch

Branch `claude/sweep-b9`, one commit per item of bundle B9 (ui-thread-3), then one commit per finding of the review of this PR (`review.md` on `claude/review-b9`), merged with main at 366613c.

## What was wrong

- subsystems-12 (rest): every applied update batch pokes `wake::CHROME`, and the tray loop in `main.rs` ran `sync_tray` and `sync_icon_badge` on each poke with no throttle. Each wake also armed a fresh 10 s fallback timer that could not be cancelled. The recount itself was already memoized by `Session::revision` in #1297 (`notify_accounts.rs` `UnreadMemo`), so a wake with no unread change was already cheap; the throttle and the timer were what remained.
- ui-thread-4: on Windows each of Quill's own notification toasts is a window, and opening one built the renderer's ten shader pairs again on the UI thread (debug builds also compiled the HLSL from source with `D3DCompileFromFile` each time).
- ui-thread-5 (rest): Done in the photo editor's Set mode cloned the full canvas and applied its pending turns on the UI thread before the off-thread render (`photo_editor_set.rs` `set_render_job`).
- L04: "Adapt from a Sticker" on a WebP sticker decoded it on the UI thread before opening the editor.
- N94: `note_frame_timing` read the window's display on every main window render; off macOS `display_vsync` always answers `None`, so the read was thrown away.
- N95: the reply strip's tinted emoji copies (`reply_pattern::copies`) and a spoiler's blurred preview (`blurred_preview`) were decoded inside render on a cache miss. A preview that failed to decode was retried on every frame.
- N96: a custom theme's palette needs the colour of its background picture, which decodes the picture. That ran on the UI thread when the chooser listed custom themes, when a theme was applied, when the editor opened, on every colour change in the editor, and on import.
- N97: opening the onboarding hashed the 870 KB intro clip and wrote it to the app folder on the UI thread.
- N98: a finished background spell check notified the whole app instead of the composer, where the underlines are drawn.

- Review finding 1: "Adapt from a Sticker" on a still sticker opened the photo editor whenever the off-thread decode landed, even after the set box was closed (Done then uploaded to that set), and an older pick that finished after a newer one replaced its editor.
- Review finding 2: Edit on a custom theme read the file in a detached task that opened the editor whenever it landed: after Settings closed, over a Create new editor (losing its edits), on a theme deleted meanwhile (Save brought it back), or twice on a double click.
- Review finding 3: ui-thread-4 had no Windows timing, and the shader cache's device-loss path had never run.
- Review finding 4: with a custom day theme and a custom night theme, the one-slot palette cache missed at every switch, so the other mode's theme showed until the read landed.
- Review finding 5: the blurred spoiler preview was cached per row only, so a failed decode, or a preview of media an edit replaced, stayed until eviction. Render also copied the minithumbnail bytes on every frame of every spoiler row.
- Review note on Replay: Replay while the first intro write ran started a second write onto the same file. On Windows its rename can fail while the player holds the clip open, and if that result landed first the notice showed with no clip.

## What changed

- subsystems-12: the tray loop runs on the shared `run_poll_loop` through `ui::run_chrome_loop` with `CHROME_PACE` (250 ms gap, 10 s ceiling): the first update after a quiet spell syncs at once, a burst syncs at most four times a second (the menu loop's own guard is once a second), and one fallback timer is kept across rests. The macOS minimized-window check still asks for a 1 s pass. `main.rs` carries a `Hotspot-change` trailer.
- ui-thread-4: `PipelineState::new` in the vendored `gpui-pre-windows` takes its shaders from `shared_shaders`, a per-thread cache keyed by the D3D device, so every window on a device shares them. Instance buffers, the swap chain and DirectComposition stay per window. Blend and sampler states are left alone: D3D11 returns the existing object for an identical description. A new device after a device loss replaces the cache. Recorded in `third_party/gpui-pre-windows/QUILL-CHANGES.md`. `QUILL_TRACE_TOASTS=1` prints how long each toast window took to open (documented in `docs/build.md`), for the timing run. The brief's other option, pooling hidden popups, was not taken: it needs a move and resize path on each platform and holds swap chains, and the brief prefers sharing unless numbers say otherwise.
- ui-thread-5: `into_set_render_job` moves the canvas and its orientation into the job; the turn happens in the worker's `render`.
- L04: the sticker decode runs through `run_off_ui_scoped_then`, and the editor opens when it lands, as adding a sticker inside the editor already did.
- N94: `note_frame_timing` returns at once outside macOS.
- N95: a small shared cache, `ui/render_decode.rs` (`OffRender`, `made_or_start`), makes these pictures on the background executor from an `ImageSource::Custom`. The image draws nothing until its picture lands, then the views that asked are notified, as `image_budget`'s sized loads do. Evicted pictures go to `image_budget::retire_all` as before. A picture that cannot be made is remembered, not retried each frame.
- N96: the chooser's list, a newly applied theme, the editor's opening and an import read on the background executor (`app_theme/reads.rs`). The theme on screen stays until the read lands, then the appearance is applied again. The editor works out the background colour once and reuses it for colour changes (a background change is a solid colour, which needs no decode). `custom_theme_names` now lists the folder without reading the files. At startup, with nothing applied yet, the theme is still read at once so the first frame shows it; Telegram Desktop also applies its theme before showing the window.
- N97: the hash and write run on the background executor. Meanwhile the welcome shows the intro's black, skippable cover, and the 4 s open timeout still applies. A skip before the clip is written runs its way out on black.
- N98: the result calls `notify_composer`, as the per-keystroke underline shift already does.

Review fixes, one commit each:

- Finding 1: the set box records the still pick being decoded (`pending_still`), as it already did for a video with `PendingVideo`. Closing the box (`reset`) or picking another sticker clears or replaces it, and `land_still_pick` opens the editor only for the pick still pending. `reset` runs on close and on a set change, so a pending pick never outlives its set.
- Finding 2: the editor's read is kept in `AppThemeUi::editor_read` instead of detached. It is dropped when the editor closes, when a Current editor opens, when the custom themes are refreshed (delete, import, save) and when Settings closes. A second Edit on the same theme while it is read does nothing, and a read that lands while an editor is open is ignored.
- Finding 3: timed on a Windows runner (numbers below). `QUILL_DEBUG_NOTIFICATION_CUSTOM` now takes a count, so the developer check opens that many toast windows in a row (`docs/build.md`). `shared_shader_tests` in the vendored renderer makes shaders on one WARP device, then on a second, as the recovery after a device loss does, and checks that the second device's shaders are made on it and the first device's are dropped. No popup pool: toasts after the first open in about 5 ms, under the review's 10 ms bar.
- Finding 4: the palette cache keeps the two custom themes read last, so a custom day theme and a custom night theme both stay read. `refresh_custom_themes` clears it.
- Finding 5: the blurred preview is keyed by row and a hash of the minithumbnail bytes. `MiniThumbnail::data` is now an `Arc<[u8]>`, built once in the envelope, so render clones a pointer instead of copying bytes.
- Replay note: `intro_file` writes one at a time. A second write waits for the first and finds the complete file, so there is never a second rename onto the open clip.

## Numbers

Before: time the work took on the UI thread. After: zero on the UI thread for every item except ui-thread-4, since the work moved to a worker. Measured with a release-mode scratch crate using the same `image` 0.25.10 calls on this Linux container (medians):

- ui-thread-5: Set canvas clone and turn at Done, 512 x 512: 1.8 ms.
- L04: 512 x 512 WebP sticker decode: 3.5 ms (lossless test image).
- N95: blurred spoiler preview from a 40 x 30 minithumbnail: 2.1 ms per message. Reply pattern, three tinted copies from a 100 px PNG: 0.25 ms per emoji and colour.
- N96: background colour of a 2560 x 1440 JPEG theme picture: 40 ms, paid on each editor colour change before this branch.
- N97: FNV hash of the 870 KB intro: 1.1 ms; write and `sync_all`: 2.4 ms on the container's disk (a real disk can be much slower).
- subsystems-12: in the test clock, 20 pokes over one second now run 5 syncs (the first at once), where every poke ran one before.

- ui-thread-4 on Windows (review finding 3): release builds with `demo-capture`, the `ready-chats` demo, `QUILL_DEBUG_NOTIFICATION_CUSTOM=20`, `QUILL_TRACE_TOASTS=1`, on a hosted `windows-latest` runner (no GPU, so DXGI gives WARP, Microsoft's software renderer). Before is main with only the trace and the count hunks added. The branch build ran first, then main in the same job. The 20 lines from each:
  - main: 13.1, 7.9, 9.7, 9.0, 7.7, 7.0, 7.4, 7.2, 7.6, 6.8, 6.3, 6.5, 6.5, 7.3, 6.6, 7.4, 6.7, 7.1, 6.3, 7.4 ms.
  - branch: 47.7, 5.9, 4.8, 4.8, 4.6, 5.1, 6.1, 5.0, 5.6, 4.8, 6.4, 4.9, 4.9, 4.7, 4.5, 5.5, 5.2, 6.2, 4.8, 5.0 ms.
  - Toasts 2 to 20: median 7.2 ms on main, 5.0 ms on the branch, so about 2 ms per toast on WARP. What remains is the window, the swap chain and DirectComposition.
  - The first toast is not comparable. The branch ran first on a cold runner and main second, so main's first toast found the runner warm. In both builds the main window already built the shaders on the same device before the first toast, so the cache does not explain the branch's 47.7 ms. A run with the order swapped would settle it.

## What ran

- `cargo fmt`.
- `cargo clippy --features demo-capture --all-targets -- -D warnings` and `cargo clippy --no-default-features --all-targets -- -D warnings`: clean.
- `cargo test --features demo-capture --bin quill`: 2446 passed, 2 ignored, 1 failed. The failure is `background_accounts::deleting_tests::a_log_out_that_cannot_be_recorded_keeps_the_passcode_and_says_why`, the chmod 0555 test that fails as root.
- `cargo test --features demo-capture --test structure_checks`, `scripts/check-hotspots.sh`, `check-file-size.sh`, `check-lang.py`, `check-app-name.sh`, `check-window-scope.py`: pass.
- `cargo check --target x86_64-pc-windows-gnu` (default features) for the whole app, and `cargo check -p gpui-pre-windows --target x86_64-pc-windows-gnu`: pass, with mingw installed in the container. Two warnings it prints are in files this branch does not touch (`update_install/layout.rs`, `video_player/mod.rs`). `--all-targets` for Windows fails on test code this branch does not touch (`video_player/decoder_load.rs`, `startup_failure/vulkan.rs`).
- Review fixes: `cargo fmt`, both clippy runs clean, `cargo test --features demo-capture --bin quill`: 2452 passed, 2 ignored, 1 failed (the same chmod 0555 test). `structure_checks`, `check-hotspots.sh`, `check-file-size.sh`, `check-lang.py`, `check-app-name.sh`, `check-window-scope.py` pass. `cargo check --target x86_64-pc-windows-gnu` passes. The tests ran before the final merge of main (366613c), which brought only main's own changes.
- New tests for the review, each shown to fail with its fix taken out: `sticker_set_box::tests::a_sticker_picked_then_closed_opens_no_editor`, `the_later_pick_wins_when_the_earlier_decode_lands_last`, `app_theme::ui_tests::a_late_custom_read_leaves_a_newer_editor_alone`, `closing_settings_drops_the_editor_read`, `switching_between_two_custom_themes_never_shows_the_other`, `message_media::surfaces::tests::a_new_minithumbnail_is_blurred_after_a_failed_one`. On Windows: `directx_renderer::shared_shader_tests::a_new_device_gets_its_own_shaders` passed (`cargo test -p gpui-pre-windows -p quill --lib --features quill/demo-capture shared_shader_tests`).
- New tests: `wake_wait_tests::a_burst_of_chrome_pokes_syncs_the_badge_four_times_a_second`, `photo_editor_set::tests::done_hands_the_canvas_over_unturned`, `sticker_set_box::tests::a_still_sticker_decodes_off_the_ui_thread_before_the_editor_opens`, `render_decode::tests::a_picture_is_made_off_render_and_lands_once`, `first_run_state::tests::the_intro_waits_on_black_while_its_clip_is_written`, `composer_keystroke_tests::a_spell_check_result_redraws_only_the_composer`, `app_theme::ui_tests::a_custom_theme_is_read_off_the_ui_thread`. None uses sleep or the wall clock.

## What did not run

- No real device loss. The hosted runner has no GPU, only WARP, so there is no driver to reset with `dxcap -forcetdr`. The reset path ran through `shared_shader_tests` on two WARP devices instead, which is what a recovery does to the cache. An actual TDR with a toast up is not exercised.
- No GPU timing. The toast numbers are WARP's. A hardware driver's window, swap chain and shader costs differ, and so may the share the cache saves.
- The Replay write race has no test: it needs Windows file locking (a rename onto a file another handle holds open), and on Linux two concurrent writes both succeed with or without the fix.
- macOS: the tray loop's minimized-window branch and the `cfg!(target_os = "macos")` path of N94 are not compiled here. The tray branch is the same code moved into a closure, with `wait` replaced by `due`.
- N94 has no test: the saved display lookup is not visible to the user, and off macOS the result was already `None`.
- No screenshots: the visible behaviour is the same apart from a picture or theme landing a moment later.

## What stays open

- `cursor/sweep-b9`, the temporary branch that ran the Windows timing job, could not be deleted from this session: the git proxy refuses branch deletes. Please delete it by hand.
- The first-toast figure, as above: the run order makes main's first toast and the branch's first toast incomparable.
