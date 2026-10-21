# Composer placeholders and intraword markup

## What tdesktop does

`HistoryWidget::updateFieldPlaceholder` (history_widget.cpp) picks the field
text in this order: editing ("New message…"), the bot keyboard's own
placeholder (keyboard shown, or a force-reply markup), Stars, a forum topic
("Message in {topic}", "Reply in {topic}" with a reply pending), broadcast,
anonymous admin, a bot that makes its own topics ("Off-thread message"),
plain. `replyTo()` falls back to the General topic for a forum that is not a
bot, so a forum without a reply says "Message in General". A topic with no
known title reads "General" for topic 1 and "Topic" otherwise.

`ComposeControls::updateFieldPlaceholder` has the same topic branch but only
fires when the controls have no topic root of their own (`!_topicRootId`),
and `ChatWidget` always gives a forum one. Quill's main composer maps to the
`HistoryWidget` shape (keyboards, force reply), so the topic text also shows
inside an open topic. If that turns out wrong on real hardware, the fix is
to return `None` from `Session::composer_topic` while `open_topic` is set.

Markdown in the input field (lib_ui `TagStartExpression`, `Separators`): a
marker opens only after a separator, line start or text start, and closes
only before a separator or the text end; it may not touch a marker of its
own kind. So `my_cmd_x`, `a_b_c` and `**x**y` stay plain.

## What changed

- `composer::placeholder`: `PlaceholderFacts` gained keyboard, topic and
  bot-off-thread facts; `ComposerPlaceholder::for_facts` follows the order
  above. Lang keys `lng_forum_message_in` and `lng_bot_off_thread_ph`
  (`lng_forum_reply_in` existed).
- `Session::composer_topic` and `Session::bot_chat_off_thread` derive the
  topic and bot facts from state; the topic of a reply target is found in the
  loaded topic histories. A reply thread has no topic text.
- `composer::markup_boundary`: the boundary rule, used by the markup parser
  and by clear-formatting.
- Departure: marker characters count as separators, because Quill writes
  nested and adjacent spans as markup (`**_x_**`) where tdesktop keeps tags.
- A span that really starts or ends inside a word (toolbar bold on "ell" in
  "hello") would now be lost on send, so the markup writer puts tdesktop's
  own separator character U+0010 next to the marker in that case; the parser
  drops it. Raw rich-editor markup written with the toolbar mid-word has no
  barrier and stays plain, as typed markdown does in tdesktop.

## Verified

Unit tests (placeholder choice, topic and bot state, markup cases), UI tests
over the `ready-composer-bars` demo for each placeholder, and light/dark
captures of `topic`, `topic-reply`, `keyboard` and `offthread` modes.
Not verified: real TDLib data, force-reply keyboards in a live chat.
