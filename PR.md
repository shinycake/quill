Avatars on disk show at once, reply thumbnails from other chats arrive on their own, and the chat list and badge do less work per update
## What was wrong

Seven items from bundle B17 (audit of main at 5072e9662), each checked against main first:

- L02: `updateUser` kept only `profile_photo.small.id`. The file store never learned that TDLib already had the avatar on disk, so each launch claimed it and sent `downloadFile` just to learn the path.
- N99: a reply in the open chat to a message from another chat draws that message from the other chat's loaded history. When that message turned into a photo, no media pass ran (the change was in a chat nobody shows), and the thumbnail pass never collected thumbnails of other-chat originals. The strip's thumbnail waited for an unrelated update in the open chat.
- N100: the tray, icon badge and title memos keyed on `Session::revision`, which every applied update moves, so the badge walked every chat and forum topic after each status, typing or file progress update.
- N101: `forum_topics_unread` walked every topic of each loaded forum on every badge count.
- N102: `flush_chat_order` collected and sorted every chat in the main list and the archive after any batch that moved one chat.
- N103: every `updateChatLastMessage` and `updateChatDraftMessage` dropped every stored folder tab order, even when no folder position changed.
- N104: `updateSupergroup` walked all chats twice (forum flag, member status).

## What changed

One commit per item:

- L02: the parsed user carries the small avatar file with its `local` block, and the `updateUser` reducer stores it in the file store, the way `updateNewChat` and `updateChatPhoto` store a chat photo. A second commit gives two download test fixtures the `localFile` block TDLib always sends.
- N99: an `updateMessageContent` counts as shown when a shown chat's row replies to that message from another chat, and `reply_thumb_file_ids` includes the thumbnails of other-chat originals loaded in their own chat's history.
- N100: a separate `ChatListState::unread_revision` keys both memos. `Session::apply` moves it for every payload except user status, chat action, read outbox, online member count, file and file download updates, unknown payloads and web proxy relay data. Local reads and unread marks move it through `Session::touch_unread`. This needs one line in `session_apply.rs`, a hotspot, with a `Hotspot-change` trailer.
- N101: `threads.forum_topics` is now a `ForumTopicsMap`. It keeps each forum's counted unread and recomputes only the forum whose topics changed. It hands out no mutable access, so every write goes through `insert`, `remove`, `clear`, `update` or `update_topic`, and none can skip the sum. `topic_badge` and the sum share one rule (`topic_unread_count`).
- N102: position updates record which chats moved in the main list or archive. Closing a batch removes those chats and inserts each at its sorted place. A batch that moves more than an eighth of the chats (the first loads) still sorts once, and the pinned reorder and its rollback keep the full sort.
- N103: a folder's stored order is dropped only when a chat's position in that folder changes, and only for that folder.
- N104: `updateNewChat` records the chat of each supergroup, and a supergroup with no such record uses the id TDLib derives (`-1000000000000 - supergroup_id`). The brief asked for the derived id alone. Over 200 test fixtures give a supergroup chat the supergroup's own id, so the derived id alone breaks about 30 tests. The index fixes those without changing fixtures.

## What ran

- `cargo fmt --check`, and `cargo clippy -D warnings` for `--features demo-capture --all-targets` and for `--no-default-features --all-targets`, after merging main.
- The full test suite with `demo-capture` after merging main: lib 6858 passed and 1 failed, the UI binary 2466 passed and 1 failed, and every integration and doc target passed. Both failures are tests that make a directory 0555 and then expect writes to fail, which does not happen as root (`atomic_write_failure_keeps_old_file`, `a_log_out_that_cannot_be_recorded_keeps_the_passcode_and_says_why`).
- New tests:
  - L02: an `updateUser` whose avatar is already downloaded gives its path with no download claim. Fails without the reducer change (the path is `None`).
  - N99: a cross-chat reply target that turns into a photo gets its thumbnail downloaded in the same batch. Fails without the fix. Removing either half of the fix fails it too.
  - N100: user status, typing and file progress leave the unread revision alone, and a read moves it. Removing any one of those three from the skip list fails the test.
  - N101: the kept forum sum equals the per-topic badges after a `getForumTopic` answer and a local read.
  - N102: 300 random batches of moves, ties, leaves and rejoins on 200 chats give the same main and archive lists as a full sort. A wrong insert position fails it.
  - N103: a positions set that repeats a folder's order keeps both folders' stored orders, and a move in one folder drops only that folder's order.

## What did not run

- No timing. No B17 item gives a number to measure against, and I ran no before/after benchmark. The gains above are argued from the code, not measured.
- N102, N103, N104 and N101 are performance fixes. Their tests show the result stays correct (N103 also shows the stored order is kept), but a test of what the user sees cannot fail without a performance fix.
- N104 has no new test. The existing supergroup tests cover the index path. Only a chat that is inserted directly, without `updateNewChat`, takes the derived-id path, and TDLib always sends `updateNewChat`.
- No app run on macOS, Windows or Linux, and no `cargo check` for Windows. None of the changes have platform branches.

## What stays open

- The N100 skip list is hand-kept, like the emoji list from #1297. A missing frequent kind costs a recount. A listed kind that turns out to feed the badge would leave the badge stale until the next update.
- `ForumTopicsMap::update` recomputes the changed forum's sum over its topics. That costs one forum's topics per change, not every forum per badge count, but it is not a running delta.
