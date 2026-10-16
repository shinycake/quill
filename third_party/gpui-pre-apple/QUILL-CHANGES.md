# Changes made by the Quill project

This directory is gpui-pre-apple 0.3.9 from crates.io (Apache-2.0,
`LICENSE-APACHE`), GPUI's Metal renderer and, since 0.3.9, its CoreText text
system, used through `[patch.crates-io]` in Quill's `Cargo.toml`. Commit
`09fbdce6` added the unmodified registry copy, including `vendor/gpui` (the
GPUI sources `build.rs` reads to generate the shader header; `.gitignore`
keeps `third_party/*/vendor/`), without the registry's `.cargo-ok`,
`.cargo_vcs_info.json`, `Cargo.toml.orig` and `Cargo.lock`, so
`git diff 09fbdce6 -- third_party/gpui-pre-apple` shows every change. Changed
code is marked `QUILL`. (The patch was first made on 0.3.8, vendored in
`088b96c3`, and carried to 0.3.9 in `docs/decisions/gpui-kit-0-7-3.md`.)

The Quill project changed it in 2026 to show HDR photos in the media viewer
(`docs/decisions/hdr-viewer-macos.md`):

- `src/shaders.metal`: `hdr_surface_fragment` draws a `64RGBAHalf` surface
  (linear extended sRGB, premultiplied). It tone-maps to the display's
  headroom and encodes with the extended sRGB curve. On an 8-bit drawable
  the headroom is 1, so the result is clamped standard range.
- `src/metal_renderer.rs`:
  - `draw_surfaces` picks the pipeline from the surface's pixel format.
    Upstream asserted the bi-planar video format, which crashed on any
    other buffer. The YUV path now also skips a surface whose textures
    can't be made (CoreVideo returns an error, a null texture, or no Metal
    texture), instead of 0.3.9's `assert_eq!`/`expect`. 0.3.9 moved the
    renderer from the `core-video` crate to `objc2-core-video`; the HDR
    path uses the same `CVMetalTextureCache::create_texture_from_image`.
  - The headroom comes from the buffer's `QuillHdrHeadroom` attachment
    (`CVBuffer::attachment`, a CFNumber read as `f32`).
  - The pipelines that draw into the drawable follow its pixel format
    (`match_drawable_format`), so a layer switched to `RGBA16Float` for EDR
    draws correctly. The `BGRA8Unorm` set is kept for the way back. Path
    rasterization keeps its 8-bit intermediate.
  - Test-support readback (`read_texture_to_image`) reads an `RGBA16Float`
    drawable as clamped 8-bit sRGB, so screenshot captures of an HDR still
    work.

The Quill project also changed it in 2026 to remove a panic a user can hit
(step 7 of `docs/decisions/no-crash-audit.md`; first made in gpui-pre-macos,
whose text system 0.3.9 moved here):

- `src/text_system.rs`: `layout_line` skips a CoreText glyph run without
  attributes or a font, and a glyph whose string index is negative, instead
  of unwrapping; the font is read with `CFDictionary::find`, since `get`
  panics on a missing key. See `docs/decisions/gpui-fork-panics.md`.

The Quill project changed it again in 2026 for liquid glass on macOS. The
backdrop-blur pass is ported from the GPUI fork in crabtalk/zed (Apache-2.0,
commit 693d6a82, `crates/gpui_apple`), which bezel
(github.com/crabtalk/bezel, MIT) builds on:

- `vendor/gpui/src/scene.rs`: `BackdropBlur` and the `Shadow::inset` doc,
  mirrored from gpui-pre so cbindgen puts them in the shader header.
  `build.rs` exports `BackdropBlur` and `BackdropBlurInputIndex`.
- `src/shaders.metal`: `backdrop_blur_vertex` / `backdrop_blur_fragment`
  (the fork's transfer `gain * saturated(backdrop) + tint`, the rim lens
  with chromatic dispersion, the lit edge, coverage mixed against the sharp
  snapshot). 0.3.9 has no `mask_alpha`, so the content mask clips through
  `clip_distance` alone. `shadow_fragment` reads `inset == 1` as an inset
  shadow (0.3.9 treated any non-zero value so) and `inset == 2` as a drop
  shadow cut to outside its element.
- `src/metal_renderer.rs`:
  - `draw_primitives_to_texture` interleaves `Scene::backdrop_blurs` by
    draw order: before the first batch at or above a blur's order it ends
    the pass, blits the drawable into a scratch texture, runs a cached
    `MPSImageGaussianBlur` (clamped edges; skipped at radius 0) into a
    second texture, resumes the pass with a load action and draws the
    region. A blur above every batch is drawn after the last one (the fork
    drops it).
  - The blur pipeline (blending off) is built per drawable format with the
    other drawable pipelines (`match_drawable_format`); the scratch
    textures take the drawable's size and pixel format, so the
    `RGBA16Float` EDR path works. Both are allocated on the first frame
    with a blur. A frame without blurs writes no blur instances, allocates
    nothing and never breaks the pass.
  - Never-crash changes against the fork: a pipeline that fails to build,
    an MPS class or kernel that is missing (nil), or a framebuffer-only
    drawable skip the blur (or its gaussian) instead of panicking; the
    scratch textures and the pending-blur iterator use no `unwrap`.
  - A frame with blurs asks the layer for readable (non framebuffer-only)
    drawables, since the pass copies the drawable; a frame without hands
    that back, except in debug test-support builds, which keep readback.
  - Links MetalPerformanceShaders.
