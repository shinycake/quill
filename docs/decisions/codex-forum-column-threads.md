## codex/forum-column-threads (2026-10-09)

Parity items: `forum-second-column` (done), `thread-section` (not claimed, see below).

### What tdesktop does
- `Dialogs::Widget::showForum` (`dialogs/dialogs_widget.cpp`) puts the forum's topics where the chat list was, under a top bar with the forum name and a back arrow. The main column shows the chosen topic.
- `RepliesWidget` (`history/view/history_view_replies_section.cpp`) shows a message's replies as a section with its own composer, a reply count in the header and a bar for the root message.

### What changed
- Topic column. When the open chat is a forum shown as topics, the topic list is a 280 px column between the chat list and the conversation. The pane on the right asks you to choose a topic until you pick one. The selected topic is highlighted, and switching topics no longer needs a trip back to the list.
- Narrow windows. Below 1100 px the column replaces the chat list, as in tdesktop. A back button in the column header brings the chat list back; choosing a chat there ends that. The decision lives in `Session::forum_column` (`Hidden`, `Beside`, `Replacing`) so it is testable without a window.
- Threads in topics. A thread opened from a topic remembers the topic. Sends and typing stay in the topic (`messageTopicForum`) and reply to the thread root, where before they were rerouted to `messageTopicThread`. Replies to replies join the open thread, since forum messages carry the topic, not the thread. Leaving the topic or choosing another closes its thread.
- Only methods already in `schema/td_api.tl` are used (`getMessageThread`, `getMessageThreadHistory`, `sendMessage`).

### Not done
- `thread-section` is not claimed. The section itself existed already; the forum-topic path is covered by state and driver tests and a demo, but I could not check it against a live forum, and `messageThreadInfo.reply_info` may be null for forum threads.
- The chat list does not animate into the topic column.
- The column width is fixed.

### How verified
- Tests: layout choice, view-as-messages, topic switching, thread topic memory, nested replies, closing on topic change, request routing for forum topics, and a driver flow.
- Demo `ready-forum-column` with `QUILL_DEMO_FORUM_COLUMN_VIEW=topics|topic|thread`, captured and looked at. English fixtures, no account used, nothing posted.
- The narrow layout was not captured; the demo window is wide.
