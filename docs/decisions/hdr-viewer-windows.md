# HDR photos in the media viewer (Windows)

The macOS viewer shows HDR photos sent as files at full brightness
(`hdr-viewer-macos.md`). This brings the same thing to Windows: on a
display with Windows HDR ("Use HDR") on, an Ultra HDR / gain-map JPEG or
an HDR JPEG XL opens with its highlights above UI white. The rest of the
window looks as it did.

Design: `quill-tools/designs/hdr-viewer.md`, "Windows (second)".

## What the user sees

The same as on macOS. The file pill shows an `HDR` tag. The viewer
toolbar has an `HDR` tag reading "Shown in high dynamic range", or a
dimmed tag reading "Shown in standard range on this display" when Windows
HDR is off for the display the window is on.

Which files are HDR on Windows:

- **JPEG with a gain map**: Android's Ultra HDR (Pixel, Samsung and other
  phones since Android 14) and ISO 21496-1 gain-map JPEGs (libultrahdr 2,
  recent Lightroom and Photoshop exports). Both the binary ISO metadata and
  Adobe's `hdrgm` XMP are read.
- **JPEG XL** in PQ or HLG, or linear with an intensity target above SDR's
  255 nits.
- **HEIC and AVIF stay SDR**, with no tag. Microsoft's HEIF and AV1
  codecs decode only the base picture: WIC exposes neither the HEIC gain
  map image nor 10-bit PQ / HLG samples. iPhone photos therefore show in
  SDR on Windows. See the follow-up below.

## How it works

**Scene payload** (`third_party/gpui-pre`, newly vendored; see its
`QUILL-CHANGES.md`). On macOS a `PaintSurface` carries a `CVPixelBuffer`.
Elsewhere it now carries an `Arc<SurfaceFrame>`: width, height, a format
(`Rgba16Float` only), the bytes, a unique id, and an atomic headroom.
`surface()` builds on every platform, and `SurfaceSource::Frame` paints
through `Window::paint_surface_frame`. That is all that changed in core.

**DirectX** (`third_party/gpui-pre-windows`, see its `QUILL-CHANGES.md`).

- *The picture.* `draw_surfaces` used to be an empty stub. It now
  uploads each frame into an `R16G16B16A16_FLOAT` texture, keyed by frame
  id, keeping the last two. It generates mips so a large photo shrunk to
  the window stays smooth. Then it draws with the new `hdr_surface`
  shaders: unpremultiply, tone-map to the frame's headroom with the
  Metal shader's curve, and write scRGB.
- *The swap chain.* While the window asks for HDR output and its display
  is in HDR mode, the renderer switches it to `R16G16B16A16_FLOAT` with
  `DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709` (scRGB: linear, 1.0 = 80
  nits). When either stops, it switches back to 8-bit sRGB. Quill asks for
  HDR output with `hdr_output::set_hdr_output` from `sync_viewer_hdr`,
  where macOS switches its layer.
- *The rest of the window.* In scRGB mode every pixel shader that writes
  the swap chain ends with `to_output`: exact sRGB decode, times the
  user's SDR white in scRGB units ("SDR content brightness" in Windows
  settings, read with `DISPLAYCONFIG_SDR_WHITE_LEVEL`). UI white lands
  exactly on the user's SDR white, as in every other app on an HDR
  desktop. In 8-bit mode the shaders write what they always did.
- *Headroom.* Display peak (`IDXGIOutput6::GetDesc1().MaxLuminance`)
  divided by SDR white. The display reading is cached for a second, and the
  viewer polls it as on macOS, so changing the brightness slider or moving
  the window to another display takes effect within a second.

**Decode** (`src/ui/hdr_image/cpu/`). Pure Rust on the background
executor, behind `contained` (a panic costs the picture, not the app).

