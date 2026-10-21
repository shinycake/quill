# Message menu: fixed order for media rows

## What tdesktop does

`FillContextMenuItems` (`history/view/history_view_context_menu.cpp`) adds the
media's own actions (`AddPhotoActions` / `AddDocumentActions`) first, then Copy
Text, then Translate. Inside the media block the order is: cancel download,
Open GIF, Save GIF, sticker pack info, add to own set, favorite, Show in
Finder/Folder, Save for Notifications, Save As, Copy Filename. A photo is Save
Image, Copy Image. Quill does not model tdesktop's attached-stickers row or the
voice "Rate/Transcribe" row.

## What was wrong

Media rows were sorted by `order::MEDIA + index as u8`, the index of the action
in the list that happened to apply. The block had no declared width, so enough
actions could tie with or pass `COPY_TEXT` and `TRANSLATE`, and `index as u8`
could wrap.

## What changed

- Every `MediaAction` has one fixed `slot()`, and `sort_key()` adds it to
  `order::MEDIA` with a saturating add. A `media_actions!` macro declares the
  enum, `MediaAction::ALL` and `canonical()` from one list (now in
  `src/message_menu/media_actions.rs`), so no variant can be missing from
  `ALL`. A test checks each entry's `canonical()` sits at its own index in
  `ALL`, which is what `slot()` relies on.
- `MEDIA_SLOTS` is the length of `ALL`, so the block width follows that list.
  A `const` assertion fails the build if `MEDIA + MEDIA_SLOTS` passes
  `COPY_TEXT`. The orders from `COPY_TEXT` on moved
  up by 5 to make room for all 13 actions; only their relative order matters.
- A unit test iterates `MediaAction::ALL` and checks each slot equals its index
  and the keys are increasing and inside the block. It guards the slot order;
  it does not reproduce the original tie.
- `REPLY_TIMECODE` and `REPLY_ELSEWHERE` shared key 11, and a playing voice
  message can show both. Reply in Another Chat is Quill's own row (tdesktop has
  none), so it now follows the timecode row at 12 and the rows after moved up one.
- A UI test opens the menu on a captioned song with every action it can have
  and reads the rows from the accessibility tree: media rows, then Copy Text,
  then Translate.

## How verified

Unit and UI tests above; light and dark captures of the menu on the
`ready-audio` song (see the PR).
