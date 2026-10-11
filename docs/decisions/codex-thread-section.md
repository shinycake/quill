## codex/thread-section (2026-10-11)

Parity item: `thread-section`, reply threads as a full section with a
composer.

### What tdesktop does
- `ChatWidget` in `Mode::Replies` (`history/view/history_view_chat_section.cpp`,
  formerly `RepliesWidget`) shows a message's replies as a section: the root
  message injected at the top with a "Discussion started" divider, a top bar
  titled "N comments" / "N replies" (`setReplies`), its own compose controls
  that reply to the root, and the corner "jump down" button whose badge is
  the thread's unread count (`refreshUnreadCountBadge`).
- It opens at `ShowAtUnreadMsgId`: `RepliesList::loadAround(inboxReadTill)`
  loads the slice around the read position, with an unread bar.
- `RepliesList` (`data/data_replies_list.cpp`) owns the counters: `readTill`
  advances the read position as rows are shown and recounts unread replies
  locally (`computeUnreadCountLocally`: by the loaded list when it reaches
  the newest reply, by subtraction otherwise); the root post's comments bar
  is updated at once (`setCommentsInboxReadTill`); a new reply bumps the
  count (`changeUnreadCountByPost`).
- The context menu of a group message with replies says "View N Replies",
  or "View Thread" without a count (`lng_replies_view` /
  `lng_replies_view_thread`). Channel posts open comments from the bar.

### Audit: what Quill already had
Earlier slices (`codex-comments-threads`, `codex-forum-column-threads`,
`codex-forum-thread-stories`) built most of the section but did not claim
the item:
- Parsing: `messageReplyInfo`, `messageThreadInfo`, `messageTopicThread`.
- State: `ThreadView` (`state/thread_types.rs`, `session_thread.rs`) with
  status, counters, root ids, a reply map, the unread divider anchor and
  the forum topic; live routing of new messages, edits, deletes and send
  results into the thread; `open_chat` drops a thread that lives elsewhere.
- Driver (`connect/threads.rs`): `getMessageThread`, the chat switch to the
  discussion group, `getMessageThreadHistory` paging from the newest page
  back to the root, `route_into_thread` on every send / typing request
  (`topic_id = messageTopicThread`, reply to the root), `viewMessages` with
  `messageSourceMessageThreadHistory`, back navigation.
- UI (`ui/threads.rs`, conversation header): the comments bar under channel
  posts and the replies link under group messages, "View Comments" / "View
  Thread" menu items, header title + "N comments" / "N replies" subtitle,
  back and "View in chat" buttons, the pinned root bar with the info card,
  the loading / failed pane with Retry, deep links, the shared history list
  and composer.

### What was missing
- The thread always opened at its newest page. With more unread replies
  than one page, the unread divider never appeared, and reading started at
  the wrong end.
- No forward window: `has_newer`, newer pages and the jump-to-latest reload
  existed only for the main history. Inside a thread the jump button just
  scrolled, and it cleared the discussion group's unread anchor instead of
  the thread's.
- `unread_count` was set once from `messageThreadInfo` and never moved: the
  jump button's badge stayed stale, `reply_info.last_read_inbox_message_id`
  from `updateMessageInteractionInfo` was ignored, and the post's comments
  bar kept its unread dot until TDLib echoed the read.
- The group menu entry never showed the count.

### Design: the thread is a windowed scope on the open conversation
The existing shape stays: a thread is a view of the discussion chat, the
session's `open_chat` is that chat, and the composer, drafts, menus, typing
and read marks work unchanged. What changed is that `ThreadView::history`
is now one contiguous window with the same contract as `HistoryState`:
- `has_newer`, `newer_failed`, `window_epoch`, `last_message_id` (from
  `reply_info`), `reload_needed` and `stale_pages` join `ThreadView`.
- First page (`ConnectDriver::fetch_thread_history`): an unread thread loads
  around its read position with `getMessageThreadHistory(from =
  last_read_inbox_message_id, offset = -25, limit = 50)` (schema 1.8.68 line
  12231; a negative offset returns that many newer messages too). Otherwise
  the newest page, as before. Older pages are unchanged.
