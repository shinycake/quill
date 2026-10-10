# Composer field with formatting, mention tags and custom emoji

Branch `codex/composer-input`. Parity items `composer-wysiwyg`,
`composer-mention-tags` and `composer-custom-emoji`.

## What Telegram Desktop does

Read from lib_ui `ui/widgets/fields/input_field.cpp` and tdesktop
`chat_helpers/message_field.cpp` and `chat_helpers/field_autocomplete.cpp`.

- The field is a `QTextEdit`. Its document is `TextWithTags`: plain text plus
  tags on character ranges (`**` bold, `__` italic, `^^` underline, `~~`
  strikethrough, `` ` `` code, a link address, `mention://<user id>…`,
  `custom-emoji://<id>`). `PrepareTagFormat` draws them: a bold or italic
  font, link colour for links and mentions, monospace in the code colour for
  code. The markers are not in the text.
- Typing the closing marker of `**text**` (`processMarkdownReplaces`)
  replaces the markers with a tag when the text between them is not empty, has
  no space at either end and stays on one line, and the opener is not inside a
  link (`://`) or an open code span. Typing then continues untagged
  (`cursor.setCharFormat(_defaultCharFormat)`), and Backspace right after the
  replacement brings the markers back (`_reverseMarkdownReplacement`).
- Text typed inside a tag joins it. Text typed at the end of a tag continues
  it only over letters and digits (`breakTagOnNotLetter`), so a space ends it.
- A format shortcut with a selection toggles the tag over it: removed when the
  whole selection has it (`HasFullTextTag`), added otherwise. With nothing
  selected (`toggleCurrentMarkdownTag`) it changes what the next typed text
  gets.
- Picking a member without a username from the `@` list (`insertTag`)
  replaces the typed `@query` with their first name tagged
  `mention://<id>…`, then a space. It is sent as `MentionName`.
- A custom emoji is an object in the document (`kCustomEmojiFormat`), drawn
  inline and animated, one character for the caret.
- Undo, copy and paste carry the tags (`application/x-td-field-tags`).

## What Quill had

The composer was a kit `Textarea` holding markup. Bold showed as `**bold**`, a
mention as `[Ann](tg://user?id=…)` and a custom emoji as
`![😀](tg://emoji?id=…)`. `composer::parse_format_markup` turned the markup
into entities on the send path. The markup did not nest.

## Decision: patch the vendored input engine

Two ways were open.

1. A custom element. Quill would own a new text field: layout, caret,
   selection, IME, undo, clipboard, accessibility, and the bidi work of
   `codex-rtl-composer`. That is the bulk of `gpui-base`'s input (about 33,000
   lines) written again, and every fix in the kit would have to be redone.
2. Extend `third_party/gpui-base`, which Quill already vendors for the
   bidi patch. Its engine has most of the pieces: atomic inline tokens with
   undo history (`inline_tokens.rs`), styled text runs from highlight styles
   (`highlight_lines`), and a wrapper that measures inline objects.

This branch takes the second: it needs far less code and keeps IME, undo and
right-to-left text in the code that already handles them. The patch stays
upstreamable: an input without spans or tokens takes the old code paths.

What was added to the engine (details in `third_party/gpui-base/QUILL-CHANGES.md`):

- Formatting spans (`text_spans.rs`). A span is a tag string over a byte
  range. Spans live in the document next to the text: edits move them with
  Telegram Desktop's rules above, every change records the spans before and
  after it so undo and redo restore them, `InputContent` carries them, and copy
  and paste keep them (JSON metadata on the clipboard item, used only when the
  clipboard text still matches). `set_typing_spans` sets the caret's format.
  `set_span_styler` installs the presentation, a `HighlightStyle` plus an
  optional font family per tag. Overlapping spans combine (bold and italic over
  one word).
- Wrapping with formatting. The wrapper measured every line in the base
  font, so a bold line would have stuck out past the edge. A line that holds
  spans is now measured with their fonts.
- Tokens in right-to-left paragraphs. Token rows were laid out left to
  right only, so a custom emoji in an Arabic or Persian draft would have broken
  the bidi layout. A token row with right-to-left text now takes the bidi path,
  with each token as an object of its measured width in its run's visual order,
  drawn in its own box.

On the Quill side:

- `quill::composer_doc` is the document model: text, spans
  (`ComposerTag`) and custom emoji. It converts to and from markup and TDLib
  entities, and holds the editing rules: toggling over a selection, Clear
  formatting, and the typed-markdown match.
