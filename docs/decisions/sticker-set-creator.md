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

## Follow-up: the sticker editor and video stickers (branch `sticker-creator-parity`)

The two gaps that kept `stickers-creator` unticked are closed here.

**The sticker editor** now does what tdesktop's `PrepareEditorState` does.
The picked image (or the still of an adapted sticker) sits as a movable,
zoomable item (drag, wheel, bottom-right handle) on a transparent 512 px
canvas (`sticker_image::CANVAS_SIDE`; tdesktop prepares the scene at
`kStickerSide` for emoji too). The crop is fixed to the canvas, so a
non-square image gets transparent padding instead of losing content. The
Crop tools are Rotate, Flip and a "Corners" menu (`lng_photo_editor_corners_*`:
the caption "Choose how rounded the sticker corners should look.", then
Large / Medium / Small / None with a check on the current one, Large by
default) and, for stickers whose ratio isn't 1 (within 0.01, clamped to
1/3..3; emoji force 0), "Keep original aspect ratio", which makes the crop
the largest centered rectangle of that ratio. "Done" renders the canvas,
fits it to the set's side (`FittedStickerSize`) and cuts the rounded mask
(`ApplyShapeMask`, radius = shorter side × 0.3 / 0.2 / 0.12 / 0). Images
the editor refuses (empty, or longer than ten times the shorter side) toast
"Could not load image.". Code: `src/sticker_image.rs` (pure, tested),
`src/ui/photo_editor_set.rs`, `photo_editor.rs` (base item painted first,
rounded frame, wheel zoom), `photo_editor_items.rs` (the base is never
deleted).

**Video stickers.** "Adapt from a Sticker" on a WEBM sticker, and a saved
GIF picked in the "Add an Existing Sticker / Emoji" chooser (tdesktop's
panel has a GIFs tab there, `handleChosenGifs` → `AdaptGifToSet`), open
Quill's video editor with a set target (`RunVideoEditorAndCreate` with
`VideoStickerEditorData`): the range starts at the first three seconds and
can't leave 200 ms..3 s, the crop starts as the centered square and stays
square in pixels, there is no quality or GIF choice, rotate resets to the
turned square. "Done" encodes the sticker off the UI thread and the "New
Sticker" / "New Emoji" box opens with a frame of the result and the
sticker's own emoji pre-chosen; "Done" there uploads it with
`stickerFormatWebm`. A file that isn't downloaded yet is fetched and the
chooser stays open for a second pick. A source shorter than 200 ms, a file
ffmpeg can't read, or a result that won't fit any quality step toasts
"Sorry, Quill can't process that type of video." (`lng_bad_video`).

**Encoder choice.** tdesktop links libavcodec and drives libvpx-vp9 in
process. Quill already runs the external `ffmpeg` for the video editor and
`ffprobe`/libvpx for decoding WEBM stickers, so the sticker encoder is the
same CLI (`src/sticker_video.rs`): `-c:v libvpx-vp9 -pix_fmt yuva420p -b:v 0
-crf N -auto-alt-ref 0`, no audio, `fps=min(source_fps,30)`, scaled to the
set's side, walking tdesktop's ladder (crf 28, then 34 / 40 / 47 / 55 / 63)
until the file is under 256 KiB (stickers) or 64 KiB (emoji). No FFI, no
new crate, no new system dependency: where ffmpeg is missing the editor
says so with the install hint (`media_tools::ffmpeg_missing_message`), and
the never-crash rule holds because every step is a child process whose
failure is a `Result`. Linux CI has no ffmpeg; the encoder test returns
early there (like the video editor's existing ffmpeg test) and the argument
builder, limits, crop and range math are tested without it.

**Verified**: unit tests in `sticker_image.rs`, `sticker_video.rs` (an
ffmpeg-gated end-to-end encode of a generated clip into a ≤ 64 KiB 100×100
emoji WEBM), `photo_editor_set.rs`; the driver test sends
`stickerFormatWebm`; UI tests in `sticker_set_box/tests.rs` cover the padded
canvas and corner mask through "Done", the refused image, and the WEBM
adapt flow into the video editor and back.

## Still different from tdesktop

- The existing-sticker chooser is a dialog listing what the session has
  loaded (plus saved GIFs), not tdesktop's full tabbed sticker panel.
- "This Sticker is already in the Set." is checked before the request
  against the open box or the panel's cached contents. tdesktop also
  reports it after the request when the set did not grow; Quill shows
  "Sticker added." then.
- Failed adds toast tdesktop's fallback wording. tdesktop shows the
  server's error type first, but Quill's `TdError` keeps only the code
  (the message can carry secrets), so the fallback is all it has.
- "Delete for Myself" reuses the box's remove path, which closes the box
  as the footer's "Remove Stickers" always did in Quill; tdesktop keeps
  the box open.
- The video editor's preview frame is the uncropped frame with the crop
  drawn over it, as in Quill's composer editor; tdesktop paints the cropped
  frame.
- Set creation stays with the @stickers bot, as in tdesktop.

## Kit components

The box, its menus and the confirm dialogs are gpui-kit `Dialog`,
`Button`, `PopupMenu`, `Input` and `Tag`; the corners menu is a kit
`DropdownMenu` with check items. Two parts are plain `div`s: the
"+" cell is a round disc the kit has no component for (tdesktop draws it
as a circle in the sticker grid), and the New Sticker box's emoji grid is
a 48-cell search result over `emoji_catalog`, smaller than the media
panel's emoji tab, which is bound to the composer and the panel's own
state.
