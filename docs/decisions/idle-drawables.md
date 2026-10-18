# Idle drawables: free the window's spare surfaces on macOS

An idle, signed-in Quill used 166 MB (physical footprint, what Activity Monitor shows) with no chat open in a
1697×1243 pt window on a 2x display. 99 MB of that was three `CAMetalLayer` drawables, 33 MB each
(`vmmap`: `CAMetalLayer Display Drawable`, 3394×2486 BGRA). The rest: about 47 MB of malloc (TDLib about half,
Rust and GPUI the other half), 8–10 MB of GPU textures (the sprite atlas), and code and system data.

## What changed

- `third_party/gpui-pre-apple` `MetalRenderer::release_idle_drawables`: sets the layer's drawable size to 1×1 and
  back. A size change empties the layer's drawable pool; the surface on screen stays as the layer's contents
  until the next present, so the window looks the same.
- `third_party/gpui-pre-macos` `step`: when the display link stops for idling (250 ms with nothing to draw), it
  schedules the release 3 s later on the main queue. The release is skipped if the window drew again in
  between (`FrameIdle::idle_stops` counts idle stops). An occluded window (covered, minimized, hidden) releases
  at once.

The first frame after a release allocates one drawable. Measured once with a temporary probe in the release
build: `nextDrawable` took 0.7 ms for that frame, against 7.5 ms for the very first frame of the window.

## Measured

Release builds, launched directly, signed in to the same account, no chat open, footprint 60 s after launch
(`footprint -p`), three runs each, alternating:

| | run 1 | run 2 | run 3 | median |
| --- | --- | --- | --- | --- |
| before (v2026.1017.2948) | 166 MB | 166 MB | 167 MB | 166 MB |
| after | 110 MB | 106 MB | 109 MB | 109 MB |

Drawables alive: 3 before, 1 after, every run. The saving scales with window area: about 38 MB for a
1039×1148 pt window.

## Not done

- Linux and Windows keep their swapchains as they are. Freeing their images needs a swapchain rebuild on
  every wake, and DXGI flip-model chains need at least two buffers. Their idle cost is two to three
  window-sized images, the same as macOS before this change.
- The surface on screen (one drawable) stays: it is what the window shows.
