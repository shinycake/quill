# Laid-out text kept once per message, not once per measured width (2026-10-30)

## What was wrong

`src/ui/paragraph_cache.rs` keeps laid-out right-to-left paragraphs so a
scroll frame doesn't shape every visible message again. It kept them by
text, style and wrap width, up to 2,048 entries. Layout measures a message
paragraph at four widths every frame: unconstrained, its widest line (about
2,000 px for a long post), the room the bubble offers (404 px in the test
below) and one narrower width (318 px). The bubble draws it at the room it
offers. All four were laid out and kept, so three quarters of the cache was
paragraphs nothing drew, and its 2,048 entries held about 512 messages.

After #1246 (`docs/decisions/shaped-line-size.md`) an entry was still about
53 KB on the Mac (55 KB in the test below). Where it goes, per laid-out
width:

- one glyph per character (`ShapedGlyph`, 24 bytes) and one cluster per
  character (`Cluster`, 24 bytes): two thirds of it;
- per row (one per wrapped line) a 208-byte `Row`; per single-direction
  run of a row an 80-byte `Fragment`, a 136-byte `ShapedLine` slot and its
  48-byte `LineLayout`;
- the text four times: in the cache key, the row, the fragment and the
  shaped line;
- vector slack: the fragment and shaped-line vectors doubled as runs were
  pushed, so half their slots were empty.

Wrap boundaries are not stored (rows keep byte ranges), links and entities
are not in the paragraph (the element rebuilds its click ranges every
frame), and a run keeps one decoration run inline since #1246. Selection
and link hover underlines are painted as quads, so they are not in the key;
colours are, so a theme change lays text out again and the old entries age
out.

GPUI's own text caches are not part of it: `LineLayoutCache` keeps the
lines of the current and the previous frame only (`finish_frame` swaps and
clears), and a cached paragraph shares its `LineLayout`s with those maps
rather than copying them.

## The change

- The cache keeps one entry per paragraph (text and style) with the sizes
  it was measured at, up to eight widths. Measuring returns the size;
  drawing returns the laid-out paragraph. A width laid out by a measure
  keeps its paragraph until the paragraph is drawn, which in practice is
  the same frame, so drawing never lays it out a second time. Once drawn,
  the widths it was only measured at let their paragraphs go, and at most
  two drawn widths stay laid out (a panel opened and closed again finds
  both). The unconstrained measure is never drawn and keeps only its size.
- `CAPACITY` is now 512 paragraphs: what the 2,048 width entries held in
  practice, so scrolling back finds the same messages laid out as before.
  When full, the least recently used half goes, as before. The cache is
  keyed by text, not by chat: a closed chat's paragraphs stay until newer
  ones push them out, so it never holds more than 512. A paragraph measured
  but never drawn (a row in the list's overdraw that is scrolled away
  from) keeps its measured widths laid out until then.
- `BidiLine` (one-line previews) measures through the cache and draws the
  paragraph laid out for the room it was measured in, as it did with its
  own slot.
- gpui-base's `BidiParagraph` sizes its row vectors exactly and lets each
  run's `ShapedLine` drop its copy of the text, which painting never reads
  (`third_party/gpui-base/QUILL-CHANGES.md`).

Nothing about what is drawn changes, and no paragraph is laid out more
often: each width is still laid out once, and drawing reuses the measured
layout.

## Measured

Linux test build (`MALLOC_ARENA_MAX=1`), the demo's group chat with 300
right-to-left posts of about 1,170 bytes (110 words, a number, a link),
16 rows each at the bubble's 404 px. The test brings each row into view
in turn, newest to oldest, then scrolls back and down again, then empties
the cache and reads how much heap that frees (glibc `mallinfo2`).

| | Before | After |
| --- | --- | --- |
| Laid-out paragraphs kept | 1,200 (4 per post) | 300 |
| Heap they hold | 66.2 MB | 17.4 MB |
| Per post | 220 KB | 58 KB |
| Layouts, first pass | 1,188 | 1,188 |
| Layouts, scrolling back, then down again | 0 | 0 |

About 3 MB of the cut is the gpui-base trims (from the vectors'
capacities); the rest is the widths nothing drew. A full cache now holds
at most 512 drawn paragraphs, about 30 MB of posts this long, where its
2,048 widths held about 113 MB.

Tests (`src/ui/conversation/paragraph_cache_scroll_tests.rs`):
`a_scrolled_history_keeps_each_paragraph_laid_out_once` and
`resizing_keeps_two_widths_laid_out` fail on the old cache (160 laid-out
widths for 40 posts), and `scrolling_back_lays_out_nothing_again` checks
that drawing reuses the measured layout and a scroll back lays out
nothing.

## Not changed

- Glyphs and clusters are each 24 bytes a character and now most of a
  paragraph. Clusters could use 32-bit offsets (16 bytes) or be derived from
  the glyphs on the first hit test; both touch the composer's bidi layout,
  which shares the type.
- `ShapedGlyph::index` is a `usize` in GPUI's public type, read by every
  platform's text system.
