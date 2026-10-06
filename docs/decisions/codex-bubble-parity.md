# Bubble parity: sender inside the bubble, reactor avatars, time on the reaction row

Found comparing a group chat in Quill and Telegram Desktop.

- **Sender name.** tdesktop paints a group sender's name inside the
  bubble, on its first line, in the sender's color. Quill drew a kit
  `MessageHeader` above the bubble. The name now leads the bubble content;
  bubble-less rows (stickers, round videos, big emoji) keep it above,
  having no bubble to hold it.
- **Who reacted.** tdesktop shows the reactors' userpics instead of a
  count when there are at most three and all are known (not in
  channels). `MessageReaction` now parses `recent_sender_ids`; chips draw
  overlapping 18 px avatars in that case.
- **Time with reactions.** tdesktop keeps the time on the reactions' line.
  The chip row now ends with the footer (time, receipt, views, signature)
  and the bubble drops its separate footer line.

Verified live in a group chat.
