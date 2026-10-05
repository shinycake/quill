# Core TDLib follow-ups

This pass covers the follow-ups listed under "Deliberately left out" in `codex-core-tdlib-correctness.md`. The scope is only the core library (`--no-default-features`). The references are the TDLib 1.8.67 schema (`schema/td_api.tl`) and Telegram X (TGX-Android/Telegram-X, a read-only shallow clone; paths below are relative to `app/src/main/java/org/thunderdog/challegram/`). Every fix has a test that failed first, except item 3. Item 3 is a performance change, so its new tests pin down existing behavior and passed both before and after. There are no UI, dependency, README or DECISIONS changes.

## Fixed

1. **`viewMessages` marked too much as read.** After every history page and every new message, the driver sent every loaded row of the open chat with `force_read: true`. Rows the user never saw had their channel views counted and their mentions and reactions read. The new behavior:
   - **Before the UI reports anything** (in each open generation), only the newest sent row is due. A chat opens at its bottom, and viewing that row with `force_read` reads the history up to it. This matches TGX `telegram/Tdlib.java` `markChatAsRead`, which views `chat.lastMessage`. Older rows loaded by paging are never viewed just because they were loaded.
   - **After the UI reports**, only reported rows are viewed. The UI reports through `ConnectDriver::view_messages(chat_id, &[MessageId])`, which goes through `Session::report_visible_messages`. This matches TGX `component/chat/MessagesManager.java` `viewMessages(byScroll)` → `telegram/TdlibMessageViewer.java` `Viewport.viewMessages` / `viewMessagesImpl`, which views only on-screen rows.
   - **Delivery.** A report made while a `viewMessages` is in flight waits in `HistoryState::visible` and goes out when the request is answered. After a TDLib error or a failed send, the reported ids stay due.
   - **Tests:** `connect::tests::chat_state::view_messages_reports_only_what_is_shown` and `state::tests::messages::reported_visible_messages_survive_a_view_error`. `tests/replay_messaging.rs` now expects only the newest id on open.
2. **Topic views went stale.** These updates reached only `histories`: `updateMessageContent`, `updateMessageEdited` (reply markup), `updateMessageInteractionInfo`, `updateMessageIsPinned`, `updateMessageContentOpened`, `updateMessageEphemeralContent` and `updatePoll`. The topic view reads only `topic_histories`. They now apply to every loaded copy of the message through `Session::edit_loaded_message`, which covers the main history plus any loaded topic of that chat. The update carries no topic id. The now-unused `HistoryState::update_*` setters were removed. Test: `state::tests::stories::per_message_updates_reach_loaded_topic_histories`.
3. **Linear per-ingest scans (performance; behavior unchanged).**
   - **`updatePoll`** used to scan every row of every loaded history. It now uses `Session::poll_messages`, an index from poll id to `(chat, message)`.
     - **Filled by:** `upsert_message` (live messages, history pages and around-jumps), send succeeded/failed rows, topic pages, search-jump rows, and content edits that produce a poll.
     - **Pruned by:** `apply_update_poll`, when a row no longer holds the poll.
     - **Test:** `state::tests::messages::update_poll_reaches_polls_loaded_by_every_path`.
   - **`maybe_download_chat_list_photos`** used to walk every chat and user on every ingest. Avatars are now reference-counted in `avatar_file_refs`, which is updated where chat and user photos are set (`updateNewChat` / `chat`, `updateChatPhoto`, `updateUser`). An avatar goes into `avatar_downloads_due` when it is newly set or when its file or download state changes (`upsert_file`, `unstick_download`).
     - **Draining:** `Session::take_due_chat_list_photos` drains the due set.
     - **Full pass:** one full scan runs for a new `Session` and after any authorization change, since request invalidation drops pending downloads and `Closed` clears files.
     - **Paused or inactive:** while data saver is on or the chats path is inactive, nothing is drained, so due avatars go out later.
     - **Failed sends:** ids whose `downloadFile` send fails go back into the due set.
     - **Tests:** `connect::tests::messaging::chat_list_photos_download_whenever_they_become_due` (avatars that arrive before Ready, data saver toggled, and a completed avatar evicted from the cache), plus the existing dedupe and stall tests.
4. **List paging never restarted.** `chats_exhausted`, `archive_chats_exhausted` and `folder_chats_exhausted` now reset on `authorizationStateLoggingOut` / `Closed` and when Ready is entered. TDLib leaves Ready only through LoggingOut/Closing/Closed. The driver's existing `became_ready` → `loadChats` (main list, then archive after its 404) therefore pages again after a re-login within one `Session`. Test: `state::tests::requests::chat_list_paging_restarts_after_logout_and_login`.
5. **`updateChatLastMessage` with `last_message: null`.** The existing handling was already correct. TDLib sends null when the last message "became unknown" (td_api.tl:10504). TGX `telegram/Tdlib.java` `updateChatLastMessage` stores the null and still applies positions, and `data/TGChat.java` then draws an empty preview. Quill clears the preview, its style, sender and `ChatSummary::last_message`, and keeps the positions. The related bug was in edit handling. The preview followed `updateMessageContent` only when the edited id was the newest *loaded* history row. So an edit to the last message of a never-opened chat left a stale preview. And after a null last message, an edit to a loaded row re-invented one. The preview is now keyed on `ChatSummary::last_message`, as in TGX `data/TGChat.java` `updateMessageContent`. Ephemeral-content edits use the same rule. Test: `state::tests::chat_list::edits_refresh_the_preview_only_for_the_chats_last_message`.

## UI hook for visible-message reporting (not wired yet)

```rust
// After the open chat's history list lays out, when a scroll settles, and
// when a new row scrolls into view:
let visible: Vec<MessageId> = /* ids of rows currently inside the viewport */;
driver.view_messages(chat_id, &visible)?; // Ok(None) when nothing new is due
```

- **Which chat:** pass `session.open_chat`. Reports for any other chat are ignored. For an open forum topic, pass the forum chat id and the visible topic rows.
- **What to send:** sending the full visible set each time is fine. Rows that were already viewed or are in flight are filtered out.
- **Until it is wired:** the core keeps viewing the newest message on open and on each new incoming message. That is right while the view sits at the bottom. A user scrolled up still has new messages read, which is the remaining gap the hook closes.

## Deliberately left out / suspicious

- **`chat.last_message` from `updateNewChat` / `chat` objects is not parsed.** A chat whose first appearance carries its last message has no preview until an `updateChatLastMessage` arrives. TGX keeps it from `updateNewChat`. This is worth a follow-up: parse it in the chat envelope and set the preview when the chat is new.
- **The topic view uses `messageSourceChatHistory`, not `messageSourceForumTopicHistory`.** The auto newest-row view also uses the chat's main history even while a topic is open. This behaved the same before this pass.
- **The ephemeral-content preview refresh still needs the row in the main history.** The new ephemeral payload alone does not give the preview.
- **The avatar index only sees reducer paths.** Code that writes `session.chats` / `session.users` directly bypasses `avatar_file_refs`. Only demo / test fixtures do that, and they do not rely on automatic avatar downloads.
- **Resetting the exhaustion flags does not clear the previous account's chat rows.** In practice `LiveConnect` builds a new `Session` per connection, so this reset is defensive.
- **`edit_loaded_message` and the poll update still iterate the `topic_histories` keys.** This is bounded by the number of loaded topics, not messages.

## Validation

`cargo fmt --all -- --check`, `cargo clippy --no-default-features --all-targets --locked -- -D warnings` and `cargo test --no-default-features --locked` pass. None of this was exercised against a live account.
