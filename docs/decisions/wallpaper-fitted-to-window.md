# A chat's wallpaper is fitted to the window

The chat wallpaper shifted a little on every chat switch in Classic. Classic
painted it behind the history and rendered it at the history's own size. That
size changes from chat to chat: Saved Messages has its tag row, a channel has
a pinned bar and an Unmute bar where a group has a composer. A taller composer,
the info panel and the chat list's width change it too. Everything the
wallpaper draws follows that size. A pattern's tile is scaled to the area's
height, its columns are centred on its width, a gradient spreads over the area,
and a photo is cropped to cover it. So each chat got a slightly different
picture, and for a moment the old one showed stretched to the new area before
the new one was ready.

`wallpaper_cache::Fit` now names what a wallpaper is fitted to:

- `Fit::Window` for a chat's wallpaper (`Slot::History`, `Slot::Window`). The
  picture is laid out over the whole window and rendered at the window's
  size. Classic's history shows its own part of it. Only the window's size
  or scale changes the picture: chats, bars, the composer, the side panel,
  the drawer, the chat list width and density all leave it alone, and
  Classic and Floating show the same picture at the same window position.
  tdesktop fits it to the window's body as well
  (`SectionWidget::PaintBackground`).
- `Fit::Area` for previews and gallery thumbnails, which fit their own box.

Two rules follow:

- A window-wide picture stays in the cache while it is shown (`Keep::Shown`),
  in Classic too. Two windows' pictures can pass the budget together and
  would otherwise retire each other on every paint.
- Only a resize waits `SETTLE` before the new picture renders
  (`render_after`). Another wallpaper (a chat's own, the other theme's) or
  the same one turned renders right away.

Cost: Classic now renders one window-sized picture, as Floating already did,
in place of one per history size. A chat switch that keeps the same wallpaper
no longer renders anything.

Verified with `ready-chats` captures (`QUILL_DEMO_LAYOUT=classic`,
`QUILL_DEMO_BACKDROP=pattern|photo`). The wallpaper pixels at the same window
point are identical across chat A, chat B, the info panel open and a
three-line composer, and they line up in Classic and Floating.
