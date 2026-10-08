# codex/message-context-menu — gap-audit batch 5

Follows Telegram Desktop's `FillContextMenuItems` order and wording (`history_view_context_menu.cpp`).
Sort keys live in `quill::message_menu::order`; the pure planner `media_actions` decides the media rows
per message type and is unit tested.

## Added
- Media rows: Save As..., Copy Image, Show in Finder/Folder, Copy Filename, Cancel Download, Open GIF,
  Add to GIFs, View Sticker Set / Add Stickers (dialog over `getStickerSet`), Add to / Remove from
  Favorites, songs get "Save to..." (Profile via `addProfileAudio`, Saved Messages, Downloads).
  Save As uses the native save dialog. Cancel Upload deletes the pending message (not verified live).
- Report: `reportChat` with message ids and TDLib's option flow (OptionRequired, TextRequired, Ok), a
  dialog with Back; also a "Report N" button in the selection bar. Never auto-sent.
- "N Seen" / "N/M Reacted" / "N Listened" row with avatars, opening a page with reactors (emoji, time)
  and viewers; private chats show the read date (`getMessageReadDate`, all privacy states).
  Lookups chain from the `messageProperties` answer.
- Date lines: "Sent today at 12:34", "Edited ...".
- "Copy Post Link" (channels) vs "Copy Message Link"; the toast says whether the link is public.
- Admin delete box: Report Spam, Delete all from {user}, Ban {user} (supergroups, other members).
- No-forwards note in protected chats; "Go To Message" in the Shared Media list (right click).
- Translate hook: sort keys `TRANSLATE_SELECTED` and `TRANSLATE` are reserved for batch 7.
- Demo: `--screenshot-demo ready-message-menu` with `QUILL_DEMO_MENU=<scenario>`; dialogs open after a
  delay because the kit dialog layer does not exist during `new_with_demo`.

## Left out
- Scheduled "Send Now"/"Reschedule" (batch 10; only the `can_edit_scheduling_state` flag is parsed).
- Attached stickers, fact check, reply options, delete reactions, poll extras, tag menu.
- Edit window limit and Pin/Unpin already follow `messageProperties` (`can_be_edited`, `can_be_pinned`).
- Pin confirm box (notify / also pin for).
- Not exercised live: every report, ban, delete-all, upload cancel.
