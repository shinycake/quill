# Glass under a display link: the window that vanished

Reported 2026-11-26 on macOS, v2026.1126.7035, with Glass effects on: after
switching to another app and back, the Quill window was gone. The app kept
running with no window.

## Cause

Glass reads the drawable back, so frames with glass need drawables that are
not framebuffer-only (`lens::DrawableReads`). Two things met:

- `MetalRenderer::release_idle_drawables` (idle for 3 s, or the window
  covered, hidden or minimized) set the layer back to framebuffer-only, so
  the drawables made after it could not be read.
- The first frame after that, with glass, got one of those drawables from
  the `CAMetalDisplayLink` and swapped it for `layer.nextDrawable()`. While a
  display link drives the layer, `nextDrawable` throws
  `CAMetalLayerInvalidOperation`. The Objective-C exception unwound through
  GPUI's frame, dropped the `Window` (and with it the native window), and
  AppKit caught it in the run loop. No crash, no window.

The debug demo builds never hit it: with `test-support` they keep drawables
readable for captures, so the swap never ran. Unified log line:
`CAMetalLayerInvalidOperation`, from `-[CAMetalLayer nextDrawable]`.

## Fix

- `metal_renderer/drawables.rs`: a frame in a display link's update never
  asks the layer for a drawable. Glass on a framebuffer-only link drawable
  draws flat for that frame; a link drawable in the wrong pixel format is
  skipped. Either way the renderer `owes_present`, and the window asks the
  link's next update to present the scene again (`require_presentation`,
  and the link keeps running). Without a link the layer is still asked, as
  before.
- `release_idle_drawables` keeps readable drawables readable. An idle window
  draws nothing, so readability costs nothing then, and the first frame after
  idling almost always has its glass again. `DrawableReads` still gives the
  direct path back after 120 frames without glass.
- `MacWindow::draw` runs the renderer inside `objc2::exception::catch`. Any
  other exception thrown while drawing now costs one frame, logged, instead
  of the window.

## Verified

Offline demo `ready-chats`, Floating and Classic, dark and light, glass on,
with a temporary local patch giving the debug build release behaviour
(framebuffer-only drawables). Steps: deactivate, hide the app, unhide it,
deactivate again, reading the window list and the window's pixels between
steps. Before: the window was gone after the unhide in every run with glass
on, and never with glass off. After: the window stays, and its pixels after
the unhide match the frame before it. A second temporary patch restored the
old idle reset to force the flat-glass path: it ran once per unhide, the
next update presented the real glass, and the window stayed.

## Getting the window back on an affected build

Quit Quill (right-click the Dock icon, Quit) and open it again. Turning
Glass effects off avoids it until the fix ships.
