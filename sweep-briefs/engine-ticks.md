# Sweep bundle engine-ticks

## engine-2: Every tick with any update walks all chats, the open history and the custom emoji cache
- Severity: P2
- Kind: cpu
- Platforms: macOS, Windows, Linux (shared code)
- Where: `src/connect/core/mod.rs:816` calls `sync_open_chat_custom_emoji` for every envelope, `src/connect/core/batch.rs:55` sets `emoji_dirty` with no filter, `end_batch` (`batch.rs:42`) then runs `resolve_open_chat_custom_emoji` (`batch.rs:61`). That reaches `open_chat_custom_emoji_ids` (`src/emoji/mod.rs:462`), `maybe_resolve_message_custom_emoji` (`src/connect/emoji_sets.rs:249`, which builds a `HashSet` of the whole cache through `resolved_custom_emoji_ids`, `src/emoji/mod.rs:596`) and `custom_emoji_files_for` (`src/emoji/mod.rs:613`).
- Trigger: any envelope at all: `updateFile` progress, typing, a user going online, a read receipt. Also on background accounts, where `poll_background` calls `end_batch` although nothing shows those chats.
- Effect and size: one pass does this for each tick:
  - all messages of the open chat, including a full `reply_header` per reply just to read `background_emoji` (`src/emoji/mod.rs:523`, `src/state/session_reply.rs:621`): quote trim, preview string, sender name, all allocated and dropped
  - every chat in `session.chats` (`src/emoji/mod.rs:537`), with `chat_badge_inputs` twice per chat, each a user or supergroup lookup
  - every forum topic list, every shared media item
  - sort and dedup, a `HashSet` of up to 4000 ids, a walk of up to 4000 stickers

  Inferred: 1 to 1.5 ms per tick at 5000 chats and a 1500 row group, about 0.5 ms on a background account. At 10 to 30 ticks per second on a busy online account (guess) that is 1.5% to 4.5% of the main thread, all the time, and it grows with account size. On background accounts it also resolves stickers and downloads custom emoji at `PANEL_DOWNLOAD_PRIORITY` for rows nobody sees.
- Why it is real: `docs/decisions/sweep-perf-update-scans.md` moved this from per envelope to per batch, but a batch is one tick, and with pokes a tick is often one envelope. The only gate is `emoji_dirty`, which every envelope sets.
- Fix sketch: three small cuts.
  1. Do not set `emoji_dirty` for payloads that cannot add an id: a short deny list of the frequent kinds (`UpdateFile`, `File`, user status, chat action, read inbox and outbox, unread counters, online member count). Anything not named still dirties, so a missed case costs time, not a missing emoji.
  2. Skip the pass for an account not on screen, and run it once when the account comes forward.
  3. Replace `reply_background_emoji_ids` with a lookup of the reply target's sender only, and build `resolved` from the existing `CustomEmojiStickers` index.
- Proof plan: extend `src/connect/tests/update_batch.rs`: a batch of only `updateFile` and status envelopes runs no pass (a test-only pass counter stays put) and sends nothing; a message with a new custom emoji still resolves it. Demo measurement: main-thread busy share over a scripted stream, before and after.
- Scope: `src/connect/core/batch.rs`, `src/connect/core/mod.rs`, `src/emoji/mod.rs`, `src/state/session_reply.rs`, `src/ui/background_accounts/mod.rs`. Low to medium risk: a stale deny list would delay an emoji until the next message.

## engine-3: The open chat's media and reply passes run per envelope, not per batch, for updates of any chat
- Severity: P2
- Kind: cpu
- Platforms: macOS, Windows, Linux (shared code)
- Where: `src/connect/core/mod.rs:171` (`thumbs_after`), `:846` runs `maybe_download_shown_thumbs` and `maybe_download_open_chat_media`; `:90` (`view_after`), `:721` and `:843` run `maybe_view_open_messages` and `maybe_fetch_replied_messages`. The passes: `map_thumbs_to_request` and `map_thumb_file_ids_to_download` (`src/state/map_thumbs.rs:113`, `:133`), `message_effect_ids_in_view` (`src/state/domains/stickers/message_effects.rs:56`), `thumb_file_ids_to_download` (`src/state/session_files.rs:294`), `reply_thumb_file_ids` (`src/state/session_reply.rs:558`), `auto_download_media_file_ids` (`src/state/session_files.rs:570`), `reply_fetch_candidates` and `reply_story_candidates` (`src/state/session_reply.rs:479`, `:594`). `maybe_download_shown_thumbs` (`src/connect/media.rs:50`) repeats the first group for each chat window.
- Trigger: any `updateFile` or `file` answer for any file, any `updateNewMessage` or `updateMessageContent` in any chat, and every envelope of any kind while a sticker, GIF or emoji panel is open or sticker suggestions show (`:846-850`).
- Effect and size: 5 to 8 full walks of the open history per envelope, up to 1500 rows, plus `should_download` per candidate. Inferred: 100 to 200 us per envelope at 300 rows, 0.5 to 1 ms at 1500 rows with media, so one 8 ms tick applies only 8 to 80 envelopes during a file storm. A file downloading at 20 `updateFile` per second adds that cost 20 times a second with no change in what the passes find.

  A worse case: `thumb_file_ids_to_download` and `custom_emoji_files_to_download` (`src/state/session_files.rs:394`, `:146`) still use `.iter().find` over `custom_emoji_stickers`, up to 4000 entries, per custom emoji entity, although `CustomEmojiStickers::lookup` (`src/emoji/custom_stickers.rs:24`) is O(1). The scratch replica walks 248-byte entries at about 1 ns each, so a miss costs about 4 us. A 1500 row chat with two emoji per message then spends about 6 ms in that one pass (inferred), which is nearly the whole tick budget, on every `updateFile`.
