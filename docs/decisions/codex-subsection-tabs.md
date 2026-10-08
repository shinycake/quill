# codex/subsection-tabs — topic tabs for bots with topics and forums with tabs

## Telegram Desktop behavior (reference, read-only)

- `history/view/history_view_subsection_tabs.cpp`: `SubsectionTabs::UsedFor` is
  `amMonoforumAdmin() || peer->displaySubsectionTabs()`.
- `data/data_peer.cpp`: `displaySubsectionTabs()` = `displayAsForum()` for bots, else
  `useSubsectionTabs()`. For bots `displayAsForum()` needs `isForum()` (TDLib
  `userTypeBot.has_topics`), and when `Data::IsBotCreatesTopics` holds
  (`botInfo && !userCreatesTopics`, i.e. TDLib `allows_users_to_create_topics == false`)
  it also needs a non-empty topic list. `data/data_channel.cpp`: `useSubsectionTabs()` =
  `isForum() && ForumTabs` (TDLib `supergroup.has_forum_tabs`).
  Note: the brief described the empty-list rule the other way round; Quill follows the
  source (`IsBotCreatesTopics` is the `!allows_users_to_create_topics` case).
- Layout modes `SubsectionTabsMode` Top=0 / Left=1 / Bottom=2, saved per peer
  (`subsectionTabsMode(peerId)`); `toggleModes()` cycles Top → Bottom → Left → Top.
- `ui/chat/chat.style` `chatTabs*`: strip height 36px, semibold labels, `windowSubTextFg`
  inactive / `lightButtonFg` active; vertical column 64px wide, 28px topic icon at top 8px,
  10px name text 54px wide below, active bar 8px stroke / 4px radius at the edge.
- Tabs: "All" (the whole chat) first, then topics. Vertical shows the round topic icon
  (letter on `forumTopicIcon.color`, or custom emoji) with the name; General gets a "#".
  Right-click = `Window::FillDialogsEntryMenu` for the topic (section `SubsectionTabsMenu`).

## What changed

- Parsing: `ParsedUser.has_topics` / `allows_users_to_create_topics` (bots only);
  `has_forum_tabs` on `updateSupergroup` / `supergroup` (stored in
  `Session::forum_tabs_supergroups`); `ForumTopic` gains icon color / custom emoji id,
  last message id, last read inbox id and notification settings; new envelope payloads
  `UpdateForumTopicInfo` and `UpdateForumTopic` (previously ignored).
- State (`state/session_subsection_tabs.rs`): `bot_topics`, `chat_has_topics` (forum
  supergroup or bot with topics), `subsection_tabs_used_for` (the tdesktop rule above),
  per-chat mode in `MediaPrefs.subsection_tabs_modes` (`media_prefs.json`; Top is not
  stored), live topic updates (rename/recolor, new topics first, pin, mute, read position),
  per-topic unread counting from `updateNewMessage` (TDLib sends no per-topic unread count
  in `updateForumTopic`; reading up to the last known message clears it), topic header
  ("name" + "N messages" from `foundChatMessages.total_count`), chat-row topic line.
- Driver: `getForumTopics`, `select_topic` and topic history now gate on
  `chat_has_topics`, so bot chats fetch and page topics (sending already used
  `messageTopicForum` via `send_topic`). A bot with topics loads its topic list as soon as
  `updateUser` / `updateNewChat` makes it known (for the chat-row line). The topic action
  gate also admits bot chats (pin/unpin, delete). New `connect/subsection_tabs.rs`:
  mode toggle (persists), "Mark as read" (`viewMessages` on the last topic message with
  `messageSourceForumTopicHistory`), mute/unmute (`setForumTopicNotificationSettings`).
- UI (`ui/subsection_tabs.rs`): the three layouts, toggle button (PanelTop / PanelBottom /
  PanelLeft glyph of the current layout; `PanelTop` registered in `src/main.rs`), "All"
  first, unread pills (gray when the topic is muted), accent text + underline (horizontal)
  or edge bar (vertical) for the active tab, horizontal / vertical scrolling, right-click
  menu: Mark as read (when unread), Pin/Unpin, Mute/Unmute, Close/Reopen (forums with
  `can_manage_topics`), Delete (forums with the right; bots when users create topics) —
  delete is confirmed through the existing group-confirm dialog. No animation.
