# Reply in another chat, reply options, emoji pack footer

## What Telegram Desktop does

- `history/view/controls/history_view_draft_options.cpp`: clicking the reply
  bar opens an options box. It shows the message so a part of it can be
  selected as the quote ("Update Quote" when one is set), then three actions:
  Reply in Another Chat (only when the message allows forwarding), Show in
  Chat, and Do Not Reply. A footer reads "You can select a specific part to
  quote."
- `ShowReplyToChatBox`: a "Reply in..." chat chooser. It lists the message
  author first ("Message author"), then the chat list ("Your chats"). Picking
  a chat stores the reply in that chat's local draft and opens it, so the
  reply bar is there when the chat opens. Typed text in the target's draft is
  kept.
- `history_view_context_menu.cpp`, `AddEmojiPacksAction`: at the end of the
  message menu, after a separator, a line says "This message contains emoji
  from **X pack**." with the pack name in bold for one pack, or a pack count
  for several. Clicking it opens the pack(s).
- The Saved Messages tag menu (`ShowTagMenu`) offers Filter by Tag, Add or Edit
  Name and Remove Tag.

## What changed in Quill

- Reply model: `telegram::SendReply` gained `source_chat`. With it set, every
  send builder emits `inputMessageReplyToExternalMessage` (chat_id, message_id,
  quote, checklist_task_id, poll_option_id), otherwise the old
  `inputMessageReplyToMessage`. `ComposerReplyTo` gained `target_chat`, the chat
  the reply is aimed at, plus `into_chat`, `belongs_to`, `send_target` and
  `with_new_quote`. All send paths (text, albums, stickers, GIFs, voice,
  polls, checklists, contacts and locations) now ask `send_target(chat)`
  instead of building a `SendReply` by hand. Some of them used to attach the
  reply without checking the chat.
- Drafts stay same-chat only. A reply aimed elsewhere is not written to the
  source chat's draft and is not restored later. The reply bar is kept only
  while the target chat is open (and back in the source chat); opening any
  other chat drops it.
- Reply bar: a "Reply options" menu with Update Quote, Reply in Another Chat,
  Show in Chat and Do Not Reply (`reply_options.rs`). The title names the
  source chat when it differs from the open one ("Reply to Maya Chen from
  Studio standup").
- Update Quote: Quill has no selectable text inside a bar, so the picker lists
  the message split into lines and sentences (`quote_segments`, UTF-16
  positions, 1024 character cap). One click sets the quote, another click on
  the chosen part clears it.
- Reply in Another Chat: a chooser panel above the composer with search
  (`searchChatsOnServer` through the share box path), the message author and
  the chat list. Secret chats are left out. Picking one opens that chat with
  the reply attached. The message menu has the same row, gated on TDLib's
  `can_be_replied_in_another_chat`; the reply bar row uses a local rule (not a
  protected chat, not secret) because the property is only loaded for the
  menu's message.
- Go To Message: it already existed for search hits and shared media. It is
  now also on pinned list rows. Chat-list previews do not show messages, and
  saved lists reuse search, so nothing else needed it.
- Emoji pack footer: the message menu ends with the footer for messages whose
  custom emoji are loaded. The single pack's title is fetched when the menu
  opens (`getStickerSet`, `RequestPurpose::EmojiPackTitle`). Clicking opens the
  sticker set dialog (the first pack when there are several). Only the
  message source is covered, not reactions, tags or poll options.
- Saved tag menu: checked, Filter by Tag, Add or Edit Name (needs Premium, as
  in tdesktop) and Remove Tag are all in `saved_sublists.rs`; nothing to change.

Only methods and constructors from `schema/td_api.tl` are used, nothing is
platform specific.

## How it was verified

- Unit tests for the request shapes (same-chat, external, with and without
  quote, schema constructors), the reply model, the driver send, quote
  segmentation, option lists, the footer wording and the pack lookup.
- Demo captures with English fixtures: `ready-reply-elsewhere` (the chooser),
  `ready-reply-quote` (the quote picker) and `ready-reply-external` (the reply
  bar in the target chat).

Not verified: a live send of an external reply, since no real messages are
sent from this work, and the footer row in a rendered menu.
