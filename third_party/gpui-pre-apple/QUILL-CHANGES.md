# Changes made by the Quill project

This directory is gpui-pre-apple 0.3.8 from crates.io (Apache-2.0,
`LICENSE-APACHE`), GPUI's Metal renderer, used through `[patch.crates-io]` in
Quill's `Cargo.toml`. Commit `088b96c3` added the unmodified registry copy
(without the registry's `.cargo-ok`, `Cargo.toml.orig` and `Cargo.lock`), so
`git diff 088b96c3 -- third_party/gpui-pre-apple` shows every change. Changed
code is marked `QUILL`.

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
    can't be made, instead of unwrapping.
  - The headroom comes from the buffer's `QuillHdrHeadroom` attachment.
  - The pipelines that draw into the drawable follow its pixel format
    (`match_drawable_format`), so a layer switched to `RGBA16Float` for EDR
    draws correctly. The `BGRA8Unorm` set is kept for the way back. Path
    rasterization keeps its 8-bit intermediate.
