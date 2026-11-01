# Changes made by the Quill project

This directory is gpui-pre 0.3.10 from crates.io (Apache-2.0,
`LICENSE-APACHE`), GPUI's core, used through `[patch.crates-io]` in Quill's
`Cargo.toml`. Commit `6be84cd7` added the registry copy without its
`examples/` directory and their `[[example]]` tables (and without the
registry's `.cargo-ok`, `.cargo_vcs_info.json`, `Cargo.toml.orig` and
`Cargo.lock`), so `git diff 6be84cd7 -- third_party/gpui-pre` shows every
later change. (The patch was first made on 0.3.8, vendored in `f55b3086`,
and carried to 0.3.9 in `docs/decisions/gpui-kit-0-7-3.md`, and to 0.3.10 in `docs/decisions/gpui-kit-0-8-1.md`.)

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
- The vendored Windows and wgpu (Linux) renderers draw NV12 (the wgpu one
  since `docs/decisions/video-nv12-linux.md`). macOS keeps CoreVideo
  surfaces.
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

## Color emoji glyphs can be swept

Added in 2026 to give back GPU memory after browsing
(`docs/decisions/idle-wallpaper-and-emoji-trim.md`). GPUI keeps every glyph
it rasterized until the window closes, and color emoji are big (4 bytes a
pixel, often 64 px and up): one look at the emoji panel kept 16 MB.

- `src/platform.rs`: `AtlasState` stores each tile with the use mark it was
  last asked for under and counts its color emoji glyphs. `PlatformAtlas`
  gains `next_use_mark`, `emoji_glyph_count` and
  `remove_emoji_glyphs_unused_since`, which do nothing by default; the
  headless atlas forwards them to its `AtlasState`.
- `src/window.rs`: `Window::emoji_glyph_count`,
  `Window::begin_emoji_glyph_sweep` (a new use mark, then `refresh`, so the
  next frame redraws every view and asks for each glyph on screen) and
  `Window::sweep_emoji_glyphs`, called once that frame is drawn.
- Text glyphs, SVGs and images are never swept.

Tests (in Quill, since the crate's own do not build in Quill's tree):
`glyph_sweep::tests::a_sweep_drops_only_the_emoji_nothing_drew_since_it_began`
and `text_glyphs_stay_whatever_their_mark`, on the headless atlas.

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

## Paint filter and window capture

Added in 2026 for Agent access's UI driver, step 3 (screenshots,
`docs/decisions/agent-access.md`).

- `src/window.rs`: `Window::set_paint_filter(Option<PaintFilter>)`. While
  a filter is set, `push_a11y_tag` / `pop_a11y_tag` ask it, with the tag
  stack, whether what is painted now reaches the scene, and set
  `Scene::suppressed` from the answer. A tagged element the filter turns
  off is covered with `PaintFilter::hidden` over its bounds. Setting a
  filter refreshes the window, and a filtered frame does not replay cached
  paint (`Scene::filtered`), since that paint was never filtered.
  `Window::capture_frame` hands the last drawn scene to
  `PlatformWindow::capture_image`.
- `src/scene.rs`: `insert_primitive` drops primitives while `suppressed`;
  `replay` does nothing while `filtered`. Both reset in `clear`.
- `src/elements/div.rs`: an element's tag is pushed around its own
  background and border as well as its children (it used to wrap the
  children only), so a filtered row loses its background too.
- `src/platform.rs`: `PlatformWindow::capture_image(&Scene)`, by default an
  error. `src/platform/test/window.rs` answers with `render_to_image`.
- `src/window.rs`, `src/elements/div.rs`, `src/elements/text.rs`: a
  tooltip is drawn at the root after everything else, where the tag stack
  is empty. Under a paint filter, the element that owns a tooltip records
  the tag stack it is painted under (`Window::record_tooltip_tags`), and
  the tooltip is painted under the same tags, so the filter cuts it with
  its owner (a tooltip over a hidden chat row is hidden too). Nothing is
  recorded while no filter is set.

## A focused element without a node reports its focusable frame

