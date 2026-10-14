# Sticker and custom emoji set editing (the "creator")

Parity item `parity:stickers-creator`.

## What tdesktop does

Telegram Desktop does not create sticker sets in the app. There is no call
to `stickers.createStickerSet` anywhere in its source; the set box's menu
ends with "Check the @stickers bot for more options" (a link to
https://t.me/stickers). What it does have is *editing* of sets the user
created (`stickerSet.flags & creator`, `AmCreator`), in
`boxes/sticker_set_box.cpp` and `api/api_stickers_creator.cpp`:

- The box of an owned set shows a round "+" cell after the stickers while
  the set is not empty and under its limit (120 stickers, 200 emoji). It
  opens a menu: "Create a New Sticker" / "Add an Existing Sticker", and for
  emoji sets also "Adapt from a Sticker".
- "Create a New Sticker" picks an image ("Choose Image"), opens the photo
  editor with a fixed square crop (512 px side for stickers, 100 px for
  emoji), then a "New Sticker" / "New Emoji" box with a preview and an emoji
  picker ("Choose emojis that match your sticker", at most seven). "Done"
  without an emoji toasts "Please choose an emoji."; the image is uploaded
  and `stickers.addStickerToSet` runs; toasts "Sticker added." / "Emoji
  added." / "Sticker upload failed. Please try again." / "Could not load
  image."
- "Add an Existing Sticker" opens the sticker panel; the chosen sticker is
  added under its own emoji or 🙂 when it has none. A full set toasts "This
  Sticker Set is full."; a repeat "This Sticker is already in the Set."
- "Edit Stickers" / "Edit Emoji" in the footer enters reorder mode (drag the
  stickers; `stickers.changeStickerPosition`), where the only footer button
  is "Done". The ⋯ menu also has "Reorder".
- The ⋯ menu adds, for the creator, "Edit name" (box "Edit Sticker Set
  Name", "Choose a name for your set.", 50 characters,
  `stickers.renameStickerSet`) and "Delete Pack" with "Delete for Everyone"
  ("Are you sure you want to delete this sticker set for everyone? This
  cannot be undone." / "Remove", `stickers.deleteStickerSet`) and "Delete
  for Myself" (`Remove "{title}"?`, uninstall).
- A sticker cell's context menu offers the creator "Delete sticker" /
  "Delete emoji" (box with the sticker's picture, "Are you sure you want to
  delete the sticker from your sticker set?", "Delete" / "Close",
  `stickers.removeStickerFromSet`). In someone else's set it offers "Add to
  Sticker Set" / "Add to Emoji Set" listing the user's own sets.
- A sticker message that belongs to no set gets the same "Add to Sticker
  Set" / "Add to Emoji Set" in its context menu, between the set item and
  favorites: an animated sticker fits both kinds, one at most 100 px a side
  is an emoji, the rest are stickers.
- The sticker and emoji panels show a small "edit" badge next to the
  header of a set the user created.
- After an edit tdesktop re-applies the answered set; after a reorder it
  does not (that would make the stickers blink).

## What Quill does

Everything above, through TDLib: `addStickerToSet` (with `newSticker`,
`inputFileLocal` for a rendered image or `inputFileId` for an existing
sticker; TDLib uploads), `removeStickerFromSet`, `setStickerPositionInSet`,
`setStickerSetTitle`, `deleteStickerSet`. `is_owned` on `stickerSet` /
`stickerSetInfo` is tdesktop's `AmCreator`.

- Core (`src/sticker_set_box.rs`): limits, sides, labels, menu entries,
  `has_add_cell`, `joinable_kinds`, `fitted_size`, `valid_set_name`,
  `emoji_or_default`, tested.
- State (`src/state/domains/stickers/creator.rs`):
  `StickersPurpose::Creator(StickerCreatorPurpose)`; `StickerSetEdits` on
  the open view (pending count, `refresh_needed`, `deleted`); the toast in
  `stickers.creator_note`; `apply_sticker_creator_ok` / `fail_sticker_creator`;
  `owned_sticker_set_name` / `owned_sticker_sets` for the badge and the
  menus.
- Driver (`src/connect/sticker_creator.rs`): one method per edit, each
  naming the set by its short name and counting itself in the view's edits;
  `sync_sticker_set_view` asks `getStickerSet` again after a confirmed edit
  (not after a move) without dropping the box to "Loading". Tested with the
  recording sender.
- UI (`src/ui/sticker_set_box/`): the set box moved out of
  `message_menu_ui` and gained the "+" cell and its menu, the creator footer
  and reorder mode (drag cells, `on_drag`/`on_drop`), the cell context menu,
  the ⋯ entries, and the boxes: rename, delete sticker, delete for
  everyone, delete for myself, "New Sticker" (preview, emoji search and
  grid, chosen emoji), and the existing-sticker chooser (favorites, recent
  and loaded sets, since Quill's panel is not a dropdown). The photo editor
  got an `EditorPurpose::Set` with a pixel-square crop that stays square
  while a corner is dragged; "Done" fits the result to the set's side and
  writes a PNG (TDLib accepts PNG for `stickerFormatWebp`). The panel header
  of an owned set shows a kit `Tag` reading "edit". The message menu of a
  set-less sticker gets "Add to Sticker Set" / "Add to Emoji Set" pages.
- Demo `ready-sticker-set-owned`; UI tests in `src/ui/sticker_set_box/tests.rs`.

## Not done here

- Video stickers (WEBM) as new stickers or adapted emoji, and GIF
  adaptation: tdesktop runs its video editor and a WEBM encoder. Quill has
  no WEBM encoder yet; choosing a WEBM sticker to adapt toasts "Video
  stickers can't be adapted into emoji yet." Adding an existing WEBM
  sticker as it is works (`inputFileId`).
- The existing-sticker chooser is a dialog listing what the session has
  loaded, not tdesktop's full tabbed sticker panel.
- Set creation stays with the @stickers bot, as in tdesktop.
