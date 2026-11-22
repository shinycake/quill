# Backdrop glass, stage 1: the primitive and its Metal renderer

Liquid glass that bends, frosts and lights what the window drew under it,
drawn by the GPU. The design is quill-tools/gpui-liquid-glass/GPU-PLAN.md;
the looks are the CPU optics in quill-tools/gpui-liquid-glass (a port of
bilalgursen/liquid-glass, MIT). No product UI uses the glass yet.

## What stage 1 adds

- **A scene primitive.** `Window::paint_backdrop_lens(bounds, corner_radius,
  &LensStyle)` inserts a `BackdropLens` (vendored gpui-pre,
  `scene/backdrop_lens.rs`). Its draw order is the next multiple of 2^24
  above every order it covers, so the lens draws after its backdrop, every
  lens with no glass under it shares one batch (one copy of the frame), and
  content painted over it draws on top. Glass over glass takes one more band.
  A lens inside a `paint_layer` (one order for the whole layer) or above the
  last of the 255 bands draws as its fallback: a rounded quad of the glass's
  colour.
- **Metal.** The lens batch ends the render pass, blits each lens footprint
  into a pooled mip-mapped atlas, blurs four levels down with dual-Kawase
  passes, reopens the pass and draws the lenses with a port of the CPU
  optics. Drawables stay readable (`framebufferOnly = false`) only while
  frames have glass and for 120 frames after; the atlas goes with the idle
  drawables. When the drawable can't be read, the lens draws as its
  fallback.
- **wgpu and DirectX** draw the fallback quad, so Linux and Windows build and
  show something sensible.
- **Demo.** `ready-glass` (`src/ui/glass_demo.rs`) lays a clear and a tinted
  button, a tab bar with its selection lens, a frosted card and a magnifier
  over the live showcase conversation. `QUILL_DEMO_GLASS=off` draws the same
  frame without the glass.

## Decisions

- `Primitive::Lens` holds a `Box<BackdropLens>`. The lens is about 240
  bytes; unboxed it would grow every primitive the frame records in
  `paint_operations` (thousands per frame) for a primitive that is rare.
- The footprint copied for a lens is its drawn footprint (the body plus the
  shadow's room), not the plan's body + 48 px. The bends sample inward and
  the frost reaches a few pixels past the rim; taps are clamped inside the
  copy, so nothing outside it is needed.
- Lens pipelines are built lazily on the first frame with glass, so launch
  pays nothing.
- The ordering tests live in `src/ui/backdrop_lens_tests.rs`: gpui-pre's own
  unit tests do not build in this repository (missing `util_macros` and
  font assets).

## Verification

- Captures (light, dark, glass off) of `ready-glass`, and a side-by-side of
  three lenses against `gpui_liquid_glass::render` over the glass-off frame:
  the magnifier matches to 0.3/255 mean difference; the card and tab track
  match apart from the labels, which the GPU path draws over the glass.
- Linux and Windows were cross-checked from macOS (clippy, stub C tools).
- Frame cost, 2026-11-22, M-series Mac on AC power, 1200x800 window at 2x,
  debug demo-capture build, `ready-glass` redrawing every frame, three runs
  each with and without glass in alternating order: the six lenses (two
  batches) add about 1.1 ms of GPU time per frame (median 4.2 ms against
  3.1 ms, from the command buffer's GPU start and end times) and about
  1.1 ms of CPU encoding (2.9 ms against 1.75 ms, unoptimised build). The
  probe was a temporary patch and is not committed.

## Not done here (later stages)

- Real glass on wgpu (stage 2) and DirectX (stage 3).
- Moving gpui-liquid-glass onto the primitive (stage 4) and the stage 5
  ideas (merging lens groups, skipping the copy when the backdrop did not
  change, label contrast, a sharp magnifier).
- The presentation cost of readable drawables was not measured: the
  demo-capture build keeps drawables readable in both runs.
