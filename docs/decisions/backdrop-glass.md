# Backdrop glass: the primitive, Metal (stage 1) and wgpu (stage 2)

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
- **wgpu and DirectX** drew the fallback quad, so Linux and Windows built and
  showed something sensible. Stage 2 gives wgpu the real glass (below).
- **Demo.** `ready-glass` (`src/ui/glass_demo.rs`) lays a clear and a tinted
  button, a tab bar with its selection lens, a frosted card and a magnifier
  over the live showcase conversation. `QUILL_DEMO_GLASS=off` draws the same
  frame without the glass.

## What stage 2 adds (wgpu: Linux)

- **The same passes as Metal** (`third_party/gpui-pre-wgpu/src/wgpu_renderer/lens.rs`):
  the lens batch ends the frame's pass, copies each footprint into a pooled
  five-level atlas, fills the levels with dual-Kawase downsamples, resumes
  the pass and draws the lenses with `fs_lens` (`shaders_lens.wgsl`), a
  line-by-line WGSL port of the Metal `lens_fragment`.
- **Readable frames.** The surface asks for `COPY_SRC` whenever its
  capabilities offer it, which Vulkan's do as a rule. OpenGL's never do: there
  a frame with glass is drawn into a readable texture of the surface's size
  and a full-screen pass copies it onto the surface; frames without glass go
  straight to the surface and the texture is freed. Window captures
  (`capture_scene`) and the headless renderer draw into readable textures
  already.
- **Failing safe.** The pipelines are built on the first frame with glass
  inside error scopes. A shader a driver rejects makes the glass draw flat
  (its fallback quads) for the rest of the renderer's life, logged once; it
  never becomes a GPU error on every frame. A target that can't be read,
  footprints too large for the atlas and the WebGL instance transport also
  draw flat.

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
- wgpu keeps `COPY_SRC` on the Vulkan swapchain for as long as the window
  lives, not only while glass is on screen as Metal does with
  `framebufferOnly`. Turning it on and off means reconfiguring the swapchain
  mid-frame; the flag's own cost on Linux drivers was not measured (nothing
  in the product uses glass yet). Frames without glass do no other extra
  work: no pipelines, no atlas, no copies.
- wgpu frees the atlas after the first frame without glass, not after an
  idle stretch: the Linux backends have no idle hook to free it from, and a
  window that went idle would keep it.
- The WGSL lens takes group 1's bindings 4 to 7 so it can be compiled with
  `shaders.wgsl`, whose surfaces use 0 to 3. `lens.rs` asserts at compile
  time the `BackdropLens` offsets WGSL's 8-byte `Bounds` alignment could
  move.
- `pack` (the atlas shelving) is a copy of the Metal one; DirectX (stage 3)
  is the moment to lift it into gpui-pre.
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

## Verification, stage 2

- Real Linux, 2026-11-22: the `ready-glass` demo on a GitHub
  `ubuntu-latest` runner under Xvfb, screenshots of the real X window,
  Mesa 26.0.8. Shots: Vulkan on lavapipe (light, dark, glass off), and
  OpenGL on llvmpipe with the Vulkan driver hidden (light, glass off). The
  run came from a throwaway branch, since deleted. Both backends draw the
  glass: the card frosts the chat behind it, the magnifier bends the
  bubble's text, and the selection pill sits on the tab track. Compose,
  Save, the tab names and the card's text stay crisp above the glass. In
  the four glass regions, Vulkan and OpenGL differ by no more than 1% at
  any pixel. Each region differs from the glass-off shot at 8 to 22% of its
  pixels.
- The two backends exercise the two paths. lavapipe's swapchain offers
  `TRANSFER_SRC` (from vulkaninfo), so Vulkan copies straight from the
  frame. wgpu's GL surfaces offer only `COLOR_TARGET` (wgpu-hal 30.0.2), so
  OpenGL draws offscreen and blits. Both software adapters report `Cpu`,
  and Quill's adapter sort prefers Vulkan on a tie, so the Vulkan shots did
  not fall back to GL.
- wgpu on Metal (a scratch harness that turns on wgpu's Metal backend and
  renders the six demo lenses headless over stage 1's glass-off frame)
  matches stage 1's Metal capture to 0.00/255 mean (max 1) on the
  magnifier in light and dark. The card, the tab track and the buttons
  differ by 1.4 to 7.4/255 mean, only where the harness bakes the labels
  into the backdrop. Quads and a 1-px black-and-white stripe sprite drawn
  after the lenses come out exact (at most 1/255), so whatever is drawn
  after a lens batch stays sharp on top of it.
- A shader that fails to build falls back to flat quads, and the frame is
  still drawn. The first harness run hit a WGSL parse error and fell back
  this way.
- Not run: Wayland, a real GPU on Linux (Intel, AMD or NVIDIA drivers), the
  Omarchy VM (aarch64), resize and device loss on a live window, and frame
  cost on Linux. `lens.rs`'s packing tests do not build in this repository,
  for the same reason as gpui-pre's.

## Not done here (later stages)

- Real glass on DirectX (stage 3).
- Moving gpui-liquid-glass onto the primitive (stage 4) and the stage 5
  ideas (merging lens groups, skipping the copy when the backdrop did not
  change, label contrast, a sharp magnifier).
- The presentation cost of readable drawables was not measured: the
  demo-capture build keeps drawables readable in both runs.
