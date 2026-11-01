Seeks stay off the audio thread, hidden clips end and stay current, sound gaps play at their time, and GIFs start at once

Branch `claude/sweep-b11`, bundle B11 of the sweep. One commit per item.

## What was wrong

- L26: a stopped or replaced voice note's build read the whole Ogg file before it first checked its cancel flag.
- L27: a seek inside a playing track ran the decoder's seek on the audio thread. On a long MP3 that is a scan from the file's start.
- N29: a seek past the end keeps the pictures it skips so the last one shows. When the hardware decoder failed during that seek, the software restart cleared that flag and the clip ended with no picture.
- N30: a clip ended only once its picture queue was empty, and only paint empties it. A round video scrolled off screen read as playing past its end.
- N31: with no sound, or after the sound ended, nothing moved the pictures of a clip nobody paints. When paint came back they caught up at decode speed.
- N32: the sound output played chunks back to back whatever their timestamps. After a real gap the later sound played early, and the picture jumped with it.
- N34: the decode thread polled every 40 ms while it had sound, so a paused clip woke it 25 times a second.
- N35: a GIF row stayed on its still until every frame had decoded. Before #1292 GPUI animated that still meanwhile, which hid the wait. Without that, the row's first-second gap grew from 165 to 265 ms.
- N37: a GIF opened in the viewer while the video shim was still loading went to the player. On Linux with the shim missing, the player then failed after the viewer had opened.
- L21: when the Metal renderer could not build its float pipelines it left extended range, but the viewer's tag still said HDR.
- N36 (owner default: play): a GIF sent as a picture file showed as a still. tdesktop plays an `image/gif` document as an animation (`data_document.cpp:1889-1896`, `isAnimation`).
- L08 (owner default: keep the refusal with a file row fallback): a GIF over 64 MB must not decode in memory. tdesktop caps in-memory clips the same way (`media_clip_ffmpeg.cpp:404,417`, `kMaxInMemory`).

## What changed

- L26: `OpusSource::new` takes the build's cancel flag and checks it for every packet. The build also checks it before the open.
- L27: a seek in a playing track is a build, like a start. The build thread opens the file at the target, and `settle` swaps the new player in. The old sound plays until then, the position reads the target, and a failed build leaves the old sound playing with the reason shown. tdesktop's loader thread does this work for a seek outside its buffer. The seek board, its worker thread and `OutputPlayer::try_seek` are gone, because nothing waits on the audio thread any more. `docs/decisions/audio-start.md` says so.
- N29: `restart()` in `quillvideo.c` no longer clears `keep_skipped`. Only `qv_seek` sets it.
- N30: a clip ends once the file is read and every queued picture is already due.
- N31: the player's `Clock` moved into the state the decode thread shares. There is one copy, not a mirror. With no sound playing, the decode thread drops the pictures the running clock has passed and sleeps until the next one is due.
- N32: the audio tap plays silence for a timestamp gap longer than 5 ms and moves its clock through it.
- N34: the tap passes through the state lock before it wakes the decode thread, so a wake cannot fall between the room check and the wait. `IDLE_POLL` is gone. Every idle wait is the one-second safety net.
- N35: the clip is shared with the decode thread. The row plays from the first picture, holds on the last decoded picture until the next one arrives, and loops once the file is read.
- N37: only a finished shim load sends a GIF to the player. Before that, the in-process decoder plays it, and that decoder plays every GIF.
- L21: while an HDR picture shows, the viewer reads the layer's `wantsExtendedDynamicRangeContent`. Once the renderer has turned it off, the tag says standard range.
- N36: an `image/gif` picture-file row autoplays through the same in-process decoder as a GIF animation. Its file pill and name are drawn over the playing picture. A file labelled `image/gif` that is not a GIF stays still instead of going to the inline player.
- L08: no code change was needed. `shows_as_photo` already sends a picture file over 64 MB (`READ_BYTES_LIMIT`) to a file row, and the viewer shows such a file as a card. The N36 commit adds `a_gif_file_too_large_for_the_viewer_keeps_its_file_row_and_does_not_play`, so the row never plays it.

## Measurements

- N34: the decode thread's idle wait for a paused clip with sound went from 40 ms to 1 s, so wakes went from 25 to 1 a second. This is the wait the loop returns. I did not count wakes on a live thread.
- N32: with a 1 s gap in the sound and 1.5 s pulled, what is heard reads 2.43 s without the fix and 1.43 s with it.
- N35: debug build, a 60-frame 400x300 GIF, 3 runs. The first picture now plays after 111 to 178 ms. Before, nothing moved until the whole file had decoded, at 6.6 to 7.2 s. Release timings and the 265 ms row gap on a Mac were not measured.
- N31: a silent clip nobody paints for 5 s wakes its decode thread once per picture, at most 127 times. A variant with the clock floor taken out woke 4880 times and kept a stale picture.

## What ran

- `cargo fmt --check`, and `cargo clippy -- -D warnings` with `--features demo-capture --all-targets` and with `--no-default-features --all-targets`. Both are clean on Linux after merging main.
- `scripts/check-file-size.sh`, `check-hotspots.sh`, `check-app-name.sh`, `check-lang.py` and `check-window-scope.py` all pass. No hotspot file is touched.
- 492 bin tests in the touched areas (audio, playback, GIF, inline playback, video player, HDR, media viewer, message media, history) pass.
- 79 `video_decode` lib tests pass with `QUILL_REQUIRE_VIDEO_LIB=1` against a shim built here from this tree, with Ubuntu's FFmpeg 6.1 and the CI's `-Wall -Wextra -Werror`.
- Each new test fails without its fix:
  - N29 fails against a shim built from main, with no picture.
  - N30 fails with 3 pictures queued and no end.
  - N31 and N32 fail as listed under Measurements.
  - N34 fails with the 40 ms wait.
  - L26, L27, N35, N37 and N36 fail on the old logic, by reading.

## What did not run

- macOS: `hdr_image/macos.rs::window_edr` and the L21 viewer test (`viewer_tests.rs`, macOS and `demo-capture` only) were not compiled. A Mac check needs `cargo test --features demo-capture --bin quill -- hdr_image`.
- Windows: the `x86_64-pc-windows-gnu` target is not installed here, so `cargo check` for it did not run. No Windows-only code changed.
- No app run or screenshot. The playing picture-file GIF row (N36) and the first-second GIF gap (N35) were not seen on screen. Nobody listened to the silence across a sound gap (N32).

## What stays open

- `AudioError::DeviceStalled` now comes only from the demo simulation (`QUILL_DEMO_AUDIO_STALL`). The real engine no longer waits on the audio thread, so it cannot report a stall from a seek.
- During a seek in a playing track, the old sound plays until the new build lands. tdesktop goes silent there instead. On a long VBR MP3 the build's scan can take noticeable time.
- L21 reads the renderer's state on macOS only. A Windows renderer that falls back from scRGB is not detected.
- A clip nobody paints with no sound now decodes its pictures to keep them current. That costs CPU while it is hidden, as the sound-driven case already did.
