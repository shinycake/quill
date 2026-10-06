# Right-to-left message text (tdesktop)

In a Hebrew chat compared side by side with Telegram Desktop:
- tdesktop aligns each paragraph by its direction, using the first
  strong character (Unicode bidi). Hebrew and Arabic paragraphs are
  right-aligned in the bubble. Quill left-aligned everything, so Hebrew
  lines had a ragged right edge.
- When a message's last line is RTL, tdesktop gives the time its own
  line under the text, because that line ends at the bubble's left and
  can't share its right end with the time.

Now:
- `quill::text::is_rtl_text` / `last_line_is_rtl` (first strong
  character: Hebrew, Arabic and other RTL blocks; unit-tested);
- message paragraphs that read RTL align right;
- a message (text or caption) ending in an RTL line doesn't reserve
  inline footer space, so the footer drops to its own line.

Verified live in the family group against Telegram Desktop.
