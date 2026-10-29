# Highlighting words that have an emoji

Idan asked for this on 2026-10-29. The composer gives a soft highlight to a
few words that have an emoji. Pointing at one shows the strip `:word` would
show, and a click swaps the word for the emoji. Settings, Chat, Emoji has a
toggle, "Highlight words with emoji". It is on by default, and it only works
while "Suggest emoji replacements" is on.

Telegram Desktop has nothing like it. Quill does not invent features that
tdesktop lacks unless Idan asks for them, and he asked for this one.

## Research

- **iOS Messages, "emoji replacement" (iOS 10).** This is the closest match.
  The user opens the emoji keyboard and a shimmer passes over the draft.
  Words that have an emoji are left tinted orange, and a tap offers the
  emoji. Write-ups say it lights up every word it can match, that matching
  is hit and miss, and that animals, food and faces work best. It runs only
  when the user asks for it.
  ([MacRumors](https://www.macrumors.com/how-to/ios-10-messages-emoji),
  [Six Colors](https://sixcolors.com/post/2026/05/orange-you-glad-i-didnt-say-emoji/),
  [iMore](https://www.imore.com/how-use-emoji-and-tapbacks-imessage))
- **Phone keyboards** (QuickType, Gboard, SwiftKey). They suggest an emoji
  for the last word in the prediction bar. Nothing is marked in the text
  itself.
- **Underlines in running text.** Readers take an underlined word for a link.
  Unbroken lines across a text distract, and design systems soften them by
  making them thinner and lighter.
  ([Smashing](https://smashingmagazine.com/2017/12/underlined-text-improve-ux),
  [Adobe XD](https://xd.adobe.com/ideas/process/ui-design/use-underline-text-improve-ux/),
  [Queensland Health](https://www.design-system.health.qld.gov.au/styles/underlines))
  A red wavy line already means a spelling error in this composer.

Conclusions: the mark must not look like a link or a spelling error. It has
to be rare, because iOS can get away with marking every match only because
it does so on request. It should go only on words whose emoji the reader
would expect, like iOS's animals and food.

## Rules (`quill::emoji_words`)

- **The word is finished.** Words the caret or the selection touches are
  skipped, so nothing flickers while typing and a selection about to be
  formatted or replaced is left alone.
- **A plain word.** At least 3 letters, letters only (an inner `'` or `-` is
  fine). Not in code, links or mentions. Not part of a `#tag`, `@name`,
  `/command`, `:query`, address or path. Not one of about 130 common words
  ("the", "good", "time", "person", "sign"...). A decomposed accent
  ("cafe" plus U+0301) stays part of its word.
- **A whole emoji keyword, and a specific one.** The word must be a whole
  keyword with at most 8 emoji. "pizza" has 1 emoji, while "face" has dozens
  and tells the reader nothing.
- **Density.** One highlight per 6 words of the message, rounded down, but
  at least 1 and never more than 3, so a message of 2 to 11 words gets one.
  The words with the fewest emoji win, and the same word is highlighted
  once. A message of one word gets none, because the strip above the field
  already covers it.
- **Stable.** A word that has lit up keeps its place for the rest of the
  draft, even while the caret passes through it, and its sweep does not
  replay. A more specific word typed later waits for a free place.
- **Esc dismisses.** Esc over a word's strip stops highlighting that word for
  the rest of the session.
- **Scope.** Only the first 4 KB of a draft is scanned, and only the 48
  candidate words nearest the caret are looked up, so a long paste does
  not fill the keyword lookups the `:` strip shares.

## Look

- A two-color wash (the accent fading into violet) is painted over the
  letters at 12% opacity (the Textarea has no layer under its text), with a
  1.5 px line under them in the same gradient. It is different from
  a link (plain accent text) and from a spelling mark (red and wavy). On
  hover the wash goes to 28%.
- When a word first lights up, a light band sweeps across it once in 0.9 s
  and fades as it goes. With Reduce Motion on there is no sweep.
- The strip is anchored just above the word. It has the same tiles as the
  `:` strip and the same rows as `:word`.

## Cost and privacy

- The highlights are picked again only when the text, the selection, the
  formatting, the keyword answers or the setting change. Every other frame
  compares the composer's text with a kept copy of its rope (cheap to
  clone, nothing is copied out) and its few code and link spans.
- The overlay paints with the composer. It asks for animation frames only
  while a sweep runs, so an idle window does no extra work.
- The lookups go through TDLib's on-device keyword search (`searchEmojis`),
  with the same cache and in-flight cap as the `:` strip
  (docs/decisions/emoji-keyword-suggestions.md). Only language codes and
  whole keyword packs go over the network, never the typed words.

## Tests

- `emoji_words` unit tests cover candidates (caret, short and common words,
  tags, mentions, paths, scripts), skipped spans, specificity and density,
  a lone word, pending answers and the scan limit.
- `emoji_words_ui_tests` (GPUI) checks that finished words light up while the
  word being typed waits, and that a hovered word offers its emoji and
  becomes the one picked, with the caret left where it was. It also checks
  that a selection darkens the words it covers, that Esc stops highlighting
  that word but closes an open `:` strip first, and that the setting and
  "Suggest emoji replacements" turn the highlights off.
- The test text system has no glyph positions, so pointer hit-testing was
  checked in demo captures instead.
