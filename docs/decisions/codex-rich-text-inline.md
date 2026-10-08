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
- RTL: paragraphs aligned right (PR #395/#440 path) are handled by
  `glyph_origin`, which adds the wrapped line's alignment slack because
  `position_for_index` ignores `text-align`. Same code path serves LTR.
- A wrap right before the emoji makes `position_for_index` report the
  previous line's end; `emoji_bounds` detects it (y differs) and uses the
  box just before the placeholder's end.
- Links around an emoji keep their click range; hover/cursor, selection,
  spoilers and bold/italic/code are unchanged highlights over one text.
- Copy: the placeholder is stripped like the footer padding, so a copied
  selection omits custom emoji (their fallback emoji is not in the shaped
  text). Known limitation; tdesktop copies the emoji.
- Unresolved emoji and hidden spoilers keep their text.

## Tests

`ui::selectable_text::tests` (needs `--features demo-capture`): a
placeholder mid-sentence does not change the visual line count versus a
plain character, and the emoji box sits on its line, inside the column,
at about text size.

## Screenshots

`ready-custom-emoji` demo now includes a post with this structure
(`rich_post_message_json`). docs/screenshots/rich-text-inline/
`{before,after}-{dark,light}.png`.
