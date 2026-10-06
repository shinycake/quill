# Owner feedback round: menus, viewer, receipts, toasts, paste, previews

Each item below follows how Telegram Desktop handles it (rule: read
tdesktop before designing).

- **Reaction chevron closed the menu.** GPUI delivers a click to every
  hitbox under the cursor down to the first one that occludes. The menu
  panel didn't occlude, so a click on the strip's plain cells also reached
  the full-window backdrop, which closes the menu. The panel now occludes.
- **Viewer zoom/Reset clicks closed the viewer.** Same cause: the bars and
  the media didn't occlude, so the backdrop got the click too. The top bar,
  bottom bar, arrows and the media occlude now. Clicking the dark area
  around the media still closes, as in tdesktop.
- **Read receipts** were text glyphs (`✓✓` drew far apart, `…` while
  sending). The footer now draws icons as tdesktop does: a clock while
  sending, `Check` when sent, the joined `CheckCheck` when read. The
  sending state lives on the bubble.
- **Toasts after every action.** `status_note` is set from ~770 places,
  mostly progress chatter. `status_note_is_toast` lets only failures,
  restrictions (Premium, slow mode, permissions) and confirmations of
  invisible actions (copied, saved) reach the toast; everything else stays
  silent. Unit-tested policy.
- **Channel post menu offered Reply, Pin, Delete…** The menu now asks
  TDLib `getMessageProperties` when it opens (`MessageActions`, a local
  call) and shows each item only when allowed: `can_be_replied`,
  `can_be_copied`, `can_be_forwarded`, `can_be_edited`, `can_be_pinned`,
  `can_get_link`, `can_get_message_thread` and the delete flags. Until the
  answer arrives, channel posts offer only what any reader can do.
- **Emoji-only messages** (the ones drawn big) render without a bubble,
  like stickers; their time takes the theme text color.
- **Pasting an image did nothing.** GPUI stops an action at its first
  handler, so the window's bubbling Paste handler never ran after the
  focused textarea. It is now a capture handler: clipboard images and
  Finder-copied files become attachments; text pastes fall through.
- **Attachments showed only a file name.** Photos and videos are preview
  tiles (the photo itself), files stay compact rows. Dropped or pasted
  files that are all photos/videos attach as media (an album), as in
  tdesktop; any other mix attaches as files.
