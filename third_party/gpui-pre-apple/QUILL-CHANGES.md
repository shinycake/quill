# Changes made by the Quill project

This directory is gpui-pre-apple 0.3.10 from crates.io (Apache-2.0,
`LICENSE-APACHE`), GPUI's Metal renderer and, since 0.3.9, its CoreText text
system, used through `[patch.crates-io]` in Quill's `Cargo.toml`. Commit
`6be84cd7` added the unmodified registry copy, including `vendor/gpui` (the
GPUI sources `build.rs` reads to generate the shader header; `.gitignore`
keeps `third_party/*/vendor/`), without the registry's `.cargo-ok`,
`.cargo_vcs_info.json`, `Cargo.toml.orig` and `Cargo.lock`, so
`git diff 6be84cd7 -- third_party/gpui-pre-apple` shows every change. Changed
code is marked `QUILL`. (The patch was first made on 0.3.8, vendored in
`088b96c3`, and carried to 0.3.9 in `docs/decisions/gpui-kit-0-7-3.md`, and to 0.3.10 in `docs/decisions/gpui-kit-0-8-1.md`.)

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
- `src/metal_renderer.rs`: `draw` notes when each drawable (by its
  IOSurface) was handed out, and `trim_spare_drawables` runs the same release
  when the pool holds three and one has gone unused for 3 s while others
  were handed out in the last second: the spare a burst (the first frames,
  a scroll) left in the pool while a slow animation keeps the window
  drawing. A resize or a release forgets the drawables it knew.

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

The Quill project also changed it in 2026 so color emoji glyphs can be swept
(`docs/decisions/idle-wallpaper-and-emoji-trim.md`):

- `src/metal_atlas.rs`: `MetalAtlas` forwards `PlatformAtlas::next_use_mark`,
  `emoji_glyph_count` and `remove_emoji_glyphs_unused_since` to its
  `AtlasState` (see gpui-pre's `QUILL-CHANGES.md`).

The Quill project also changed it in 2026 so small pictures don't keep big
textures (`docs/decisions/images-left-behind.md`):

- `src/metal_atlas.rs`: tiles are sorted into `TileClass`es (glyphs and
  SVGs, images up to 128 px a side, larger images), and a texture holds one
  class. Small images get 512² (1 MB) textures; the rest keep 1024². An
  avatar left on screen kept the 4 MB texture larger pictures had come and
  gone in. Test: `test_small_pictures_get_small_textures`; the tile-reuse
  test's keeper tile is 200² so it shares the large tiles' texture.

The Quill project also changed it in 2026 so a failed Metal setup costs one
window, not the app:

- `src/metal_renderer.rs`: `MetalRenderer::new`, `from_layer` and
  `new_headless` return `Result`. Building the library, locating a shader
  function, creating a pipeline and creating the CoreVideo texture cache report
  errors where 0.3.10 used `expect` and `assert_eq!`, and a device-less Mac gets
  an error where 0.3.10 called `process::exit`. `with_shaders` retries a failed
  setup once, since `MTLCompilerService` can fail under load. A failed build
  of the pipelines for an EDR drawable is remembered in `failed_format`, keeps
  the stashed set, and takes the layer back to 8-bit with
  `leave_extended_range`, so the window goes on drawing and never rebuilds per
  frame. `MetalHeadlessRenderer::new` keeps its `expect`, because the registry
  crate gpui-pre-platform calls it without a `Result`. It is test support only.
  Tests: `shaders_that_do_not_compile_are_an_error`,
  `a_library_without_our_functions_is_an_error`,
  `one_failed_library_build_is_retried`,
  `a_failed_edr_switch_leaves_edr_and_still_draws`,
  `a_format_that_failed_is_not_built_again`,
  `a_failed_switch_keeps_the_stashed_pipelines`.
- `Cargo.toml`, `src/thermal.rs`, `src/metal_renderer.rs`: so CI can run
  `cargo test -p gpui-pre-apple` on its own, `runtime_shaders` is a default
  feature (Quill always builds with it, through gpui-kit), and the thermal test
  and `MetalHeadlessRenderer` no longer need `cfg(test)` to mean
  `gpui/test-support`, which the command line can't turn on for a crate outside
  the workspace.

The Quill project also changed it in 2026 so a new user's first launch doesn't
compile the shaders on the main thread:

- `build.rs`: gpui-kit forces `runtime_shaders` on, which turned the metallib
  off. The script now tries `xcrun metal` whatever the feature says and sets
  `cfg(quill_metallib)` on success. A machine without the Metal toolchain
  carries on with the source, unless `QUILL_REQUIRE_METALLIB` is set, as it is
  in `.github/workflows/package-macos.yml` and in `ci.yml`'s `macos-metal` job.
- `src/metal_renderer.rs`: `built_in_library` loads the metallib under that cfg
  and falls back to compiling the source if it doesn't load. Test:
  `the_precompiled_shaders_build_every_pipeline`, which needs the toolchain.
  Redo this at each GPUI re-vendor. The metallib's minimum version stays the
  crate's `10.15.7`.
