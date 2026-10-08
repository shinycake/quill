# History animations on a layer: the conversation stops re-rendering per frame

After `codex-perf-pass`, the chat list's animated preview emoji were drawn
by a small layer while the list replayed its cached frame, but everything
that animates in the open conversation still ticked the conversation slice.
With the window active, the live account's Saved Messages (two animated
stickers, custom emoji, inline video thumbnails, a round video note) ran at
23–29% CPU with `slice renders/s: {conversation: 30}`: every animation frame
rebuilt every visible message row, laid it out and painted it.

## What GPUI 0.3.7 allows (read from `gpui-pre-0.3.7`)

- `view.rs` `prepaint_view` / `paint_view`: a cached view replays its last
  prepaint and paint (`Window::reuse_prepaint` / `reuse_paint`: hitboxes,
  deferred draws, scene primitives) unless it is in `dirty_views`, the
  window is refreshing, or its bounds, content mask or text style changed.
  On a replay none of its element code runs.
- `window.rs` `mark_view_dirty` / `invalidate_entities`: a notified view
  dirties itself and its ancestors (`view_path_reversed`), never its
  siblings. A view rendered next to a cached slice can be notified every
  frame while the slice replays.
- `window.rs` `draw_roots`: the whole element tree is prepainted (in tree
  order), then deferred draws are prepainted, then the tree is painted, then
  the deferred draws, then the prompt / drag preview / tooltip. So a sibling
  that comes after a slice prepaints after the slice's prepaint and *before*
  the slice paints, and everything deferred (kit popovers, context menus,
  tooltips) paints after it.
- `scene.rs` `insert_primitive` / `replay`: a primitive's order comes from
  the bounds tree in insertion order, and a replayed range is re-inserted in
  order, so whatever paints after a replayed slice draws above it.
