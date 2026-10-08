# Perf pass: cached slices, a chat-list animation layer, bounded image memory

After `codex-idle-redraw`, one animated custom emoji in a chat-list row
still cost 10–14% CPU with the window active, and the footprint only grew
as you scrolled. Both come from how Quill used GPUI: `QuillApp` was one
view, so any `cx.notify()` laid out and painted the whole window, and every
image it ever showed stayed decoded and in the sprite atlas.

## What GPUI 0.3.7 allows (read from `gpui-pre-0.3.7` source)

- `Entity::cached(style)` (`view.rs`) replays a view's previous prepaint
  and paint unless the view is in `window.dirty_views`, the window is
  refreshing, or its bounds, content mask or text style changed.
- `mark_view_dirty` (`window.rs`) dirties the notified view **and every
  ancestor**. A re-rendered cached view sets `window.refreshing` for its
  subtree, so cached views nested in it re-render too.
- Reading another entity in `render` does not dirty the reader; only
  `App::notify` of a rendered view (or a descendant) does.
- `notify` during a draw goes into the next frame's dirty set without
  running observers.
- `img(path)` without an image cache goes through `window.use_asset`,
  which keeps the decoded image for the life of the app, and nothing
  removes it from the atlas. An `Arc<RenderImage>` stays in the atlas until
  `drop_image` is called (dropping the `Arc` is not enough).

So small self-notifying views for animated content can't help on their
own: a 20 px emoji view still re-renders every ancestor, including the
whole chat list, and through it every row. Caching works only for
**siblings** of whatever animates.

## What changed

### Cached slices (`ui/app_slice.rs`)

The chat list (Ready mode) and the conversation (header, history,
composer) render as two cached child views ("slices"). A slice's render
borrows `QuillApp` and calls the same `sidebar(…)` / `conversation(…)`
code as before, so listeners, context menus, drag-reorder and the
virtual lists keep working as they did.

The invalidation contract:

- `QuillApp` observes itself; **every `QuillApp` notify notifies both
  slices**, so all the existing `cx.notify()` call sites still refresh
  everything.
- The frame clock records which slice asked for a tick (`SliceScope`
  marks a slice's subtree while it renders, lays out and paints, which
  covers rows that virtual lists build during prepaint). A tick notifies
  only the slices that asked; the others replay. Requests made outside any
  slice (spoiler specks, overlays) still redraw the whole app.
- Views rendered inside a slice (inputs, loading images, hover and scroll
  state) dirty it through GPUI's ancestor walk, and `window.refresh()`
  bypasses every cache, as before.
- Text selection: a participant's selection change now refreshes the
  window (`refresh_window_on_change`), since a cached slice would otherwise
  replay the old highlight.
- Inline video players are only treated as orphaned when the conversation
  actually rendered without them (a replayed conversation still shows its
  clips).

Scrolling one pane now leaves the other one replaying: wheel events
notify the slice under the pointer, not `QuillApp`. The floating date
pill's scroll observer (`history_fx::note_history_scroll`) now notifies
only the conversation slice for the same reason.

### The chat-list animation layer

Because ancestors always re-render, a ticking emoji in a chat row would
still rebuild the chat list. Decoded custom emoji in chat-list previews are
now `LayeredFrames` elements: a row lays the emoji out and, when the chat
list really paints, reports its bounds and content mask. A tiny
`AnimationLayer` view, rendered right after the chat list, paints the
current frame at those bounds every frame, picking it by time. The frame
clock's tick for these emoji notifies only the layer, so the chat list and
the conversation replay. Everything painted after the layer (menus,
dialogs, popovers, drag previews) still covers it; nothing inside a chat
row overlaps its preview emoji.

Emoji, stickers and videos in the conversation keep the per-frame path:
they tick the conversation slice only, so the chat list replays.

### Bounded image memory (`ui/image_budget.rs`)

- `BoundedImageCache` is the main window's `ImageCache` (installed over
  `QuillApp` by `CacheScope`, which, unlike GPUI's `image_cache` element,
  also covers prepaint, where virtual lists and cached slices build their
  content). Once decoded path images pass 128 MB (or 4096 entries), the
  least recently used images that can no longer be on screen are evicted
  and dropped from the atlas.
- "Can no longer be on screen" accounts for replays: an image last used
  by a slice is only gone once that slice rendered again without it;
  images used by `QuillApp` itself must have missed the last frame.
  Trimming runs at the start of a frame, before anything renders.
- Quill-made images now leave the atlas when their cache lets go of them:
  evicted or stopped sticker/emoji clips, the blurred-preview cache when it
  rolls over, and the call backdrop cache (now capped at 8 palettes).
  `retire_all` queues them and `sweep` drops each one once no replayable
  frame can show it and nothing else holds it.

Thread-local caches checked and left as they are: `inline_video` masks
and `round_seek` shades are keyed by size (and color), so a handful;
`spoiler_fx` keeps reveal timestamps only; `vanish` already caps at 4096
bounds.

