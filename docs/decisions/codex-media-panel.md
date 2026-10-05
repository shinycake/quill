# codex/media-panel — one emoji / sticker / GIF panel, after Telegram Desktop

## Replaces
Three separate pickers that pushed the composer up: the emoji picker (paged 120 at a time,
one kit Button per emoji, "Next/Previous emoji"), the sticker panel (one set at a time as
text-button tabs, all state cloned every render, no virtualization, every animated sticker
playing), and the GIF panel.

## The panel
- A 404×452 popover anchored above the composer's left edge. It occludes the history below
  and doesn't push the layout. The composer's smile button toggles it; Esc closes it. The
  attach menu's "GIFs" opens it on the GIFs tab, and sending a sticker closes it.
- Tabs: Emoji | Stickers | GIFs, plus a close button.
- Emoji and Stickers are virtualized `list`s of sections (header row + grid rows of 9 emoji or
  5 stickers), with a search field on top and a footer strip of section icons that jumps
  `list.scroll_to` to a section. Rows store references (set id + index), not cloned items, and
  rebuild when the session revision, tab, query, recent emoji or premium state changes,
  keeping the scroll position within the same view.
- **Emoji tab:** Recently used (plain + custom), the nine Unicode categories (skin-tone
  variants are left out of the grid), then the installed custom-emoji packs. Custom emoji
  insert `![😀](tg://emoji?id=N)` markup (#367) and are dimmed with a "needs Telegram
  Premium" note for non-premium accounts. Search filters the catalog by name.
- **Stickers tab:** Favorites, Recently used, then every installed set. Unloaded sets
  reserve placeholder cells sized from `stickerSetInfo.size`, so rows don't jump when contents
  arrive. Search goes to TDLib (`searchStickers`). Right-click adds or removes a favorite.
- **Loading:** a rendered header or placeholder asks `ensure_library_sets` for its set (4 in
  flight). Rendered cells ask `ensure_media_files` for their static display file. Cells are
  static images; only the hovered sticker animates (`sticker_image`).
- **GIFs tab:** the existing GIF picker, without its own title and close button when embedded.
- Removed: `emoji_ui.rs`, `emoji_picker_open`/`emoji_category`/`emoji_visible_count`, and the
  old sticker/GIF toggles. The old sticker panel stays only for the Archived stickers dialog.
- Demos: `ready-emoji-panel` (new), `ready-stickers` (Stickers tab), `ready-gifs` (GIFs tab).