Added in 2026 for Agent access's UI driver (`docs/decisions/agent-access.md`,
"Phase 5: fixes from a live test"). A text input puts its role on a frame
(gpui-component's `Input` renders `InputBase` with the role and a focus
handle of its own) while the editor inside, which has an id and no role,
takes the focus. `A11y::set_focus` only reported a node that had been
pushed, so the focused composer left the tree with no focus at all, for
the driver and for screen readers alike. `src/window/a11y.rs`:
`A11y::focus_nearest_focusable_ancestor` reports the nearest enclosing
node that registered a focus handle this frame (`set_focusable`) instead,
unless a node already holds the focus; `set_focus` falls back to it for a
focused element with no node. `src/elements/div.rs`: a focused element
with no id falls back the same way before logging that it has no node.
The fallback is marked (`A11yNodeBuilder::focus_is_fallback`): a node
painted later in the same frame that owns the focused handle replaces it
(`clear_fallback_focus`, then `set_focus`) instead of tripping the debug
assertion "set_focus called more than once in a single frame". Only a
second real focus still asserts.
The crate's own tests do not build in Quill's tree (missing dev
dependencies and example assets), so Quill's window test
`the_clicked_message_box_is_the_focused_node` covers it;
`a_node_that_owns_the_focus_replaces_the_fallback` in `src/window/a11y.rs`
documents the takeover for when they do.

## `ShapedLine` keeps one decoration run inline

