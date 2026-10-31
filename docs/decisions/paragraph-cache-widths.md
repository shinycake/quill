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

- The cache keeps one entry per paragraph (text and style) with every
  width it was measured or drawn at. Measuring returns the size; drawing
  returns the laid-out paragraph, laid out by the measure before it in the
  same frame. The unconstrained measure is never drawn (drawing always
  passes a width) and keeps only its size.
- The cache tells frames apart by window and frame number
  (`Window::frame_number`, `third_party/gpui-pre/QUILL-CHANGES.md`). The
  first request of the next frame settles each wrapped paragraph the last
  one used: a layout at its widest line or wider (the unconstrained width,
  give or take the pixel taffy rounds by) that the frame did not draw lets
  go and keeps its size, if it is wider than the window. A long post's
  widest line, about 2,000 px, is never drawn: a bubble is at most
  430 px wide. A shorter paragraph's widest line that fits the window stays
  laid out: when its bubble grows to fit it (a panel closed, the window
  widened) it is drawn there.
- Every other width stays laid out, drawn or only measured: the room a
  bubble offers, the narrower width it is measured at, a row measured just
  off screen at one width and drawn at another, and every width a window
  or splitter drag crosses. Text drawn at several widths in one frame
  (replies quoting the same message, a paragraph in two panes) and a return
  to any earlier width lay nothing out again. There is no cap per
  paragraph.
- Laid-out widths of both kinds together are kept to the old cache's one
  budget, 2,048: past it, at the end of a frame, the least recently used
  of either kind go down to three quarters of it, never one the frame
  used, as the old cache let its least recently used entries go. Their
  sizes stay.
- Sizes are kept to 8,192 widths of each kind, laid out or not (about 40
  bytes each): past it, the least recently used of those holding only a
  size go down to three quarters of it, never one the frame used, and a
  paragraph left with no width goes. That is more than four times the old
  cache's entries, so a measure finds the size of every width the old
  cache still held.
- One-line texts (chat list rows, quotes, names) keep the widths they were
  only measured at too, as every width was kept before: a row can measure
  one at the room it offers, draw it narrower, and draw it at that room a
  few frames later, and a quote drawn from a cached view's replay never
  reaches the cache.
- Each kind keeps up to 2,048 texts, as many as the old cache's 2,048
  entries could be, counted apart so a long history can't push the chat
  list out. A new text past that and an eighth more lets the least
  recently used go, down to 2,048 with it, never one the frame being drawn
  used: one search of the pool per 256 new texts, not one per text. The
  layout and size budgets bound what the texts hold. The cache is keyed by
  text, not by chat: a closed chat's paragraphs stay until newer ones push
  them out.
- `BidiLine` (one-line previews) measures through the cache and draws the
  paragraph laid out for the room it was measured in, as it did with its
  own slot.
- gpui-base's `BidiParagraph` sizes its row vectors exactly
  (`third_party/gpui-base/QUILL-CHANGES.md`). Each run's `ShapedLine`
  keeps its copy of the run's text: `ShapedLine::cursor` and `split_at`
  read it, and dropping it saved only 0.5 MB in the test below, 1.7 KB a
  post.

## Every way a layout goes, against the old cache

The old cache held up to 2,048 (text, width) entries of both kinds, each
laid out, and when full kept the most recently used half. A layout goes
here only:

1. Unconstrained: never kept. Nothing draws at no width.
2. At the widest line or wider, wider than the window, not drawn in the
   frame that measured it. Drawing it needs the window to grow past it and
   the text's box with it. A bubble's text is at most 404 px wide (430 px
   in the plain look), so in the history that is a window narrower than
   that (the narrowest is 380 px) widened until such a paragraph fits a
   line: it is laid out once more, where the old cache had it.
3. Past 2,048 laid-out widths of both kinds together, least recently used
   first: the old budget.
4. With its text, past 2,048 texts of a kind, least recently used first:
   the old cache's 2,048 entries never held more.

Sizes go only past 8,192 widths of a kind, and only those without a
layout, which keeps every width among the old 2,048.

