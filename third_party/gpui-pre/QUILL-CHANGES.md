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

The Quill project changed it again in 2026 so video pictures can reach the
DirectX renderer as YUV instead of BGRA (`docs/decisions/video-player-nv12.md`):

- `src/scene.rs`: `SurfaceFormat::Nv12` (8-bit 4:2:0, luma plane then
  interleaved chroma, even sides). `SurfaceFormat::byte_len(w, h)` replaces
  `bytes_per_pixel` (`w * h * 8` for RGBA16F, `w * h * 3 / 2` for NV12),
  and `SurfaceFrame::new` returns a `SurfaceFrameError` (zero or odd side,
  overflow, wrong length) instead of `None`. `YuvMatrix` (BT.601, BT.709)
  and `YuvColor { matrix, full_range }` ride on the frame
  (`SurfaceFrame::with_yuv`, `yuv`); `YuvColor::rgb_rows` is the one place
  the matrix and range become the numbers a renderer uploads.
  `SurfaceFrame::nv12_planes` splits the two planes.
- Only the vendored Windows renderer draws NV12. macOS keeps CoreVideo
  surfaces and Linux (gpui-pre-wgpu) draws no surfaces, so Quill produces
  NV12 frames on Windows only.
- `SurfaceFrame::draw_state` / `mark_drawn` / `mark_refused` and
  `SurfaceDrawState { Pending, Drawn, Refused }`: the renderer that draws a
  frame says what happened to it, so the producer can replace a frame its
  window can't draw (with an image) and knows when a paused picture is on
  screen. The answer belongs to the frame, so each window answers for
  itself.

## List scroll range counts the padding

Added in 2026 with gpui-kit 0.8.0 (`docs/decisions/gpui-kit-0-8-0.md`).
`src/elements/list.rs`: `ListState::max_scroll_offset`, which backs
`max_offset_for_scrollbar` and the bottom-aligned
`scroll_px_offset_for_scrollbar`, left out the list's vertical padding,
while the list's own wheel scroll and `set_offset_from_scrollbar` include
it. Kit 0.8.0's `ScrollBounce` scrolls a list through the scrollbar handle
and clamps to that smaller range, so on macOS a padded `MessageScroller`
(the chat history) stopped one padding short of its end: tail following
never resumed and the jump-to-latest button stayed up. The range now adds
`padding.top + padding.bottom`, the same bound the list clamps to.

## Animations read the executor clock

Added in 2026 (`docs/decisions/fix-gift-delete-flake.md`).
`src/elements/animation.rs`: `with_animation` and the spring element took
their start and step times from `Instant::now()`, the wall clock, so a test
that advanced the executor's clock never moved them and a loaded machine let
real time decide whether a kit dialog or alert had finished entering. They
now read `cx.background_executor().now()`, which is the wall clock in a
running app and the simulated clock under `TestAppContext`.

## Images get atlas textures of their own

Added in 2026 to give back memory after browsing
(`docs/decisions/sweep-r3-browse-memory.md`). `src/platform.rs`:
`AtlasBackend::insert_image`, which `AtlasState::get_or_insert_with` calls
for `AtlasKey::Image` tiles (glyphs and SVGs still go through `insert`).
Its default forwards to `insert`, so backends without the split (the
headless atlas) behave as before. A texture is freed only once its last
tile goes, and glyph and SVG tiles are never removed, so a colour emoji
placed in the space a photo left in its texture kept that texture (4 MB
and up) alive for good.

## A drag can start later than 2 pixels

Added in 2026 for dragging a message out by its picture
(`docs/decisions/drag-media-to-chat-row.md`). `src/elements/div.rs`:
`Interactivity::drag_start_distance` and the fluent
`StatefulInteractiveElement::drag_start_distance`, which set how far the
pointer moves (Manhattan distance) before `on_drag` starts (default still 2). Telegram Desktop
starts a drag after 10 pixels, and a press that moves less must still click
the picture it is on.

