# Elastic chat list and pull-to-expand stories

README parity id: `chatlist-elastic-overscroll`.

## What tdesktop does

- `dialogs_widget.cpp` makes the list a `Ui::ElasticScroll` and sets
  `setOverscrollTypes(stories ? Virtual : Real, Real)`: the bottom always
  stretches and springs back (`Real`); the top is a virtual pull when the
  stories strip is shown.
- The pull side follows the finger closely (`kPullBandStiffness` 2.5, against
  20 for a plain bounce). The pull's height drives
  `Stories::List::setExpandedHeight`. Once it passes `kExpandAfterRatio` 0.72
  of the strip's height (55 px) the strip expands (`requestExpanded`) and
  stays expanded (`storiesToggleExplicitExpand`, 200 ms). A shorter pull
  springs back.
- Momentum (a fling hitting the top) never expands it (`_expandIgnored`).
- The strip collapses when the list scrolls a row past where it expanded,
  or when the bottom overscrolls. A tap on the collapsed stack expands it.
- Discrete wheel steps (`Qt::NoScrollPhase`) never stretch the list; only
  phased trackpad events and touch do. lib_ui has no reduced-motion switch.

## What changed

- The chat list is wrapped in the kit's `ScrollBounce` on every platform
  (the kit enables it on macOS only by default). Reduced motion turns it
  off, as in the kit. Line-based wheel input does not bounce.
- `third_party/gpui-base` gains `ScrollBounce::displacement`, which reports
  the edge's offset and whether a finger holds it, so the pull can be told
  from a fling. Nothing else in the engine changed.
- `quill::stories_pull` (pure) holds the state and decisions. The strip now
  rests collapsed (stack beside the search field), as in tdesktop. A held
  pull of 55 px or more expands it: it becomes the first list row and
  slides in over 200 ms. A shorter pull, or a thrown edge, does nothing.
  While pulling, the strip shows in the gap above the list and the stack
  fades. At the top the bounce uses a closer-following motion (tracking
  0.5, stretch up to 96 px); elsewhere the kit's default.
- The strip row is dropped once scrolled out of sight (the offset shifts by
  its height, so nothing moves on screen). Tapping the stack expands it.

## Deviations

- tdesktop collapses a row's height (62 px) past the expand point; Quill's
  strip scrolls with the list, so it collapses once fully out (77 px).
- Bottom overscroll does not collapse the strip (it is long scrolled away).
- A list shorter than its viewport cannot stretch (the kit resets it), so
  the strip cannot be pulled open there; the stack still expands it.
- The pulled-open strip shows its lower part first, as the list slides down.
- Where reduced motion is on, only the stack tap expands the strip.
- The default look changes: the strip is collapsed until pulled or tapped.

## Verified

Unit tests (`stories_pull`) and UI tests with synthetic phased scroll
events and the test clock (`stories_pull_ui_tests`): stretch and return at
both ends, short pull no expand, long pull expands and slides in, fling no
expand, line wheel no bounce, reduced motion off, stack tap, collapse with
offset shift. Captures of the `ready-showcase` demo (light and dark) mid-pull
and after an expanding pull. Not verified: a real trackpad, Windows precision
touchpads, Linux X11/Wayland.