So once a frame is over this cache holds at most the old cache's 2,048
layouts, plus up to 8,192 sizes of each kind (about 330 KB each) and the
texts' keys.

Earlier versions of this change let a paragraph's other widths go as soon
as one was drawn (text drawn at several widths in one frame was laid out
again every frame), kept two drawn widths (a return to the third width
back laid every visible paragraph out again), then kept 64 widths a
paragraph, laid out or not, and let every width only measured go (a drag
of more than about 30 frames laid everything out again on the way back,
and a row measured just off screen was laid out again when it scrolled in
at that width).

## Measured

Linux test build (`MALLOC_ARENA_MAX=1`), the demo's group chat with 300
right-to-left posts of about 1,170 bytes (110 words, a number, a link),
16 rows each, in a 1,100 px window. Each post is measured unconstrained,
at its widest line (about 2,000 px), at the bubble's 404 px and at 318 px,
and drawn at 404 px. The test brings each row into view in turn, newest to
oldest, then scrolls back and down again, then empties the cache and reads
how much heap that frees (glibc `mallinfo2`).

| | Before | After |
| --- | --- | --- |
| Laid-out paragraphs kept | 1,200 (4 per post) | 600 (2 per post) |
| Heap they hold | 66.2 MB | 37.5 MB |
| Per post | 220 KB | 125 KB |
| Widths laid out more than once | 0 | 0 |
| Layouts, scrolling back, then down again | 0 | 0 |

The cut is the widest line and the unconstrained width, which nothing
draws, and a few MB of vector slack gpui-base no longer keeps. Every width
is laid out once, as before.

The budget is the old one in layouts, not in bytes. The layouts kept are
the wrapped ones, 62.5 KB each for these posts, where the old cache's mix
of wrapped and one-line layouts was 55 KB. Up to 1,024 such posts it
holds two layouts each, where the old cache held four (up to 512 posts).
Past that both hold part of 2,048 layouts: here three quarters to all of
it, about 96 to 128 MB, and in the old cache half to all of it, about 56
to 113 MB. At the budget this cache holds about 13% more than the old one
did.

How many texts one frame uses, against the counts: the same
test with 150 chats and 400 short posts and replies, every name, title,
preview and post right-to-left, at the smallest message text (12 px),
scrolled through at three window sizes. The most any frame drew:

| Window | Wrapped paragraphs | One-line texts |
| --- | --- | --- |
| 2560 by 1600 | 15 | 30 |
| 3840 by 2160 | 20 | 42 |
| 6016 by 3384 | 32 | 67 |

So each count holds the texts of many full screens, and the frame being
drawn is never evicted from under itself.

Tests: `src/ui/conversation/paragraph_cache_scroll_tests.rs` scrolls and
resizes a history and counts layouts: no widest line kept, nothing laid
out again on a scroll back, each width laid out once while resizing,
nothing laid out on a return to the widths before (A, B, C, then A), none
of over a hundred widths a post laid out again after a window dragged
8 px a step across 90 steps and back (the 64-width version laid out 120
again), and no post laid out again when a window widened until its posts
fit a line (the version that let every widest line go laid out 7).
`src/ui/paragraph_cache_tests.rs` draws one quote at four widths in the
same frame (cut short or whole), the same quotes in cached views drawn
again one at a time, one wrapped paragraph in four columns, and one in two
columns dragged a pixel a frame across 64 frames and back (128 widths; 64
laid out again with the 64-width cap); it checks the frames after the
first lay nothing out, what a frame lets go, that each budget and count
lets the least recently used go and never the frame's, and that both kinds
together keep to one budget of layouts (2,304 held with one per kind).

## Not changed

- Glyphs and clusters are each 24 bytes a character and now most of a
  paragraph. Clusters could use 32-bit offsets (16 bytes) or be derived from
  the glyphs on the first hit test; both touch the composer's bidi layout,
  which shares the type.
- `ShapedGlyph::index` is a `usize` in GPUI's public type, read by every
  platform's text system.
