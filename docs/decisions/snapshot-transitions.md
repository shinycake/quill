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
- `SnapshotState` is `Empty`, `Ready { size, scale }` or `Lost`. A scale
  factor change and a lost GPU device turn every `Ready` snapshot of the
  window into `Lost`; the next capture makes it `Ready` again.
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
  paints live. `.backing(color)` clears the texture to an opaque colour
  first (subpixel text stays correct); `.margin(px)` captures beyond the
  bounds for a shadow.
- `Window::snapshot_last_frame(bounds, scale)` fills a snapshot from what
  the last frame drew in `bounds`, at once, for a picture of something
  about to go away.

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

## Limits

Each live snapshot owns one texture in the drawable's format, sides rounded
up to 256 as the lens canvases are, at most 16384 a side. The window refuses
a capture that would take its snapshots past 1.25 times its own device pixel
area (counted in exact texels, so the rounded textures can exceed it a
little). `snapshot_last_frame` tries the window's scale, then half of it,
and refuses a region a native view or a lens reaches, since neither can be
drawn again from the scene. It waits for the GPU, because the atlas tiles
it reads may be freed right after.

## Drawing

A snapshot quad is a `PolychromeSprite` with the whole texture as its tile.
The texture holds premultiplied colour, so the sprite's flag word (the
`grayscale` field, read as flags by the shaders) gains bit 2: divide the
sample by its alpha, then the pipeline blends as for any sprite. This is one
branch in an existing pipeline instead of a new one. Sampling is linear, so
at identity on pixel-aligned bounds the quad draws its content's pixels
exactly (the Metal test compares them, within 1 of 255).

## Backends

Metal (`gpui-pre-apple/src/metal_renderer/snapshots.rs`) draws snapshot
scenes through `draw_scene_into`, each on a command buffer committed ahead
of the frame. wgpu and DirectX 11 skip the batch until their ports land;
their windows report no snapshot support, so elements paint live there.

## Tests

`src/ui/snapshot_tests.rs` drives the public API through a test window
(gpui-pre's own lib tests don't build in this checkout): live painting
without support, a capture drawn once and then kept, the draw's scale,
move and fade, a lens left in the frame, release at the next frame, loss on
a scale change or a device loss, the area limit, and a last-frame snapshot.
`gpui-pre-apple`'s `a_snapshot_at_identity_draws_the_pixels_of_its_content`
draws on a real Metal device.
