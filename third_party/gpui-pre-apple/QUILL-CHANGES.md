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

The Quill project also changed it in 2026 to give back idle window memory
(`docs/decisions/idle-drawables.md`):

- `src/metal_renderer.rs`: `MetalRenderer::release_idle_drawables` sets the
  layer's drawable size to 1×1 and back, which empties the drawable pool
  (up to three window-sized surfaces). The surface on screen stays as the
  layer's contents until the next present.

The Quill project also changed it in 2026 to give back memory after
browsing (`docs/decisions/sweep-r3-browse-memory.md`):

- `src/metal_atlas.rs`: implements `AtlasBackend::insert_image` (see
  gpui-pre's `QUILL-CHANGES.md`). Each `MetalAtlasTexture` records whether
  it holds images, and `allocate` only places a tile in a texture holding
  the same kind, so an image texture is freed once its images leave.
  Test: `test_glyphs_never_keep_an_image_texture_alive`.

The Quill project also changed it in 2026 for Agent access's screenshots
(`docs/decisions/agent-access.md`, phase 5 step 3):

- `src/metal_renderer.rs`: `render_scene_to_image` and the texture readback
  helpers are built in every configuration (they were test-only), the path
  intermediate textures are resized only when the size changed, and
  `MetalRenderer::capture_scene` renders a scene offscreen at the layer's
  drawable size and reads it back. Nothing is presented and no drawable is
  taken from the layer.
