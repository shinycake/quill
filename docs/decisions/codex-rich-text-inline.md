# Rich message text: custom emoji flow inline

## Problem

A channel post mixing custom emoji, bold, and links rendered broken
(emoji-led lines pushed right, a lone "." after a link, stray leading
spaces, an emoji floating on its own line).

## Root cause

`inline_paragraph` (src/ui/message_text.rs) shaped a paragraph as one
`StyledText` only when it had no resolved custom-emoji image. As soon as
one emoji image was present it fell back to a per-run `flex_wrap` row:
every run (text span, link, emoji) became its own flex item, so each run
wrapped as an indivisible block (a run's leading space became a line-start
space, a run that did not fit moved whole to the next line, and "\n" inside
a run did not break the row's flow).

## Design

One `StyledText` per source paragraph, always. A resolved custom emoji
(animated clip or still) is one em-space placeholder glyph in the shaped
text (`selectable_text::EMOJI_PLACEHOLDER`, a break opportunity only after
it). `SelectableRichText` receives `InlineEmoji { range, visual }` and,
in `prepaint` (after layout), measures the placeholder with
`position_for_index`, then builds and prepaints the image or the
`anim_layer::frames` element at that box and paints it over the glyph.

- Animation layer: the `Layered` element is prepainted at the inline
  box, so its recorded bounds come from the new positions (the layer is
  captured with `with_layer`, as the element is built outside render).
- RTL: paragraphs with right-to-left text take PR #442's `BidiParagraph`
  path (logical wrapping, visual row order). The placeholder is just an
  em space in that text, so `Laid::range_rects(placeholder)` returns its
  exact visual box and the image is painted there (`emoji_box`); LTR uses
  `position_for_index`. Both feed the same prepaint, so animation-layer
  bounds come from these boxes. Copy uses logical byte ranges from the
  selection projection, which the bidi geometry already supplies.
- A wrap right before the emoji makes `position_for_index` report the
  previous line's end; `emoji_bounds` detects it (y differs) and uses the
  box just before the placeholder's end.
- Links around an emoji keep their click range; hover/cursor, selection,
  spoilers and bold/italic/code are unchanged highlights over one text.
- Copy: each paragraph records which of its emoji lie in the current
  selection (`SELECTED_EMOJI`), and `selected_message_text` replaces the
  placeholders in the copied string, in order, with the entity's own
  characters (as tdesktop does); leftover placeholder characters are the
  footer padding and are dropped.
- Unresolved emoji and hidden spoilers keep their text.

## Tests

`ui::selectable_text::tests` (needs `--features demo-capture`): a
placeholder mid-sentence does not change the visual line count versus a
plain character, and the emoji box sits on its line, inside the column,
at about text size; copying maps placeholders back to the emoji text.

## Screenshots

`ready-custom-emoji` demo now includes a post with this structure
(`rich_post_message_json`). docs/screenshots/rich-text-inline/
`{before,after}-{dark,light}.png`.
