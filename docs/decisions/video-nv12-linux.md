# NV12 video surfaces on Linux

Sweep bundle B7: findings linux-windows-6 and media-7.

## Problem

Windows has drawn FFmpeg video pictures as NV12 surfaces since
`video-player-nv12.md`: the decoder skips its BGRA conversion and the
DirectX renderer converts on the GPU. Linux stayed on BGRA images because
its wgpu renderer was a registry crate that drew no surfaces
(`PrimitiveBatch::Surfaces(_) => {}`). That crate is vendored now
(`third_party/gpui-pre-wgpu`), so the reason has gone. Each Linux picture
still paid:

1. the shim's YUV to BGRA `sws_scale` (1080p: 8,294,400 bytes, 5.23 ms in
   the Windows doc's measurement, against 3,110,400 bytes and 0.78 ms for
   NV12);
2. `rotate_bgra`'s tight copy into a fresh `Vec`;
3. the atlas's `bytes.to_vec()` before upload;
4. `queue.write_texture`'s staging copy;
5. above 1024 px, a texture of its own, created for each picture and freed
   after it.

## What changed

### wgpu renderer (`third_party/gpui-pre-wgpu`)

`yuv_surfaces.rs` ports gpui-pre-windows' `yuv_surfaces.rs`:

- An NV12 frame is uploaded into an `R8Unorm` luma texture and an
  `Rg8Unorm` chroma texture at half size. Both upload with
  `bytes_per_row = width`, the frame's own packing, so no repacking.
- Texture pairs are kept per clip (at most 16) and filled again in place
  while the size stays the same. The slot rules are Windows': a frame
  already uploaded is not uploaded again, a slot whose frame is in this
  scene is never taken by another frame, a slot not drawn for two frames
  is freed, and all of them once no surface is on screen.
- Each draw's bounds, content mask and `YuvColor::rgb_rows` go into one
  uniform buffer at dynamic offsets (64 draws a frame at most; more are
  refused). The bind group per slot is made once.
- `fs_surface` takes the rows from that uniform. It used a fixed
  full-range BT.601 matrix before, which was wrong for most clips
  (limited-range BT.709).
- Frames are marked drawn or refused: another format (HDR stills, which
  Linux doesn't produce), a side over the device's texture limit, or more
  than 64 surfaces. The producer shows a refused frame as an image, as on
  Windows.

Unlike DirectX, wgpu reports a failed texture creation through the device
error handler, not a return value, so there is no "retry in 120 frames"
step: an out-of-memory there is a device error the renderer already
handles.

### Player (`src/ui/video_player`)

`surface::NV12_SURFACES` is on for every platform but macOS, and the code
that was built for Windows only (`Picture::Surface(Arc<SurfaceFrame>)`,
`live_surface.rs`, the corner masks over surfaces, the refused-frame
replacement) now builds off macOS. The Linux-only branches that assumed
`Picture` had one variant are gone. Live feeds take the same NV12 path.
macOS is unchanged.

## How verified

- `yuv_surfaces::tests` in the vendored crate, on lavapipe:
  limited-range white and black, BT.709 red and blue come out as those
  colours; a second picture of a clip reuses its one texture pair; an
  empty scene frees it; an HDR frame is refused. Run with the crate's
  `test-support` feature on and its font-reading test modules left out,
  as the atlas tests were (`QUILL-CHANGES.md`).
- `cargo test --features demo-capture --bin quill video_player`: the NV12
  backend tests that ran only on Windows now take the surface branch on
  Linux.

- The app under Xvfb and lavapipe, with the pinned FFmpeg
  (`scripts/build-ffmpeg.sh`): `--screenshot-demo ready-video-controls`
  with `QUILL_DEMO_VIEWER_CLIP` set to a 1920x1080 limited-range BT.709
  MPEG-4 clip of colour bars over a grey ramp. `QUILL_TRACE_VIDEO` shows
  `VF,1,nv12,1920,1080,3110400,...` for every picture, and the screen shows
  the red, green, blue and white bars and a smooth ramp, with the caption,
  controls and the ended clip's last picture as before.

## Parity

Telegram Desktop's OpenGL media viewer uploads the decoder's YUV planes
and converts them in a shader (lib_ui `ui/gl/gl_shader.cpp`,
`FragmentYUV420Texture` and `FragmentNV12Texture`), so Linux now works
like it, as Windows already did.
