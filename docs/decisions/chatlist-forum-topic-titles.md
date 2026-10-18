# Forum rows show the latest topic titles

Parity id: `chatlist-forum-topic-titles`.

## What tdesktop does

- A chat shown as a forum (`PeerData::displayAsForum`) gets `forumDialogRow`
  (`dialogs/dialogs_row.cpp:407-422`, `dialogs/dialogs.style:119-129`): 80px
  against 62px, with a 21px topics line. The row is name and date, then the
  topics line with the unread badge at its right (`PaintWideCounter` paints at
  `texttop`, `dialogs/ui/dialogs_layout.cpp:1193-1199`), then "Sender: message"
  (`dialogs/ui/dialogs_message_view.cpp:441-461`). The height depends only on
  "shown as forum" (for a bot that makes its own topics: once it has one,
  `data/data_peer.cpp:1709-1717`), so a row never changes height while topics
  load.
- `TopicsView::paint` (`dialogs/ui/dialogs_topics_view.cpp:221-238`) draws the
  titles left to right as elided text, 8px apart (`topicsSkip`), no commas. A
  title that does not fit ends in an ellipsis and nothing follows it. Unread
  titles are bold and coloured (`:89-95`). General reads "# General"
  (`data/data_forum_topic.cpp:164-174`). Before the list arrives the line says
  "Loading…", afterwards, if empty, "No chats" (`:209-219`).
- The topic of the forum's last message goes first; when it is unread, a
  rounded jump chip is drawn behind it (`:99-112`, `FillJumpToLastBg`). That
  chip is the only hit area: a press there opens the topic of the last message
  (`dialogs/dialogs_inner_widget.cpp:3146-3151`); a press anywhere else opens
  the forum. Selection mode, Ctrl-click, drag and the long-press peek act on
  the row as for any other.
- The forum badge counts topics with unread messages, not messages
  (`history/history.cpp:2765-2770`).
- Topics load when a forum enters the chat list or turns into one
  (`data/data_forum.cpp:47-56`, `data/data_session.cpp:5716-5717`,
  `data/data_channel.cpp:255-259`); a failed request marks the list loaded and
  is not retried (`data_forum.cpp:200-206`).

## What changed

- `Session::chat_shown_as_forum`, `chat_row_lines` and `chat_row_forum` decide
  the row. A forum row is always drawn with the three-line height, so the
  virtual list, the pinned-drag heights and the row agree. `chat_row_forum`
  returns the titles (latest message first, at most eight, hidden skipped,
  General as "# General"), whether the list loaded, the number of unread
  topics and the jump target.
- `chat_row/topic_titles.rs` draws the second line: one elided `StyledText`
  with bold runs for unread titles, "Loading…" or "No chats" from lang keys,
  and the jump chip (`forum-jump-<chat>`). The chip is the only clickable part:
  it opens the chat on the topic of the last message, Ctrl/Cmd-click opens a
  new window, and while selecting it is plain so the row toggles its check.
  It is not drawn on the open row. `chat_row/row_badges.rs` holds the counters
  so they can sit on either line.
- The unread badge of a forum row counts unread topics.
- Topic lists load from the update hook (`maybe_preload_topics`): on a new
  chat, a chat entering the main list, and `updateSupergroup` with `is_forum`,
  plus once when the account becomes ready for forums that arrived earlier. A
  forum is marked asked only after its request went out; a TDLib error is final
  (`forum_topics_failed`) and the row reads "No chats". The per-poll walk over
  the first 40 chats is gone.
- `note_forum_topic_message` also advances the topic's `last_message_id`, so
  the titles reorder at once.
- Tests start from default appearance preferences instead of reading the
  machine's saved ones.

## Deviations

- Topic icons: Quill has no custom emoji inside a text line of the list, so the
  custom-emoji icon in front of a title is not drawn (General's "# " is).
- The jump chip surrounds the first title and an arrow, not the title and the
  message line as one two-part shape, and the click area is the chip only.
- The gap between titles is an en space plus a three-per-em space, about 8px
  at the row font.
- Hidden topics are filtered out; tdesktop's `topicsList` handling of a hidden
  General was not confirmed.

## How verified

- State tests: order, unread flag, "# General", `forum_rows_follow_display_as_forum`
  (row lines, loaded, unread topics, jump target).
- Driver test: topics are requested when a supergroup turns into a forum, once,
  not after an error, and later when the account was not ready.
- GPUI tests (`chat_row/topic_titles_tests.rs`): the forum row is exactly the
  three-line height and the plain row keeps two, the topics line sits between
  name and message and stays in the row, the badge is on the topics line and
  counts topics, a click on the line opens the chat without a topic, the chip
  opens the last message's topic, and in selection mode both toggle the row.
- Light and dark captures of `ready-forum-row-topics` were viewed: date,
  topics line with the jump chip, ellipsis, badge, "Loading…" and the message
  line.
- Not verified: the preload against a real account.
