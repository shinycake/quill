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
  it was measured at, up to 64 widths (a window or splitter dragged across
  30 or so widths and back finds all of them). Measuring returns the size;
  drawing returns the laid-out paragraph, laid out by the measure before
  it in the same frame. The unconstrained measure is never drawn and keeps
  only its size.
- What a frame lays out stays laid out until that frame is over. The
  cache tells frames apart by window and frame number
  (`Window::frame_number`, `third_party/gpui-pre/QUILL-CHANGES.md`); the
  first request of the next frame settles each wrapped paragraph the last
  one used: the widths it was only measured at let their layouts go,
  unless the frame measured the paragraph without drawing it (a row in the
  list's overdraw, about to scroll in): then the width layout settled on,
  the last one measured, keeps its layout, and only that one.
- Widths ever drawn keep their layouts, so text drawn at several widths in
  one frame (replies quoting the same message, a paragraph in two panes)
  lays nothing out in the frames after, and neither does a return to an
  earlier width: a panel closed, opened, the window maximized and the
  panel closed again, or a splitter dragged back over the widths it just
  crossed. Other windows' frames count like the window's own.
- The laid-out widths of each kind are kept to the old cache's budget,
  2,048: past it, at the end of a frame, the least recently used go down
  to three quarters of it, never one the frame used, as the old cache let
  its least recently used entries go. Their sizes stay. Since widths only
  measured never stay laid out past their frame, the whole budget goes to
  text that was drawn; while nothing is resized, that is one width a
  paragraph.
- One-line texts (chat list rows, quotes, names) also keep the widths they
  were only measured at, as every width was kept before. Most are short; a
  row can measure one at the room it offers, draw it narrower, and draw it
  at that room a few frames later; and a quote drawn from a cached view's
  replay never reaches the cache.
- Two counts: 512 wrapped paragraphs (what the 2,048 width entries held in
  practice, so scrolling back finds the same messages laid out as before)
  and 1,024 one-line texts (chat list rows, quotes, names: what the old
  widths held of them, two each), so a long history can't push the chat
  list out. When one is full, its least recently used half goes, as
  before, but never anything the frame being drawn used. The cache is
  keyed by text, not by chat: a closed chat's paragraphs stay until newer
  ones push them out.
- `BidiLine` (one-line previews) measures through the cache and draws the
  paragraph laid out for the room it was measured in, as it did with its
  own slot.
- gpui-base's `BidiParagraph` sizes its row vectors exactly
  (`third_party/gpui-base/QUILL-CHANGES.md`). Each run's `ShapedLine`
  keeps its copy of the run's text: `ShapedLine::cursor` and `split_at`
  read it, and dropping it saved only 3% (0.5 MB of the 17.5 MB measured
  below, 1.7 KB a post).

A first version of this change let a paragraph's other widths go as soon
as one was drawn, which laid text drawn at several widths in one frame out
again every frame; a second kept at most two drawn widths, which laid
every visible paragraph out again on a return to the third width back.
Now nothing about what is drawn changes, and no paragraph is laid out more
often than before: each width is laid out once, drawing reuses the
measured layout, and a drawn width (up to 64 a paragraph) stays laid out
until the budget, which is the old one, pushes it out.

## Measured

Linux test build (`MALLOC_ARENA_MAX=1`), the demo's group chat with 300
right-to-left posts of about 1,170 bytes (110 words, a number, a link),
16 rows each at the bubble's 404 px. The test brings each row into view
in turn, newest to oldest, then scrolls back and down again, then empties
the cache and reads how much heap that frees (glibc `mallinfo2`).

| | Before | After |
| --- | --- | --- |
| Laid-out paragraphs kept | 1,200 (4 per post) | 300 |
| Heap they hold | 66.2 MB | 17.5 MB |
| Per post | 220 KB | 58 KB |
| Widths laid out more than once | 0 | 0 |
| Layouts, scrolling back, then down again | 0 | 0 |

Most of the cut is the widths nothing drew; a few MB is the vector slack
gpui-base no longer keeps. Every width is laid out once, as before, so
the layouts are the same. Scrolled without resizing, a full cache holds
512 paragraphs laid out once each, about 30 MB of posts this long; after
resizing, the widths drawn on the way can fill the budget, 2,048 layouts,
as many as the old cache held whenever it was full.

How many paragraphs one frame uses, against the two counts: the same
test with 150 chats and 400 short posts and replies, every name, title,
preview and post right-to-left, at the smallest message text (12 px),
scrolled through at three window sizes. The most any frame drew:

| Window | Wrapped paragraphs | One-line texts |
| --- | --- | --- |
| 2560 by 1600 | 15 | 30 |
| 3840 by 2160 | 20 | 42 |
| 6016 by 3384 | 32 | 67 |

So each count holds the paragraphs of more than ten full screens, and the
frame being drawn is never evicted from under itself.

Tests: `src/ui/conversation/paragraph_cache_scroll_tests.rs` scrolls and
resizes a history and counts layouts: one laid-out width per drawn post,
nothing laid out again on a scroll back, each width laid out once while
resizing and none only measured kept, nothing laid out on a return to the
widths before (A, B, C, then A), or on a splitter dragged across twelve
widths and back. `src/ui/paragraph_cache_tests.rs` draws one quote at four
widths in the same frame (cut short or whole), the same quotes in cached
views drawn again one at a time, and one wrapped paragraph in four columns,
and checks the frames after the first lay nothing out; it also checks the
budget lets the least recently used layouts go and never the frame's. The
same-frame tests fail on the first version, the return and drag tests on
the second.

## Not changed

- Glyphs and clusters are each 24 bytes a character and now most of a
  paragraph. Clusters could use 32-bit offsets (16 bytes) or be derived from
  the glyphs on the first hit test; both touch the composer's bidi layout,
  which shares the type.
- `ShapedGlyph::index` is a `usize` in GPUI's public type, read by every
  platform's text system.
