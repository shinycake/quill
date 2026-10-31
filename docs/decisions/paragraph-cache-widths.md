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
  drawing returns the laid-out paragraph, laid out by the measure before
  it in the same frame. The unconstrained measure is never drawn and
  keeps only its size.
- What a frame lays out stays laid out until that frame is over. The
  cache tells frames apart by window and frame number
  (`Window::frame_number`, `third_party/gpui-pre/QUILL-CHANGES.md`); the
  first request of the next frame settles each wrapped paragraph the last
  one used:
  - every width the frame drew stays laid out, and earlier drawn widths
    too, up to two in all (a panel opened and closed again finds both);
  - widths it was only measured at let their layouts go;
  - a paragraph the frame measured but did not draw (a row in the list's
    overdraw, about to scroll in) keeps the width layout settled on, the
    last one measured, and only that one.
- One-line texts (chat list rows, quotes, names) keep every width they
  were laid out at, up to eight, as every width was kept before. Most are
  short; a row can measure one at the room it offers, draw it narrower,
  and draw it at that room a few frames later; and a quote drawn from a
  cached view's replay never reaches the cache, so the cache can't tell
  which of its widths are still on screen.
- So text drawn at several widths in one frame, such as replies quoting
  the same message in bubbles of different widths, keeps each width laid
  out, and the frames after it lay none out again. A first version of this
  change let the other widths go as soon as one was drawn, which laid
  such text out again every frame; the frame boundary is what fixes it.
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
- gpui-base's `BidiParagraph` sizes its row vectors exactly and lets each
  run's `ShapedLine` drop its copy of the text, which painting never reads
  (`third_party/gpui-base/QUILL-CHANGES.md`).

Nothing about what is drawn changes, and no paragraph is laid out more
often: each width is laid out once, drawing reuses the measured layout,
and a width a frame drew is still laid out in the next frame.

## Measured

Linux test build (`MALLOC_ARENA_MAX=1`), the demo's group chat with 300
right-to-left posts of about 1,170 bytes (110 words, a number, a link),
16 rows each at the bubble's 404 px. The test brings each row into view
in turn, newest to oldest, then scrolls back and down again, then empties
the cache and reads how much heap that frees (glibc `mallinfo2`).

| | Before | After |
| --- | --- | --- |
| Laid-out paragraphs kept | 1,200 (4 per post) | 300 |
| Heap they hold | 66.2 MB | 17.0 MB |
| Per post | 220 KB | 57 KB |
| Widths laid out more than once | 0 | 0 |
| Layouts, scrolling back, then down again | 0 | 0 |

About 3 MB of the cut is the gpui-base trims (from the vectors'
capacities); the rest is the widths nothing drew. Every width is laid out
once, as before, so the layouts are the same. A full cache now holds at
most 512 drawn paragraphs, about 30 MB of posts this long, where its
2,048 widths held about 113 MB.

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
resizes a history and counts layouts (one laid-out width per drawn post,
nothing laid out twice, nothing laid out again on a scroll back, each
width laid out once while resizing, at most two kept);
`src/ui/paragraph_cache_tests.rs` draws one quote at four widths in the
same frame, cut short or whole, and in cached views drawn again one at a
time, and checks the frames after the first lay nothing out. Those fail
on the first version, which let the other widths go at each draw.

## Not changed

- Glyphs and clusters are each 24 bytes a character and now most of a
  paragraph. Clusters could use 32-bit offsets (16 bytes) or be derived from
  the glyphs on the first hit test; both touch the composer's bidi layout,
  which shares the type.
- `ShapedGlyph::index` is a `usize` in GPUI's public type, read by every
  platform's text system.
