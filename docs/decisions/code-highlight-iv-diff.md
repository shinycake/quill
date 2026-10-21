# Code highlighting: Instant View blocks and diff lines

Follows `code-syntax-highlight.md`.

## What tdesktop does

- Instant View code blocks highlight. `iv_rich_page.cpp` reads
  `pageBlockPreformatted.language` (trimmed) into a `BlockKind::Code` block,
  and `MarkdownArticle::Impl::tryHighlightSyntax` runs it through the same
  `Spellchecker::TryHighlightSyntax` queue and cache as messages. The reader
  uses the same eight `SyntaxHighlightColors`.
- `diff` and `patch` use libprisma's `git` grammar. Its "inserted" token (lines
  starting with `+`) takes slot 8, `statisticsChartLineGreen`; its "deleted"
  token (lines starting with `-`) takes slot 3, the red `statisticsChartLineRed`
  that literals use (the compiled class is `[-\x13]`, so an en dash never
  matches); `#` lines are comments. On the other lines the grammar's `string`
  rule (slot 4, orange) colors same-line `"…"` and `'…'` strings. It is listed
  before `command`, `coord` and `commit-sha1`, which have no color, so hunk
  headers and context lines get orange strings too; the rest stays plain.

## What changed

- `src/rich/parse.rs`: `pageBlockPreformatted` keeps its language as a
  `PreCode` entity (plain `Pre` when empty). That is all the reader needs: it
  already draws every `pre` run through `pre_block`, so the cached highlighter
  applies unchanged, and a block without a language stays plain.
- `src/ui/code_highlight.rs`: new `Token::Inserted` slot (tdesktop green,
  held to WCAG AA on the code background like the other slots). `diff` and
  `git` blocks are classified line by line (`diff_line_token`), as the `git`
  grammar does, instead of through the kit's diff grammar, which has no
  inserted/deleted names. Deleted lines reuse the red `Literal` slot, as in
  tdesktop.
- The Instant View overlay reads its page from `session()` so the demo session
  can show it; the close button clears it there too.

## How verified

Unit tests: diff line classification, diff spans (CRLF, multibyte, empty
text), parser language handling, the inserted/deleted colors in dark, light and
both installed themes, and AA contrast for every slot. UI tests: the
`ready-code-highlight` chat (now with a diff) and the new
`ready-iv-code-highlight` reader draw colored runs from the cache. Light and
dark captures of both demos.
