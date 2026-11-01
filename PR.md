Quill keeps decoding, file writes and shader setup off the UI thread, and the tray badge stops recounting on every update batch

Branch `claude/sweep-b9`, one commit per item of bundle B9 (ui-thread-3), on top of main at 948ad90.

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

## Numbers

Before: time the work took on the UI thread. After: zero on the UI thread for every item except ui-thread-4, since the work moved to a worker. Measured with a release-mode scratch crate using the same `image` 0.25.10 calls on this Linux container (medians):

- ui-thread-5: Set canvas clone and turn at Done, 512 x 512: 1.8 ms.
- L04: 512 x 512 WebP sticker decode: 3.5 ms (lossless test image).
- N95: blurred spoiler preview from a 40 x 30 minithumbnail: 2.1 ms per message. Reply pattern, three tinted copies from a 100 px PNG: 0.25 ms per emoji and colour.
- N96: background colour of a 2560 x 1440 JPEG theme picture: 40 ms, paid on each editor colour change before this branch.
- N97: FNV hash of the 870 KB intro: 1.1 ms; write and `sync_all`: 2.4 ms on the container's disk (a real disk can be much slower).
- subsystems-12: in the test clock, 20 pokes over one second now run 5 syncs (the first at once), where every poke ran one before.

## What ran

- `cargo fmt`.
- `cargo clippy --features demo-capture --all-targets -- -D warnings` and `cargo clippy --no-default-features --all-targets -- -D warnings`: clean.
- `cargo test --features demo-capture --bin quill`: 2446 passed, 2 ignored, 1 failed. The failure is `background_accounts::deleting_tests::a_log_out_that_cannot_be_recorded_keeps_the_passcode_and_says_why`, the chmod 0555 test that fails as root.
- `cargo test --features demo-capture --test structure_checks`, `scripts/check-hotspots.sh`, `check-file-size.sh`, `check-lang.py`, `check-app-name.sh`, `check-window-scope.py`: pass.
- `cargo check --target x86_64-pc-windows-gnu` (default features) for the whole app, and `cargo check -p gpui-pre-windows --target x86_64-pc-windows-gnu`: pass, with mingw installed in the container. Two warnings it prints are in files this branch does not touch (`update_install/layout.rs`, `video_player/mod.rs`). `--all-targets` for Windows fails on test code this branch does not touch (`video_player/decoder_load.rs`, `startup_failure/vulkan.rs`).
- New tests: `wake_wait_tests::a_burst_of_chrome_pokes_syncs_the_badge_four_times_a_second`, `photo_editor_set::tests::done_hands_the_canvas_over_unturned`, `sticker_set_box::tests::a_still_sticker_decodes_off_the_ui_thread_before_the_editor_opens`, `render_decode::tests::a_picture_is_made_off_render_and_lands_once`, `first_run_state::tests::the_intro_waits_on_black_while_its_clip_is_written`, `composer_keystroke_tests::a_spell_check_result_redraws_only_the_composer`, `app_theme::ui_tests::a_custom_theme_is_read_off_the_ui_thread`. None uses sleep or the wall clock.

## What did not run

- ui-thread-4 has no timing yet: there is no Windows machine here, so neither the shader cache nor the brief's "time `open_popup` with 20 toasts" ran on Windows. To measure: on Windows, run with `QUILL_TRACE_TOASTS=1` and trigger 20 toasts on main, then on this branch, and compare the `toast window opened in <ms> ms` lines. The first toast still builds the shaders; later ones should not. The change is type-checked only. Device-loss behaviour with the cache is not exercised.
- macOS: the tray loop's minimized-window branch and the `cfg!(target_os = "macos")` path of N94 are not compiled here. The tray branch is the same code moved into a closure, with `wait` replaced by `due`.
- N94 has no test: the saved display lookup is not visible to the user, and off macOS the result was already `None`.
- No screenshots: the visible behaviour is the same apart from a picture or theme landing a moment later.

## What stays open

- ui-thread-4 timing on Windows, as above. If the numbers show the swap chain and DirectComposition setup still dominate, the next step is the popup pool the brief describes.
