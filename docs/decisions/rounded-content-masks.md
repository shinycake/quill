# Rounded content masks

A content mask in Quill's GPUI fork can clip to a rounded rectangle, so a
card or a pill cuts its content at its corners: a hovered row, a profile
cover or a selection fill may run to the edge. It is a general renderer
feature; the Floating layout (issue #1507, plan section 9) is its first user.
Classic calls nothing new, so its primitives take the shader paths they took
before.

## The type

`ContentMask<P>` (vendored gpui-pre, `src/window.rs`) is a rectangle and at
most one rounded rectangle:

```rust
pub struct ContentMask<P> {
    pub bounds: Bounds<P>,         // intersected exactly, clipped in the vertex stage
    pub rounded: RoundedMask<P>,   // one rounded shape cut per fragment; all radii zero: none
}
pub struct RoundedMask<P> {
    pub bounds: Bounds<P>,
    pub radii: Corners<P>,
}
```

The rounded shape is kept apart from the rectangle on purpose. A scroll
viewport inside a card narrows `bounds` and leaves `rounded` alone, and
"inside the rectangle and inside the rounded shape" is exactly the visible
region. One rounded rectangle alone could not express a card's viewport.

Every primitive instance carries its mask, so each grew by 32 bytes in Rust
and in the three shader mirrors (Metal through the generated header, WGSL by
hand, HLSL by hand). Each shader file states every struct's size and mask
offset in a `QUILL-LAYOUT` comment that `src/ui/rounded_mask_tests.rs`
compares with the Rust structs; the wgpu and DirectX crates also assert the
lens's offsets at compile time. A side table of masks addressed by index
would cost 4 bytes instead and one more buffer binding in every pipeline of
three backends, including the wgpu path without storage buffers; it is the
lever if the upload ever shows in a frame profile.

API (`src/scene/rounded_mask.rs`, `src/window.rs`):

- `ContentMask::rect(bounds)`, `ContentMask::rounded(bounds, radii)` (radii
  clamped to half the shorter side; zero radii give a plain rectangle),
  `ContentMask::contains(point)`.
- `Window::with_rounded_content_mask(bounds, radii, f)`.
- `Styled::clip_rounded()` sets `Style::content_clip` to
  `ContentClip::Rounded` (the default, `ContentClip::Overflow`, is GPUI's
  own meaning). A `div` with it paints its own background, border and
  shadow as before, then its children inside its rounded shape: its bounds
  inside a visible border, each corner radius less the larger of its two
  borders. The div's own hitbox sits under that mask too, so its cut-away
  corners do not take the pointer.

`overflow_hidden()` keeps its rectangular meaning. Making it follow the
corner radii would change pixels wherever a rounded element hides overflow
today, in Classic too.

## Nesting

`inner.nest_in(&outer)` intersects the rectangles exactly. For the rounded
shapes, a corner is *active* when its radius × radius square overlaps the new
rectangle:

1. One side has no shape: the other's. Exact.
2. The outer shape has no active corner: the inner's. Exact, because inside
   the new rectangle the outer shape is a rectangle that `bounds` already
   holds.
3. The inner shape has no active corner: the outer's. Exact.
4. Both have active corners and the inner shape lies inside the outer one at
   each active outer corner (the inner corner's disc inside the outer
   corner's disc, `|c − C| + r ≤ R`, or the inner rectangle does not reach
   that corner square): the inner's. Exact.
5. Otherwise the inner's, and the outer one's rounding is lost where the two
   cross.

A result whose shape has no active corner is stored as no shape. Rule 5 is
the only inexact case. The exact alternative is two shapes per mask, another
32 bytes on every primitive and a second distance per fragment, for a case
nothing in Quill or the kit produces: a card holds rectangular scroll areas
(rule 3) and a pill inside a card sits clear of the card's corners (rule 2).
Rule 5 bumps `Scene::inexact_rounded_masks` and logs once in a debug build,
so a design that hits it is noticed.

## Corner culling

Most primitives under a rounded mask are nowhere near a corner. When a
primitive enters the scene (`Scene::try_insert_primitive`, the one entry
every paint function goes through), its bounds (transformed for a
transformed sprite) clipped to the mask's rectangle are tested against the
shape's corner squares. If they touch none, the primitive is stored with no
shape: the output is identical, it takes the old shader path, and an opaque
quad keeps the opaque pre-pass. The test is four rectangle overlaps for a
primitive painted under a rounded mask and none under a plain one. A
shadow is tested with its bounds grown by three blur radii, how far the
shader draws its blur, so a blurred edge that reaches a corner keeps the
shape.

## Shaders

A primitive whose mask has a shape multiplies its coverage by
`saturate(0.5 − quad_sdf(position, rounded.bounds, rounded.radii))`, the
quad shader's own distance function, at the fragment's device position (so a
transformed sprite needs nothing special). A quad with a shape leaves the
"simple" fast return and the opaque pre-pass.

The native view is the exception: it is an OS view, so the rectangle clips
it and the rounded part is ignored. Quill hosts no native view inside a card.

## Snapping, hit testing, caching, glass

- The shape's bounds snap to device pixels like a quad's; radii do not. The
  shape belongs to the card, so it does not move while a list scrolls under
  it.
- A hitbox keeps the mask it was inserted under; the hit test asks
  `ContentMask::contains`, so the pointer over a cut-away corner does not hit
  the clipped content.
- Cached views replay primitives with the masks they were painted with, and
  a view's cache key holds its content mask, so a cached view is drawn again
  when the shape over it changes.
- Glass: `SlotSpace::mask` maps both parts into a sub-scene, and the reuse
  hash covers both (whole instances are hashed; paths hash the shape
  explicitly).

## Verification (2026-11-25, this Mac, Metal)

- Unit tests (`src/ui/rounded_mask_tests.rs`, run with
  `--features demo-capture`): zero radii make a rectangle; radii clamp;
  the five nesting rules and a viewport inside a card; corner culling,
  including the part of a primitive outside the rectangle; `contains`;
  scene insertion culls; scaling scales both parts; every `QUILL-LAYOUT`
  line of the seven shader files against the Rust structs; a `div` with
  `clip_rounded` gives a child at its corner the shape and a child in its
  middle none, and its own fill none; `overflow_hidden` with radii keeps
  a rectangle; a click in the cut-away corner of a child does not reach
  it, and does with `overflow_hidden`.
- Pixel test on headless Metal (gpui-pre-apple
  `a_rounded_mask_cuts_an_opaque_quad_at_its_corners`): an opaque white
  quad under a mask rounded to 24 matches the CPU coverage within one grey
  level at every pixel; a band away from the corners is culled to the
  plain path and drawn whole.
- The Classic capture set (`ready-chats`, `ready-chat-wallpaper`,
  `ready-showcase`, `ready-unread`, `ready-albums`, `ready-info-group`,
  `ready-group-call-bar`, `ready-rtl-text`, light and dark at 1200x800,
  `ready-chats` at 560x800, `ready-glass` with `QUILL_GLASS=on`) against
  main captured twice: the demos whose two main runs agree are identical
  after this change; the others (`ready-showcase`, `ready-rtl-text`,
  `ready-info-group`, `ready-glass`, whose animations or pointer state
  differ run to run on main too) differ by the same kind of change as
  main's own two runs.
- `ready-rounded-mask` captured in light and dark (see the PR).
- WGSL: every module validates with naga (storage, texture-instance,
  subpixel, glass) and gpui-pre-wgpu checks for x86_64-unknown-linux-gnu.
  HLSL: every entry point of `shaders.hlsl` and `shaders_lens.hlsl`
  compiles with glslang's HLSL front end and gpui-pre-windows checks for
  x86_64-pc-windows-msvc. Neither ran on a GPU here: lavapipe and WARP in
  CI, and real Windows and Linux hardware, are still to see them.

## Not done here

- The native view ignores the rounded part (an OS view).
- A video surface tile is not in the demo: the NV12 and HDR paths carry
  the shape (Metal, wgpu, DirectX) but no capture shows it.
- The frame-profile measurement of the 32 bytes (plan section 13: release
  build with the frame profiler, A-B-B-A against main) was not taken in
  this step; the side table stays the lever if it shows.

