# Work per update tick

Follows `sweep-perf-update-scans.md`, which moved the open chat's custom
emoji pass from each envelope to each poll batch. With the poke wait a
batch is often one envelope, so that pass and the shown-chat passes still
ran about once per envelope.

## Custom emoji pass

- `batch::adds_no_custom_emoji` names the frequent payloads that cannot
  bring an id into view: file progress and file answers, user status,
  typing, read receipts, list unread counters and the online member count.
  They no longer mark the pass dirty. Anything not named still does, so a
  kind missing from the list costs time, not a missing emoji.
- A background account ends its batches with `end_offscreen_batch`, which
  keeps the pass pending. The first batch after the account comes on screen
  runs it once.
- The reply-strip emoji are read from the reply target's sender
  (`reply_background_emoji`) instead of building a whole `ReplyHeader` per
  reply. `reply_header` uses the same function, so the two cannot disagree.
- Resolved ids come from the `CustomEmojiStickers` index, not from a
  `HashSet` built per pass, and `custom_emoji_files_for` looks each id up
  instead of walking the cache.

## Shown-chat passes

`maybe_view_open_messages` with `maybe_fetch_replied_messages`, and
`maybe_download_shown_thumbs` with `maybe_download_open_chat_media`, ran in
the body of `ingest` for every envelope that might concern them. They now
set `views_dirty` and `media_dirty` in a batch and run once in `end_batch`,
as the emoji pass does; outside a batch they run at once. A new message or
a content change in a chat that is not shown no longer starts them.

One case now waits for the next update: a message in another chat that is
the target of a cross-chat reply in the open chat changes its content. The
reply strip's thumbnail then downloads on the next shown-chat update.

The two remaining linear scans of the custom emoji cache in
`session_files.rs` use `CustomEmojiStickers::lookup`.

## Background account ticks and the forum walk

The 40 ms background gap the finding asked for landed with `poll-pacing.md`.
What remained is the badge: `forum_topics_extra` walked every chat to add
forum topics, once per list and up to twice per `AccountUnread::of`. It now
walks only the chats with loaded topics (`threads.forum_topics`), which are
the only ones `forum_topics_unread` can count, so an account without forums
pays nothing. A chat window's title uses the window title's `BadgeCache`.

## Tray and dock badge

`sync_tray` and `sync_icon_badge` each counted the account on screen on
every `wake::CHROME`. `total_unread` now goes through `UnreadMemo`, keyed
like `BadgeCache` on the session, its revision, its chat count and its
badge preferences. The two syncs of one wake count once, and a wake with
no applied update counts nothing. The count still lands on the first wake
after an update. The throttle the finding suggested would sit in
`main.rs`, a hotspot, and is not needed once an idle wake costs nothing.
