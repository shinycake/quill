# Forum rows show the latest topic titles

Parity id: `chatlist-forum-topic-titles`.

## What tdesktop does

- `dialogs/ui/dialogs_topics_view.cpp` draws the topic titles of a forum row on
  the second line. Each topic is a piece of the line; an unread topic is drawn
  in the bold, unread colour and a read one in the plain one (`TopicsView::
  paint`, `Ui::TopicsView::jumpToTopic` for the click). A click on a title opens
  that topic (`topicJumpRipple`, `dialogs_inner_widget.cpp`).
- `data/data_forum.cpp` `Forum::reorderLastTopics` picks the topics to show: the
  most recently active ones first, hidden General skipped, at most
  `kShowTopicNamesCount` = 8.
- The titles load with the chat list, before the forum is opened.

## What changed

- `Session::chat_row_topics` replaces `chat_row_topic_names`. It returns up to
  eight `RowTopic { id, name, unread }`, ordered by the topic's last message
  (newest first), hidden topics and nameless ones skipped. `unread` is the
  same rule the topic tabs use for their badge (`topic_badge`).
- `src/ui/chat_row/topic_titles.rs` draws the line: each title is its own
  element, unread titles semibold in the foreground colour, read ones in the
  muted colour, separated by commas. A click selects the chat and then the
  topic; it does not reach the row's own click. The 3-line row layout keeps
  its plain text sender line (`topics_text`).
- `ConnectDriver::pump_forum_row_topics` asks for the topic list of forums in
  the first 40 chats of the main list, four per poll and each forum once
  (`ThreadsState::forum_row_asked`), so a failed answer is not retried each
  tick. Before this, a forum row showed titles only after the forum was
  opened (bots with topics were already fetched).
- Demo `ready-forum-row-topics` (two forums, one with unread topics).

## Deviations

- A title that does not fit is clipped at the edge of the line; tdesktop
  elides it with an ellipsis.
- tdesktop also shows a jump chip and a loading state while the topics load;
  Quill shows the normal preview until they arrive.
- The order is by last message id, not by `adjustedChatListTimeId`.

## How verified

- Unit test of the order and the unread flag (`state/tests/subsection_tabs.rs`);
  GPUI tests that the row lists the latest topics and that a title click
  opens the chat on that topic (`chat_row/topic_titles_tests.rs`).
- Light and dark captures of the demo were viewed: unread titles are bold, read
  ones muted, and the line clips at the row edge.
- Not verified: the fetch pump against a real account (no forum was available
  here); it uses the same `getForumTopics` request as opening a forum.
