## Scrolling: message text layout, Hebrew wrapping, tooltip hiding (2026-10-12)

- **How it was measured:** a release build with line tables, the new
  `frame-profiler` feature (`QUILL_FRAME_PROFILE=<path>` writes GPUI's
  draw / dirty-to-present / input-latency histograms every two seconds)
  and Instruments' Time Profiler attached while scrolling long chats.
  Main-thread samples were grouped into busy intervals and only the
  intervals of 20 ms or more were aggregated, so the slow frames are not
  averaged away by the fast ones.
- **What the slow frames were doing:** shaping message text.
  `SelectableRichText` and `BidiLine` kept one measured paragraph each,
  but taffy measures a leaf several times per frame (min-content,
  max-content, definite) and the rows are rebuilt every frame, so the
  slot never matched and every frame re-shaped visible right-to-left
  text. Inside a fresh layout, `BidiParagraph::wrap` built a CoreText
  line per word (twice: with and without its trailing space) and per
  character of an over-long word, each with its own font-fallback
  lookup.
- **Changes:**
  - `src/ui/paragraph_cache.rs`: a thread-local cache of laid-out
    paragraphs keyed by text, a fingerprint of the styled runs, font
    size, line height, wrap width and one-line mode (2048 entries; the
    least recently used half goes when full). Both bidi elements measure
    and prepaint through it.
  - `third_party/gpui-base` `BidiParagraph::wrap`: the hard line is
    shaped once and word / grapheme widths are differences of a prefix
    sum of glyph advances (each glyph's advance runs to the next glyph in
    visual order and is credited to the byte it starts at).
  - Hebrew paragraphs spilling out of the bubble: the measured layout
    now has a zero minimum width. Its min-content is the unwrapped line,
    so a right-aligned flex row could not shrink it and it overflowed
    instead of wrapping.
  - Tooltips: gpui-base's `TooltipOverlay::request_hide` kept the
    tooltip for a 300 ms grace period after the pointer left (and after
    a scroll carried the trigger away). It now hides at once; the grace
    period only keeps the "recently shown" state, so a neighbouring
    button still shows its tooltip without the show delay. Telegram
    Desktop's `Ui::Tooltip` hides on leave as well. The history wheel
    listener also dirties the conversation slice, since GPUI's own
    tooltip clear refreshes the window and a cached slice would replay
    its last frame.
- **Result (one owner scroll session each, so indicative):** draw p90
  went from 26 ms to about 5 ms for repeat frames after the cache;
  after the wrap change the share of main-thread time spent in frames of
  20 ms or more fell from 48% to 13%, and wrapping no longer shows in
  them.
- **Left:** most of the remaining slow-frame cost is `shape_row`
  shaping each row's visual runs for the first time (a row that fits on
  one line could reuse the wrap shaping), then gpui's `BoundsTree`
  inserts and memmove.
