# Laid-out lines without 3 KB of empty space (2026-10-30)

## What was wrong

The real-account soak on Idan's Mac scrolled three long Hebrew media
channels. One pass took Quill from 139 MB to 540 MB (with malloc stack
logging on), and 202 MB of the heap was laid-out message text held by
`src/ui/paragraph_cache.rs`. That cache keeps up to 2,048 paragraphs so
that a scroll frame doesn't shape every visible message again.

Hebrew text is laid out by gpui-base's `BidiParagraph`
(`third_party/gpui-base/src/input/bidi_paragraph.rs`). It keeps a
`ShapedLine` for every visual row (`Row::line`, an `InputLine` whose
largest variant is a `ShapedLine`) and one for every single-direction run in
the row (`Content::Bidi::lines`). GPUI's `ShapedLine` stored its decoration
runs in a `SmallVec<[DecorationRun; 32]>`, 32 runs of 92 bytes inline, so
each one was 2,984 bytes whatever it held. A line rarely has more than one
or two colours. About 137 MB of the 202 MB came straight from those
vectors, roughly 100 KB per cached long post. The glyphs themselves took
about 35 MB.

## The change

`ShapedLine::decoration_runs` keeps one run inline
(`SmallVec<[DecorationRun; 1]>`, the `DecorationRuns` alias in gpui-pre's
`src/text_system/line.rs`). A line with more runs keeps them on the heap:
one small allocation when it is shaped. A cached paragraph is shaped once,
and GPUI's `WrappedLine` already keeps its runs in a `Vec`. Nothing about
how text looks or is measured changes. See
`third_party/gpui-pre/QUILL-CHANGES.md`.

The cache keeps its 2,048 entries: what made them heavy was the empty
space, not their number.

## Measured

On the Mac (release build, the screenshot demo, the same scroll through
1,500 long right-to-left posts with links, two runs each, on main
2785fa858):

| | Before | After |
| --- | --- | --- |
| Footprint after the scroll | 346 to 350 MB | 196 MB |
| Heap | 367 MB | 125 MB |
| Paragraph cache (malloc stacks) | 481 MB | 109 MB |
| Average draw | 6.14 to 6.18 ms | 6.15 to 6.19 ms |
| Frames over 16.7 ms, of about 16,000 | 1,077 to 1,084 | 1,053 to 1,083 |
| Worst frame | 50 to 52 ms | 42 ms |

The cache holds the same number of blocks (about 543,000) in less space,
and captures at the start and end of the scroll are identical byte for
byte, link colours included. The demo redraws the whole window at 60 Hz,
so the frame numbers are a worst case for both builds.

