Linux plays video on the GPU as NV12, with fewer copies per picture
## What was wrong

- linux-windows-6: the vendored wgpu renderer ignored `PrimitiveBatch::Surfaces`, so `NV12_SURFACES` was off on Linux. Every Linux video picture paid a CPU YUV to BGRA conversion, two UI-thread copies and, above 1024 px, a new texture.
- media-7: on the way to the GPU each picture was copied more often than needed. NV12 pictures were packed into a fresh `Vec`, then copied again into the `Arc<[u8]>` the renderer shares (`Vec` into `Arc<[T]>` copies). The atlas copied every upload with `bytes.to_vec()` before `queue.write_texture`. A picture larger than 1024 px got a texture of its own for each picture. The NV12 resize kept `SWS_ACCURATE_RND`.

## What changed

1. Draw Linux video pictures as NV12 surfaces on the GPU (linux-windows-6). New `third_party/gpui-pre-wgpu/src/yuv_surfaces.rs` ports the DirectX `yuv_surfaces.rs`: R8 luma and RG8 chroma textures, one pair per clip, filled again in place, freed two frames after the clip goes and all at once when no surface is on screen. Matrix rows and bounds go into one uniform buffer at dynamic offsets. Frames are marked drawn or refused, so the refused-frame fallback works as on Windows. `fs_surface` used a fixed full-range BT.601 matrix; it now takes the frame's matrix and range. In `src/ui/video_player`, `NV12_SURFACES` and the code that was Windows only now cover every platform but macOS, including live feeds. The Linux-only one-variant `Picture` branches are gone. macOS behaviour is unchanged: its cfg branches select the same code as before.
2. Stop copying and reallocating each Linux video picture (media-7). NV12 pictures pack straight into pooled `Arc<[u8]>` buffers (`nv12::BufferPool`, refilled once no picture holds one). This also helps Windows. `rotate_bgra` fills a buffer of the right size. The atlas sends uploads to the queue at once, with no `to_vec` (it copies only for an RGBA atlas swizzle), and keeps a freed picture-sized texture as a spare (two at most, for eight frames). The shim's NV12 resize drops `SWS_ACCURATE_RND`, as tdesktop passes flags 0. `--video-probe` prints mean and worst convert time, and `QUILL_VIDEO_PROBE_OUTPUT=nv12` asks for NV12.
3. Owner decision L41 (fast-path chroma): default applied, keep. The BGRA flags (`qv_bgra_sws_flags`) are unchanged. After this PR, BGRA is only the fallback path on Linux.

Measured with release builds in this container (4 cores, no GPU, mean of two runs, convert time per picture):

| Clip | Pixels | Before | After |
| --- | --- | --- | --- |
| 1920x1080 | NV12 | 2.62 ms | 0.70 ms |
| 2560x1440 scaled to 1080p | NV12 | 6.39 ms | 3.99 ms (pool 5.64, then the flag) |
| 1920x1080 | BGRA | 1.74 ms | 1.96 ms (same code; run-to-run noise) |

Against the old Linux default (BGRA, 1.74 ms and 8,294,400 bytes a 1080p picture), a picture is now 0.70 ms and 3,110,400 bytes, with no atlas copy and no texture create.

## What ran

- `cargo fmt`; `cargo clippy -- -D warnings` for `--features demo-capture --all-targets` and `--no-default-features --all-targets`.
- `gpui-pre-wgpu` tests on lavapipe, run with `test-support` on and the font-reading test modules left out (as `QUILL-CHANGES.md` describes for the atlas tests): `yuv_surfaces::tests` (BT.709 limited white, black, red and blue render as those colours; a clip reuses one texture pair; an empty scene frees it; HDR frames are refused) and `wgpu_atlas::tests`, including the new `video_pictures_take_turns_in_two_textures` (2 textures for 30 1080p pictures, where 30 were made before; spares freed after eight frames).
- `cargo test --features demo-capture --bin quill video_player` (187 passed) and `--lib video_decode` with the real shim (76 passed, including the new pool tests).
- The real app under Xvfb and lavapipe, with FFmpeg n9.0.2 built by `scripts/build-ffmpeg.sh`: `--screenshot-demo ready-video-controls` with `QUILL_DEMO_VIEWER_CLIP` set to 1080p and 1440p colour-bar clips. `QUILL_TRACE_VIDEO` shows `nv12,1920,1080,3110400` for each picture. The screen shows correct bars and a smooth grey ramp, and the ended clip keeps its last picture.
- After merging origin/main: fmt, both clippy runs and the video_player tests again.

## What did not run

- No real GPU: the surface path ran on lavapipe only.
- `cargo check --target x86_64-pc-windows-gnu` does not work here: gpui-pre's build script fails before any Quill code compiles. The Windows cfg branches select the same code as before, and the NV12 pool in `src/video_decode` is shared code that ran here.
- No macOS compile. The macOS cfg branches are unchanged in what they select, but a Mac check should build `src/ui/video_player` and `src/ui/inline_video.rs`.
- No frame-pacing comparison under load, and no atlas frame-time measurement: the viewer no longer draws BGRA pictures on Linux unless the stream needs it.
- A 10-bit or 4:2:2 clip (BGRA fallback) through the app.

## What stays open

- Nothing from the bundle. The BGRA fallback still allocates a `Vec` per picture because the `RenderImage` it becomes owns it; pooling it would need the GPUI core to hand image buffers back.
