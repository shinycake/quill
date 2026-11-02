# Fix notes D: B7, B11, B16 and B17 findings

Branch `claude/fix-final-d`, from `origin/sweep-all` at ba0cf717. One commit per finding. Every test named here fails on the code before its fix, unless a finding says otherwise. No test uses a sleep, a timeout or the wall clock.

## Fixed

| Finding | Commit | What changed | Test |
| --- | --- | --- | --- |
| B7-1 | Show every playing clip's own picture when more than 16 are on screen | NV12 slots: one per draw (64). A slot drawn in the current frame is never replaced, a replacement happens in place so recorded indices stay valid, and with nothing to replace the frame is refused. | `seventeen_clips_each_show_their_own_picture`. Before the fix, tile 0 showed clip 15's picture. |
| B7-2 | (already merged) | 21035932 is an ancestor of sweep-all (merged through a619b43d). Nothing to do. | |
| B7-3 | Let a video's idle BGRA buffers go once it shows NV12 pictures | The pool keeps at most 3 idle buffers instead of 8. Once an NV12 picture is handed out, it drops them and refuses hand-backs until a BGRA picture is decoded again. The inline decode budget now counts the 3. | `a_pool_lets_its_buffers_go_once_nv12_pictures_show`. The two inline budget tests were updated to the new numbers: at 1x, 31 players fit instead of 54, and at 2x, 7 instead of 13. |
| B7-4 | Give a closed video's GPU textures back in the frame it leaves | A slot not drawn in a frame is freed at the end of that frame. A spare atlas texture is freed at the next frame once no texture of its size and format is in use. | `a_clip_that_leaves_frees_its_textures_at_once`, `a_closed_video_gives_back_its_spares_in_the_next_frame` |
| B11-1 | Keep sound after a seek or a late start at its time | The gap fill counts from the seek target, stored with the seek's generation, and from 0 at open. | `a_seek_into_a_gap_in_the_sound_waits_for_the_sound_after_it` (before: 2.23 s heard instead of 1.73 s), `sound_that_starts_late_plays_at_its_time` (before: 1.43 s instead of 0.43 s) |
| B11-2 | Keep pictures moving when a clip's sound has a broken timestamp | A gap over 5 s, or one that is not finite, counts as a broken timestamp. The sound goes on where it was, and later chunks are shifted by the same amount. The sample count saturates. | `sound_with_a_broken_timestamp_plays_on_and_the_pictures_keep_going` (before: the picture froze at 4 s) |
| B11-3 | End a clip seeked while nothing paints it when its sound runs out | Plausible, now proven: after a hidden seek the clock stayed held, so the clip never ended. Once the sound passes the held clock, the clock follows the sound, and the end check accepts a clock running without a shown picture. | `a_clip_seeked_while_nobody_paints_ends_when_its_time_runs_out` (before: position stuck at 1 s) |
| B11-4 | Stop a hardware fallback at the end of a clip replaying its last picture | Plausible, now proven. `keep_skipped` is cleared where the seek's skip resolves. Rust also re-reads the hardware flag at the end of the file. A new test-only stand-in, `QV_HW_FAIL_AT_END` (`StandIn::DrainFails`), fails once as the decoder drains. | `a_hardware_failure_at_the_end_after_a_seek_repeats_no_picture` (before: 0.9 s shown twice) |
| B11-5 | Tone-map an HDR photo into standard range once the window leaves HDR | Once extended range is lost, the frame gets a headroom of 1, so it is tone-mapped instead of clipped. gpui-pre-windows records a failed scRGB switch (`hdr_output::hdr_failed`), and `renderer_left_edr` reads it on Windows. | `the_tag_says_standard_range_once_the_renderer_left_extended_range` also asserts the frame headroom is 1. This test is macOS-only. |
| B11-6 | Wake the decode thread through its lock when a clip's sound output goes | The tap's `Drop` passes through the state lock before it notifies, as `next_sample` does. | None, see below. |
| B11-7 | Play a GIF again at once when it is restarted while still decoding | The decode record keeps the clip once its first picture is in. A replay that takes the decode back plays from it at once. | `a_gif_played_again_while_it_decodes_starts_at_once` (before: nothing played until the decode finished) |
| B11-8 | Keep a GIF row going forward when it decodes slower than it plays | Plausible, now proven. Past the decoded pictures, the play waits at the last one (the wait is kept per play start), then goes on with the next picture and loops from where it waited. | `a_gif_that_decodes_slower_than_it_plays_goes_on_where_it_waited` (before: the second picture showed instead of the fourth) |
| B16-1 | Drop topic pages that land after the window they were asked for went | An own send that replaces the window, and closing a topic (trim or reset), mark every page in flight stale. A newer page that lands on a window already at the latest is dropped. | The review's probes, committed: `own_send_reset_drops_older_page_in_flight`, `closing_short_topic_drops_newer_page_in_flight` |
| B16-3 | Load a topic's latest page on jump to latest after an older page failed | `reset_topic_window` clears the topic's load-failed flag. | `jumping_to_the_latest_after_an_older_page_failed_loads_the_latest` |
| B17-1 | Map a supergroup to its chat id the way TDLib does | Parenthesised the constant. `subsection_tabs` and `deep_links` now call `ChatId::from_supergroup` instead of their own copy. | `a_supergroup_chat_id_follows_tdlib` (before: -998765432110) |
| B17-2 | Update a reply's fetched original when that message is edited | `edit_loaded_message` also applies the edit to a matching `ReplyTarget::Loaded` copy, without counting it as a loaded row. This covers the content, edited and ephemeral-content handlers at once. | `an_edited_original_fetched_for_a_reply_updates_the_strip` |
| B17-3 | Leave a forum's unread topics out of the badge once the user left it | A chat the user left that is in no list counts in neither badge. This applies to the topic sum and to the fallback sum. | `badge_count_skips_topics_of_a_forum_the_user_left` (before: 3 instead of 0) |
| B17-4 | Look up cross-chat reply targets in an index | `HistoryMessages::replies_to` uses a set of reply targets that is rebuilt only after the history changed, so another chat's update no longer walks every shown message. | `replies_follow_inserts_edits_and_removals`. For the timing, see Measurements. |

