# Stickers manager box

## What Telegram Desktop does

`boxes/stickers_box.cpp` is one box with tabs: Installed, Trending (shown when there are sets or unread ones), Archived (shown when non-empty). Masks have their own box (Masks and Archived tabs) opened from the photo editor. Rows show a cover thumbnail, title and count, a reorder handle on installed sets, and a remove button. Removal is deferred: the row shows Undo and requests go out when the box closes. Trending rows have Add or Added and an unread dot; the list loads more when scrolled to the end. Settings > Chats > Stickers and emoji holds the suggest-by-emoji choice and the entry to the box. There is no per-row archive action.

## What changed

- New `src/ui/stickers_manager/` (rows, tabs, drag reorder, settings section) shared by the stickers and masks boxes.
- `src/stickers_manager.rs`: pure edit model (removed ids, dragged order).
- `src/connect/sticker_manager.rs`: one-at-a-time commit queue (removals, then reorder), cover downloads, viewing trending sets. `StickerSetInfo.is_viewed` added.
- The picker in `stickers_ui.rs` keeps sending only; the old archived dialog, options menu and mask dialog are gone.
- Counts use `trn()`; unused lang keys removed.

## Deviations from the task wording

- Masks are a separate box, not a tab (as in tdesktop).
- No per-row archive action (tdesktop has none; the set box keeps Archive).
- Removal is committed on close, with Undo until then.

## Verification

Driver tests (6) for the request sequence, GPUI UI tests (12) for tabs, remove and Undo, trending Add/Added, archived Add, open set box, masks box, settings entry; unit tests for the edit model and load-more distance. `/verify` captures in light and dark of the Installed, Trending and removed states. Not shown: real thumbnail downloads over TDLib, drag in a live window, masks box capture, behavior on a real account.
