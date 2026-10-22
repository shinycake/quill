# Drag media out of a message onto a chat row

README: `parity:chatlist-drag-media-forward`. Builds on
`drag-forward-to-chat-row.md` (the time label and selected rows).

## What Telegram Desktop does

`ListWidget::prepareDrag` (`history_view_list_widget.cpp`, same logic in
`history_inner_widget.cpp`) picks the drag source in this order: a standing
selection when all of it can be forwarded, the date label (the message or its
album), then a press on a media handler
(`media()->dragItemByHandler(pressedHandler)`: photo, video, GIF, sticker,
round video, dice and document thumbnails; `UnwrappedMedia` accepts anything
but the reply link) or on an album part (`isHiddenByGroup()`), which carries
that single message when `allowsForward()`. A chat with copy restrictions
never drags. The drag starts after `startDragDistance` (10 px); a shorter move
is still the click.

## What changed

- `quill::forward_drag::media_press_drags` lists the contents whose picture
  starts a drag. The press carries only that message (`ForwardDrag::begin`
  with no selection), with the same refusals as the time label.
- A single message's media is wrapped in a drag source
  (`forward_drag_source::media_block`, called from `session_history_row`); each
  album tile has its own (`album_tile`), so an album part lifts that part
  and the time label still lifts the album. The click stays on the media
  inside the wrapper, so opening the viewer, spoiler reveal and the text
  selection of captions are untouched.
- GPUI starts a drag after 2 px and then drops the click. The vendored GPUI
  gets `drag_start_distance` (see `third_party/gpui-pre/QUILL-CHANGES.md`),
  and message drags use tdesktop's 10 px, so a press that moves less opens
  the picture as before. The distance is Manhattan (`|dx| + |dy|`), as in
  tdesktop and `selection_drag::moved_enough`; the check is the same for
  every drag source, so the 2 px default now counts a diagonal move a little
  sooner than before.
- The #1095 drag sources (the time label, the selection-mode row overlay,
  the bubble footer) go through the same `draggable`, so they now also start
  at 10 px instead of 2 px.
- Play buttons on media let their press through (so the picture's drag can
  start from the centre of a video) and stop their own click instead, so a
  click still plays the video once and the tile does not also open it.

## Deviations

- Voice notes and audio rows do not drag: their press belongs to the seek
  bar (tdesktop only drags from the play button).
- Paid-media albums do not drag, locked or bought.
- tdesktop also drags polls, contacts, checklists, locations (their link),
  games, invoices and link-preview media. Quill's `media_press_drags` does
  not list them yet, so pressing those and moving does nothing.
- The ghost is the count pill of the earlier slice, not the picture.

## Verified

Unit tests (`forward_drag`), GPUI UI tests
(`chat_list/media_drag_tests.rs`: lift from an album tile, a photo, a sticker,
a document, a round video and a video's play disc, drop forwards that one
message, a 4 px wiggle and a plain click still open the viewer, a drag
released back on its tile is no click and no swipe to reply, protected chat,
pending message and paid media do not lift; `drag_start_tests.rs`: other drag
sources keep the 2 px start, the far start is Manhattan 10, a button's click
is its own), the earlier forward tests, and demo captures
`ready-drop-media-forward` in light and dark.
