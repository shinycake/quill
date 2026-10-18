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
  General as "# General", each with its custom emoji icon), whether the list
  loaded, the badge and the jump target.
- `chat_row/topic_titles.rs` draws the second line: the titles as elided text
  (the last text piece shrinks and ends in an ellipsis) with bold, coloured
  unread titles and the topic icons drawn as pictures before their titles,
  through the same still and animated emoji path as the preview line. The icon
  ids join the custom emoji the driver resolves. "Loading…" and "No chats" come
  from lang keys.
- The jump chip is two parts on the same fill, as `FillJumpToLastBg`: the unread
  topic's title on the topics line and the message line with the arrow at its
  end. A click on either part opens that topic (Ctrl/Cmd-click: a new window).
  The press is not swallowed, so pinned drag and the long-press peek start from
  the chip; only the click stops, so the row does not open the chat again. The
  chip is plain while selecting (the row toggles) and is not drawn on the open
  row. Nothing else on the line is clickable.
- The badge follows `IncludeInBadge::UnmutedOrAll`: unmuted unread topics, or
  all of them (grey) when every one is muted; a forum marked as unread with no
  unread topic keeps its dot. `chat_row/row_badges.rs` holds the counters so
  they can sit on either line.
- Topic lists load from the update hook (`maybe_preload_topics`) for forums in
  a chat list: on a new chat, on any chat position, on `updateSupergroup` with
  `is_forum` (the chat id is derived from the supergroup id, no scan), and once
  when the account becomes ready for forums that arrived earlier. A forum is
  marked asked only after its request went out; a TDLib error is final
  (`forum_topics_failed`) and the row reads "No chats". Bots with topics are
  preloaded as before.
- `note_forum_topic_message` also advances the topic's `last_message_id`, so
  the titles reorder at once.
- UI tests run on a temp app root: `new_with_demo` calls
  `use_isolated_app_root` under `cfg(test)`, so no test reads or writes the
  owner's real settings, and a test fails if a window resolves the real root.

## Deviations

- The two chip parts are separate rounded rectangles; tdesktop joins them with
  inverted corner pixmaps where their widths differ.
- The gap between titles is an en space plus a thin space, about 8px at the
  row font.
- Unread titles that sit after a topic icon keep their bold and colour; a
  title whose icon picture has not downloaded yet shows without it.
- Hidden topics are filtered out; tdesktop's `topicsList` handling of a hidden
  General was not confirmed.

## How verified

- State tests: order, unread flag, "# General", `forum_rows_follow_display_as_forum`
  (row lines, loaded, unread topics, jump target), the unmuted-or-all badge,
  the dot and the icon ids sent for resolving. A unit test checks the piece
  layout with an icon.
- Driver test: topics are requested when a forum in a chat list appears or
  enters one, once, not after an error, not for a forum in no list, and later
  when the account was not ready.
- GPUI tests (`chat_row/topic_titles_tests.rs`): the forum row is exactly the
  three-line height and the plain row keeps two, the topics line sits between
  name and message and stays in the row, the badge is on the topics line and
  counts topics, a click on the line opens the chat without a topic, either
  chip part opens the last message's topic, a press on the chip starts the
  long-press peek, and in selection mode both toggle the row. A guard test
  checks that UI test windows use a temp app root.
- Light and dark captures of `ready-forum-row-topics` were viewed: date,
  topics line with the jump chip, ellipsis, badge, "Loading…" and the message
  line.
- Not verified: the preload against a real account.