- Newer pages (`fetch_thread_history_newer`, purpose
  `ThreadsPurpose::GetMessageThreadHistoryNewer`): `from` = newest loaded
  reply, `offset = -(50 - 1)`, while `has_newer`; deduped; stops after a
  failure until a jump. `has_newer` is recomputed after each page from the
  newest loaded id against `last_message_id`; an empty newer page closes
  the newer side regardless.
- Live replies (`ThreadView::note_live`): inside a complete window they join
  the rows; while newer replies are unloaded an incoming reply only moves
  `last_message_id` and `unread_count`, and an own send replaces the window
  (root and pending rows stay, `reload_needed`) so the newest page shows it,
  as tdesktop's `finishSending` → `showAtEnd`. The UI's per-frame
  `advance_thread` requests the page.
- Jump to latest (`thread_jump_to_latest`): drops the divider anchor and,
  when the window stops short, resets it (`Session::reset_thread_window`
  marks in-flight pages stale, like `stale_history_requests`) and loads the
  newest page; the scroller lands at the end when it arrives.
- Unread tracking (`ThreadView::read_till`): `Session::report_visible_messages`
  also moves the thread's read position to the newest row on screen and
  recounts exactly like `computeUnreadCountLocally`; the origin post's
  `reply_info.last_read_inbox_message_id` follows, so its comments bar loses
  the dot at once. `updateMessageInteractionInfo` on the root or the origin
  post feeds the same `read_till` (reads from other devices) plus
  `reply_count` and `last_message_id`. The divider anchor stays where the
  thread opened, as in tdesktop.
- UI: `history_message_list` reads a `ThreadWindow` (anchor, count,
  `has_newer`, `window_epoch`) and feeds the same divider / badge / anchor /
  prefetch code the main history uses; `maybe_auto_load_newer` and
  `jump_to_latest_messages` dispatch to the thread driver methods inside a
  thread. The group menu entry reads "View N Replies" / "View Thread"
  (`replies_menu_label`).
- Only schema 1.8.68 methods are used: `getMessageThread`,
  `getMessageThreadHistory` (`offset` now variable), `sendMessage` with
  `topic_id: messageTopicThread`, `viewMessages` with
  `messageSourceMessageThreadHistory`. There is no `updateMessageThread` in
  TDLib; read positions arrive through `updateMessageInteractionInfo`.

### Not done
- No "Discussion started" / "No comments here yet..." service divider after
  the root: the row model has no synthetic service row yet.
- Thread drafts: Quill saves composer drafts for private chats only
  (`accepts_composer_draft`), so `messageThreadInfo.draft_message` and
  `setChatDraftMessage` with a thread topic are not wired. A thread opened
  in the same chat keeps the group's composer text.
- The header keeps the channel or group name as title with "N comments" as
  subtitle (tdesktop titles the bar with the count).
- Not checked against a live account; no message was sent anywhere.

### How verified
- Reducer tests (`state/tests/threads.rs`): opening around the read
  position with `has_newer`, a live reply held outside the window, local
  read-till by subtraction and by loaded rows, the newer page closing the
  window, an own send replacing the window with stale pages dropped and the
  pending row kept, the post's comments bar losing its dot, server read
  marks from `updateMessageInteractionInfo`, the menu label.
- Driver tests (`connect/tests/threads.rs`): the first-page shape
  (`from = 44, offset = -25, limit = 50`), newer paging (`offset = -49`,
  deduped), jump to latest (newest page requested, stale forward page
  dropped, no request when already at the newest), `viewMessages` still
  read as thread history; the existing comments flow updated for the
  around-the-read-position first page.
- Request shape test for the `offset` argument.
- Gate (fmt, clippy with and without `ui`, tests), `check-hotspots.sh`,
  `check-file-size.sh` pass. No screenshot: the thread demo fixtures were
  removed with the gallery, and the view reuses the main history's list,
  divider and jump button code.
