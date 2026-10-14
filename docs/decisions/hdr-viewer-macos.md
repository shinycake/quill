# HDR photos in the media viewer (macOS)

A picture sent as a file can carry HDR data: an iPhone HEIC with a gain
map, an AVIF or HEIC in PQ or HLG. Until now Quill showed the SDR picture.
On a Mac whose screen has EDR headroom (any XDR display, most recent
MacBooks, Pro Display XDR, HDR external screens) the viewer now shows the
HDR rendition: highlights go brighter than UI white, the way Photos shows
them. Everything else in the window looks the same.

Design: `quill-tools/designs/hdr-viewer.md`. Windows: `hdr-viewer-windows.md`.
Linux keeps the SDR picture for now (below).

## What the user sees

- The chat bubble's file pill (`HEIC`, `AVIF`, …) gains an `HDR` tag when
  the file carries HDR data.
- In the viewer an `HDR` tag sits at the start of the toolbar, with the
  tooltip "Shown in high dynamic range". On a screen without headroom,
  or while the picture is rotated or flipped, the tag is dimmed and the
  tooltip says "Shown in standard range on this display". The picture is
  then the SDR one from slice A.
- Zoom and pan work as before. Zooming in re-decodes at a larger size,
  up to the picture's own size or 24 Mpx, one decode at a time.
- A picture whose HDR data turns out to hold nothing above SDR white, or
  that fails the HDR decode, shows the SDR picture with no tag.
- Leaving the viewer, or stepping to another item, turns extended range
  off on the next frame.

## How it works

**Renderer.** `third_party/gpui-pre-apple` is the vendored GPUI Metal
renderer (changes listed in its `QUILL-CHANGES.md`). GPUI's `surface`
element already draws a `CVPixelBuffer` outside the 8-bit atlas. An HDR
still is a `64RGBAHalf` buffer (linear extended sRGB, premultiplied), and
the renderer has a second surface pipeline for it, `hdr_surface_fragment`.
It tone-maps to the headroom, then encodes with the sign-preserving sRGB
curve. Every pipeline that draws into the drawable follows the drawable's
pixel format, so the same shaders work on `BGRA8Unorm` and on
`RGBA16Float`. Because the layer's colour space stays (extended) sRGB,
UI colours, text and blending come out the same numbers as before; only
the range above 1.0 opens up.

**Extended range.** While the viewer shows an HDR frame upright,
`sync_viewer_hdr` (called every render) sets the window's `CAMetalLayer`
to `RGBA16Float`, extended sRGB and `wantsExtendedDynamicRangeContent`.
Otherwise it puts back `BGRA8Unorm` and GPUI's default colour space. The
layer is reached through the window's `NSView`, the same way
`capture_block.rs` reaches the window, so the macOS platform crate needed
no patch.

**Decode** (`src/ui/hdr_image/macos.rs`). ImageIO's thumbnail call with
`kCGImageSourceDecodeRequest = DecodeToHDR` and `ShouldAllowFloat` gives
the HDR rendition, EXIF orientation applied, at the asked long edge. It
is drawn into an IOSurface-backed, Metal-compatible `64RGBAHalf` buffer
through a half-float CoreGraphics context in extended linear sRGB, which
converts from the file's colour space (Display P3 PQ for a gain map,
BT.2100 PQ or HLG). If no channel clears 1.01, the picture has nothing
above SDR white, and the buffer is dropped.

- The decode runs on the background executor. The result carries a
  token, so a late result for another item is dropped. A larger decode
  for the same picture waits until the running one lands.
- The long edge is the media box times the screen scale, rounded up to
  512 px, and capped by the picture's size and 24 Mpx (192 MB at 8 bytes
  a pixel). One frame lives at a time, outside `image_budget`.

**Headroom.** `NSScreen.maximumPotentialExtendedDynamicRangeColorComponentValue`
at 1.0 or below means the screen can't show HDR, and the viewer never
decodes one. Otherwise the current
`maximumExtendedDynamicRangeColorComponentValue` (it moves with
brightness and power) is re-read once a second while an HDR frame shows.
The frame repaints only when it changed. A screen reports 1.0 until
EDR content is on it and then ramps up, so while `current` is 1.0 the
frame aims at the potential headroom, and the first second polls every
100 ms.

