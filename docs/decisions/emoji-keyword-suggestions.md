# Emoji suggestions from Telegram's emoji keywords

Typing `:` and a word in the composer offers emoji, as in Telegram Desktop.
Before this change the popup only searched Unicode emoji names and a short
alias list, so `:happy` (no emoji is named "happy") found nothing.

## What tdesktop does

`chat_helpers/emoji_suggestions_widget.cpp` and `emoji_keywords.cpp`:

- The query is `:` plus what follows it up to the caret. The `:` must not
  follow a letter or digit and must be followed by a non-space. One character
  is enough.
- Rows come from the emoji keyword packs of the input languages
  (`messages.getEmojiKeywords`, searched on the device): every keyword that
  starts with the query, in keyword order, each emoji once. The legacy
  shortcode list follows for `:` queries.
- `:D` and `:P` (one character, no lower case) match whole keywords only.
- When the whole field is one short line with no `:` and the caret is at its
  end (`suggestExactFirstWord`, on for the chat composer), the field itself is
  an exact keyword query. That strip shows after 300 ms (`kShowExactDelay`)
  unless a strip is already showing.
- `SkipExactKeyword`: exact queries skip one-character words other than
  letters, `10`, and in English single letters other than `$` and two-letter
  words other than `us`, `uk`, `hi`, `ok`.
- Recently used emoji move to the front (`PrioritizeRecent`).
- A `:` query highlights the first emoji; an exact-word strip starts with
  none, so Enter still sends. Tab takes the highlighted or the first emoji.
  Right or Down with nothing highlighted highlights the first; Left or Up
  with nothing highlighted is the caret's. Left off the first clears the
  highlight; Right off the last wraps.
- More than 7 emoji scroll horizontally (`kMaxNonScrolledEmoji`).

## What Quill does

- `quill::suggest::detect` finds `:` queries (one character is enough now)
  and the whole-field word query (`SuggestKind::EmojiWord`).
- `searchEmojis` is TDLib's on-device search of the same keyword packs:
  TDLib downloads each language's pack by language code
  (`StickersManager::prepare_search_emoji_query`, `load_emoji_keywords`) and
  searches its SQLite copy (`search_language_emojis`). The typed text is not
  sent to Telegram. The language codes come from
  `emoji::keyword_language_codes` (script of the text, system language,
  English), as for the emoji panel's search.
- `emoji::KeywordLookups` keeps the answers by query (up to 2048, the older
  half forgotten past that), with at most 8 requests in flight. Errors are
  stored as empty answers so a query is not retried on every keystroke.
  Requests that will never answer are dropped from the flight list on the
  next lookup. The popup is rebuilt when an answer's generation moves
  (`refresh_suggest_menu`, an integer compare per frame).
- Ranking is `keyword_lookup::keyword_emoji` (TDLib's order is unspecified,
  so keywords are sorted), then catalog names for ASCII `:` queries, then
  `prioritize_recent` over the emoji panel's recent list. Skin-tone variants
  are left out, as in the panel's search.
- The name line under the strip is gone; tdesktop shows emoji only.

## Differences from tdesktop

- tdesktop knows which pack a keyword came from and applies the English
  `SkipExactKeyword` rules to the English pack only. A `searchEmojis` answer
  does not say, so the English rules apply to ASCII words.
- tdesktop's `PrioritizeRecent` does not advance past a recent emoji that is
  already first, so the next recent one can push it to second place. Quill
  keeps it first.
- tdesktop applies the user's chosen skin tone (`lookupEmojiVariant`). Quill
  has no per-emoji skin tone setting; skin-tone results are left out.
- tdesktop appends matching custom emoji from installed emoji sets
  (`appendCustom`). Not done here; it is a follow-up.
- A `:` query keeps Quill's character rules (letters, digits, `_`, `-`, `+`
  and single spaces). tdesktop takes any characters, but its prefix search
  finds nothing for the others anyway.

## Privacy

Only language codes and whole keyword packs cross the network. With Debug
mode on, TDLib's own log (verbosity 4) records the searched words, as it
records everything else at that level; with Debug mode off TDLib's log goes
nowhere (`secure_native_logging`).

## Tests

- `emoji::keyword_lookup` tests: prefix and exact ranking, `SkipExactKeyword`,
  recent ordering, flight and answer bookkeeping, the cap.
- `suggest` tests: one-character `:` queries, the whole-field word query.
- `composer_suggest` tests: keyword rows then names, exact words, replacing
  the word, the arrow rules.
- `composer_suggest_tests` (GPUI): the strip appears when the answer lands,
  Right and Enter insert in place of `:happy`; a lone word waits 300 ms of
  test clock and starts with nothing highlighted, Left is not consumed, Tab
  inserts; Esc keeps the strip closed when a new answer lands.
