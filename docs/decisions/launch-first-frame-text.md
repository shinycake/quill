# launch-first-frame-text: the first frame's text from the last launch

## Problem
After `launch-warm-waits.md`, the first frame of a launch spent about 20 ms of main-thread
time on text that the last launch had already worked out. A Time Profiler trace of five
cold launches of that build (`scripts/launch-profile.py --cold`, 2026-11-06) charged the gap
before the chat list's first draw with:

| work | ms per launch |
| --- | ---: |
| `CTLineCreateWithAttributedString`: Core Text shaping each line of the chat list | 12 to 14 |
| `CTFontGetBoundingRectsForGlyphs`: each glyph's raster bounds | 5 to 9 |
| the Core Animation commit (AppKit's own) | 12 to 15 |

Glyph bitmaps already come from the last launch (`launch-time-macos-appkit.md`, round 3),
but their bounds were still measured, and every line was shaped again.

The same profile showed when that cache misses: the first launch of a different binary
(its stamp names the executable's modification time) drew every emoji again, 26 ms.
Launch benches that alternate two builds therefore never hit it on either side.

## Decision
- `gpui-pre-apple/src/glyph_cache.rs` keeps each glyph's raster bounds next to its bitmap,
  and `raster_bounds` answers from there for a glyph the last launch measured.
- `gpui-pre-apple/src/line_cache.rs` keeps the line layouts the first frame shaped.
  `layout_line` takes one when the line has the same text, font size and font runs, and
  the text system finds every font the layout draws with under its PostScript name;
  otherwise it shapes the line. A run's font is named by its PostScript name, its OpenType
  features and its fallbacks, so tabular digits and the plain font are different lines.
- The first frame is where a launch first meets most fallback fonts (emoji, other
  scripts, `LastResort`), so they are not loaded yet when a kept layout names them. A
  missing font is opened by its PostScript name: a system font (its name starts with a
  dot, which `CTFontCreateWithName` refuses) from the fallback list of a font the line
  asks for (`CTFontCopyDefaultCascadeListForLanguages`), any other with
  `CTFontCreateWithName`. It is used only when it opens under that name. Before this, a
  debug count on 2026-11-06 found 10 of 125 first-frame lines shaped again, the ones
  with fallback fonts.
- Asking Core Text which font covers the run (`CTFontCreateForString`) finds the same
  fonts, but it reads the character set of each fallback in turn: a warm profile charged
  it 6.5 ms of the first frame, against 0.8 ms to open the same three fonts by name in a
  standalone test.
- A layout holds its line's text: chat names and message previews, which TDLib keeps in a
  database encrypted with the account's key. So the layouts are never written in the
  clear. `src/launch_lines.rs` derives a key from the account's database key
  (HKDF-SHA-256) and seals them with XChaCha20-Poly1305 in
  `accounts/<id>/launch-lines.bin`. The key is derived on the early connection worker, where
  the database key is read for TDLib. With a local passcode, the database key, and so the
  layouts, wait for the passcode. The glyph cache stays in the clear: it holds glyph
  pictures without their order.
- Layouts are recorded from the start of the launch, so a database key read after the
  first frame still saves that frame's lines. Recording stops on the frame after the chat
  list draws, when both caches are written.

## Results
`scripts/launch-bench.py --onscreen --until chat_list_presented`, 2026-11-06, Apple M1 Pro,
macOS 27.2, real account, release builds. Each side ran in two blocks of 6 launches,
in the order main, branch, branch, main, with the bench's warm-up launch left out (see
Measuring). Load average 5 to 15. Times are ms, medians of 12 launches per side; the
bracket is a bootstrap 95% interval of the difference.

| gap inside each launch | main warm | branch warm | main cold | branch cold |
| --- | ---: | ---: | ---: | ---: |
| `chat_list` to `first_display` (the first frame) | 32 | 23 [-13, -7] | 58 | 32 [-27, -23] |
| `window_ordered` to on screen | 60 | 52 [-16, -4] | 103 | 74 [-30, -23] |
| `kit_ready` to `window_ordered` (unchanged code, a control) | 40 | 41 [-5, +6] | 128 | 125 [-10, +3] |

Window on screen after `open`: warm 299 to 289 ms [-20, -3]; cold (`--cold`, `purge`
before each launch) 815 to 773 ms, an interval of [-73, +16] because whole cold launches
drift by about 50 ms between blocks. The gaps inside a launch do not drift that way.

A warm Time Profiler trace (`scripts/launch-profile.py --runs 4 new chat_list`) charged
the first frame 29 ms of main-thread samples on main, 17 ms on this branch. Line shaping
went from about 11 ms to under 1 ms, glyph bounds from about 4 ms to under 1 ms. What is
left is the Core Animation commit (about 2 ms warm) and frames the trace did not
symbolicate.

## Measuring
The caches are stamped with the executable's modification time, so compare two builds in
separate runs of the bench, never alternating in one, and keep each block's warm-up launch
out of the results (the bench does that):

    QUILL_ENV_FILE=…/quill.local.env QUILL_CODESIGN_IDENTITY="Apple Development: …" \
      python3 scripts/launch-bench.py --onscreen --runs 6 --json a.json \
      --template dist/Quill.app base=<main binary>
    … new=<branch binary>, then new again, then base again

`scripts/launch-profile.py --runs 4 new` then shows whether the first frame still shapes
lines (`CTLineCreateWithAttributedString`) or measures glyphs.
