## codex/rtl-composer (2026-10-08)

Right-to-left (Hebrew, Arabic) typing in the message composer. The report:
"the text should start from the right and the cursor should move left as the
user types."

### What Telegram Desktop does (read from source)

- `lib_ui/ui/widgets/fields/input_field.cpp` is a `QTextEdit`. It never sets a
  layout direction on the document: every block keeps Qt's default
  `Qt::LayoutDirectionAuto`, so each paragraph (text between `\n`) takes the
  direction of its first strong character (UAX #9 rules P2/P3). A paragraph
  with no strong character falls back to the application direction (LTR).
  `PrepareBlockFormat` sets the block alignment to `Qt::AlignLeft` without
  `AlignAbsolute`, which Qt reads as "start edge": the right edge of an RTL
  paragraph. Mixed runs (digits, Latin words) are ordered by the bidi
  algorithm inside the paragraph; the caret starts at the paragraph's start
  edge and advances against the reading direction; arrows follow Qt's visual
  cursor movement (Left moves visually left, which in an RTL run is forward
  in typing order).
- The placeholder keeps the UI direction (task brief), so it stays left in an
  LTR UI; here an empty composer is not a bidi paragraph, so it is untouched.
- `lib_ui/ui/text/text.cpp` (`StringDirection`, `computeParagraphDirection`)
  does the same per-paragraph "first `DirL`/`DirR`/`DirAL` character" test for
  message text (no strong character: `LayoutDirectionAuto`, the UI direction).
  Quill already right-aligned RTL bubble paragraphs
  (`quill::text::is_rtl_text`, `message_text.rs`).

### What GPUI does, per platform

Read from `gpui-pre-0.3.7` and its platform crates; macOS was exercised with
demo captures, Linux and Windows were reasoned from source (not run).

- **macOS (CoreText)**: `layout_line` builds one `CTLine`. CoreText runs the
  bidi algorithm with the base direction taken from the first strong
  character and returns glyphs in *visual* order with visual x positions
  (`string_indices` descending inside an RTL run). So a Hebrew line already
  *looked* right, just left-aligned. A trailing space of an RTL run is
  placed at a *negative* x, outside the width CoreText reports.
- **Linux (cosmic-text)**: `ShapeLine::new` with `Shaping::Advanced`, same
  auto paragraph level via `unicode-bidi`, glyphs in visual order from x = 0.
- **Windows (DirectWrite)**: `IDWriteTextLayout` with the default flow
  direction, i.e. the base direction is always LTR. Hebrew-only text is
  fine (the run is RTL by script) but a Hebrew paragraph with an English word
  or number puts the runs in LTR order, and a trailing space of a Hebrew run
  lands on the wrong side of the word.
- Whatever the platform, `LineLayout::x_for_index`, `index_for_x` and
  `closest_index_for_x` assume glyph indices ascend with x. In an RTL run they
  descend, so **every logical offset inside a Hebrew run mapped to x = 0**:
  the caret was stuck at the left edge, clicks landed at the wrong glyph and
  selections drew nothing sensible. That is the "cursor does not move" half of
  the bug; "text starts at the left" is the other half.
- The kit input has no hooks for this: `InputBaseState::text_align` exists
  but `set_text_align` returns early unless the input is single-line, and it
  is one alignment for the whole input, not per paragraph. `Textarea` exposes
  no direction or alignment option.

### Design

The engine that lays out, paints and hit-tests the composer text lives in
`gpui-base`, a registry crate. Nothing short of changing it can right-align
one paragraph, place the caret inside a bidi line, or draw a selection that is
not one visual interval, so Quill vendors it:

- `third_party/gpui-base` is `gpui-base 0.7.0` copied from the registry
  (`src/`, `tests/`, `benches/`, manifest) behind `[patch.crates-io]`. The
  first commit of this PR is the **pristine copy**; the second is the patch,
  so `git diff` between them is exactly what changed (about 500 lines in six
  files plus one new module). Not patching `~/.cargo/registry`, and no change
  to `gpui-kit` / `gpui-component`. The patch is deliberately upstreamable: it
  only changes behaviour for a paragraph that contains right-to-left text;
  ASCII and LTR-only paragraphs take the unchanged code path.
- New module `input/editor/display_map/bidi.rs` (pure `f32` geometry, 30 unit
  tests, also compiled into Quill's own `cargo test` through
  `src/lib.rs`):
  - `needs_bidi` / `Paragraph::analyze`: per buffer line, `unicode-bidi`
    (`ParagraphBidiInfo`, auto base level = P2/P3, neutral-only -> LTR).
  - `Paragraph::visual_runs(row)`: the runs of a wrapped visual row, left to
    right (rules L1/L2 with the paragraph's own level, so a wrapped row of an
    RTL paragraph keeps its direction). Neutral characters at the edges of an
    RTL run (spaces, punctuation, brackets) become runs of their own, one
    character each, so no platform shaper has to guess their side.
  - The engine **shapes each single-direction run on its own** and places
    the fragments itself. A single-direction run shapes identically on
    CoreText, cosmic-text and DirectWrite, so the layout no longer depends on
    the platform's base direction (the Windows problem above). Lone
    right-to-left neutral runs get their brackets mirrored
    (`unicode-bidi-mirroring`).
  - `clusters_from_glyphs` / `fragment_extent` read caret geometry back from
    the shaped glyphs whatever order the platform emitted them in (clusters,
    fold combining marks into their base, ligatures split the caret evenly,
    trailing whitespace that a platform hangs outside the reported width).
  - `BidiLine`: `x_for_index` (leading edge of the character that starts at
    the offset, logical end at the end of the paragraph),
    `closest_index_for_x` (hit test), `index_for_x`, `range_rects` (a logical
    range is several visual rectangles when it crosses a direction change),
    `visual_step` (the caret stop one step visually left/right).
- `LineLayout` gets the paragraph direction. An RTL paragraph is aligned to the
  right edge row by row (`sub_line_offset`; the old code aligned a wrapped
  paragraph by its longest row), painted right-aligned, its caret rests
  against the right edge like a right-aligned input (no horizontal scroll
  nudge, caret clamped inside), selections use `range_rects` (several paths
  per row when disjoint, a selected newline's cell sits left of an RTL row).
- Arrow keys: in a paragraph with right-to-left content Left/Right/Shift+Left/
  Shift+Right move to the caret stop next to the caret on screen
  (`step_horizontally`); Ctrl/Alt+Left/Right swap in an RTL paragraph; the
  collapse of a selection by Left/Right is mirrored. At the visual edge of a
  row the caret stays put, except at the logical end the arrow points at, where
  it crosses into the neighbouring paragraph. LTR paragraphs are untouched.
- Public API addition: `TextareaState::range_to_rects` (rectangles of a byte
  range). Quill's spellcheck underlines use it (`spellcheck_ui.rs`): they
  used to span from the start to the end position of the word, which in RTL
  text is backwards and across run boundaries is wrong.
- Quill: `quill::text::is_rtl_text` (message bubbles) now uses the same
  `unicode-bidi` first-strong test, so bubbles and composer agree on every
  string (neutral leading characters, RLM/LRM marks, isolates).

### What I did not change

- Placeholder: per UI direction (empty text is not a bidi paragraph).
- Single-line inputs (link address, search boxes): unchanged, the platform
  shapes them.
- IME, copy/paste, formatting, mention/hashtag/emoji popups, the link panel:
  they work on byte offsets and `bounds_for_range`, which now go through the
  bidi-aware layout; no code change was needed. Marked text keeps its
  underline run through the per-run shaping.

### Limits

- Linux and Windows were not run. The design removes the reliance on a
  platform base direction, but the glyph-order/hanging-whitespace handling was
  verified against CoreText only.
- Visual caret movement is per paragraph and per row; word movement
  (Ctrl+Arrow) swaps direction per paragraph but is not run-aware.
- Caret affinity at a direction change: the caret at a logical offset is
  drawn at the leading edge of the character that starts there (Qt's default
  too), so the second visual position of that offset is not reachable by
  keyboard.
- **Message bubbles still mis-wrap long RTL paragraphs** (follow-up, not in
  this PR): `gpui` shapes the paragraph as one line and `compute_wrap_boundaries`
  walks glyphs in visual order, so the first row of a wrapped Hebrew message
  holds the *end* of the sentence (see `after-bubbles.png`). Alignment is
  right. Fixing it means wrapping RTL paragraphs by logical order in Quill's
  `SelectableRichText` path, a separate piece of work.
- `third_party/gpui-base` adds 4 MB of vendored source; dropping it is a
  matter of upstreaming the patch (`gpui-kit` has no RTL support today) and
  bumping gpui-kit.

### Tests

- `cargo test --no-default-features` (gate): `quill::text` direction tests
  (first strong character, neutral-only, digits, marks, Arabic) and the 30
  bidi geometry tests (`vendored_input_bidi`).
- `cargo test --features demo-capture --bin quill composer_rtl` (13 tests,
  needs gpui-kit `test-support`): types Hebrew into a real kit `Textarea`
  through a test text system that shapes like CoreText / cosmic-text (visual
  glyph order) and, for one test, like DirectWrite (forced LTR base); asserts
  caret x decreases per character and the first character sits at the right
  edge, per-paragraph direction, visual order of mixed runs, arrows, click
  hit-testing, IME marked-text composition, wrapping, selection rectangles
  and spellcheck rectangles, and that the layout is identical for both base
  directions.

### Screenshots

`docs/decisions/codex-rtl-composer/`: `before-*.png` / `after-*.png` for a
Hebrew draft, a Hebrew+English+digits draft and a three-paragraph draft
(Hebrew, English, digits), `after-mixed-selection.png`, and
`after-bubbles.png`. Reproduce with
`cargo build --features demo-capture` and
`QUILL_DEMO_RTL=he|mixed|lines QUILL_DEMO_CAPTURE=out.png target/debug/quill --screenshot-demo ready-rtl-composer <dir>`
(`QUILL_DEMO_RTL_SELECT=8..37` adds a selection).
