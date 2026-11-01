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
