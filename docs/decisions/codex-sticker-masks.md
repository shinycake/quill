# Masks tab and mask stickers (`parity:stickers-masks`)

## What tdesktop does

- Mask sets (`StickersType::Masks`) are a separate list from regular stickers
  and custom emoji: their own installed order, archived order and
  `getInstalledStickerSets`-style load (`ApiWrap::updateMasks`).
- tdesktop never shows masks in "Settings > Stickers and emoji" or in the
  composer. The only places are the media editor (story editor) sticker
  panel, `TabbedSelector` with `Mode::MediaEditor`, which has a Stickers tab
  and a Masks tab, and the box the panel's gear opens:
  `StickersBox(Section::Masks, isMasks = true)` with two tabs, Masks and
  Archived (`stickers_box.cpp`, `refreshTabs`). The "Trending" tab does not
  exist for masks.
- Rows show the title and "N masks" (`lng_masks_count`), can be dragged to
  reorder, and have a remove control. Archived rows have a restore control.
  Archiving a mask set is in the set's own menu (`lng_masks_archive_pack`).
- A mask sticker picked in the editor is placed as a plain sticker image.
  tdesktop does not fit a mask to a face (`mask_position` is not used on
  desktop). In a chat a mask sticker is an ordinary sticker message.

## What Quill did before

Nothing: only `stickerTypeRegular` and `stickerTypeCustomEmoji` were
requested. `updateInstalledStickerSets` for any non-regular type was routed
to the emoji panel, so a mask order update would have reshuffled the emoji
list.

## What changed

- `src/telegram/requests/masks.rs`: `getInstalledStickerSets`,
  `getArchivedStickerSets` and `reorderInstalledStickerSets` with
  `stickerTypeMask`. `changeStickerSet` is untyped and reused.
- `src/state/domains/stickers/masks.rs`: `MasksPanel` (installed, archived,
  paging, per-set stickers for the editor strip) and its reducer methods.
  New `StickersPurpose` variants: `GetInstalledMaskSets`,
  `GetArchivedMaskSets`, `ChangeMaskSet`, `ReorderInstalledMaskSets`,
  `GetMaskSet`. Answers, errors and `ok` are handled in the stickers domain
  (`apply.rs`, `error.rs`) plus one call in `session_apply_ok.rs`.
- `StickersPayload::UpdateInstalledStickerSets` gained `is_mask`; a mask
  order update now reorders the mask list and no longer touches the emoji
  list.
- `src/connect/masks.rs`: open, tabs, paging, change (install, archive,
  remove), reorder, and the editor strip's loader. Changes are serialized
  (one at a time) and lists refetch after TDLib confirms.
- `src/ui/masks_ui.rs`: the Masks dialog (Masks and Archived tabs, drag to
  reorder, Archive, Remove with an inline confirm, Restore, "More archived
  masks") and a Masks row in the photo editor's Stickers mode with a
  "Manage masks" button. Picking a mask places it like any sticker.
- Rendering: a message with `stickerFullTypeMask` already parsed as a normal
  sticker; a test now pins that.

## Not done (and why)

- No face fitting from `maskPosition`: tdesktop does not do it either.
- No Masks section in settings: tdesktop has none.
- No trending or search for masks: tdesktop's masks box has neither.

## How verified

- Unit tests: request JSON with `stickerTypeMask` (`requests/masks.rs`),
  reducer tests (`state/tests/masks.rs`: installed, archived paging, stray
  answer ignored, set contents, confirmed change, error, order update
  routing), driver tests (`connect/tests/masks.rs`: requests sent, one change
  at a time, refetch after `ok`, editor strip loads four sets once), and a
  mask sticker message parse test.
- A temporary demo capture (not committed) showed the dialog with three
  sets, the count labels and the inline remove confirm.
- Not verified against a live account: no sets were installed, archived,
  removed or reordered on the real account.
