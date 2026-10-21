# Message menu: fixed order for media rows

## What tdesktop does

`FillContextMenuItems` (`history/view/history_view_context_menu.cpp`) adds the
media's own actions (`AddPhotoActions` / `AddDocumentActions`) first, then Copy
Text, then Translate. Inside the media block the order is: cancel download,
Open GIF, Save GIF, sticker pack info, add to own set, favorite, Show in
Finder/Folder, attached stickers, Save for Notifications, Save As, Copy
Filename. A photo is Save Image, Copy Image, attached stickers.

## What was wrong

Media rows were sorted by `order::MEDIA + index as u8`, the index of the action
in the list that happened to apply. The block had no declared width, so enough
actions would tie with or pass `COPY_TEXT` and `TRANSLATE`, and the row keys
would collide. `index as u8` could also wrap.

## What changed

- Every `MediaAction` has one fixed `slot()` (an exhaustive match, so a new
  action cannot be added without choosing its place), and `sort_key()` adds it
  to `order::MEDIA` with a saturating add.
- `MEDIA_SLOTS` is the block width. A `const` assertion fails the build if
  `MEDIA + MEDIA_SLOTS` passes `COPY_TEXT`. The orders from `COPY_TEXT` on moved
  up by 5 to make room for all 13 actions; only their relative order matters.
- A unit test checks the slots are distinct, increasing and inside the block.
- A UI test opens the menu on a captioned song with every action it can have
  and reads the rows from the accessibility tree: media rows, then Copy Text,
  then Translate.

## How verified

Unit and UI tests above; light and dark captures of the menu on the
`ready-audio` song (see the PR).
