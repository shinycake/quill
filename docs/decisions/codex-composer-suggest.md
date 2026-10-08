# Composer suggestions: `#hashtag` and `:emoji`

Branch `codex/composer-suggest`.

## What exists already

- `@` mention popup (`ui/composer.rs`, `mention_menu_*`) and the `/` command
  menu. The new popups reuse the mention popup's look and its key routing in
  `app_demo.rs` (Esc, Up/Down, Tab, Enter, mouse-down pick).
- Sticker suggestions by trailing emoji (`sticker_suggest.rs`,
  `sync_sticker_suggestions`) and animated-emoji suggestions already ship, so
  request item 3 (stickers by emoji) needs no work here.

## Behavior and tdesktop references

Paths are under `Telegram/SourceFiles`.

- Trigger rules, `#`: `chat_helpers/message_field.cpp:933-1000`
  (`ParseMentionHashtagBotCommandQuery`). Scan back from the caret over
  letters, digits and `_`, at most 63 chars; the `#` must not follow a tag
  character. Added on top: a `#` inside a URL or path (the whitespace-delimited
  word before it contains `/`) does not trigger, so `https://x.org/#frag` is
  left alone.
- Hashtag data: `storage/localstorage.cpp:1395` (`incrementRecentHashtag`:
  64 entries, use count, ties favour the newest, counts halve past 0x4000) and
  `storage/storage_account.cpp:2895-2925` (tags found by `RegExpHashtag`, all-digit
  tags excluded). Ported as `suggest::RecentHashtags`, so the list is ranked by
  use count rather than a plain MRU (this is what tdesktop does).
  Persisted as `recent_hashtags.json` next to the other app-level prefs;
  updated in `submit_composer` for new messages (not edits).
- Hashtag filter: `chat_helpers/field_autocomplete.cpp:659-673`: empty filter
  lists everything, otherwise case-insensitive prefix and the fully typed tag is
  not offered back. Capped at 8 rows (tdesktop scrolls).
- Trigger rules, `:`: `chat_helpers/emoji_suggestions_widget.cpp:880-980`
  (`getEmojiQuery`): the colon must not follow a letter or digit (`http:`,
  `12:30`), must not be followed by a space, names may contain single spaces.
  Quill also requires 2 characters after the colon (per the task) and rejects
  other punctuation inside the name. `Suggest emoji replacements`
  (`settings/sections/settings_chat.cpp:1480`, `core/core_settings.cpp`
  `suggestEmoji`, default on) is `ChatPrefs.suggest_emoji`, a switch in Settings
  > Appearance > Emoji.
- Ranking follows `emoji_suggestions.cpp` `prepareResult` in spirit: exact
  name/shortcode, then names that start with the query, then first-word matches,
  then the rest; fewer-word names first within a tier.
- Keys follow `emoji_suggestions_widget.cpp:484-520`: Up/Down/Left/Right step
  (Left/Right only while the emoji strip is open, otherwise they move the caret),
  Enter/Tab insert, Esc closes until the query changes. First row preselected.
  Hashtag insertion adds a trailing space (tdesktop `insertTag`).
- The caret decides (`selected_range`), not the end of the text. A non-empty
  selection, an open `@` mention query, or an edit in progress shows nothing.

## Choices and gaps

- Emoji data is local: names from the bundled Unicode 17 table
  (`assets/emoji/emoji-17.tsv`) plus a short hand-written shortcode list
  (`:smile`, `:thumbsup`, `:+1`, `:fire`...). TDLib `searchEmojis` was not used:
  it is async, shares one result slot with the picker, and Quill's UI is English
  only, so there is no localization gain. tdesktop's own shortcode table
  (`lib_ui/emoji_suggestions`) is not copied (license and size); a bigger
  alias table can be added to `suggest::ALIASES`.
- Skin-tone and hair-component variants are left out of suggestions.
- Not done: the delete button on hashtag rows (`field_autocomplete.cpp:1635`),
  accent-insensitive hashtag matching, `:D`/`:-P` exact smiley matches,
  hashtags seen in a channel, suggestion fade animation (no animation, so no
  tick needed). `replaceEmoji` (auto replace of `:)`) is a separate setting.
- No README parity item exists for either feature, so no parity fragment.

## Verification

- `cargo test --lib suggest::` and `ui` unit tests: caret/word-boundary/URL/`http:`
  triggers, recent-hashtag ranking, cap, halving, JSON round trip, emoji ranking
  and filtering, replacement text and caret.
- Demo captures `ready-suggest-hashtag` and `ready-suggest-emoji`
  (`QUILL_DEMO_CAPTURE`), viewed by hand: highlighted prefix, selection row, emoji
  strip with the selected name.
- `gate.sh` GATE OK.
