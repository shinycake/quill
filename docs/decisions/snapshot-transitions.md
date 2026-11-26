# Snapshot transitions

An element in Quill's GPUI fork can be drawn through a snapshot: its
subtree paints into a texture of its own, and the frame draws that texture
as one quad, scaled, moved and faded. A panel that opens, closes or swaps
can then animate as a picture without laying out or painting its content
again on every frame. It is a general renderer feature; the Floating layout
(issue #1507, plan section 10) is its first user. Classic calls nothing new.

## The model

- `Snapshot` (`window/snapshot.rs`) is a handle to one texture, `Rc` and not
  `Send`, made by `Window::new_snapshot`. Dropping the last handle frees the
  texture at the next frame.
- `SnapshotState` is `Empty`, `Ready { size, scale }`, `Lost` or
  `Failed`. A scale factor change and a lost GPU device turn every `Ready`
  snapshot of the window into `Lost`; the next capture makes it `Ready`
  again. A snapshot the renderer could not draw (an allocation that
  failed, say) turns `Failed`, alone: it is never captured again, so its
  owner paints live instead of retrying every frame, and the next
  transition's new snapshot tries afresh.
- `SnapshotDraw` is how a snapshot is drawn: a uniform scale about an
  origin, a translation and an opacity. It is not a general transform. A
  rotation or a skew would need a quad with four free corners and a
  sampler for each, and nothing in the plan needs one; a uniform scale
  keeps the quad axis-aligned, so content masks and the batch path stay
  what they are for sprites.
- `snapshot(child, &snapshot, Some(draw))` (`elements/snapshot.rs`) lays
  out, prepaints and paints the child as always, so its hitboxes, listeners
  and focus are real, but its pixels go into the snapshot, which is drawn
  as `draw` says. With `None`, or on a window without snapshots, the child
  paints live. `.backing(color, radius)` paints an opaque rounded quad
  under the element's bounds inside the capture, so subpixel text has an
  opaque background to blend against; the margin stays clear, so a shadow
  keeps its own alpha. `.margin(px)` captures beyond the bounds for a
  shadow.
- `Window::snapshot_last_frame(bounds, scale)` fills a snapshot from what
  the last frame drew in `bounds`, at once, for a picture of something
  about to go away.

## The animation helper

`element.with_snapshot_animation(id, animation, |t, bounds| draw)`
(`elements/snapshot_animation.rs`) draws an element through a snapshot for
one animation. Its element state is the run:

```rust
enum SnapshotRun {
    Running { started: Instant, snapshot: Snapshot },
    Ended,
}
```

The first frame starts the run with a new snapshot; each frame draws it as
the animator says for the eased time and asks for the next frame. Past the
end the state becomes `Ended`, which drops the snapshot, and the element
paints live from then on. If the element is not rendered in a frame, GPUI
drops its state and the snapshot with it. Where the window has no snapshots
or motion is reduced the run ends at once; the owner picks its own
fallback. The clock is the executor's (`background_executor().now()`), as
for `with_animation`, so tests drive it by hand. The animator also gets the
element's bounds, since the origin of a scale (a menu's corner) is only
known once the element is laid out.

## The switch

`SnapshotSwitch::Off` makes `Window::snapshots_supported` false in every
window, so every snapshot element paints live and every owner takes its
fallback, on a machine where snapshots work. gpui-pre reads no
environment: the app sets the switch with `App::set_snapshot_switch` (an
app global; each window reads it at the start of its next frame) and
reads it back with `App::snapshot_switch`. Quill sets it from
`QUILL_SNAPSHOTS` (`off` or `0`, `on` or `1`, parsed by
`SnapshotSwitch::parse`); until it does, snapshots stay on. A test sets
one window's switch with `set_test_snapshot_switch`, which wins over the
app's.

## The demo

`ready-snapshot` (`src/ui/snapshot_demo.rs`) draws one card, with large
and small text, emoji, an image, a shadow and a rounded clip, live and
through snapshots: at identity, at .8 about each corner, moved, at half
opacity, at identity without a backing colour, and through a snapshot
replaced after the first frame (captured again from empty). With
`QUILL_SNAPSHOTS=off` every card is the live card.

