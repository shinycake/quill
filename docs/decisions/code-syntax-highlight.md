# Syntax highlighting in message code blocks

## What tdesktop does

`HistoryItem::setTextValue` calls `Spellchecker::TryHighlightSyntax`
(`lib_spellcheck/spellcheck/spellcheck_highlight_syntax.cpp`) on every message
text. For each `pre` entity that names a language it hashes the block's text
and language (xxHash), looks the hash up in a 256-entry cache and, on a miss,
queues the block to a single worker queue (`crl::object_on_queue`) running
libprisma's `SyntaxHighlighter`. The message draws plain until the worker
finishes, then the history refreshes it. A block with no language is not
highlighted (there is no auto-detection), and a language libprisma does not
know yields no spans. `patch` and `diff` use the `git` grammar.

libprisma's token types fold into eight slots (comment; punctuation;
property, tag, boolean, number, constant, symbol; selector, attribute name,
string, char, builtin; operator, entity, url; at-rule, attribute value,
keyword, function; class name; inserted). `ChatStyle::highlightColors` gives
the slots theme colors (light blue, red, red, orange, red, blue, purple,
green). Messages, document captions and instant-view code blocks highlight.
The composer does not: its highlighter is the spell checker.

## What changed

- `gpui-kit` gets its `syntect` feature (the kit's syntect highlighter with
  `fancy-regex` and the `two-face` syntax set; no `onig`, no C dependency).
  The vendored gpui-component makes `highlighter::capture_names` public; see
  its `QUILL-CHANGES.md`.
- `src/ui/code_highlight.rs`: the kit's highlight names fold into the same
  slots (minus "inserted", which the kit does not name). `lookup` hashes
  (language, text), returns plain for no language and `Pending` plus a job on
  a miss; `start` tokenizes on the background executor, stores the spans in a
  256-entry cache and notifies the window. The syntax set loads on the first
  job, off the main thread. An unknown language is cached as an empty span
  list so it is not tried again.
- `pre_block` draws `StyledText` with the cached spans. Slot colors come from
  Quill's theme tokens when the block draws, each held to WCAG AA on the code
  background; high contrast keeps one text color. The cache stores slots, not
  colors, so a theme change needs no new tokenizing.

Gaps left open:

- Diffs have no "inserted" green: the kit gives no name for added lines.
- Instant-view (rich page) code blocks are not wired to the highlighter and draw
  plain.

Colors: slots use tdesktop's chart palette (`stat_chart::ColorKey`) held to AA
on the code background; comments use Quill's muted text, because tdesktop's
light blue and blue come out the same color once both are held to AA on a light
background. `styled` drops spans that are empty, out of order or off character
boundaries before they reach the text shaper, since the cache is keyed by hash.

## How verified

Unit tests for language mapping, unknown languages, the slot fold and Rust /
Python spans; a UI test that the demo chat tokenizes its blocks; light and dark
captures of a Rust, a Python and an unknown-language block. The release binary
size delta was not measured; the embedded syntax set is about 0.9 MB
compressed, resident after the first highlight.