- Hooks: `conversation.rs` — Top strip under the header, Bottom strip above the composer,
  Left column wraps the history; forums with tabs skip the old topic list and the
  "‹ Topics" strip; the header shows the open tab topic. `chat_row.rs` — one optional
  parameter: the topic names replace the sender line (3-line rows) or the preview line
  (2-line rows) for chats with loaded topics. Forum supergroups without `has_forum_tabs`
  keep the old topic list / pane.
- Demo fixtures: `ready-bot-topics`, `ready-bot-topics-bottom`, `ready-bot-topics-left`.

## Not done

- Drag-reorder of pinned tabs (`setPinnedForumTopics`), Ctrl-click "open in new window",
  custom-emoji topic icons (letter fallback), topic list paging beyond the first
  `getForumTopics` page, the monoforum (channel direct messages) admin case.
- No README parity item covers these tabs, so no parity fragment is added.

## Verification

- Tests: `state::tests::subsection_tabs` (flag parsing, UsedFor gating for plain bot /
  users-create / bot-creates with and without topics / forum with and without tabs, mode
  cycle, topic updates + unread + chat-row line), `connect::tests::subsection_tabs` (bot
  chat fetches `getForumTopics` once, select topic → `searchChatMessages` with the topic,
  mark read, mute, pin; mode cycle persists to `media_prefs.json`), `subsection_tabs` unit
  test for the cycle order, UI unit test for active-tab logic.
- Screenshots (demo-capture, 1300x900): `docs/screenshots/subsection-tabs-top.png`,
  `subsection-tabs-bottom.png`, `subsection-tabs-left.png`.
- Not verified live: real bot topic data, right-click menu actions against the server,
  `updateForumTopic*` delivery for bot chats.

## Live-test follow-up

- Unread counts now come from TDLib only. `updateForumTopic`, `updateForumTopicInfo` and a
  new topic message trigger `getForumTopic` (schema 1.8.67 line 12679) for that topic, and
  its `forumTopic` answer replaces the cached entry. The local "+1 per incoming message"
  guess is gone: own messages, pending sends (temporary ids) and messages read in another
  client skewed it (a topic showed 16 while Telegram Desktop showed none).
- Tabs activate on press (`on_mouse_down`) as well as on click (keyboard). `select_topic`
  also accepts any topic in the loaded list. The click path was verified with synthesized
  input in the demo build (scrolled history, a frame between press and release) and by
  `bot_private_chat_topic_select_sends_exact_request`, which asserts
  `searchChatMessages{chat_id, query:"", from_message_id:0,
  topic_id:messageTopicForum{forum_topic_id}}` for a bot private chat (schema line 3003:
  "A topic in a forum supergroup chat or a chat with a bot").
- Demo capture gained `QUILL_DEMO_CLICK="x,y;s:x,y,dy"` (demo-capture builds only):
  left clicks / scroll-wheel steps at window points before the capture.
- Second live re-test: real clicks switch topics; the earlier failure came from an
  accessibility press that sends no mouse event. Press-to-select is reverted, because
  Telegram Desktop's slider activates on release.
- Badge came back after opening a topic (list 0, then 16 once the topic was open). Topic
  rows were read with `messageSourceChatHistory`, and rows already seen in "All" were
  never read again. So TDLib's read position for the topic stayed behind, and its
  `forumTopic` count was computed from the newly loaded messages. Now:
  - a topic view reads its rows with `messageSourceForumTopicHistory`;
  - selecting a topic clears the chat's viewed set, so on-screen rows are read again as
    topic history;
  - any topic whose read position covers its last message shows no badge, whatever count
    TDLib sends.
  Regression test: `opening_topic_reads_it_and_never_resurrects_a_stale_count`.
- `QUILL_TRACE_TOPICS=1` (temporary) prints every unread-count write with its source:
  `getForumTopics`, `getForumTopic`, `updateForumTopic`, `markRead(local)`.
- Third live trace: the bogus 16 came straight from `getForumTopics`, for topics with
  `last_read_inbox_message_id = 0`. TDLib passes the server's `read_inbox_max_id` and
  `unread_count` through unchanged (`td/telegram/ForumTopic.cpp`). Telegram Desktop never
  shows that count: `RepliesList::setInboxReadTill` clamps the read position to at least 1,
  and `RepliesList::displayedUnreadCount()` returns 0 unless the read position is above 1.
  `Data::ForumTopic::chatListBadgesState()` then shows only a count-less unread mark, for
  bots and joined channels, when the topic's last message is newer than the whole chat's
  read position. `Session::topic_badge` copies that rule, and tabs show the count, a dot or
  nothing. Regression test: `topic_badge_follows_tdesktop_for_unknown_read_position`,
  using the exact traced values.
