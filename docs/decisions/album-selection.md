# Album selection and forwarding

Albums (photo/video mosaics and file albums) select and forward the way
Telegram Desktop's `HistoryInner` does.

- An album row has the same frame as a single message's row: the
  selection check and tint, right-click menu, drag selection, swipe to
  reply and the double-click action (`conversation/album_row_frame.rs`).
  Before this, album rows had none of these.
- In selection mode a click on one tile toggles that message alone
  (`PointState::GroupPart`); a click anywhere else on the row toggles the
  whole album (`changeSelectionAsGroup`). The row's check and tint show
  only when every item is selected; a tile selected on its own is washed
  in the selection colour.
- Tiles note where they were painted (`album_select::TileHits`, read the
  frame after, like `hit_rows`), so a press or a right-click can tell a
  tile from the rest of the row while the selection overlay covers it.
- The message menu records its `MenuScope` when it opens: over a tile it
  acts on that message, anywhere else on the whole album. Forward sends
  the album's ids in one `forwardMessages` (TDLib keeps them one album),
  Select adds the album, Delete asks once for all of it.
- Drag ranges, Shift+click, "Select up to this message" and Space on the
  focused row take albums whole.

- The album's Delete box is the single message's box for all of it
  (`selection_mode/delete_dialog.rs`): "Delete N messages?", delete for
  everyone, and a moderator's Report Spam, Delete all from the user and
  Ban, which act on every message of the album (tdesktop `deleteAsGroup`
  opens `CreateModerateMessagesBox` for the group's items).

## Forward preview

tdesktop's forward bar (`ForwardPanel`) names who the messages are from and
shows the first message's text, or "N messages", with a small picture when
one message is forwarded. Its forward box shows nothing of the messages.

Quill draws the same lines and the pictures of the forwarded media
(`forward_preview.rs`) in two places:

- the forward bar, up to three 32 px pictures left of the lines, then
  "+N" for the rest (tdesktop: a picture only for a single message; an album
  is several messages and still deserves to be seen);
- the forward box, a card above the search with up to four 44 px pictures
  and the lines, so chats are chosen with the album in view.

Spoiler and secret media show no picture, as in the reply bar.

Demo: `ready-albums` with `QUILL_DEMO_ALBUM_FORWARD=box` or `=bar`.

Not done: the selection box's Delete has no moderation checkboxes yet
(tdesktop offers them when every selected message allows them); the
message properties they need are fetched per message when its menu opens.