## Shown not to be a defect

**B16-2 (a failed send that completes late wipes the window).** No code change.
- `TopicHistory::note_live` gets its rows from `history_message(..)`, which always sets `failed: false`. So `is_local` there matches only pending sends.
- A late failure arrives as `updateMessageSendFailed`. `domains/messages/apply.rs` handles it with `replace_id` on the topic history and never calls `note_live`, so it cannot reset the window.
- To check, I sent a `message` with `messageSendingStateFailed`. It reaches `note_live` through `upsert_message(message, pending = true)`, the answer to a send request. TDLib answers `sendMessage` with a pending message and reports a failure only by update, so this path does not happen in real use.

## Not done, or done with limits

- **B11-6 has no test.** The race needs the notify to land between the decode thread's room check and its wait. Under the rule against sleeps and timeouts, no test can force that interleaving, and a test that hits it by chance would hang. The fix copies the pattern d649d3e6 already uses on the normal path. The existing `dropping_the_sound_falls_back_to_a_wall_clock_without_stalling` still passes.
- **B11-7 measurement.** The review asked for the 165 to 265 ms row gap before and after. I could not measure it here, since there is no display that paints rows on a real frame clock. The fix removes the wait for the rest of the file's decode, so a replay plays at once.
- **B11-3, silent clips.** A clip with no sound, seeked while nothing paints it, still holds its clock at the seek target. Nothing plays until it is painted, and it does not end on its own. The finding was about a round video, which has sound.

## Mac and Windows checks still needed

These edits are small. They could not be compiled or run on their own platform here:
- `src/ui/hdr_image/viewer.rs` (tone-map headroom on fallback) and `viewer_tests.rs`. The test is `cfg(macos, demo-capture)` and needs a Mac run: `the_tag_says_standard_range_once_the_renderer_left_extended_range`.
- `src/ui/hdr_image/windows.rs`, `src/ui/hdr_image/mod.rs` (`renderer_left_edr` on Windows), and `third_party/gpui-pre-windows/src/hdr_output.rs` / `hdr_surfaces.rs`. `cargo check --target x86_64-pc-windows-gnu --bin quill --features demo-capture` passes, with one warning that was already there in `src/update_install/layout.rs:101`. No Windows run: a forced swap-chain failure should show the "standard range" tag and a tone-mapped photo.
- `native/quillvideo/quillvideo.c` is plain C shared by all three platforms. It built with `-Wall -Wextra -Werror` on Linux against FFmpeg 6.1.

## What ran (Linux, as root)

- `cargo test --lib video_decode`: 89 passed, 1 ignored. I also ran it with `QUILL_VIDEO_LIB` pointing at a shim built from this tree against the system FFmpeg 6.1 (`apt install libav*-dev`) and `QUILL_REQUIRE_VIDEO_LIB=1`, so the FFmpeg and hardware stand-in tests ran instead of skipping.
- The `gpui-pre-wgpu` GPU tests (`yuv_surfaces`, `wgpu_atlas`): 15 passed on lavapipe. They ran from a scratch copy of the crate with its own `[workspace]`, the repo's patches, a naga dev-dependency and stand-in font files. The crate's own test target needs Zed's font assets, which are not in this repo.
- `cargo test --lib` filtered to topic, threads, history, reply, edit, bubble_headers, tray, badge, notify, unread, ids, subsection, deep_link, supergroup and forum: all passed.
- `cargo test --features demo-capture --bin quill` filtered to gif_clip, on_demand, inline_playback, inline_players, tray and badge: all passed.
- `cargo clippy --all-targets -D warnings` with `--features demo-capture` and with `--no-default-features`: clean. The first run found three issues in this branch's code, fixed in the commit "Satisfy clippy on this branch's fixes".
- `scripts/check-hotspots.sh origin/sweep-all`: OK. `scripts/check-file-size.sh`: OK. The fixes had taken `src/video_decode/mod.rs` to 1021 lines, so `Clock` moved, unchanged, to `src/video_decode/clock.rs`. `cargo fmt`: clean.

## Measurements

- **B17-4.** I timed one lookup for a shown chat holding 1500 messages (the window cap) in a debug test build, with a scratch test that was not committed. It averaged 99 us before (a walk of every message) and 75 ns after (a set lookup, with the set unchanged since the last call). The release build of the whole crate took too long in this container, so there is no release number. Each shown chat costs one lookup per background update. A change to a shown chat rebuilds its set once, at the cost of the old walk.
- **B7-3.** Idle pool memory at 1080p: up to 8 x 8 MB (about 66 MB) before, at most 3 x 8 MB while BGRA plays, and 0 once NV12 pictures show. These numbers are worked out from the code, not measured.
- **B7-1, B7-4.** Not timed. The tests count texture pairs instead: 17 pairs for 17 clips, kept across frames, and freed in the frame a clip leaves.
