# Streaming bot replies (tdesktop + Zed-style reveal)

Bots stream with `sendTextMessageDraft`. Clients receive
`updatePendingMessage` drafts and must show them in place until the real
message lands (schema: replace the pending message with the same
`draft_id` with animation; delete it on any new incoming message). Hermes
streams 15–25 drafts per reply.

Quill parsed the drafts but showed them nowhere useful:
- **Wrong thread key.** The panel looked them up by `(chat, open_topic ?? 0)`.
  Hermes answers within a thread of the private chat (forum_topic_id
  482317), so in the main history no draft ever matched.
- **Wrong place.** When one did match, it rendered as a bordered "Bot reply ·
  generating" box over the composer, not as a message.

Now (`ui/bot_stream.rs`):
- `open_chat_draft` matches the open topic, or in the main history, the
  chat's draft in any thread;
- the draft renders as the bot's next incoming message at the end of the
  history. It's a synthetic `HistoryMessage` with a negative id, so no
  reactions, pins or menu, and it uses the normal bubble renderer (text,
  entities, rich content minus callback buttons). An empty draft shows
  "Thinking…";
- **smooth reveal, like Zed's streaming output:** new text appears over the
  next ~⅓ s (at least 80 chars/s) instead of jumping by draft chunks.
  It's driven by the frame clock, with entities clipped to the revealed
  prefix;
- the history rows key includes the stream state, so drafts and reveal
  frames rebuild the growing row;
- the old box is now only a compact "Stop generating" control, shown when
  the bot allows stopping.

Verified live with the Hermes bot (test prompts labeled as tests). The
reply grows in place, then the final message replaces it.