- Markup remains the format for sending, drafts and edits. Changing every send
  path (text, captions, edits, scheduled, split long messages, quick replies)
  to carry entities would be a larger and riskier change, and markup can
  express the document once it nests. The field converts at the boundary:
  `composer_markup` reads it, `set_composer_markup` loads it. The rich editor
  keeps raw block markup in the field, so opening it writes the markup out and
  closing it reads it back.
- The markup parser now nests inline spans (`**a _b_ c**`, a bold word inside a
  link label) and reads `[name](tg://user?id=N)` as `FormatKind::MentionName`,
  which the request builder sends as `textEntityTypeMentionName`. Consecutive
  `> ` lines are one quote. The writer (`ComposerDoc::to_markup`, also behind
  `entities_to_markup` for drafts) keeps spans nested by closing and reopening
  them where they cross. It writes italic as `_` when a `*` is next to it or
  bold is involved, and cuts spans at line breaks, since the parser keeps inline
  spans on one line.
- Custom emoji are inline tokens (`custom-emoji:<id>`, the fallback emoji as
  their text). They are drawn from the same animated cache as message text.
- `composer_field.rs` wires it all into the composer: the styler (the theme is
  read at draw time), the token renderer, format toggles from the menu,
  shortcuts and the link dialog, the code-language box on a code block span,
  mention insertion, emoji insertion, and typed markdown with Backspace to undo
  it. Spellcheck skips code, links and mentions. Range edits (emoji
  replacement, hashtag and emoji suggestions) keep the formatting around them.
  Previously they rewrote the whole value.

## Differences from Telegram Desktop

- `__` stays underline and `*`/`_` italic, as Quill's markup has always meant
  (Telegram Desktop uses `__` for italic). The send path and the field agree.
- Spoilers show as a tinted background, not animated dots. A quote shows a
  tinted background, not a bar.
- A quote applies to whole lines. A code block can't hold other formatting,
  since Telegram drops it.
- The mention tag stores the user id. Telegram Desktop also stores the access
  hash, which TDLib does not need.
- Paste between Quill and other apps is plain text. Inside Quill the tags come
  along; that needs the platform clipboard to keep the metadata, which gpui
  does on macOS. On Linux and Windows paste falls back to plain text when the
  metadata is missing.

## Verification

- Engine (`cargo test -p gpui-base --features test-support --lib` with
  `third_party/gpui-base` added to the workspace for the run; not committed):
  all 1360 tests pass, 15 of them new. They cover the edit rules, undo and redo
  of span changes, content insertion as one step, typing spans, and copy, cut
  and paste with spans and tokens.
- Core (`cargo test --no-default-features`): markup to document to markup for
  every tag, nesting and crossing spans, quotes over several lines, spans
  across line breaks, custom emoji inside formatting, markup to entities and
  back, mention-name entities, toggling and clearing, and the typed-markdown
  rules (`snake_case`, `2*3*4`, `https://x/__y__`, open code spans).
- UI (`cargo test --features demo-capture --bin quill composer_field`, 7
  tests, through a real `Textarea` with the bidi test text system). The caret
  moves one character per arrow across formatted runs, and a selection across
  them is one rectangle. A custom emoji, including one with a skin tone, is one
  caret stop; Backspace removes it whole and undo brings it back. In an Arabic
  paragraph the emoji is drawn in its own box and one Left crosses it. A
  mention tag survives typing before and after it, a deleted letter inside it,
  and undo, and is still sent as `MentionName`. Typed `**hi**` becomes bold and
  undo brings the markers back. An IME composition inside a bold word stays
  bold. Markup loads into the field and reads back unchanged. The existing 31
  `composer_rtl` tests and the whole UI suite (263 tests) pass.
- Demo captures, viewed: `ready-composer-wysiwyg` (English: every format, a
  mention and a custom emoji), `QUILL_DEMO_WYSIWYG=rtl` (Persian draft with
  bold, italic, a custom emoji and a mention), `=wrap` (a long bold line wraps
  inside the field), `=select` (a selection across formats), and
  `ready-code-language` (a code block span with the language box). Screenshots
  are in `docs/decisions/codex-composer-input/`.
- Not measured: there is no performance claim. A draft without formatting
  takes the unchanged layout and wrap paths. A formatted line is wrapped by
  shaping its pieces with their fonts, which costs more than the old
  one-font measure, but only on lines that hold spans.
- Not exercised: a live send to Telegram (no real messages were sent), and
  clipboard metadata on Linux and Windows.
