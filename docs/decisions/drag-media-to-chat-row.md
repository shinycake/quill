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
  (`forward_drag_source::media_draggable`, in `session_history_row`); each
  album tile has its own (`album_tile`), so an album part lifts that part
  and the time label still lifts the album. The click stays on the media
  inside the wrapper, so opening the viewer, spoiler reveal and the text
  selection of captions are untouched.
- GPUI starts a drag after 2 px and then drops the click. The vendored GPUI
  gets `drag_start_distance` (see `third_party/gpui-pre/QUILL-CHANGES.md`),
  and message drags use tdesktop's 10 px, so a press that moves less opens
  the picture as before.

## Deviations

- Voice notes and audio rows do not drag: their press belongs to the seek
  bar (tdesktop only drags from the play button).
- A press on the video tile's play button takes the press itself (kit
  buttons swallow it), so a drag starts beside it, not on it.
- Paid-media albums do not drag.
- The ghost is the count pill of the earlier slice, not the picture.

## Verified

Unit tests (`forward_drag`), GPUI UI tests
(`chat_list/media_drag_tests.rs`: lift from an album tile and a photo, drop
forwards that one message, a 4 px wiggle and a plain click still open the
viewer, a drag released back on its tile is no click, protected chat and
pending message refuse), the earlier forward tests, and demo captures
`ready-drop-media-forward` in light and dark.
