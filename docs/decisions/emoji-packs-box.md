# Emoji packs box

Settings > Chat Settings > Emoji sets opens a list of the custom emoji packs.

## What Telegram Desktop does

tdesktop has no tabbed box for custom emoji packs. Its Settings > Chat
Settings entry "Choose emoji set" (`lng_emoji_manage_sets`, `Ui::Emoji::ManageSetsBox`)
picks the emoji font (Apple, Google and so on), which Quill covers in
`emoji_style_ui`. `StickersBox` has a custom emoji mode only for a group's emoji
pack (`isEmoji` with a megagroup) and for attached sets. Its tabbed form
(Installed, Featured, Archived) is the stickers box.

So the Quill packs box has no tdesktop screen to copy. It borrows the
stickers box rows (`StickersBox::Inner::Row`): title, `lng_custom_emoji_count`
status line, a remove control on the installed tab and `lng_stickers_featured_add`
on the trending tab, and it opens a pack in the sticker set box like
`boxes/sticker_set_box.cpp` does. Archive, drag to reorder and the unread dot of
featured sets are not done for emoji packs: the stickers box does them for
stickers only, and the emoji list is TDLib's order.

## What changed

- Rows show the first four emoji of each pack (`stickerSetInfo.covers`), 28 px,
  with the downloaded thumbnail or the emoji character until it arrives.
- Installed and Trending tabs (kit `TabBar`), a single search field above the
  list. Typing searches after a 300 ms pause; clearing it returns to the last tab.
- The trending list asks for its next page when scrolled near the end.
- A row opens the pack in the sticker set box (add or remove there). The row's
  X asks for confirmation before removing; Add on a trending row installs.
- Covers download when a list arrives (`ensure_emoji_pack_covers`), once per pack.
- Removed: Search packs, More packs, Download or retry pack, the download-state
  text, the in-box preview grid and the "Dynamic emoji pack order" switch
  (tdesktop has no such setting; the stored preference and its ordering stay).
  The driver code that only served them went too (`preview_emoji_pack`,
  `download_emoji_pack`, `emoji_pack_download_state`, `GetEmojiSet`).

## How it was verified

GPUI UI tests in `src/ui/emoji_sets_tests.rs`, a driver test for the cover
downloads, and /verify captures of `ready-emoji-packs` in light and dark.
`scripts/macos-accessibility-smoke.sh` now checks the row names, the remove
buttons and that no manual control is left.