### Measurement fixtures

- `QUILL_ASSUME_ACTIVE=1`: animate as if the window had focus (a demo
  launched from a terminal never gets it, and animations pause when
  inactive).
- `QUILL_DEMO_STRESS` now previews one animated custom emoji in the newest
  stress chat; `QUILL_DEMO_STRESS_REDRAW=0` drops the fixture's forced
  60 Hz redraw; `QUILL_DEMO_STRESS_AVATARS=<dir>` gives stress chat `i` the
  photo `<dir>/avatar-<i>.jpg`.
- `QUILL_DEMO_AUTOSCROLL=<x>,<y>[,<dy>[,<steps>]]` scrolls whatever is
  under that point with synthetic wheel events.
- `QUILL_TRACE_TICKS=1` now names the tick target and logs slice renders
  per second.

## Numbers

Release build on this Mac (M1 Pro), 1200×900 demo window, same fixture
before and after, `top` CPU over 6 one-second samples after the window
settled, `vmmap --summary` footprint, `sample` main-thread busy share
(samples outside `mach_msg2_trap`).

| Scenario | CPU before | CPU after | Main thread busy before → after |
| --- | --- | --- | --- |
| Idle, one animated emoji in a chat row (300 chats, small chat open) | 4.5–5.0% | 1.5–1.8% | 8.3% → 1.0% |
| Idle, same emoji, 3000 chats, 2000-message chat open | 7.0–10.9% | 1.8–2.2% | 13.0% → 1.2% |
| Two animated stickers in the open chat | 5.6–6.3% | 4.5–4.8% | — |
| Scrolling the chat list (3000 chats), emoji visible | 24.1–24.8% | 14.8–16.1% | 45% → 26% |
| Scrolling the history (2000 messages) | 23.3–26.3% | 13.5–16.2% | 63% → 50% |
| Forced full redraw at 60 Hz (everything dirty) | 24.2–25.2% | 23.2–24.0% | no regression |

The remaining idle CPU is mostly outside the main thread (Metal
presentation for the 18 fps layer redraw).

Footprint, scrolling 3000 chats with distinct 160×160 photos for 35 s:

| | Physical footprint | Atlas (IOAccelerator graphics) | Malloc |
| --- | --- | --- | --- |
| Before | 777 MB | 345 MB | 404 MB |
| After | 420 MB | 155 MB | 243 MB |

Idle footprints of the small fixtures are unchanged (83 MB / 123 MB); the
user's 527 MB comes from a live account's images, which this bounds the
same way.

## Inactive window (follow-up to the live test)

The live test found idle CPU higher with Quill behind another app than in
front (Saved Messages: ~28% active, 35–38% inactive). Only the
sticker/emoji requesters checked `window_active`; inline video loops,
round videos, GIF autoplay, the date pill, jump highlight, typing dots and
the other fades kept ticking, the muted AVPlayer loops kept decoding, and
GIF autoplay notified the whole app at its own frame rate. Now:

- One gate in the frame clock (`tick_wanted`): no tick is requested or
  delivered while the window is inactive, except for media playing with
  sound (`request_media_tick`, a round video or clip the user unmuted).
- `observe_window_activation` updates the gate as soon as the window
  changes state and redraws, so content asks for ticks again on return.
- Muted inline loops pause behind another app and resume on return
  (`InlineVideos::set_window_active`); GIF autoplay stops redrawing there.
- Fixtures: `QUILL_DEMO_DEACTIVATE=1` (a second window takes key status
  once the demo is ready, so the window goes active → inactive) and
  `QUILL_DEMO_BACKGROUND=1` (never activate). `QUILL_TRACE_TICKS` lines
  now include the window's active state.

Release build, `QUILL_ASSUME_ACTIVE` unset, CPU over 6 samples:

| Fixture | Active | Inactive before this fix | Inactive after |
| --- | --- | --- | --- |
| Round video note autoplaying | 8.1–10.9% | 7.5–10.7% | 1.1–1.9% |
| GIF autoplay | 3.9–4.7% | 5.2–9.2% | 0.3–0.4% |
| Animated stickers in history | 7.7–8.9% | — | 0.4–0.5% |
| Animated emoji in a chat row (300 chats) | 2.9–3.6% | — | 0.4–0.5% |

In every inactive run the tick trace goes quiet within a second of
deactivation.

## Risks

- A slice that reads state of an entity rendered outside it would replay
  stale content on frames where only the other slice changed. The slices
  read `QuillApp` (covered by the observer) and entities rendered inside
  them; new code should keep it that way, or notify `QuillApp`.
- A `QuillApp` notify made during a draw (render-time) doesn't reach the
  observer; GPUI already doesn't redraw for those, and the next real
  notify refreshes the slices.
- The animation layer paints over the chat list, so anything later drawn
  inside the chat list on top of a preview emoji would be under it.
  Nothing does today.
