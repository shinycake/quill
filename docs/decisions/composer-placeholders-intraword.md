# Composer placeholders and intraword markup

## What tdesktop does

`HistoryWidget::updateFieldPlaceholder` (history_widget.cpp) picks the field
text in this order: editing ("New message…"), the bot keyboard's own
placeholder (keyboard shown, or a force-reply markup), Stars, a forum topic
("Message in {topic}", "Reply in {topic}" with a reply pending), broadcast,
anonymous admin, a bot that makes its own topics ("Off-thread message"),
plain. `replyTo()` falls back to the General topic for a forum that is not a
bot, and also counts the keyboard message of a force-reply markup (`_kbReplyTo`).
A forum without a reply says "Message in General". A topic with no known
title reads "General" for topic 1 and "Topic" otherwise. The topic of a reply
is the replied-to message's own topic root.

`HistoryWidget` is the forum viewed as messages, with no topic open. A topic
opens as a `ChatWidget`, whose compose controls already have a topic root, so
the `!_topicRootId` branch of `ComposeControls::updateFieldPlaceholder` never
fires there: inside a topic, and in a reply thread, the field says "Write a
message…" (or the anonymous-admin text).

Markdown in the input field (lib_ui `TagStartExpression`, `Separators`): a
marker opens only after a separator, line start or text start, and closes
only before a separator or the text end; it may not touch a marker of its
own kind. `~~` has no "good after" characters, so a closing `~~` may touch a
word. So `my_cmd_x`, `a_b_c` and `**x**y` stay plain.

## What changed

- `composer::placeholder`: `PlaceholderFacts` gained keyboard, topic and
  bot-off-thread facts; `ComposerPlaceholder::for_facts` follows the order
  above. Lang keys `lng_forum_message_in` and `lng_bot_off_thread_ph`
  (`lng_forum_reply_in` existed).
- `Session::composer_topic` and `Session::bot_chat_off_thread` derive the
  topic and bot facts from state. `composer_topic` is `None` while a topic or
  a reply thread is open. A reply names the topic in the replied-to message's
  own `topic_id`, falling back to the loaded topic histories, then General.
  A force-reply keyboard counts as the reply in a forum.
- `composer::markup_boundary`: one boundary rule, used by the markup parser,
  clear-formatting and the on-type markdown conversion.
- Departure: marker characters count as separators, because Quill writes
  nested and adjacent spans as markup (`**_x_**`) where tdesktop keeps tags.
- A span that really starts or ends inside a word (toolbar bold on "ell" in
  "hello") would be lost on send under that rule, so the markup writer puts a
  barrier character, U+0010, next to the marker in that case and the parser
  drops it. tdesktop never writes U+0010 into field text (formatting lives in
  tags; the character only appears in lib_ui's separator list as its internal
  text-command char), and the Telegram server replaces C0 controls with a
  space. So it must never reach TDLib: every path that takes the field's
  markup parses it first. That covers send, edit, drafts, polls, business
  links, live story, the rich editor send, the AI tools (now sent as parsed
  text plus entities) and the chat-row and topic-row draft previews. The raw
  rich editor, the business-link editor and the demo echo strip it for
  display, and clear-formatting drops it. `requests::tests_markup_barrier`
  checks the request builders.

## Verified

Unit tests (placeholder choice, topic and bot state, reply topic from the
message and from topic histories, markup cases, barrier handling, request
builders carry no U+0010), UI tests over the `ready-composer-bars` demo for
each placeholder, and light/dark captures of `topic`, `topic-reply`,
`keyboard` and `offthread` modes.
Not verified: real TDLib data, force-reply keyboards in a live chat.
