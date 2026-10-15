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