- Why it is real: `begin_batch` only postpones chat order and custom emoji. These passes sit in the body of `ingest`, and the `thumbs_after` match does not look at which chat or file the envelope concerns.
- Fix sketch: add `shown_media_dirty` and `shown_views_dirty` to `BatchState` (`src/connect/core/batch.rs`), set them where `ingest` runs the passes today, and run each once in `end_batch`, exactly as `emoji_dirty` works. Outside a batch they run at once, so other callers keep their behavior. Ignore `updateNewMessage` and `updateMessageContent` for chats not shown. Replace both `.iter().find` calls with `lookup`.
- Proof plan: in `src/connect/tests/update_batch.rs`, a batch of 100 `updateFile` envelopes runs the pass once (test-only counter) and still sends the same `downloadFile` set as running it per envelope. Unit test that `lookup` and `find` give the same file for an entity list.
- Scope: `src/connect/core/mod.rs`, `src/connect/core/batch.rs`, `src/state/session_files.rs`. Low risk. `end_batch` returns a `Result` both pollers already handle.

## engine-4: Background accounts tick per envelope instead of every 40 ms, and each tick walks all their chats for the badge
- Severity: P2
- Kind: cpu
- Platforms: macOS, Windows, Linux (shared code)
- Where: `src/ui/background_accounts/mod.rs:396` waits on the shared `quill::wake::UPDATES`, so `BUSY` (40 ms, `:370`) is only a ceiling. `poll_background` (`:420`) runs `AccountUnread::of` (`:466`, `src/notify_accounts.rs:109`), which runs `badge_count` up to twice, each running `forum_topics_extra` (`src/tray/mod.rs:128`) over all chats for each list. `list_badge` (`src/tray/mod.rs:148`) adds one more walk while totals are missing. The same uncached walk runs in `chat_window_title` (`src/ui/chat_window/mod.rs:595`) on each chat window render.
- Trigger: a busy channel on an account that is not on screen. Every envelope of any account also wakes the main loop, which does a tick of fixed work for nothing.
- Effect and size: the note in the code says this loop is "slower than the screen's". In practice, with a stream, it ticks once per burst of envelopes rather than at 25 per second. Each tick clones the proxy list, walks all chats 1 to 3 times (`effective_muted` plus `forum_topics_unread` per chat, about 150 to 300 us per walk at 5000 chats, inferred) and repeats the engine-2 pass. Inferred: 1 to 2 ms per tick per busy background account, so 100 envelopes per second costs 10% to 20% of the main thread where a 40 ms pace would cost at most 5%. An account with no forums pays the same walk for nothing.
- Why it is real: `sleep_or_wake` ends on any poke, and no code keeps ticks apart. `Signal` is process-wide. `BadgeCache` (`src/tray/badge_cache.rs`) is keyed on `session.revision`, so it does not help here, and `poll_background` does not use it.
- What tdesktop does: the unread state is kept incrementally per list and forum (`Dialogs::UnreadState`), updated when a chat changes, not recomputed by a walk.
- Fix sketch: the engine-1 helper gives the 40 ms gap, with the first envelope after idle still handled at once. Then make `forum_topics_extra` iterate `session.threads.forum_topics.keys()` and look each chat up, which is O(forums with loaded topics) and zero when there are none. Use the cached count in `chat_window_title`.
- Proof plan: a test that N pokes inside 40 ms of scheduler time produce at most two `poll_background` ticks. A unit test that `forum_topics_extra` gives the same count as today on a fixture with forums, muted chats and archived chats.
- Scope: `src/ui/background_accounts/mod.rs`, `src/tray/mod.rs`, `src/ui/chat_window/mod.rs`. Low risk. Depends on engine-1.

## subsystems-12: The tray and dock badge recount every chat four to six times per update batch
- Severity: P3
- Kind: cpu
- Platforms: macOS, Windows, Linux
- Where: `src/ui/notifications/mod.rs:472-475` pokes `wake::CHROME` for each poll with updates. `src/main.rs:432-449` has no throttle (the menu loop at `src/ui/os_menus/mod.rs:408-409` has a 1 s one) and runs `sync_tray` and `sync_icon_badge`, each calling `total_unread`. `src/notify_accounts.rs:109-122` computes `badge_count` once or twice, and each `list_badge` (`src/tray/mod.rs:148`) runs `forum_topics_extra` (`:128`) over all chats. `poll_background` adds the same per ingest for each background account.
- Trigger: a busy account with many chats; the work is on the UI thread.
- Effect and size: inferred 2 to 3 passes per `of`, doubled by the two callers. At 4000 chats and about 50 ns per chat, that is about 1 ms per wake, so about 1% of the UI thread at 10 wakes per second. Forums with many topics add `topic_badge` per topic. Small for most accounts.
- Why it is real: `sync_with` dedupes the OS push, not the count, and `CHROME` is poked per batch.
- Fix sketch: compute `total_unread` once per wake and pass the value to both syncs. Throttle the loop to about 4 per second as the menu loop does, keeping the first update prompt. Better, skip the pass unless an unread-affecting update applied (a session revision bumped by the unread and forum reducers).
- Proof plan: a counting test: a wake with an unchanged revision does no pass. A test that the badge still lands within one wake after a change, driven by the epoch and not by a timer.
- Scope: `src/main.rs`, `src/tray/mod.rs`, `src/icon_badge.rs`, `src/notify_accounts.rs`. Low risk. May overlap the idle-wakeups lane.
