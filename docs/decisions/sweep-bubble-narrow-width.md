# Bubbles that fit a narrow chat pane

## The problem

Bubble widths came from fixed numbers: `msgMaxWidth` (430), `maxMediaSize`
(430), the 268 px file and poll floor, the link preview card's inner width
and the album mosaic's 430 px box. Below a pane of about 480 px, a photo,
album, link preview, file row, voice row or poll was wider than the room
beside the avatar, and the pane cut it off. Text bubbles mostly survived,
because the kit `Bubble` caps itself at 80% of the row. Any fixed `w` or
`min_w` inside a bubble beats that cap, though, so anything with a fixed
width overflowed.

## What Telegram Desktop does

`Message::resizeContentGetHeight` lays every bubble out for the history
width it is given: the width minus `msgMargin` (16 left, 56 right) and minus
`msgPhotoSkip` (40) when the row has a sender photo. Each media's
`countCurrentSize(newWidth)` fits that width. A photo scales down and keeps
its shape. `GroupedMedia::countCurrentSize` scales the whole mosaic, gaps
included, so a narrow album keeps its arrangement. Nothing is clipped, and
no width goes under `msgMinWidth` (160).

## What changed in Quill

- `quill::bubble_layout::HistoryWidth` is the history's width as bubble
  layout reads it: whole pixels, saturating at 640. From 640 px on, every
  row already gets its full `msgMaxWidth` and `maxMediaSize` (the
  narrowest row, one with a sender photo, leaves `(640 - 24 - 40) * 4 / 5 =
  460`). So every wide history reads as the same value, and resizing a
  wide window changes no layout input at all.
- `HistoryWidth::bubble_room(photo, plain)` is the pure width function. It
  takes the row (history minus the scroller's 12 px insets, minus 40 for the
  photo column) and gives the bubble four fifths of it, the same share the
  kit `Bubble` takes. That share stands in for tdesktop's `msgMargin.right`.
  A plain-look row gets all of it. The floor is `msgMinWidth`.
- `bubble_width::BubbleRoom` carries that room through the row builders.
  `outer()` caps the bubble (`msgMaxWidth`, or less), and `media(led)` caps
  a picture inside the bubble's border or padding. File, voice, poll,
  contact and link preview widths are the old tdesktop widths, capped by the
  room.
- Single photos, videos, GIFs, paid media, story cards and the action card
  pass the room into `media_frame_for` as the width `countCurrentSize`
  fits, so they scale down and keep their aspect ratio.
- `quill::album::scale_layout` ports `GroupedMedia::countCurrentSize`. The
  mosaic is laid out at full width once and then scaled, with each tile's
  right and bottom edges scaled together with the gap after them, so the
  gaps stay even. Paid albums use it too.
- Boxes that keep a fixed size (the map tile, the theme preview picture)
  use `.max_w_full().aspect_ratio(..)`. They shrink with the bubble in pure
  layout.
- The link preview's album strip shrinks its four 72 px tiles to fit the
  card.

The width is measured at paint by the probe that already measured the
chat-wide layout class (`history_width_probe`, formerly `chat_layout_probe`)
and stored in `HistoryState::width`. `BubbleLook` reads it, so every row
builder sees it the way it sees the font size or the plain look.

## The row cache from #931

The brief said the row cache key must include the width input. It doesn't,
because `HistoryRowInputs`, the cached rows, hold no widths: they hold the
message, its chrome and its grouping. Widths
enter only when the row's element is built, through `BubbleLook`, and that
happens on every render anyway. Putting the width into `rows_key` would
rebuild every row on each resize step for no change in the rows
themselves.

What does go stale on a resize is the list's measured row heights. GPUI's
list already drops all of them when its width changes. The bubble width,
though, is read from a cell that is set during the previous paint, which
leaves a one-frame lag. So when the quantized `HistoryWidth` changes, the
probe calls `scroller.remeasure` and notifies, the same way it already did
for the chat-wide layout class. Since `HistoryWidth` saturates at 640,
resizing among wide widths triggers nothing.

`a_narrow_window_rewraps_the_bubbles_without_rebuilding_the_rows` in
`rows_cache_tests.rs` covers this. Going from 1100 to 1200 px wide changes
nothing. Going to 700 px narrows the history width, narrows a long text
bubble, makes its row taller, and scales a 1200x800 photo down to its room,
with `row_stats.rebuilds` unchanged. Going back to 1100 restores the exact
painted sizes. With the photo pinned back to 430 px, the test fails.

## Evidence

Captures live in `/Volumes/Idan1TB/quill-work/scratch/sweep-bubble-narrow-width/`
(`before/` from main, `after/` from this branch).

- At 1400x900, the media, albums, link-preview, reply and downloads demos are
  identical pixel for pixel before and after. In showcase, only the clock labels
  differ. In voice, only a 20x20 patch of the live recorder waveform
  differs. Both of those change on every run.
- At 700x700 (a pane of about 400 px), light and dark: before, the link
  preview card, pending photo, album, voice rows, poll and channel photos
  ran past the pane edge. After, all of them fit. The album keeps its
  arrangement with even gaps, and the reply quote truncates.
- At 1000x700 with the downloads panel open, the 400 px history shows its
  photos whole.
- After rebasing on #939, the one-column layout at 380x600 (`after-939/`):
  the showcase reply and link preview that the original report showed cut
  off now fit, and so do the media, album, link preview and voice demos in
  light and dark. The 640x700 showcase fits beside the chat list.

## Review follow-ups

- First frame: until a history has painted, `HistoryUi::width_probed` is
  false and `seed_history_width` seeds the width from the conversation pane
  (`shell_layout::chat_pane_width` for the main window, the viewport width
  for a chat window), so the first paint no longer lays out at 640 and
  shrinks a frame later. The paint probe stays the source of truth.
- Resizing: the probe floors the pane width to 8 px steps
  (`chat_wide_ui::history_pane_width`), so a drag between 380 and 640 px
  re-lays the rows out every 8 px, and the content never exceeds the pane.
- Re-captured the showcase at 380x600 dark: bubbles fit.

## Not verified

- Real TDLib chats. Only demo data was captured.
- Linux and Windows builds were not run. The change is plain layout math
  with no platform code.
- No narrow capture of a location map tile, a theme preview, a game card or
  a sponsored message. Those take the same room through the same paths.
