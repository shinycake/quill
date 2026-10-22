# Topic chooser for drops on a forum row

README: `parity:chatlist-drop-forum-topic` (forum half only, see Deviations).
Builds on `drag-forward-to-chat-row.md` and `drag-media-to-chat-row.md`.

## What Telegram Desktop does

`Dialogs::Widget::dropEvent` hands a file or forward drop on a forum row to
`ShowDropMediaBox` (`window_peer_menu.cpp`), which shows a `PeerListBox`
driven by `ChooseTopicBoxController`: title "Forward to…", a search field
over the topic names (word-prefix match), the forum's topics in list order,
"No topics found." for an empty search, and a Cancel button. A bot with
topics adds an "All Messages" (or "Create New Thread") row for the chat
itself. Choosing a row runs `filesOrForwardDrop(thread, data, forumResolved
= true)`: the thread opens, files go to its send box, messages wait as the
forward draft. Cancel or closing the box does nothing. A monoforum
administrator gets the same box over its sublists (`ChooseSublistBoxController`).

## What changed

- `src/ui/chat_list/drop_topic.rs`: the chooser, a kit `Dialog`
  (`DialogKind::DropTopic`) with a search field, one button per topic (closed
  topics carry a lock) and Cancel. State is `chat_list.drop_topic`
  (chat, what was dropped, the search field); the rows are derived from the
  session on every render, so topics that arrive later appear.
- `quill::drop_topic` holds the pure search (every typed word starts a word
  of the name, case and punctuation ignored).
- `drop_on_row.rs`: a file drop and a forward drop on a forum row open the
  chooser instead of the "open the forum" note. Choosing a topic opens the
  chat, selects the topic, then attaches the files (a single folder as one
  archive) or puts the messages in the forward bar. The refusals tdesktop
  checks first (chat takes no files, slow mode, secret chat) still apply.
  The tail of the forward drop moved into `set_forward_bar`.
- Forwarding into a topic: the topic is bound when it is picked.
  `ForwardDraft.topic_id` carries it into `forwardMessages.topic_id =
  messageTopicForum`, so switching topics before Send does not move the
  forward and the ordinary Forward box and share box behave as before.
  `load_forum_topics` asks for a forum's topics when the drop lands on a chat
  that is not open; a request that cannot be sent, or a TDLib error for it
  (`threads.forum_topics_failed`), shows "Could not load the topics." instead
  of "Loading".
- A refusal (slow mode, secret chat, no files) or a topic that vanished while
  the box was open shows a note and keeps the box open. "All Messages" on a
  bot chat that is open in a topic leaves that topic. The box closes when its
  forum is removed, left or the account changes (tdesktop closes it on
  `forum->destroyed()`): `drop_topic_alive` keeps the dialog from showing, and
  the poll that applies TDLib updates (or the next choose) drops the state.
  Render changes nothing.
- Strings: `lng_forum_all_messages`, `lng_forum_create_new_topic`,
  `lng_topics_not_found` under their tdesktop names; the two forum notes
  were replaced by direct-messages ones.

## Deviations

- Direct messages groups: Quill has no sublist (conversation) model for the
  administrator's view, so a drop there still gets the earlier note, not a
  chooser. A member's direct messages group is an ordinary chat and now takes
  the drop like one (before it also got the note). The parity row says
  "forum (or direct messages)", so it stays unchecked.
- The box is a dialog with ghost buttons, not tdesktop's userpic rows; topic
  icons and unread counts are not drawn.
- A topic list that has not arrived shows "Loading…" and, once it has, "No
  topics found." A forum with more topics than the first page lists the
  loaded ones only (no load-more).
- Closed topics are listed with a lock, as in tdesktop; the hidden General
  topic is not listed.

## Verified

Unit tests (`quill::drop_topic`), a driver test (`forwardMessages` carries the topic
of its draft, not the open one), GPUI tests
(`chat_list/drop_topic_tests.rs`: the drop lists topics and opens nothing,
choosing opens the forum in the topic with the file attached, Cancel changes
nothing, typing narrows the list, an unloaded forum lists none, plain chats
still take the drop, a forward drop waits in the bar of the chosen topic and
Cancel leaves it alone, a vanished topic keeps the box open, All Messages leaves the open topic,
the box closes with its forum, administrator and member direct messages), the full
gate, and light and dark captures of `ready-drop-topic`.
Not shown: a live TDLib forum (the demo has fixture topics), the real
`forwardMessages` round trip, real OS file drags.