- Element content masks are applied in prepaint as well as paint (`Div` via
  `Interactivity`, `list` per item), so bounds and clip are final in
  prepaint. `Window::transact` (the list's autoscroll retry) can discard a
  prepaint; paint is never retried.
- `Window::element_opacity` is crate-private: the layer can't know an
  ancestor's opacity.

## Design (`ui/anim_layer.rs`)

Each slice (chat list, conversation) now has an animation layer: a `Layer`
(the slice's items and occluders, as of its last real frame) and an
`AnimationLayer` view rendered right after the slice in its slot. The
existing chat-list emoji layer moved here and became one instance.

Per real frame of a slice:

1. `SliceScope` prepaint clears the layer (`begin_frame`) and makes it
   current (thread-local, also set while the slice renders, so rows built
   lazily by the virtual list find it).
2. Animated content registers in prepaint: `Layered` elements record their
   bounds, content mask and what to draw; `Occluder`s record the bounds of
   what floats over the slice's content.
3. The layer prepaints (`Layer::plan`): an item an occluder intersects is
   *inline* (the slice draws it, and ticks itself for it, as before); the
   layer builds the elements it redraws (tiles, mirrors).
4. The slice paints: each item records its paint order (confirming it was
   painted; candidates of a discarded prepaint never are) and paints itself
   when inline.
5. The layer paints the confirmed, uncovered items in paint order, then
   defers `QuillApp::tick_animation_layer`, which asks the frame clock for a
   tick of the layer (layer content) and/or of the slice (inline content).

On a replayed frame steps 1–4 don't run; the items of the last real frame
stay, matching what the replay shows. Scrolling, resizing and anything that
changes the conversation re-render it, so positions update in the same
frame the conversation moves.

What goes through the conversation's layer:

| Content | Kind | Row draws | Layer draws |
| --- | --- | --- | --- |
| Animated stickers (≤60 fps), custom emoji in message text (≤30 fps) | `frames` | nothing (transparent: a still under the frame would show through) | the clip's frame for now |
| Inline video / GIF, round video message (muted loop, macOS) | `tile` | the tile with the frame it rendered with (opaque) | the same tile (surface, corner/circle mask, GIF / time-left / unseen badges) from the player's current frame |
| The time pill over media | `mirror` | the pill, unless the layer draws one under it | the pill again, above the video |
| Spoiler dust on media, spoiler specks in text | `painter` / `Layer::paint_now` | nothing | specks for now |
| Typing dots (header; chat rows on the chat list's layer) | `painter` | nothing | dots for now |

Kept on the slice (unchanged): a round video playing with sound or showing
its seek ring (interactive, rare), a spoiler cover fading out after a
reveal (it sits under an opacity the layer can't apply), the frame-file GIF
and video playback for non-native clips, and short transitions (date pill,
jump highlight, selection, reveal of new rows, Thanos dust). Stickers and
emoji in the sticker / emoji picker keep the old path.

Stills: a sticker or emoji still decoding shows its still image in the row,
as before; once decoded, the row reserves its box and the layer draws the
frames. Video tiles keep their rendered frame in the row, so a capture or a
frame without the layer shows a complete tile.

## Z-order

The layer paints after the whole slice and before anything painted later:

- Covered by construction: everything `QuillApp` paints after the shell
  (message and chat menus, the reaction selector, the media viewer, story
  viewer, dialogs, the call overlay, toasts), every deferred draw (kit
  popovers and `context_menu`s), tooltips, prompts and drag previews.
- Over history content inside the conversation (occluders): the emoji /
  sticker / GIF panel, the round-video recording circle, the floating date
  pill, the "@" / heart corner buttons, the kit scroller's jump-to-latest
  button (an occluder over its bottom-center zone while scrolled up, since it
  is drawn inside the kit's `MessageScroller`), selection-mode check circles,
  and Thanos dust while a deleted message crumbles. Animated content under
  them is drawn by the conversation as before, only while covered.
- Drawn over a layered tile inside the bubble: the time pill (a mirror).
- The kit scrollbar thumb sits in the rows' 12 px gutter (`scrollbar.rs`:
  4 px inset, 8 px wide), so it never meets message content.
- Selection tint, the jump highlight, hover and pressed backgrounds are row
  backgrounds, under the content, so under the layer too.
- `CutLeft` (a message dissolving) now clips in prepaint too, so content
  reporting there gets the cut mask.

## Inactive window and caches

- Ticks still go through `request_tick`'s gate: nothing is requested while
  the window is inactive (a clip playing with sound keeps its slice path and
  `request_media_tick`). On activation the window redraws, the layer paints
  and asks again.
- Sticker and emoji clips: a clip the layer draws marks itself shown
  (`StickerClip::used`, now shared with the layer's `LayeredClip`), so the
  "don't evict what was shown in the last 500 ms" rule still holds while the
  row replays. Retired frames stay in the atlas while the layer holds them
  (`image_budget::sweep` keeps images with other owners).
- Inline players: the layer reads frames with `InlineVideos::current`, which
  neither starts a player nor marks its row rendered; the existing sweep
  (players whose rows stop rendering are dropped) is unchanged.

## Measurements

Release build on this Mac (M1 Pro), 1200×900 demo window,
`QUILL_ASSUME_ACTIVE=1`, `QUILL_DEMO_STRESS=0,50` (50 formatted messages
above, hashed rows as on a live account), `top` CPU over 6 one-second
samples after the window settled, `sample` main-thread busy share. Fixture
`QUILL_DEMO_HISTORY_ANIM=<content>` (new, see below).

| Fixture | CPU before | CPU after | Conversation renders/s | Main thread busy |
| --- | --- | --- | --- | --- |
| `stickers`: two animated stickers | 5.8–6.3% | 3.2–4.8% | 29 → 0 | 9.9% → 2.6% |
| `emoji`: three animated custom emoji in a message | 5.5–6.8% | 2.8–4.2% | 18 → 0 | 6.5% → 2.0% |
| `spoiler`: a spoiler photo and a spoiler text | 10.3–13.2% | 5.9–6.2% | 29 (and the chat list 29) → 0 | 14.9% → 5.4% |
| `ready-typing`: typing in the header and the chat row | 4.9–6.0% | 2.2–2.5% | 12 (and the chat list 12) → 0 | 6.1% → 1.9% |
| `video`: an inline-autoplaying video | 11.5–13.7% | 6.6–7.5% | 30 → 0 | — → 3.7% |
| `note`: a round video message | 9.0–13.0% | 5.3–8.1% | 30 → 0 | 11.7% → 3.9% |
| `all`: video, emoji, stickers, round video | 11.8–13.2% | 7.9–9.2% | 30 → 0 | — → 4.0% |
| `all`, scrolling the history | 20.6–22.4% | 19.1–22.5% | 58 → 58 | no change |
| `all`, window inactive | 0.4% | 0.4% | 0 → 0 | — |
| nothing animated | 0.2–0.4% | 0.2–0.3% | 0 → 0 | — |

"0" means no conversation render after the window settled (the trace
prints nothing once renders stop). With video, most of what remains is
AVFoundation decoding and presenting the surfaces (outside the main
thread); with stickers and emoji, the 30 fps redraw of the root view, the
layer and Metal presentation, as for the chat-list emoji layer. Scrolling
re-renders the conversation for every scroll step, as before: the layer
neither helps nor costs there.

The fixture's rows are light (one sticker or tile per message, little
text); on a live account each conversation render costs more (the live
Saved Messages measured 23–29% at 30 conversation renders/s), and that is
the part this removes.

## Fixtures

- `QUILL_DEMO_HISTORY_ANIM=stickers,emoji,video,note,spoiler` (or `all` for
  the first four) on `ready-chats`: the open chat ends with two animated
  stickers (TGS, WebM), a message with three animated custom emoji, an
  inline-autoplaying video, a round video message, and (`spoiler`) a photo
  and a text behind spoilers; read to the end.
- `…,panel` / `…,menu` / `…,select`: the emoji panel, a message menu or
  selection mode over that history, to check what covers animated content.

## Risks

- A new floating element inside the conversation that overlaps message
  content must be an `Occluder` (or a `mirror`), or the layer will draw
  animations over it. Deferred and app-level overlays need nothing.
- An ancestor opacity under the conversation (crate-private in GPUI) isn't
  applied to layer content; nothing in a history row fades content that
  animates today (the spoiler-reveal fade is kept on the slice).
- The occluder for the kit's jump-to-latest button is a fixed zone (128×56
  at the bottom center), not the button's real bounds.
- The layer redraws elements it builds (video tiles, pills) every frame; it
  is a few divs per visible clip, but a history with many visible clips
  pays that per tick.
