# Highlighting words that have an emoji

Idan asked for this on 2026-10-29. The composer gives a soft highlight to
every finished word that has an emoji. Pointing at one shows the bar `:word`
would show, and a click swaps the word for the emoji. Settings, Chat, Emoji
has a toggle, "Highlight words with emoji". It is on by default, and it only
works while "Suggest emoji replacements" is on.

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

Conclusions: the mark must not look like a link or a spelling error. The
first release also kept it rare (one word in six, no common words, at most
8 emoji per word, at least 3 letters). In use that read as slow detection:
a second word waited until the message grew, and words like "funny" or "ok"
never lit (Idan, 2026-10-29). Idan chose iOS's way instead: every word that
matches lights up, and the user turns off the ones they do not want.

## Rules (`quill::emoji_words`)

- **The word is finished.** Words the caret or the selection touches are
  skipped, so nothing flickers while typing and a selection about to be
  formatted or replaced is left alone. A word is looked up the moment it
  is finished, and TDLib's on-device search answers in milliseconds.
- **A plain word.** Letters and digits (an inner `'` or `-` is fine). Not
  in code, links or mentions. Not part of a `#tag`, `@name`, `/command`,
  `:query`, address or path. Not one that tdesktop's `SkipExactKeyword`
  drops (most one- and two-letter words; "ok" stays). A decomposed accent
  ("cafe" plus U+0301) stays part of its word.
- **A whole emoji keyword.** Prefix matches ("pizz") do not light.
- **Every match.** There is no density limit and no common-word list. A
  message of one word gets none, because the bar above the field already
  covers it.
- **The user decides.** The word's right-click menu has "Suggest Emoji"
  (opens the bar for any word, lit or not, with `:word`'s prefix matches),
  "Don't Suggest Emoji for This Word" (the word joins a list in Settings,
  Chats, Emoji, where each can be brought back) and "Turn Off Emoji
  Suggestions" (turns the highlights off, with a note that Settings turns
  them back on). Esc over a word's bar stops highlighting that word for the
  session.
- **Scope.** Only the first 4 KB of a draft is scanned, and only the 48
  candidate words nearest the caret are looked up, so a long paste does
  not fill the keyword lookups the `:` bar shares.

## Look

- A two-color wash (the accent fading into violet) is painted over the
  letters at 12% opacity (the Textarea has no layer under its text), with a
  1.5 px line under them in the same gradient. It is different from
  a link (plain accent text) and from a spelling mark (red and wavy). On
  hover the wash goes to 28%.
- When a word first lights up, a light band sweeps across it once in 0.9 s
  and fades as it goes. With Reduce Motion on there is no sweep.
- A word's glimmer plays once per draft, even when the caret passes
  through it.

## The bar

Telegram for macOS shows emoji suggestions in a strip as wide as the field;
Idan preferred it to tdesktop's short strip, and asked for a way to open it
into a grid. `emoji_bar` draws it for both `:word` and a pointed-at word,
just above the field and as wide as it.

- **Fades.** A soft fade into the bar's background on an edge with more
  emoji past it, as in tdesktop: left and right in the strip, top and
  bottom in the grid. They follow the scroll position.
- **Grid.** When the emoji do not fit, a chevron at the end opens the bar
  into a grid of up to 5 rows that scrolls vertically, and closes it back.
  Up and Down move a row at a time in the grid.
- **Custom emoji.** For Premium users, the installed custom emoji whose
  emoji is among the suggestions follow the standard ones, in suggestion
  order, up to 60 (Telegram suggests them the same way). Their sets are
  loaded the first time the bar needs them. They play while on screen
  unless power saving is on.
- **Pointer.** A word's bar stays open while the pointer is on the word,
  on the bar, or in the band between them (their convex hull, 6 px of
  slack). The first release closed it as soon as the pointer left the
  word, often before it reached the bar.

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

- `emoji_words` unit tests cover candidates (caret, short words, tags,
  mentions, paths, scripts), skipped spans, every match lighting, a lone
  word, pending answers and the scan limit.
- `emoji_bar` unit tests cover the custom emoji that follow the standard
  ones and their cap. `emoji_words_ui` unit tests cover the band between a
  word and its bar.
- `emoji_words_ui_tests` (GPUI) checks that finished words light up while
  the word being typed waits, that a hovered word offers its emoji and
  becomes the one picked with the caret left where it was, that a selection
  darkens the words it covers, Esc, the setting, the blocked-words list,
  "Suggest Emoji" on a word that is not lit, and "Turn Off Emoji
  Suggestions" with its note.
- The test text system has no glyph positions, so pointer hit-testing was
  checked in demo captures instead.
