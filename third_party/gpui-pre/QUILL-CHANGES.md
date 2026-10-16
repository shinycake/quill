# Changes made by the Quill project

This directory is gpui-pre 0.3.9 from crates.io (Apache-2.0,
`LICENSE-APACHE`), GPUI's core, used through `[patch.crates-io]` in Quill's
`Cargo.toml`. Commit `cfc4f7e7` added the registry copy without its
`examples/` directory and their `[[example]]` tables (and without the
registry's `.cargo-ok`, `.cargo_vcs_info.json`, `Cargo.toml.orig` and
`Cargo.lock`), so `git diff cfc4f7e7 -- third_party/gpui-pre` shows every
later change. (The patch was first made on 0.3.8, vendored in `f55b3086`,
and carried to 0.3.9 in `docs/decisions/gpui-kit-0-7-3.md`.)

The Quill project changed it in 2026 so the `surface` element can carry CPU
pixels where CoreVideo doesn't exist, for HDR stills on Windows
(`docs/decisions/hdr-viewer-windows.md`):

- `src/scene.rs`: `SurfaceFrame` (width, height, `SurfaceFormat`, the bytes
  behind an `Arc`, a unique id renderers cache uploads by, and an atomic
  display headroom) and `SurfaceFormat::Rgba16Float`; on non-Apple
  platforms `PaintSurface` carries a `frame: Arc<SurfaceFrame>` (on Apple
  platforms it keeps 0.3.9's `CFRetained<CVPixelBuffer>`).
- `src/elements/surface.rs`: `SurfaceSource::Frame` on non-Apple
  platforms, and `surface()` builds on every platform.
- `src/window.rs`: `Window::paint_surface_frame` (non-Apple), the
  counterpart of `paint_surface`.

Renderers that don't draw surfaces (gpui-pre-wgpu on Linux) ignore them, as
before.

The Quill project changed it again in 2026 so video pictures can reach the
DirectX renderer as YUV instead of BGRA (`docs/decisions/video-player-nv12.md`):

- `src/scene.rs`: `SurfaceFormat::Nv12` (8-bit 4:2:0, luma plane then
  interleaved chroma, even sides). `SurfaceFormat::byte_len(w, h)` replaces
  `bytes_per_pixel` (`w * h * 8` for RGBA16F, `w * h * 3 / 2` for NV12),
  and `SurfaceFrame::new` returns a `SurfaceFrameError` (zero or odd side,
  overflow, wrong length) instead of `None`. `YuvMatrix` (BT.601, BT.709)
  and `YuvColor { matrix, full_range }` ride on the frame
  (`SurfaceFrame::with_yuv`, `yuv`); `YuvColor::rgb_rows` is the one place
  the matrix and range become the numbers a renderer uploads.
  `SurfaceFrame::nv12_planes` splits the two planes.
- Only the vendored Windows renderer draws NV12. macOS keeps CoreVideo
  surfaces and Linux (gpui-pre-wgpu) draws no surfaces, so Quill produces
  NV12 frames on Windows only.
- `SurfaceFrame::draw_state` / `mark_drawn` / `mark_refused` and
  `SurfaceDrawState { Pending, Drawn, Refused }`: the renderer that draws a
  frame says what happened to it, so the producer can replace a frame its
  window can't draw (with an image) and knows when a paused picture is on
  screen. The answer belongs to the frame, so each window answers for
  itself.
