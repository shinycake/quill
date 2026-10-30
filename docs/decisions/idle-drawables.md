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

## Spares while something animates (2026-10-30)

A signed-in Quill 2026.1030.3995 left in front with nothing touched still kept all three drawables: 67 MB of
its 163 MB, in a 1512×949 pt window at 2x. One animated custom emoji looped at 60 fps, so the display link
never idled and the release above never ran. Its frames went out on every other refresh of the 120 Hz
display and only ever used two drawables. The third was left over from the first frames.

- `third_party/gpui-pre-apple` `MetalRenderer::draw` notes when each drawable, known by its IOSurface, was
  handed out. Before it takes the next one, `trim_spare_drawables` runs the same release when the pool holds
  three and one has gone unused for 3 s while another was handed out in the last second. A pool of two is
  left alone: one drawable is on screen and the next frame draws into the other, so emptying it frees nothing
  the next frames don't allocate again. A resize or a release forgets the drawables it knew.
- Content that needs all three keeps handing each one out, so the trim never fires for it. A scroll grows
  the pool back to three, and it is trimmed again 3 s after the scroll ends. There is no timer: the check
  runs only when a frame is drawn.

Rejected: `maximumDrawableCount` 2 while only small animations run. It saved the same 22 MB for a slow loop,
but an animated sticker that presented 100 to 120 frames a second lost about a tenth of its frames, waiting 11
to 20 ms for a drawable every second.

### Measured

Offline demo on main (8f07b04), window 1512×900 pt at 2x, in front, on a 120 Hz display. Release builds with
local-only tracing of the drawables handed out and of when each frame reached the screen (`presentedTime`).
Each run: 30 s into a looping animation, a scroll burst, then 20 s more. Two runs of each; footprint in MB.

| | without the trim | with the trim |
|---|---|---|
| looping custom emoji, 30 s in | 3 drawables, 115 / 120 | 2 drawables, 94 / 94 |
| same, right after the scroll | 3 drawables, 115 / 120 | 3 drawables, 120 / 115 |
| same, 20 s after the scroll | 3 drawables, 115 / 117 | 2 drawables, 99 / 92 |
| animated sticker (about 100 frames a second on screen), 20 s after the scroll | 3 drawables, 131 / 139 | 3 drawables, 132 / 132 |
| trims per run | | emoji 3 / 2, sticker 0 / 0 |

The emoji presented 28.6 frames a second in every run, and no `nextDrawable` wait went past 3.5 ms. In the
second before and after each of the five trims, every interval between frames on screen was 16.7 to 50 ms,
the same spread as the rest of the run (the animation runs at about 30 fps on a 120 Hz display). The
first frame after each trim came 33, 33, 50, 33 and 33 ms after the one before. The sticker never trimmed and
presented 101 to 104 frames a second.

## Not done

- Linux and Windows keep their swapchains as they are. Freeing their images needs a swapchain rebuild on
  every wake, and DXGI flip-model chains need at least two buffers. Their idle cost is two to three
  window-sized images, the same as macOS before this change.
- The surface on screen (one drawable) stays: it is what the window shows.
