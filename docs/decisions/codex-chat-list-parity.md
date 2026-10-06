# Chat list and header parity: Saved Messages, group senders, custom emoji

Found comparing Quill and Telegram Desktop on the same account.

- **Saved Messages.** tdesktop shows your own chat as "Saved Messages"
  with a bookmark avatar and no presence line. Quill showed your name,
  photo and "last seen". `Session::is_saved_messages` (the private chat
  whose id is your user id, known since the `my_id` option fix) drives
  the row and the header; `saved_messages_avatar` draws the bookmark on
  the accent color.
- **Group sender prefix.** tdesktop previews group messages as
  "Dad: text" / "You: text", with the name in the accent color; channels
  and private chats show the text alone. Quill only named senders in the
  3-line style, and named the *group* (not the sender) for incoming
  messages. `ChatLastMessage` now records the sender;
  `Session::chat_preview_sender` resolves it to the user's first name (or
  the sending chat's title, or "You").
- **Custom emoji in previews** rendered as their fallback character
  (often a box). Preview custom emoji are now resolved and downloaded like
  the open chat's, and the preview line draws them, animated when decoded
  (`preview_emoji_images`). They show even with the rich (formatting)
  preview off: they're content, not formatting.

Verified live.