- *Gain-map JPEG.* The gain map is the second picture in the file's
  MPF index, or the `GainMap` item of an Ultra HDR 1.0 `GContainer`. The
  base decodes with the `image` crate, is colour-managed to sRGB with
  moxcms and is scaled to the asked long edge. The gain map is applied in
  full, sampled bilinearly through a 256-entry boost table per channel:
  `(sdr + offset_sdr) * 2^(min·(1−g) + max·g) − offset_hdr`. The shader's
  tone map then fits the result to the display, as ImageIO's full HDR
  rendition is on macOS.
- *JPEG XL.* jxl-oxide renders to PQ with sRGB primaries, so it applies
  no tone map of its own. Colours outside sRGB stay negative instead of
  clipping. Dividing the PQ nits by BT.2408's 203-nit reference white
  gives 1.0 = SDR white.
- Output is linear extended sRGB half floats, premultiplied, the same as
  the macOS buffer. As on macOS the frame is dropped when no channel
  clears 1.01, and the long edge is capped by the picture, 8192 px and
  24 Mpx.

**Probe.** The bubble tag reads the first 256 KB of a JPEG or JPEG XL: a
gain map announced (ISO URN or `hdrgm:Version`) and locatable, or an HDR
JPEG XL header.

## Verification

- Unit tests (they run on every platform): the gain-map decode of
  `tests/fixtures/hdr/ultrahdr.jpg`, made with libultrahdr, matches
  libultrahdr's own linear decode within 4%. The PQ JPEG XL fixture
  decodes to 1.0 and 4.0. The ISO metadata reads with and without a common
  denominator. The XMP reads in attribute and `rdf:Seq` forms and rejects
  bad values. Truncated files never panic. Half-float packing round-trips
  every finite half. `make-cpu-fixtures.py` rebuilds the fixtures.
- `cargo clippy --target x86_64-pc-windows-msvc` and
  `--target x86_64-unknown-linux-gnu` are clean for the new code, run on
  macOS (stub C compiler, so only Rust is checked). The HLSL entry points
  validate with glslang.
- **Not run on Windows, and not seen on an HDR display.** The packaging
  workflow builds Windows on `main` only. Things to check on real
  hardware: UI colours unchanged when the viewer switches modes, the
  photo's highlights above UI white, no flash on the switch, and the
  swap-chain switch with and without DirectComposition.

## Trade-offs

- In scRGB mode blending happens in linear light, not on sRGB-encoded
  values. Antialiased edges and translucent overlays are a little
  different while an HDR photo is open. Text weight may read slightly
  differently. Only the viewer's window in that state is affected.
- The base JPEG is colour-managed to sRGB before the gain is applied, so
  a Display P3 base loses the colours outside sRGB. Ultra HDR bases are
  mostly sRGB or close to it; keeping wide gamut means applying the gain in
  the base's own space and converting the linear result with a matrix.
- When Windows HDR is on but the swap-chain switch fails (logged), the
  photo is drawn clipped to SDR white with the tag still saying "Shown in
  high dynamic range". Detecting that from Quill needs a renderer query.

- The frame's CPU copy (8 bytes a pixel, 192 MB at the 24 Mpx cap) stays
  alive next to its GPU texture while the photo shows, so the renderer can
  re-upload after a lost device. Both go when the viewer closes: a frame
  without HDR stills frees the textures.
- Values are clamped to the largest half float before packing, so an
  extreme gain map can't put infinity (then NaN through the mips) on
  screen. HDR JPEG XL above 48 Mpx shows in SDR, because jxl-oxide renders
  at full size in floats.
- The display reading (DXGI output plus display configuration) runs on the
  render thread once a second while the viewer shows an HDR photo.

## Follow-ups

- HEIC gain maps on Windows (iPhone photos): read the gain-map item out
  of the HEIF container, wrap it as a one-image HEIC for WIC's HEVC
  decoder, and apply it like the JPEG path.
- AVIF / HEIC in PQ or HLG: needs a 10-bit decode path (WIC gives 8-bit).
- Linux keeps the SDR picture: gpui-pre-wgpu ignores surfaces and has no
  HDR surface format.
