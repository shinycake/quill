# Render-path lookups: folder order, custom emoji, chat row roots

Round 3 audit items 6, 7 and 8. Internal change; tdesktop has no counterpart
to compare, and nothing a user sees changes.

## What changed

- Folder tab order (`src/state/folder_order_cache.rs`). `ordered_folder_chats`
  keeps each folder's chat id order together with a revision. The revision
  moves whenever the main order is rebuilt (`flush_chat_order`) and wherever a
  chat's folder positions are written (`apply_position_fields`, the full
  positions set in `replace_main_list_from_positions`). A stored order from an
  older revision is never used; it is recomputed from `Session::chats` with
  the same sort as before. The read maps ids back to chats and skips any
  chat that left the map or the folder. A busy cell skips the cache (try_borrow).
  The `ChatListItem` list is not cached: it depends on filter, tag context,
  preview lines and per-chat state that the revision does not cover, so a
  stale hit would be easy to get wrong.
- Custom emoji lookup (`src/emoji/custom_stickers.rs`).
  `custom_emoji_stickers` is now `CustomEmojiStickers`: it derefs to the same
  slice, and changes go through `update`, which rebuilds the id index. The
  index points at the first item with an id, which is what the old
  `.iter().find` returned. `lookup(id)` replaces the scans in message text,
  the bubble header, the reaction chip glyph and poll option emoji. The media
  viewer caption only changes its argument type to the new list type.
- Chat rows: `rows_selecting` returns early when nothing is checked, and
  `media_display_roots` returns a shared `Rc<[PathBuf]>` (also stored that way
  in the per-frame cache and in `HistoryShared`).

`src/ui/app.rs` changes by more than the hotspot allowance because the return
type of `media_display_roots` changed; the commit carries the trailer.

## How verified

- Tests compare the cached folder order and the emoji lookup with an oracle
  computed from the chats / a linear scan, after inserts, reorders, removals,
  full position sets, update batches and cache trimming.
- Screenshot demo captures in light and dark: a folder tab (Work) listing its
  three chats in folder order, and inline custom emoji in message text. The
  folder capture used a local, uncommitted fixture patch; it cannot show
  timing, and no performance measurement was taken.