## Captures

A capture is the run of paint operations between a `StartCapture` and an
`EndCapture` operation. The primitives stay in `Scene::paint_operations`,
so a cached view that painted them replays them, markers included, as it
replays anything else; only `Scene::push_primitive` diverts them, by the
index of their operation, into the innermost capture that holds it. A view
painted inside a capture replays its own range, which holds no marker, and
the capture around it takes it.

At the end of the frame each capture settles, inner ones first:

- Draw: its primitives become a `SnapshotScene` in the texture's own space
  (through `SlotSpace`, orders kept) and the renderer draws it into the
  texture ahead of the frame.
- Keep: everything in it came from a replay, the snapshot was `Ready` at
  the same size and scale before the capture, and its handle settled in the
  previous full draw. The texture already holds the picture, so nothing is
  drawn. A child that is a cached view therefore costs one quad a frame.
  GPUI paints every view inside a cached view that renders again afresh,
  so the view that owns the snapshot element should not itself be a
  cached view that renders on every frame of an animation, or each frame
  recaptures.
- Live: the capture was refused (the window has no snapshots, the region is
  beyond the limit, a native view is inside). Its primitives go back into
  the frame, or into the capture around it, and the snapshot is `Empty`.

A lens inside a capture is never diverted: it stays in the frame, where its
backdrop is, and is not moved by the draw. A lens over a snapshot quad
redraws its backdrop, since the quad's texture is not in the lens's walk.

`view.rs` adds one condition to a cached view's reuse: it was painted in
the previous full draw. A view skipped for a frame and painted again would
otherwise replay a range whose captures were settled two frames ago. In
Classic a re-rendered view refreshes all its children, so the condition
always holds there.

## Losses

The renderer reports what it lost through
`PlatformWindow::take_snapshot_losses`, which the window takes at the start
of each frame: `SnapshotLosses::Slots` for snapshots that could not be
drawn, `All` when the device went (or was replaced, or a frame skipped
while recovering held snapshot scenes). A window that can't reach its
renderer at that moment (a busy `RefCell` on X11, Wayland and Windows)
gets `None`, and the renderer keeps the record for the next frame, so a
busy borrow never reads as a loss or hides one.

## Limits

Each backend keeps its textures in gpui's `SnapshotPool<T>`
(`scene/snapshot_pool.rs`): one texture per live slot, sides rounded up
to 256 as the lens canvases are, at most the backend's limit
(`PlatformWindow::snapshot_max_side`: 16384 on Metal, the device's
`max_texture_dimension_2d` on wgpu, 16384 at D3D feature level 11 and 8192
at 10), spares kept while another snapshot lives, and the losses above.
The window's own check (`snapshot_fits`) asks the same limit, so a capture
the backend would refuse is never started. The window refuses
a capture that would take its snapshots past 1.25 times its own device pixel
area (counted in exact texels, so the rounded textures can exceed it a
little). `snapshot_last_frame` tries the window's scale, then half of it,
and refuses a region a native view or a lens reaches, since neither can be
drawn again from the scene. It waits for the GPU, because the atlas tiles
it reads may be freed right after.

## Drawing

A snapshot quad is a `PolychromeSprite` with the whole texture as its tile.
The texture holds premultiplied colour, so the sprite carries
`SpriteFlags::PREMULTIPLIED` in its `flags` field (which replaced the
`grayscale: PaddedBool32` the shaders already read as a `u32`, and holds
`SpriteFlags::GRAYSCALE` beside it): divide the sample by its alpha, then
the pipeline blends as for any sprite. This is one
branch in an existing pipeline instead of a new one. Sampling is linear, so
at identity on pixel-aligned bounds the quad draws its content's pixels
exactly (the Metal test compares them, within 1 of 255).

## Backends

