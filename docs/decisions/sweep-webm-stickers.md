# WebM stickers decoded in process

Quality sweep item 13 (finding F6 in the perf audit), and the sticker part of
item 23 (the Windows and Linux packages don't ship an `ffmpeg` binary).

## Was the finding right

Yes. `quill::sticker_playback::decode_webm_sized` ran `ffprobe` for the
duration, then `ffmpeg -c:v libvpx-vp9` with `fps` and `scale` filters
writing PNG frames to a cache directory. The UI's background task
(`src/ui/sticker_playback.rs`) read each PNG back with `image::open`, put it
on a square and swapped it to BGRA. That is two processes per sticker and
custom emoji, a PNG encode and decode per frame, and a hard dependency on a
system `ffmpeg` built with libvpx. The Windows and Linux packages bundle
FFmpeg's libraries for the video player (`native/quillvideo`) but no
`ffmpeg` binary, so there video stickers and video custom emoji never
animated.

## What Telegram Desktop does

`Telegram/SourceFiles/ffmpeg/ffmpeg_utility.cpp` picks
`avcodec_find_decoder_by_name("libvpx-vp9")` for VP9 ("we need alpha channel
support") and decodes in process (`media/clip/media_clip_ffmpeg.cpp`). It
links libvpx into its FFmpeg build (`Telegram/build/prepare/prepare.py`).

## The alpha problem, and the choice

A WebM sticker carries its alpha as a second VP9 (or VP8) stream: each
Matroska block has a `BlockAdditional` with id 1 whose payload is a VP9
frame, and that frame's luma plane is the alpha plane. libavformat exports it
as `AV_PKT_DATA_MATROSKA_BLOCKADDITIONAL` side data (8-byte big-endian id,
then the payload). FFmpeg's native `vp9` decoder ignores it; libavcodec's
`libvpxdec.c` reads it by running a second libvpx decoder over the payload
and using its Y plane as the A plane.

Quill's FFmpeg build (`scripts/build-ffmpeg.sh`, n9.0.2, LGPL, decoders
only, `--disable-autodetect`) has the native decoders and no libvpx, so the
shim's existing `libvpx-vp9` preference never applies. Two options:

| | (a) second native decoder (chosen) | (b) add libvpx to the build |
| --- | --- | --- |
| What | the shim runs the native `vp9`/`vp8` decoder twice: once on the packet, once on its alpha side data, and hands swscale a YUVA420P picture made of the first's Y/U/V and the second's Y | build libvpx (BSD) on Linux, macOS and MSYS2, `--enable-libvpx`, ship `libvpx` or link it statically into `avcodec` |
| Build | no change to FFmpeg; one more C file in the shim | a new native dependency on three hosts, its own version pinning, deps-watch entry, licence texts, package checks (`check-bundle-pe.ps1`, `check-bundle-elf.sh`) |
| Output | bit-identical to `ffmpeg -c:v libvpx-vp9` on both demo fixtures (every byte of the first 224 px frame, all four channels) | identical by definition |
| Size | +~400 lines of C | +~1-2 MB per package |

I chose (a): it is what libvpxdec does, using decoders the build already
has, and it adds no native dependency. The video player keeps its own path (the shim's
`qv_*` functions are unchanged; this PR doesn't touch any file of the open
Windows NV12 PR #859).

## What changed

- `native/quillvideo/quillsticker.{h,c}` (new, MIT): `qs_open` (local file
  only, `protocol_whitelist=file`, VP9/VP8 only, at most 4096 px a side, fit
  into a box, enlarging like ffmpeg's
  `scale=W:H:force_original_aspect_ratio=decrease`), `qs_info`, `qs_next`
  (decode the next picture and return its time without converting it),
  `qs_pixels` (convert that picture to straight-alpha BGRA, bicubic like the
  `scale` filter), `qs_close`. Both decoders run single-threaded so each
  packet gives one picture and the two stay in step. An alpha block that is
  missing, broken or a different size leaves that picture opaque. Its own
  `QS_ABI_VERSION`, so the video shim's ABI is untouched.
- `scripts/build-ffmpeg.sh` compiles it into the same library
  (`libquillvideo.so` / `.dylib` / `quillvideo.dll`). The package workflows'
  FFmpeg cache key already hashes `native/quillvideo/*`, so CI rebuilds it.
- `src/sticker_playback/webm_ffi.rs` (new): loads the `qs_*` symbols from
  wherever `video_decode` finds the library (`QUILL_VIDEO_LIB`, next to the
  binary, `lib/`, `vendor/ffmpeg/prefix`). A library built before this
  change lacks the symbols and counts as not installed.
- `src/sticker_playback/webm.rs` (new): `decode_in_process`, the ffmpeg
  fallback `decode_with_ffmpeg` (the old path, now returning BGRA and
  removing its PNGs itself), and `WebmFrames` (BGRA frames, one size, fps).
  `decode_webm_sized` tries in process first and falls back only when the
  decoder isn't installed. A file the installed decoder can't read is an
  error, not a second attempt with the binary.
- Frame sampling keeps the old caps exactly: `fps = max_frames / (rounded
  seconds + 1)`, at most 24, longer than 30 s rejected. `FramePlan` picks the
  picture nearest each slot and repeats the previous frame over gaps, as the
  `fps` filter did, and stops at the duration. Frame counts match the ffmpeg
  path on both fixtures (48/24 for the 2 s demo sticker, 72/27 for the 3 s
  512 px one, at sticker and emoji size).
- Callers: `src/ui/sticker_playback.rs` centres the BGRA frames on the
  square (`WebmFrames::into_squares`) instead of decoding PNGs;
  `src/ui/editor_art.rs` takes the first frame as RGBA.
- Decoding stays on the background executor, checks the cancel flag before
  every picture, and the result goes through the same byte budget
  (`make_room`) as before.

## Measurements

macOS arm64, debug test build of Quill (the shim and FFmpeg are built at
`-O2`), seven runs per case, median (min-max). "Before" is main: the same
loop over `decode_webm_sized` plus the UI's PNG read, square overlay and BGRA
swap, run as a temporary test. "Fallback" and "after" are
`cargo test --lib webm_decode_bench -- --ignored --nocapture` on this branch
without and with `QUILL_VIDEO_LIB`, including the square step.

| Sticker, box, max frames | frames | before (main) | of which ffprobe + ffmpeg | fallback (this branch, no library) | after (in process) |
| --- | --- | --- | --- | --- | --- |
| demo 128 px 2 s, 224, 120 | 48 | 376 ms (366-460) | 93 ms | 280 ms (273-288) | 17 ms (17-34) |
| demo 128 px 2 s, 56, 36 | 24 | 100 ms (97-105) | 87 ms | 97 ms (94-103) | 3.2 ms (3.2-3.4) |
| 512 px 3 s, 224, 120 | 72 | 604 ms (601-620) | 150 ms | 471 ms (465-483) | 77 ms (74-80) |
| 512 px 3 s, 56, 36 | 27 | 108 ms (105-163) | 93 ms | 104 ms (100-157) | 28 ms (27-29) |

Part of "before" is PNG decoding in a debug build. The two processes alone
(independent of the build profile, and matching the audit's 92-145 ms) take
2-27 times as long as the whole in-process decode. On Windows each spawn
also goes through antivirus scanning.

Peak memory, one fresh test process per file (both box sizes), growth of
`getrusage` max RSS in MiB. The ffmpeg columns are the fallback on this
branch; main's run (both files in one process) peaked the same, at +24.8 MiB
in Quill and 30.5 MiB in the ffmpeg child.

| File | ffmpeg path: Quill | ffmpeg path: child process | in process: Quill |
| --- | --- | --- | --- |
| demo 128 px | +19.3 | 23.7 | +20.5 |
| 512 px | +24.4 | 30.5 | +32.7 |

In process, Quill itself holds the two VP9 decoders' frame pools and the
loaded FFmpeg libraries (the pools are freed when the decode ends; the
library mapping is shared with the video player), and there is no second
process: about 33 MiB peak instead of 55 MiB for a 512 px sticker. The
frames kept afterwards are the same size as before (72 × 224² × 4 = 14.5 MB
for the 512 px sticker), counted against the sticker cache's byte budget.

## How it was verified

- `video_decode_webm_sticker_keeps_vp9_alpha` decodes
  `tests/fixtures/sticker-vp9-alpha.webm` (951 bytes, made with
  `ffmpeg -f lavfi -i "color=c=black:s=64x48:r=12:d=0.5,format=rgba,geq=r='200':g='40*N':b='60':a='if(lt(X,32),0,255)'" -c:v libvpx-vp9 -pix_fmt yuva420p -auto-alt-ref 0 -crf 30 -b:v 0`):
  12 frames at 24 fps, 64 × 48 (32 × 24 and 128 × 96 in smaller and larger
  boxes), alpha below 16 on the transparent half and above 240 on the opaque
  half of every frame, colours in BGRA order, the picture changing over
  time, cancellation and a non-VP9 file rejected. It skips where the library
  isn't installed; "video_decode" in its name puts it in the Linux package
  job's `cargo test --lib video_decode` run against the freshly built FFmpeg.
  Before this change there was no in-process decoder for it to call.
- Unit tests for the sampling (`sample_fps`, `FramePlan` slots, gaps, cap,
  tail, late start, NaN/inf times) and the square centring (including
  truncated input).
- First frame of both demo fixtures at 224 px compared byte for byte with
  `ffmpeg -c:v libvpx-vp9 ... -vf scale=224:224:force_original_aspect_ratio=decrease -pix_fmt bgra`:
  identical in all four channels.
- Demo capture (`ready-sticker-playback`, `QUILL_DEMO_HISTORY_ANIM=stickers`)
  with the library and without it: the WebM sticker draws the same, on a
  transparent background, in both.
- Linux and Windows CI built the shim with their FFmpeg (gcc and MinGW,
  `-Werror`) on the `cursor/sweep-webm-stickers` push; results in the PR.

## Not covered

- macOS release builds ship no FFmpeg (AVPlayer plays video there,
  `codex-video-cross-platform.md`), so on macOS video stickers still need a
  system `ffmpeg` with libvpx, as before. Bundling the FFmpeg libraries in
  `Quill.app` would remove that and is a packaging change of its own.
- Not run on real Windows or Linux desktops; CI compiles the shim and runs
  the fixture test on Linux. The Windows job builds the DLL but runs no
  decoder tests (unchanged workflow, owned by #859).
- 10-bit or 4:4:4 VP9 stickers decode opaque (the alpha merge handles 8-bit
  4:2:0, which is what Telegram's sticker encoders produce).
