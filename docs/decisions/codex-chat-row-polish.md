# Chat list row polish (tdesktop)

Compared row by row with Telegram Desktop on the same account:
- **Multi-line previews.** A channel preview containing line breaks drew
  over three lines and overflowed its row. tdesktop's preview is one
  line. Line breaks now read as spaces (same byte length, so entity
  offsets hold).
- **No ellipsis.** Previews were clipped mid-glyph; tdesktop ends them
  with "…". The text sat in a flex row, where GPUI's ellipsis never
  applies. A plain preview is now a single text node. In a rich preview
  (bold, custom emoji…) the last text run shrinks and takes the
  ellipsis.
- **Time vs weekday.** tdesktop's `FormatDialogsDate` shows the time for
  anything today *or within the last 20 hours*. Quill showed "Mon" for a
  message at 23:09 last night; `chat_list_stamp` now follows the same
  rule (tested across midnight).
- **Badges.** tdesktop's unread-reaction and mention marks in the wide
  list are bare 18px icons (a red heart, an accent "@"; grey when muted),
  not filled badges. Quill now matches.

Verified live against Telegram Desktop.
