# Video player, PR 6: NV12 surfaces on Windows

## Problem

On Windows and Linux every video picture goes through the FFmpeg shim.
Before this PR, each picture was handled like this:

1. The shim converted the decoder's YUV picture to BGRA with `sws_scale`.
2. Rust packed it into a `RenderImage`.
3. The renderer uploaded it into the sprite atlas.

At 1080p that is 8 MB of BGRA a picture, on top of a CPU colour
conversion. The design (`video-player.md`, "Windows") planned to hand the
decoder's planes to the DirectX renderer as NV12 and convert on the GPU.
That cuts the upload to 3 MB and removes the CPU conversion.

## What changed

### Vendored GPUI core (`third_party/gpui-pre/src/scene.rs`)

- `SurfaceFormat::Nv12` sits next to `Rgba16Float`.
- `bytes_per_pixel` became `byte_len(width, height)`, because NV12 has no
  per-pixel size. NV12 needs even sides and `w*h*3/2` bytes.
- `SurfaceFrame::new` now returns `Result<_, SurfaceFrameError>`. It
  rejects:
  - an empty frame
  - odd NV12 sides
  - sizes that overflow
  - any byte length other than the exact one

  It used to return `Option`. The only other caller, the Windows HDR image
  path, maps the error into its decode error.
- `YuvColor { matrix: Bt601 | Bt709, full_range }` rides on the frame
  (`with_yuv`, `yuv`). `rgb_rows()` turns it into three affine rows that
  take normalised `(Y, Cb, Cr, 1)` to R, G and B, using the BT.601/709
  Kr/Kb constants and the limited-range offsets (16/255, 255/219,
  255/224). `nv12_planes()` splits the bytes.
- Per platform:
  - macOS still paints `CVPixelBuffer` surfaces.
  - The Linux wgpu renderer (registry crate, not vendored) still draws no
    surfaces, so Linux stays on BGRA images. Later superseded: the crate
    is vendored now and Linux takes this path too (`video-nv12-linux.md`).

  The doc comment on `SurfaceFrame` says so.

### FFmpeg shim (`native/quillvideo`, ABI 2)

- `qv_set_output(ctx, QV_PIXEL_BGRA | QV_PIXEL_NV12)` picks the layout.
  BGRA stays the default, and an unknown code is refused.
- `QvItem` gains these fields:
  - `format` and a second plane (`chroma`, `chroma_stride`)
  - `matrix` (BT.601/709, from the stream's colorspace through the same
    rule the BGRA path uses for `sws_setColorspaceDetails`)
  - `full_range` (from `color_range`, or `yuvj420p`)
  - `convert_us`, the shim's own conversion time
- NV12 is used only when the decoded picture is `yuv420p`, `yuvj420p` or
  `nv12`, the matrix is BT.601 or BT.709, and both the decoded and output
  sides are even. Everything else stays on BGRA from the same context:
  - alpha VP9 (`yuva420p`)
  - 10-bit, 4:2:2 and 4:4:4
  - BT.2020
  - RGB GIF pictures
  - odd sizes

  Mixed clips work because the item says which layout it carries.
- When the output size equals the decoded size, the planes are copied
  (Cb and Cr interleaved for `yuv420p`) and `sws_scale` is skipped. When
  the picture is scaled into the player's box, `sws_scale` resizes into
  NV12 with the same coefficients and range in and out, so no colour
  conversion happens.

### Rust side (`src/video_decode`)

- Picture layout, from options to frames:
  - `OpenOptions.output: PixelOutput` gives the starting layout.
  - `Player::set_output` changes it, and the decode thread passes it to
    `Demuxer::set_output` before the next picture.
  - `VideoFrame.pixels` is `Pixels::Bgra(Vec<u8>)` or
    `Pixels::Nv12 { bytes: Arc<[u8]>, color }`.
  - `VideoFrame.convert_us` adds the shim's time to the Rust packing time.
- `ffi::picture_layout` checks every item before any plane is read:
  - positive and (for NV12) even sides
  - strides at least a row
  - non-null planes
  - a known layout
  - the bytes each plane spans, with checked arithmetic

  Only then are the slices built.
- `pack_nv12` drops stride padding and turns both planes upright for
  rotated clips. It shares the plane walker with `rotate_bgra`.