Changed in 2026 for the memory sweep (`docs/decisions/shaped-line-size.md`).
`ShapedLine::decoration_runs` was a `SmallVec<[DecorationRun; 32]>`: 32
runs of 92 bytes inline made every `ShapedLine` 2,984 bytes, nearly all of
it unused, as a line rarely has more than one or two colours. Quill's
laid-out paragraphs (`src/input/bidi_paragraph.rs` in gpui-base, cached by
Quill's `paragraph_cache`) keep one per visual run, and scrolling three
long Hebrew channels on a Mac left about 137 MB of them. `src/text_system/line.rs`:
the field is now `DecorationRuns`, a `SmallVec<[DecorationRun; 1]>`, and
`src/text_system.rs` builds it in `shape_line` and `shape_line_by_hash`. A
line with more runs keeps them on the heap, a few small allocations at
shaping.

## A cached view replays its part of the accessibility tree

Changed in 2026 for the memory and idle sweep
(`docs/decisions/a11y-cached-views.md`). GPUI builds the AccessKit tree
during prepaint, and a cached view that replays its last frame
(`reuse_prepaint`) skipped that, so its nodes dropped out of the tree.
Quill worked around it by rendering every slice each frame while the tree
was built, and on macOS anything that hit-tests the window (a window
manager such as Tiles) turns the tree on until Quill quits.

- `src/window/a11y.rs`: `A11y::index` returns an `A11yIndex` (finished
  nodes and focus claims so far), which `PrepaintStateIndex` now carries.
  `A11y::replay` adds a range of the last frame's tree again: the nodes
  from the retained update (`A11yDebug::last_update`), those whose parent
  is outside the range as children of the node being built, and their
  bounds, tags, focus handles and action listeners, kept from the last
  frame in `RenderedTables`. Focus is claimed through
  `A11yNodeBuilder::claim` (`FocusClaim`: owner, fallback, active
  descendant), which logs each claim so the claims in a range are made
  again. `begin_frame` returns whether the last frame built a tree;
  `skip_frame` records one that did not. A range holding an id that fresh
  content already added (an element drawn twice) is left out, since
  AccessKit rejects a node listed under two parents, and the next frame
  renders every view.
- `src/window.rs`: `reuse_prepaint` calls `A11y::replay` while the tree
  is built, and copies the `Frame.hitbox_tags` of the hitboxes it reuses.
  `draw_roots` renders every view (`refreshing`) when the tree is built
  and the last frame built none. `PrepaintStateIndex` also records the
  focus, and while the tree is built a cached view whose last prepaint
  saw another focus renders instead of replaying
  (`Window::a11y_focus_moved`, checked in `src/view.rs`): a view that
  focuses something as it renders moves the focus mid-draw, where
  `refresh` does nothing, and replayed claims would be stale. `Frame.focus_tags` keeps every tag stack
  a focused element recorded, indexed by `PaintIndex::focus_tags_index`
  so `reuse_paint` replays them; `Window::focused_a11y_tags` reads the
  last. `Window::simulate_a11y_action` (test support) handles an action
  request as if assistive technology sent it.
- `src/window/a11y/debug.rs`: `A11yDebug::node_info` (debug builds) gives
  a replayed node its provenance.

Quill's `a11y_replay_tests` cover it.

## Live entity and subscription counts for tests

Added in 2026 for the memory sweep (`docs/decisions/entity-leaks.md`).
Upstream's `LeakDetector` only exists in leak-detection builds (gpui-pre's
own tests, the `leak-detection` feature, or `GPUI_LEAK_DETECTION` set at
build time; Quill's builds use none of them), and its
`assert_no_new_leaks` checks nothing in a plain `test-support` build, so a
Quill test could not tell whether closing a surface let go of what opening
it made. Everything here is `test-support` only, except the
`#[track_caller]` on `App::spawn_with_priority`, which is in every build
(it only changes the location a panic or a task reports).

- `src/app/entity_map.rs`: `EntityMap` keeps each live entity's type name
  (`type_names`, filled in `insert`, emptied in `take_dropped`, so an entity
  leased for an update still counts); `live_counts` tallies them by type.
- `src/subscription.rs`: `SubscriberSet::subscriber_count`, the
  subscribers over every emitter that have not been dropped.
- `src/app.rs`: `App::live_entity_counts` (live entities by type name) and
  `App::live_subscription_counts` (observers, event listeners, release
  listeners, global, new-entity, keystroke, window-closed, keyboard-layout,
  thermal, display, sleep, wake, lifecycle, memory-warning, quit and
  restart observers).
- `src/window.rs`: `Window::live_subscription_counts` (focus, bounds,
  appearance, button-layout, activation, visibility, display and
  pending-input observers).
- `src/executor.rs`: `ForegroundExecutor::live_tasks`, the foreground
  tasks of this thread that have neither finished nor been dropped, by the
  `#[track_caller]` location of the spawn (`cx.spawn` and friends pass the
  caller through; `App::spawn_with_priority` gained the attribute for
  this). `spawn`, `spawn_with_priority` and `spawn_when_idle` wrap the
  future in a guard (`LiveTask`) that counts it while it exists.

## A window counts the frames it draws

Changed in 2026 for the memory sweep
(`docs/decisions/paragraph-cache-widths.md`). Quill's `paragraph_cache`
lets the widths a paragraph was only measured at drop their layouts once
the frame that measured them is over, and keeps every width a frame drew.
It needs to tell one frame's requests from the next, in each window.
`src/window.rs`: `Window::draw` adds one to `frames_begun` as it begins,
and `Window::frame_number` returns it.

## A test window can be moved

Added in 2026 for the memory sweep (`docs/decisions/covered-main-window.md`),
test support only. A test could resize a window but not move it, nor move it
to another display through `TestAppContext`. `src/platform/test/window.rs`:
`TestWindow::simulate_move` sets the window's origin and runs the platform's
moved callback, as a window the user dragged reports itself.
`src/app/test_context.rs`: `TestAppContext::simulate_window_move` and
`simulate_window_move_to_display` (the latter calls the existing
`TestWindow::simulate_move_to_display`).

## A replayed range enters the bounds tree once

Changed in 2026 for the deep sweep. `Scene::replay` sent every layer, and every
primitive outside a layer, back through `BoundsTree::insert` on each frame a
cached view replays, to work out orders the last frame had already stored in
`paint_operations`.

- `src/bounds_tree.rs`: `insert_span(bounds, orderings_after_first)` inserts one
  box standing for that many orderings plus one and returns the first, one above
  whatever it meets. Later bounds that meet it come after the last. `insert` is
  `insert_span(bounds, 0)`.
- `src/scene.rs`: `replay` keeps the `filtered` early return, then
  `replay_as_one` takes the union of the range's clipped bounds and its lowest
  and highest stored order, inserts that box once, and moves every primitive by
  the same amount. `StartLayer` and `EndLayer` are copied, since a primitive in
  a layer already stores the layer's order. A primitive in a layer counts in the
  union even where it pokes out of the layer's bounds, which `insert_primitive`
  never put in the tree. `insert_primitive` is split in two, and the second half,
  `push_primitive`, is what both call. `PathId` is still assigned there.
- `repaint_operations` is the old loop, for a replay into an open layer (a
  primitive takes that layer's order) or while `suppressed`.
- `Scene::refuse_paint_for_test` (test support) sets `filtered` and `suppressed`.

Primitives the tree saw keep their order where their bounds overlap, but orders
are not always what painting again would give:

- A later primitive over the box but over nothing the range drew sorts higher
  than it had to, so it can land in another batch. On 777 demo frames compared
  with the old loop, 620 kept their batch count, 99 gained up to 4 and 58 lost up
  to 8.
- A layer takes an order whether anything draws in it or not, and a replay only
  knows the orders primitives stored. A view that opens with a layer that drew
  nothing slides down as a whole on its first replay. With views that do not
  touch, that changed the batch count in 9 of 200 random cases, by -3 to +1.
- Painting never shows the tree a primitive past its layer's bounds, such as a
  glyph past its line, and a replayed box counts it. So a later view's quad over
  that overhang sorts above the glyph on a replay and can sort under it on a fresh
  paint. The replay follows paint order, in all 1,246 of 10,387 cross-view
  overlaps that differed in a model with offset glyphs. These are full-strength
  pixels, at glyph edges only, and they can change between a painted frame and
  the next replay.

The tree sees a shadow's whole blur (`Primitive::painted_bounds`, the same margin as
the shaders), on paint and on replay, so a tail keeps its side between a painted
frame and the next replay. In random scenes that took 3,113 differing pairs to 0,
for about 4% more batches.

On a demo chat of 4,813 paint operations with a looping sticker, `Scene::replay`
went from 283 to 75 µs a frame and `Window::draw` from 1,003 to 797 µs of CPU.

The crate's own tests do not build in Quill's tree, so `src/ui/scene_replay_tests.rs`
holds the tests: random overlapping primitives with layers, replayed, replayed
again through outer views, and compared with painting everything from scratch.

The Quill project also changed it in 2026 so a drag doesn't turn every view
cache off:

- `src/window.rs`: a `MouseMoveEvent` during a drag called `refresh`, which
  renders every cached view again, on each update, even with the file held
  still. `dispatch_mouse_event` now refreshes only when the hit test changed,
  since `drag_over` styles follow the hovered hitboxes. A move inside the same
  hitboxes marks the window dirty, because the ghost is painted on every draw,
  and an update at the position the ghost was last painted for
  (`drag_painted_at`) does nothing. `FileDropEvent::Entered` refreshes by itself
  now, since it can arrive with the hit test unchanged. A drawn frame whose
  hovered hitboxes moved under a still pointer (`note_drag_hover`) refreshes
  once, so cached `drag_over` styles follow them. Tests:
  `src/ui/drag_redraw_tests.rs`.

The Quill project also changed it in 2026 so the end of a scroll doesn't keep
presenting:

- `src/input_rate.rs`: `InputRateTracker`, moved out of `src/window.rs` with
  injected instants so `scripts/test-input-rate.sh` can test it alone.
  `sustain_duration` goes from 1 s to 150 ms: a gesture that stopped
  re-presented the unchanged frame at display rate for a second, and held off
  the macOS frame idle by as long. Gaps inside a gesture are far shorter.

The Quill project also changed it in 2026 so tests can seed a window's atlas
(`src/ui/glyph_sweep_tests.rs`):

- `src/window.rs`: `Window::sprite_atlas`, in test support only.

## Live views: a frame that redraws one view alone

A notified view dirties every ancestor, and an ancestor that renders renders all
it holds, so a view that only animates rebuilt the window root on every tick.
`Entity::live` embeds a view that can redraw on its own. While only live views
were notified, `Window::draw` renders those and nothing else, in `window/live_views.rs`:

- Each live view's entry in the frame keeps what its place in the tree gave it
  (element ids, text style, image caches, content mask, opacity, rem size, bounds)
  and the paint operations it added to the scene.
- The frame's scene is the last whole frame's, replayed, with those operations
  swapped for new ones. Replay ranges balance, so a swap lands at the stack depth
  the view painted at.
- `rendered_frame` stays the last whole frame: hit tests, the key dispatch tree,
  the accessibility tree and the ranges cached views replay from all read it, and
  nothing a live view builds reaches them. `Window::shown_scene` is the scene last
  drawn, for presenting and for tests.
- Any notify of another view, `Window::refresh`, an inspector, a paint filter, an
  open profiler overlay, or an accessibility tree not up to date draws a whole frame.
- `window/a11y.rs` holds the tree still during a patch: live views add no nodes,
  and nothing is sent.
- A live view whose element path runs through another live view's is drawn by
  that one, so a patch skips it as a site of its own. A cached view between the two
  renders, since a patch refreshes: it has no frame to replay from but the last whole
  one, and replaying would take the listeners that frame still needs. Its state stays
  out of `rendered_frame`, as its ranges index the patch, so it renders in the next
  whole frame too.
- `Window::transact` drops the entries of a prepaint it rolls back, and a view
  prepainted twice is not nested in itself. A notified view no patch reaches draws a
  whole frame, since a tick that draws nothing would stop asking for the next.
- The whole frame after a patch marks every live view it drew dirty, so a cached
  view holding one renders again instead of replaying an older paint.
- A whole frame with no patch before it replays such a cached view: the live views
  in that paint stay in the frame with it, their operations moved, so their next tick
  is a patch too (`Window::replay_live_views`, in `paint_view` and for a replayed
  deferred draw).

A hit test answers only for a whole frame's hitboxes, so a live view that builds
any (a hover, a click, a tooltip) never patches: a notify of it draws a whole frame,
a patch for another view leaves its last paint in place, and a patch that finds new
hitboxes in a view asks for that whole frame. So a live view suits content that only
shows; anything else about it stays correct and costs whole frames.
`Window::frame_number` counts these frames like any other. With `test-support`,
`Window::interactive_live_views` counts the live views that drew in whole frames
alone in the last one.

`Window::element_opacity` and `Window::with_element_opacity` are public so that
content painted away from its place in the tree keeps the opacity it was given there.

`src/ui/layer_tick_tests.rs` holds the tests.

The Quill project also changed it in 2026 so text glyphs can be swept like color
emoji, from a much higher count:

- `src/platform.rs`: `AtlasState` counts text glyphs. `PlatformAtlas` gains
  `text_glyph_count` and `remove_text_glyphs_unused_since`.
- `src/window.rs`: `Window::text_glyph_count`, `Window::sweep_text_glyphs`.

The Quill project also changed it in 2026 so `Scene::finish` stops allocating a
sort buffer each frame:

- `src/scene.rs`: `sort_primitives` leaves a sorted list alone and otherwise
  sorts `(key, position)` pairs in a kept buffer, then moves the primitives by
  swaps. `sort_by_key` took a scratch buffer of whole structs, up to 128 bytes
  each, from `malloc`'s large path on every frame. Test:
  `src/ui/scene_replay_tests.rs`.

The Quill project also changed it in 2026 so a warning logged during a quit
reaches the log:

- `src/app.rs`: `App::shutdown` ends with `log::logger().flush()`. macOS exits
  from `applicationWillTerminate`, so `Application::run` never returns there.
  Test: `src/ui/quit_flush_tests.rs`.

The Quill project also changed it in 2026 so the docs say what macOS now
emits (`third_party/gpui-pre-macos/QUILL-CHANGES.md`):

- `src/app.rs`, `src/platform.rs`: the docs of `App::on_app_lifecycle` and
  `Platform::on_app_lifecycle` say macOS emits `AppLifecyclePhase::Active`.

The Quill project also changed it in 2026 so an app can refuse a quit the OS
started (`third_party/gpui-pre-macos/QUILL-CHANGES.md`,
`third_party/gpui-pre-windows/QUILL-CHANGES.md`):

- `src/platform.rs`: `Platform::on_quit_request`, by default never called,
  whose callback answers a `QuitAnswer`: `Allow`, `Refuse`, or `Later` when
  the app is busy further up the stack. Linux has no session-end hook to call
  it from.
- `src/app.rs`: `App::on_quit_request`. A request that finds the app borrowed
  answers `Later`, so the platform holds the quit and asks again from its
  event loop instead of letting it through unasked.
- `src/platform/test/platform.rs`, `src/app/test_context.rs`:
  `TestAppContext::simulate_quit_request`, which returns the answer.
- `src/platform.rs`, `src/window.rs`: `PlatformWindow::set_shutdown_block_reason`
  (a no-op by default) and `Window::set_shutdown_block_reason`, the text
  Windows shows while the window holds back the end of the session.
