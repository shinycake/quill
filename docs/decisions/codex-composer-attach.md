# codex/composer-attach — attach menu and attachment tray

- The paperclip opens a dropdown (Photo, Video, File, Video message, then Poll and GIFs)
  instead of toggling a row of eight labeled buttons. Emoji was dropped from it because the
  stickers button already covers it. Restricted polls show as a disabled item.
- Picked files render as kit `Attachment` cards in a scrollable `AttachmentGroup`. Each card
  has a kind icon, the file name, and a per-file remove control (`remove_attachment`).
- Batch options sit under the cards: "Send as album" and "Remember choice" checkboxes (two or
  more files), a self-destruct timer dropdown (replaces the cycle button), and Remove/Remove all.
- The caption position is a "Caption above media" checkbox rather than a "Caption: below" button.
- `attach_menu_open` is removed. Embedded icons gained Image, Film, File, ChartBar, SquarePlay
  and Timer.