- `nv12_to_bgra` is the CPU fallback. It uses the same rows as the shader
  and the nearest chroma sample.

### DirectX renderer (`third_party/gpui-pre-windows`)

- `yuv_surfaces.rs` is a pipeline next to `hdr_surfaces.rs`:
  - an `R8_UNORM` luma texture (t0)
  - an `R8G8_UNORM` chroma texture (t2)
  - the instance buffer (t1) with bounds, content mask and the three
    colour rows
- Textures are updated with `UpdateSubresource`, only when a slot holds a
  different frame than last time.
- Slots are keyed by frame id:
  - A slot is reused for a new frame of the same size, as long as the
    frame it holds is not in the current scene (`begin_frame` notes the
    scene's NV12 frames first). Two clips of one size, one paused, then
    keep their own slots and don't re-upload every frame.
  - Different sizes get their own slots, at most 16.
  - A slot not drawn for 2 frames is released.
  - Everything is released when a frame draws no surfaces.
- Textures have one mip level and are sampled bilinearly, so a picture
  drawn much smaller than its texture would alias. This is accepted: the
  decoder scales into the box the player is shown in (the viewer's
  1920 box, an inline tile's own size), so the texture stays close to the
  drawn size.
- `shaders.hlsl`: `yuv_surface_fragment` samples luma and chroma, applies
  the rows and goes through the same `to_output` as everything else, so
  the HDR swap chain still works. The chroma slot (t2) is unbound after
  each draw.
- `build.rs` compiles the new module with fxc in release builds.
- Each NV12 frame learns what the renderer did with it
  (`SurfaceFrame::draw_state`: pending, drawn or refused). A window's
  renderer refuses a frame when:
  - its NV12 pipeline couldn't be created
  - the frame's textures can't be made (it then refuses every new NV12
    frame for 120 frames, about two seconds, and tries again)
  - the planes don't match the size, or the frame is larger than a
    texture can be

  Nothing panics, the window keeps drawing, and the answer belongs to the
  frame. One window's failure doesn't change what another window does,
  and a transient failure is retried instead of disabling NV12 for the
  session.
- `hdr_surfaces.rs` skips NV12 frames, and `draw_surfaces` sends each
  surface to its pipeline by format.

### Player (`src/ui/video_player`)

- `backend_ffmpeg.rs` asks for NV12 only on Windows
  (`surface::NV12_SURFACES`). NV12 pictures become
  `Picture::Surface(Arc<SurfaceFrame>)` and paint through the same surface
  element and corner mask as AVPlayer's pixel buffers. `show()` decides
  once per picture whether it goes out as NV12.
- The element keeps asking for frames while the picture on screen is a
  surface the renderer hasn't drawn yet, so a paused clip settles too.
- When the renderer refuses the surface on screen, the next pull replaces
  it (`Backend::replace_refused`):
  - The backend converts that picture on the CPU and shows it as an image.
  - For two seconds it shows only images: the decoder goes back to BGRA,
    and pictures still queued as NV12 are converted on the CPU.
  - Then it tries NV12 again.
- Copy Frame and other snapshots of a Windows surface convert it on the
  CPU (`Picture::snapshot`). The CPU conversion takes the nearest chroma
  sample, while the GPU interpolates chroma (bilinear, centre-sited), so a
  copied frame can differ slightly from the screen at colour edges.
- Rotated or mirrored clips in the viewer (the user's turn) are turned on
  the CPU, one copy of both planes per picture (`surface::turn_frame`).
  Turning in the shader instead is a follow-up.
- macOS and Linux behave as before: macOS uses AVPlayer, and
  `QUILL_VIDEO_BACKEND=ffmpeg` on macOS stays BGRA.

### Trace lines (`QUILL_TRACE_VIDEO=1`, `docs/build.md`)

- `VT,...` is unchanged.
- `VF,<player>,<format>,<w>,<h>,<bytes>,<convert_us>` is printed after a
  `VT` line whose pull took a new picture. `format` is `nv12` or `bgra`,
  `bytes` is what the renderer uploads, and `convert_us` is the CPU time
  from the decoder's picture to that.
- These two come from the Windows renderer:
  - `VU,nv12,<bytes>,<upload_us>` for each surface texture upload.
  - `VU,atlas,<bytes>,<upload_us>` for atlas uploads of 64 KiB or more.
- `scripts/analyze-video-trace.py` summarises `VF` per player and layout
  and `VU` per path, next to the pacing numbers it already printed.

## Measurements

### Decoder side, on macOS

Run on an M1 Pro, on the Homebrew FFmpeg build (libavcodec 63) with the
shim from this branch:

```
QUILL_VIDEO_LIB=<shim dylib> cargo test --release --no-default-features --lib \
  nv12_conversion_cost -- --ignored --nocapture
```

This decodes all 3066 pictures of `site/media/hero-1920.mp4` (and, with
`QUILL_VIDEO_CLIP`, `hero-1280.mp4`) once per layout. The numbers below
are from two runs, which agreed within a few percent.

| Clip | Layout | Bytes a picture | Convert p50 / p90 | Decode wall a picture |
|---|---|---|---|---|
| 1080p | BGRA (before) | 8 294 400 | 4.98 / 5.48 ms | 5.23 ms |
| 1080p | NV12 (after) | 3 110 400 | 0.53 / 0.93 ms | 0.78 ms |
| 720p | BGRA (before) | 3 686 400 | 2.16 / 2.36 ms | 2.25 ms |
| 720p | NV12 (after) | 1 382 400 | 0.22 / 0.27 ms | 0.27 ms |

What the numbers show:

- At 1080p the CPU colour conversion was most of the decode thread's time
  per picture, matching the design's estimate of about 4 ms.
- The NV12 path's remaining cost is the plane copy in the shim plus the
  packing in Rust.
- Uploads shrink to 37.5 % of the BGRA bytes.

These are decoder numbers only. The GPU side is in the next section.

### Windows: not measured

There was no Windows machine for this PR. CI builds the Windows binary,
compiles the HLSL with fxc, builds the shim with `-Werror` and runs the
core tests there. None of that draws a video. The owner's steps:

1. Build the shim and FFmpeg on Windows as the package does
   (`.github/workflows/package-windows.yml`, the FFmpeg job), so that
   `quillvideo.dll` and the FFmpeg DLLs end up next to `quill.exe` or in
   `QUILL_VIDEO_LIB`.
2. Build both trees in release with the profiler:
   `cargo build --release --features frame-profiler`. Do it
   once on `main` (before) and once on this branch (after).
3. Use the same 1080p60 clip as the core PR: copy
   `site/media/hero-1920.mp4` over
   `docs/screenshots/fixtures/demo-clip-12s.mp4` for the run, and restore
   it afterwards.
4. Run each build:

   ```
   set QUILL_TRACE_VIDEO=1
   set QUILL_ASSUME_ACTIVE=1
   set QUILL_DEMO_LINGER_MS=15000
   set QUILL_FRAME_PROFILE=profile.txt
   quill.exe --screenshot-demo ready-video <out dir> 2> trace.log
   ```

5. Run `python scripts\analyze-video-trace.py trace.log 60` (use the
   panel's refresh rate). Compare:
   - `VF`: the `bgra` line before, the `nv12` line after; bytes and convert
     µs.
   - `VU`: before, `atlas` should show about 8 MB uploads a picture; after,
     `nv12` about 3 MB and no large atlas uploads.
   - `VT`: the gap histogram and double steps should be no worse.
   - `profile.txt`: `draw` and `dirty_to_present` p50/p90.
6. Fallback check: run once with a broken pipeline, using a one-line
   local patch that makes `YuvSurfaces::new` in `yuv_surfaces.rs` return
   an error. Check that video still plays, and that after the first
   `nv12` line the `VF` lines say `bgra` for two seconds at a time. Do it
   once paused too: the tile must show the picture, not stay empty.
7. Device-loss check: with a video playing, switch the GPU driver or run
   `dxcap -forcetdr`. Check that playback continues without a crash.

## How verified

- On macOS:
  - `cargo test --no-default-features --lib video_decode` passes with
    `QUILL_VIDEO_LIB` set to the shim built from this branch against
    Homebrew FFmpeg. That runs these tests:
    - the plane packing, rotation and refusal tests
    - the `picture_layout` table (odd sides, narrow and negative strides,
      null planes, unknown layout)
    - the matrix pick from the shim's code
    - CPU conversion of known colours
    - the player passing its asked layout to the decoder, also after a
      change
    - real-shim tests on the fixtures: H.264 at 96x64 and scaled to
      48x32, a rotated clip coming out 64x96, VP9 WebM, and a mid-stream
      switch BGRA to NV12 to BGRA
  - Against the BGRA output of the same frames, the NV12 output converted
    on the CPU differs by about 4 of 255 on average with the BT.601 matrix
    the shim reports. Read with BT.709 it differs by about 8, so the test
    also checks the matrix pick.
  - `cargo test --features demo-capture --bin quill video_player` passes.
    That covers:
    - `SurfaceFrame` length rules for NV12 and RGBA16F (odd sides, short
      and long buffers, overflow)
    - the GPU rows against the CPU rows
    - BT.601/709 red, green and blue in limited and full range
    - surface creation, turning and snapshot conversion
    - a frame's draw state (pending, drawn, refused, and a refusal that
      stays)
    - the two-second NV12 pause after a refusal
    - the `VF` line's columns
- The shim was compiled with `-Wall -Wextra -Werror` against Homebrew
  FFmpeg.
- The real-shim tests also run in CI against the FFmpeg the package
  builds: on Linux in the `linux-package` job, and on Windows in the
  `windows-ffmpeg` job. Both set `QUILL_REQUIRE_VIDEO_LIB=1`, which turns
  a missing shim from a skip into a failure.
- The HLSL was checked with glslang locally. fxc compiles it in the
  Windows CI job.
- `cargo clippy -p gpui-pre-windows --target x86_64-pc-windows-msvc` is
  clean on macOS (with a stub `llvm-rc` on `PATH` for gpui-pre's build
  script). The app itself is built for Windows only in CI.
- The Windows build is checked by the `ci` workflow's `windows` job, run
  on the same commit pushed to a `cursor/` branch. The PR links the run.
  The PR stays a draft until the Windows steps above are done.

## Not verified

- Anything on a Windows GPU: no picture, no upload trace, no draw times,
  no fallback or device-loss run. The steps are above.
- The colour of the GPU path on screen. The shader uses the same rows the
  CPU tests check, but nobody has looked at it.
- Linux. Nothing changes there: no NV12 is asked for, and pictures stay
  BGRA images.

## Deviations

- **No Windows measurement before the PR.** The design wanted
  before/after numbers on Windows before opening the PR. There was no
  Windows machine, so the PR is a draft and the numbers above are
  decoder-side only.
- **The matrix travels in the instance buffer.** It is uploaded as three
  float4 rows per surface instead of a matrix enum plus a range flag in a
  constant buffer. One instance buffer then covers surfaces of different
  colour spaces in one frame, and the CPU and GPU conversions share one
  function.
- **NV12 only for 8-bit 4:2:0 BT.601/709 with even sides.** Anything else
  stays BGRA through the same shim context. BT.2020 and 10-bit belong to
  the HDR video follow-up.
- **Chroma sampling is center-sited.** The chroma texture is sampled with
  linear filtering at the pixel center. H.264 streams usually site chroma
  on the left luma sample (MPEG-2 style), so colour edges sit half a luma
  pixel to the side. Nobody has looked at whether that shows. The CPU
  fallback uses the nearest sample, so Copy Frame and the screen can differ
  slightly at colour edges.
- **No Windows present hint.** The core PR moved the vsync thread's
  `signal_at` hint (`gpui_windows::next_present_time`) to "PR 6". It is
  not in this PR's scope, so Windows still predicts presents from draw
  starts. It remains a follow-up.
- **`SurfaceFrame::new` returns `Result`.** The brief asked for a bad
  length to come back as an error. `Option` could not say what was wrong
  with a frame, so the signature changed. `QUILL-CHANGES.md` records it.
- **The fallback is per frame, not a renderer switch.** The design had no
  fallback rule. A first version cleared one process-wide flag, which let
  one window's failure blank another window's video and turned NV12 off
  for the session after a single transient failure. Now each frame
  carries the renderer's answer (`SurfaceDrawState` in the vendored core),
  the renderer retries textures after 120 frames, and the backend retries
  NV12 after two seconds.
- **Rotated viewers turn on the CPU.** When the user turns a clip in the
  viewer, both planes are copied turned for every picture. Turning in the
  shader (a rotated texture coordinate) would remove that copy and is a
  follow-up.

## Parity

This closes no README parity item, so it has no parity fragment.