- The value reaches the shader as a `QuillHdrHeadroom` attachment on the
  pixel buffer, so the renderer needs no AppKit.
- Content whose peak fits under the screen's headroom is passed a
  headroom that puts the knee at its own peak, so it draws untouched.
  Brighter content rolls off: identity up to 0.8 of the headroom, then a
  smooth curve with slope 1 at the knee. The curve is
  `tone_map.rs::hdr_tone_map`, a tested twin of the shader's.

**Bubble tag.** `hdr_image::has_hdr` checks the file's metadata once per
path in the background: a gain map (Apple's or ISO 21496-1) or a PQ/HLG
colour space. It remembers up to 1024 paths, and the bubble repaints when
an answer is "yes". A file it couldn't open is asked again after 30 s.

## Changes from the design

- The design patched `gpui-pre-macos` with a window method for extended
  range. The layer is reachable from Quill directly, so Quill sets the
  three properties itself and the platform crate stays untouched.
- The design kept separate SDR and HDR pipeline sets and picked one each
  frame. The renderer instead swaps one active set with a spare when the
  drawable's format changes. The effect is the same, with less code.
- There's no `QuillHdrSdrOnly` attachment: the drawable's format tells the
  shader whether it can go above 1.0.
- A rotated or flipped HDR picture shows the SDR image (the existing
  rotate cache) instead of rotating the half-float buffer. It is rare, and
  `turn_surface` only knows the bi-planar video format today.
- The viewer tag is a kit `Tag`. The kit `Badge` is a count or dot on
  another element and can't hold text.
- The next item isn't prefetched. The SDR picture shows while the HDR
  decode runs.
- The `kCGImageAuxiliaryDataTypeISOGainMap` constant is a macOS 15 symbol.
  Linking it would stop Quill from launching on 14 (its minimum), so the
  probe spells out its value. macOS 14 doesn't read ISO gain maps, so on
  14 those files show in SDR without the tag; Apple gain maps and PQ/HLG
  work on 14.

## Other platforms

- **Windows**: gain-map JPEG and HDR JPEG XL, through a cross-platform
  surface payload, a DirectX HDR surface pipeline and an scRGB swap chain
  while Windows HDR is on (`hdr-viewer-windows.md`).
- **Linux**: shows the SDR rendition. Wayland and X11 compositors don't
  offer one extended-range surface path Quill can rely on yet. Every
  `hdr_image` entry point answers "SDR" off macOS, so nothing else changes.

## Safety

There are no new `unwrap`/`expect`/`panic`/indexing in production code.
Every `unsafe` block has a SAFETY note: the CoreFoundation and ImageIO
calls, the locked pixel buffer memory, the `msg_send!` layer setters and
the `Send` wrapper for the pixel buffer. A failed decode falls back to the
SDR picture. The renderer's surface path now skips buffers it can't draw,
where upstream asserted and would crash.

## How verified

- Unit tests (`src/ui/hdr_image/tests.rs`, `tone_map.rs`, `viewer.rs`):
  - Fixtures under `tests/fixtures/hdr` (made by `make-fixtures.swift`):
    a PQ HEIC decodes with a peak of about 4.0, an ISO gain-map HEIC
    above 2.0 (macOS 15+), and an SDR HEIC gives no HDR rendition.
  - The probe tells them apart.
  - The long edge cap holds.
  - Half-float conversion, peak scan and tone-curve shape are covered.
- UI tests (`viewer_tests.rs`, `demo-capture`):
  - With 4× headroom, the demo sunset shows with the `HDR` tag and the
    window in extended range. Rotating it drops to standard range, and
    closing the viewer releases extended range.
  - With an SDR screen it shows standard range and the window stays 8-bit.
- Captures (`ready-hdr-files`, `ready-hdr-file-viewer`) on a MacBook
  Pro's Liquid Retina XDR screen: the bubble pill reads `HEIC HDR`, and
  the viewer draws the sunset through the HDR surface path with the `HDR`
  tag. Screenshot readback clamps to SDR, so the extra brightness itself
  can only be judged on the screen.
- The full gate passes (core and UI tests, clippy, file size and hotspot
  checks).