## A list says how far its viewport is from either end

Added in 2026 so history pages are asked for three screens early, as Telegram
Desktop does (`docs/decisions/history-preload.md`). `src/elements/list.rs`:
`ListState::estimated_distance_to_edges` returns the distance from the
viewport to the list's start and end, counting each item not measured yet at
the mean height of the measured ones, and the viewport's height.

## The accessibility tree can be forced and read in-process

Added in 2026 for Agent access's UI driver
(`docs/decisions/agent-access.md`, phase 5). GPUI builds its AccessKit tree
only while assistive technology is connected. `src/window/a11y.rs` and
`src/window.rs`: `Window::set_a11y_forced(bool)` builds it every frame
regardless (`A11y::forced` is ORed into the per-frame active check), while
only a tree the system asked for (`A11y::is_system_active`) is sent to the
platform adapter. `Window::a11y_tree_snapshot` returns the last built tree
as plain data (`A11yTreeSnapshot`, `A11yNodeSnapshot`: id, role, label,
value, states, supported actions, children, and bounds in window logical
pixels, from the element bounds GPUI recorded or, for synthetic children,
AccessKit's physical rectangle divided by the scale factor).
`src/window/a11y/debug.rs` exposes the retained update, focus and frame
number to the snapshot.

## Elements carry a tag for in-process readers of the tree

Added in 2026 for Agent access's UI driver, step 1's security redesign
(`docs/decisions/agent-access.md`, phase 5). `InteractiveElement::a11y_tag`
(`src/elements/div.rs`, `AriaProperties.tag`; forwarded by `Stateful`
and read through the new `Element::a11y_tag` default method in
`src/element.rs`) marks an element with a string the application chooses,
such as a region or a chat id. The tag rides two places and nowhere else:
the element's node in `Window::a11y_tree_snapshot` (`A11yNodeSnapshot.tag`,
from `A11y::node_tags` filled at prepaint) and the element's hitbox
(`Frame.hitbox_tags`, `Window::tag_hitbox`); a tagged element always gets
a hitbox. The platform adapter never sees a tag: it is not part of the
`TreeUpdate`. `Window::hovered_a11y_tags` returns the tags of the hitboxes
under the mouse as of the last mouse event, topmost first and stopping at
an occluding hitbox, so a reader can tell what GPUI itself would deliver a
click to after an overlay or a clip. `Window::focused_text_input` says
whether the last frame registered a text input handler, so a reader knows
typed characters would go into a text field.

## Hover tags follow the hit test; the focused element records its tags

Added in 2026 for the UI driver's second security review (phase 5).
`Window::hovered_a11y_tags` (`src/window.rs`) now walks the hit test the
way `Hitbox::is_hovered` does: only the first `hover_hitbox_count` ids
(the rest are reachable by scroll alone, under a
`BlockMouseExceptScroll` hitbox) and no further than an occluding
`BlockMouse` hitbox, tagged or not. `Window.a11y_tag_stack` holds the
tags of the elements being painted; `Interactivity::paint`
(`src/elements/div.rs`) pushes an element's tag around its children's
paint. When a focused element is painted (a `track_focus` div in
`Div::paint`, or any element calling `Window::handle_input`), the stack
is copied into `Frame.focus_tags`; `Window::focused_a11y_tags` returns
it from the last frame, innermost first, so a reader can tell where a
key event would land the way it tells where a click would. A tag
reaches the accessibility node (`A11y::node_tags`, `src/element.rs`)
only when the element has a role; it always tags the hitbox and the
stack.

## The focused text input's caret can be read

Added in 2026 for Agent access's UI driver, step 2 (the takeover state,
`docs/decisions/agent-access.md`). `Window::focused_text_caret` returns the
caret rectangle of the input handler the platform types into, in window
logical pixels, through the same `PlatformInputHandler::selected_bounds`
the input method's candidate window uses; the handler is taken from the
platform window and put back. Quill draws the agent's typing badge there.
