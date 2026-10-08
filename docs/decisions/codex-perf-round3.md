# Perf round 3: memory that comes back, 30 fps stickers, a composer slice

Follow-up to `codex-perf-pass` and `codex-history-anim-layer` ("What's
left"). Memory after scrolling never came back, avatars were decoded at
file size, sticker clips were bounded by count only and played at 60 fps,
and every caret blink or keystroke in the composer re-rendered (and laid
out) the whole visible history.

## What GPUI 0.3.7 does (read from `gpui-pre-0.3.7` and its platform crates)

- Atlas textures are 1024² (or the image size, if larger) and are freed only
  when their last tile is removed. The Metal (`gpui-pre-apple`
  `metal_atlas.rs`), wgpu (`gpui-pre-wgpu` `wgpu_atlas.rs`, Linux) and
  DirectX (`gpui-pre-windows` `directx_atlas.rs`) atlases all work this way;
  new tiles go into the newest texture with room. So evicting oldest-first
  empties whole textures, and the policy below is the same on every
  platform.
- `ImageSource::Custom` closures run with the window and app on every
  layout/prepaint/paint; `Window::current_view` and `App::notify(EntityId)`
  are public, so a custom loader can notify the views that waited for it
  (as GPUI's own `CachedLoad` does).
- A cached view's box comes from its cached style alone (`request_layout_view`
  adds a leaf node), and its content is laid out later, in prepaint, at that
  box (`prepaint_view` → `layout_as_root`). A cached view therefore can't
  size itself to its content in the same frame.
- Notifying a view dirties it and every ancestor, never siblings; a
  re-rendered cached view re-renders the cached views inside it. Only
  siblings can stay cached while one part changes.
- `img(ImageSource::Image(bytes))` goes through `window.use_asset`, which
  keeps the decoded image and its atlas tile for the life of the app.

## Memory

### Idle trim and a smaller budget (`ui/image_budget.rs`)

- Budget while things change: 48 MB / 2048 entries of decoded images beyond
  what is on screen (was 128 MB / 4096).
- Idle budget: 8 MB / 256 entries. Two seconds (`IDLE_AFTER`) after the set
  of shown images last changed, a task evicts down to it, between frames,
  and drops the images from every window's atlas.
- "Changed" means an image started being shown that wasn't in the last
  render of its slice (it scrolled in, or is new): `newly_shown` compares a
  use with the slice's previous render. Re-renders that show the same
  images (a TDLib update, a video tick, a hover) don't postpone the trim,
  so it also happens on a busy live account.
- After an idle trim that evicted something, free heap pages go back to the
  system: `malloc_zone_pressure_relief` on macOS, `malloc_trim` on glibc
  Linux; nothing on Windows (its heap decommits large free ranges itself)
  or other libcs. Without it, decoded images freed by the trim stayed
  resident as allocator free lists (about half of the malloc zone was free
  space after a chat-list scroll).
- Eviction order is least recently used first (`plan_eviction`,
  unit-tested), which matches allocation order in the atlas.
- `QUILL_TRACE_TICKS=1` logs each idle trim
  (`image idle trim: 616 images, 19.8 MB -> …`).

### Display-size decoding

`image_budget::sized_image(source, size)` returns an `ImageSource::Custom`
that decodes the file (or inline JPEG) on the background executor and
downscales it so its shorter side is `size` × the window's scale factor
(never enlarged; `image` Triangle filter; BGRA like GPUI's loader). Inside
the main window's `CacheScope` it goes through the bounded cache (same
eviction rules); elsewhere (other windows) through GPUI's asset cache, as
before. Used by:

- every photo avatar drawn with `kit_avatar_element` (chat list rows, the
  header, sender avatars in the history, info panels): a 160 px avatar in a 46 pt row on Retina
  decodes to 92 px (33 KB instead of 100 KB);
- chat-list photo thumbnails (`last_preview_thumb`), which used to go
  through `use_asset` and stay decoded and in the atlas forever; they are
  now 36 px and evictable.

### Sticker and emoji clips (`ui/sticker_playback.rs`, `sticker_playback.rs`)

- Byte budgets on top of the clip counts: stickers 48 MB (16 clips max),
  custom emoji 32 MB (160 clips max). The "don't evict what was shown in the
  last 500 ms" rule is unchanged: with more on screen than fits, the extra
  ones stay stills (`make_room`, `over_budget` unit-tested).
- Playback is capped at 30 fps (`MAX_PLAYBACK_FPS`): a 60 fps Lottie file
  is decoded every other frame (`sampled_frame_count`, unit-tested), so a
  typical 3 s sticker holds 90 frames instead of 120 (and a 2 s one 60
  instead of 120), and its layer ticks at 30 instead of 60. WebM stickers
  already decoded at ≤ 24 fps; custom emoji were already ≤ 36 frames.

Telegram Desktop plays Lottie at the file's own rate (60 fps for most
stickers). Its `lib_lottie` submodule is not checked out in the reference
clone, so I could not read its caching; `SourceFiles` drives stickers and
emoji by frame index with no rate reduction. At the sizes Quill draws them
(stickers decoded at 128 px, emoji at 56 px) 30 fps is very hard to tell
from 60, and the measurements below show it is about a third of the CPU of
an on-screen sticker. Quill has no full-screen sticker effects yet
(premium effects, emoji interactions); when they come they should decode
with `max_fps: f64::INFINITY` (the editor's still-frame path already does).

### The ~20 MB heap growth after scrolling the history

Heap profile (`MallocStackLogging=lite`, `heap -s`, release build) before
and after scrolling up through the 2000-message stress history: 45 MB →
65 MB allocated. The top retainer was `spoiler_fx::image_tile`: 60 frames
of a 256 px RGBA tile, 16.7 MB, built the first time a media spoiler is
drawn and kept in a `static` for the life of the app (and uploaded to the
atlas, as much again). It is now released, with its atlas copy, once no
spoiler has been painted for 3 s and no cached slice can replay one
(`image_budget::paint_stamp` / `may_still_show`); it is rebuilt on the next
spoiler. Everything else in the profile was bounded data (history rows,
sticker clips, text layout).

### Footprint

Release build on this Mac (M1 Pro), 1200×900 demo window,
`QUILL_DEMO_STRESS=3000,2000` with 3000 distinct 160×160 avatar files,
`QUILL_DEMO_HISTORY_ANIM=all,spoiler`, `QUILL_ASSUME_ACTIVE=1`;
`vmmap --summary`: physical footprint, atlas = `IOAccelerator (graphics)`,
malloc = bytes allocated in all zones.

| State | Before: footprint / atlas / malloc | After |
| --- | --- | --- |
| Idle after start | 146.0 / 22.3 / 39.1 MB | 141.3 / 18.3 / 38.3 MB |
| Chat list scrolled 1040 wheel steps × 96 px (≈1600 rows), then idle 40 s | 441.2 / 171.5 / 183.0 MB | 178.0 / 38.5 / 50.8 MB |
| Chat list scrolling continuously (28 s) | 452.3 / 171.5 / 183.6 MB | 283.2 / 82.8 / 110.2 MB |
| History scrolled up through 2000 messages, then idle 40 s | 164.4 / 22.3 / 56.7 MB | 134.9 / 18.3 / 41.3 MB |
| One 60 fps animated sticker (`QUILL_DEMO_STRESS=0,50`, `stickers60`) | 110.6–111.2 MB | 102.7 MB |

(This round's chat-list scroll goes further than the previous round's
"~1600 rows, 251 MB": it reaches the old plateau, so before and after are
compared on the same run here.)

## CPU: a composer slice

### What the profile said

`sample` of the release build scrolling the history, by caller:

- 188 of 210 layout samples were in the message list's own item layout
  (each visible row is rendered and laid out on every scroll step: GPUI's
  `list` keeps heights, not elements); 12 were the conversation slice's
  root layout (header, banners, composer); the rest elsewhere.
- Chat-list scrolling: 209 of 247 layout samples were rows; the slice root
  (header, search field, folder tabs, list) 12.

So splitting the header and composer off saves only that small root part
while scrolling. The composer, though, re-rendered the whole conversation
on its own: with the composer focused, the caret blinks (kit input, 500 ms)
re-rendered and laid out every visible row 2–3 times a second, and so did
every keystroke.

### What changed (`ui/app_slice.rs`, `ui/conversation.rs`)

- A third cached slice, `SliceKind::Composer`: the composer with its
  popups (emoji / sticker / GIF panel, mention / command / hashtag menus,
  inline results, link and poll panels, record bar), or the note / channel
  footer in its place. `conversation(part, cx)` builds either part; the
  composer code itself is unchanged.
- The conversation slot is a column: the conversation slice and its
  animation layer, then the composer slice. The composer paints after the
  layer, so its popups cover the history's animations without occluders.
- The composer slice's box takes the composer height measured last frame
  (`MeasureHeight`); when the composer grows or shrinks, it lays out at the
  bottom of its box (growing over the history for that one frame, as its
  popups do) and the slot is redrawn next frame (by notifying the layer
  view, a sibling, so the chat list keeps replaying).
- Every `QuillApp` notify still notifies all slices. Notifies that only the
  composer shows now target it: sticker / animated-emoji suggestions and
  the spellcheck underline shift on each input event
  (`notify_composer`).
- Not split: the conversation header and the chat-list header/search/tabs.
  The profile above puts them at a few percent of a scroll frame, and the
  header block is interleaved with a dozen panels in `session_history`; it
  wasn't worth the risk this round.

Cheap things from the chat-list profile: unpinned rows no longer count the
pinned chats (`pinned_chat_ids` walks every chat; 1.6% of the main thread
while scrolling 3000 chats).

### CPU numbers

`top` CPU over 6 one-second samples, base and new runs interleaved; the
machine was shared with other agents' builds, so ranges are wide. "Draw" is
main-thread `Window::draw` samples in a 5 s `sample`.

| Scenario | Before | After |
| --- | --- | --- |
| Scrolling the history | 21.8–24.1% (draw 625, layout 229) | 20.9–22.3% (draw 573, layout 199) |
| Scrolling the history, quieter machine | 16.8–23.7% | 13.7–18.3% |
| Scrolling the chat list | 22.0–24.1% (draw 671) | 22.4–25.0% (draw 668) |
| Idle, all history animations | 8.7–9.6% | 8.8–9.5% |
| Idle, composer focused (caret blinking) | 8.2–9.9%, conversation renders 2–3/s (draw 79, layout 17) | 7.5–8.9%, composer renders 2–3/s (draw 61, layout 5) |
| One 60 fps sticker on screen | 8.8–10.8%, ticks 60/s (draw 93–105) | 6.3–7.3%, ticks 31/s (draw 80–83) |

## Fixtures

- `QUILL_DEMO_AUTOSCROLL=<x>,<y>[,<dy>[,<steps>[,<stop>]]]`: stop after
  `stop` wheel events (to measure what stays after a scroll). A negative
  `dy` scrolls up first (the history starts at the bottom).
- `QUILL_DEMO_HISTORY_ANIM=stickers60`: one sticker authored at 60 fps
  (`docs/screenshots/fixtures/demo-sticker-60.tgs`, the demo sticker with
  its frame times doubled).
- `QUILL_TRACE_TICKS=1` also counts composer slice renders and logs idle
  trims.

## Verification

- Demo captures before/after (debug `demo-capture` builds): chat list,
  stress history with media, composer states (reply bar, edit/delete,
  forward, link preview panel, hashtag/emoji/mention suggestions, command
  menu, inline results, bot keyboard, rich editor, record bar, slow mode,
  send media), emoji panel, selection mode, channels (member/admin
  footers), bot topic tabs: pixel-identical except animation phase (typing
  dots, voice waveform), the scrollbar thumb of one fixture, and the
  avatar fixture (resampled image, no visible difference).
- Gate: fmt, core clippy, core and UI tests.

## Risks

- One frame of lag when the composer changes height (a new line, a reply
  bar, the channel footer after a chat switch): the composer grows over the
  bottom of the history, or leaves a strip of background when it shrinks,
  for one frame.
- Something in the conversation that reads composer state would replay
  stale content when only the composer slice is notified. The conversation
  part doesn't read the composer today; new code should notify `QuillApp`
  (or `notify_composer` only for composer-only state).
- Thanos dust from a deleted message is drawn by the conversation slice,
  so the composer now paints over the dust where they overlap.
- Images re-shown after an idle trim decode again (asynchronously; the
  avatar circle is empty for that moment, as on first load).
- The spoiler tile is rebuilt on the main thread when a spoiler shows again
  after being released (as on first use).

## What's left

- Scroll CPU is per-row work: every visible row is rebuilt and laid out per
  scroll step (GPUI's `list` and the kit's virtual list keep no elements
  between frames, and a cached view's cache key includes its origin, so
  per-row cached views would not survive scrolling either). Cheaper rows
  (fewer nested divs per bubble) are the lever.
- Atlas fragmentation: after the idle trim the atlas still holds ~20 MB
  more than at start (textures pinned by a few surviving tiles, glyphs and
  emoji share polychrome textures). Fixing that needs GPUI-side compaction.
- History photos still decode at file size (up to 1280 px) in the bounded
  cache; `sized_image` could serve them at their bubble size (the viewer
  keeps the full image).
- Composer slice: the header could follow with the same mechanism if a
  profile ever shows it.