Metal (`gpui-pre-apple/src/metal_renderer/snapshots.rs`) draws snapshot
scenes through `draw_scene_into`, each on a command buffer committed ahead
of the frame. They are drawn at the top of `draw`, before the drawable is
taken, so a frame skipped for want of a drawable (or drawn outside its
display link's update) still fills and releases them; the headless entry
points draw them first too. Their instance buffer, and the video textures
a captured surface reads, stay alive until the last of their command
buffers completes (the sharp magnifier's backdrops keep their video
textures with the frame's the same way). A frame that fails after them
loses nothing: the snapshot command buffers were committed on their own,
so their textures hold the picture the window counts on.

wgpu (`gpui-pre-wgpu/src/wgpu_renderer/snapshots.rs`) draws each snapshot
scene with the frame's own recording (`record_frame` with
`FrameKind::Snapshot`) on a submission of its own ahead of the frame's.
The globals and instance data are rewritten for each submission; queue
writes apply in submission order, so each pass reads its own. A snapshot
pass plans no glass, keeps one depth texture of its own (made again when
the size changes), and does not end the video surfaces' frame. The
pipelines are built for the target's format, so the textures use it.
Snapshot scenes are drawn before the surface texture is acquired, so a
frame that is not presented still fills them. The path without storage
buffers (`shaders_webgl.wgsl`) reports no support. Recovery drops the pool
with the device and reports `All`, so what was captured meanwhile is
captured again on the new one. The X11 and Wayland windows forward the
three methods through `try_borrow`.
DirectX 11 (`gpui-pre-windows/src/snapshots.rs`) gives each live slot a
`Texture2D` in the SDR swap chain's format with a render-target and a
shader-resource view, and draws each snapshot scene with the frame's own
recording (`record`) bound to it: its own viewport and depth view, the
globals rewritten with SDR output (a snapshot holds SDR colour, like an
atlas image, and an HDR frame converts it as it does any sprite). The next
frame's `pre_draw` binds the frame's target, viewport and globals again,
so nothing has to be restored. The snapshot sprites follow the scene's own
in the polychrome buffer, as the lens fallbacks follow the quads. A lost
device drops the pool with the pipelines and reports `All`, as do frames
drawn while recovering (`skip_draws`) that hold snapshot scenes.
Direct3D keeps a resource alive until the GPU is done with it, so
`draw_snapshot` does not wait.

## Tests

`src/ui/snapshot_tests.rs` drives the public API through a test window
(gpui-pre's own lib tests don't build in this checkout): live painting
without support, a capture drawn once and then kept, the draw's scale,
move and fade, a lens left in the frame, release at the next frame, loss on
a scale change or a device loss, a snapshot the renderer can't draw failing
once and painting live, the area limit, and a last-frame snapshot.
The switch, and `with_snapshot_animation` ending, going away mid-run and
ending at once without support, are tested there too on the test clock.
`gpui-pre-apple`'s `a_snapshot_at_identity_draws_the_pixels_of_its_content`
draws on a real Metal device. The wgpu and DirectX crates' own tests do not
build in this checkout, so the pixel invariant there rests on the demo run
on lavapipe and WARP and on real GPUs.

## Deviations from the plan

- `SnapshotDraw` is a uniform scale about an origin, a translation and an
  opacity, not a general transform (see the model above).
- A released texture is freed at the next frame, not at once: the handle
  can drop in the middle of a frame that still draws it.
- `QUILL_SNAPSHOTS` is read in gpui-pre, not in `src/glass.rs`.
- A lens inside a capture stays live in the frame and is not moved by the
  draw.
- `snapshot_last_frame` walks the last scene itself, refuses regions with
  a native view or a lens, and waits for the GPU.
- A capture is kept only while everything in it replays; a cached
  ancestor that renders again refreshes its children, so they recapture.
- The area limit counts exact texels; the 256 rounding of the textures can
  go a little past it.
- The animator takes the element's bounds as well as the eased time.
- The backing is a rounded quad under the bounds, not a clear colour for
  the whole texture, which filled the shadow margin.
- The demo has no 1x capture: interface scale starts at 100% on a 2x
  screen, so it was captured at 2x and at 150% (3x).
- The `view.rs` painted-in-the-last-frame condition never changes a
  Classic frame.
