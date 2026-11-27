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

Not done: the forward box has no preview of what is forwarded, and the
album Delete box offers no moderation checkboxes (the single-message box
keeps them).
