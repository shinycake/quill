## codex/spellcheck-native (2026-10-07)

Replaces the parity:platform-spellcheck "ABC n" badge + corrections panel
(`parity-platform-spellcheck.md`) with Telegram Desktop's macOS experience.

### What Telegram Desktop does (read from source)

- `lib_spellcheck/spellcheck/platform/mac/spellcheck_mac.mm`: the system
  `NSSpellChecker`. Each word is checked alone with
  `checkSpellingOfString:startingAt:language:...` (whole-text checking is
  disabled: results depend on neighbouring words). Languages are the macOS
  system languages (`QLocale::uiLanguages`, regional variant preferred);
  the word's language is recognized with CLD3. Suggestions:
  `guessesForWordRange:` per system language of the word's script, max 5.
  Add / Remove / Ignore → `learnWord:` / `unlearnWord:` /
  `ignoreWord:inSpellDocumentWithTag:0` (system dictionary, app-lifetime
  ignore).
- `spellcheck_utils.cpp`: UAX #29 word boundaries (`QTextBoundaryFinder`),
  `’` normalized to `'`, words > 99 chars, mixed-script words and
  unsupported scripts (Han, Katakana) skipped.
- `spelling_highlighter.cpp`: red underline via a QSyntaxHighlighter;
  skips links, mentions, hashtags, bot commands and code/pre tags; checks
  asynchronously; a 1 s "cold" timer while letters are typed, immediate
  re-check after a separator; context menu gets a "Spelling" section with
  Add to Dictionary, Ignore word and up to 5 suggestions (or Remove from
  Dictionary on a learned word). Settings on macOS: a single "Spell
  checker" toggle (no dictionary manager — the system owns dictionaries).

### What changed in Quill

- `src/spellcheck.rs` (core, pure): UAX #29 segmentation via
  `unicode-segmentation` (`don't` is one word; `end.Start` is split), skip
  ranges for scheme/`www.`/`tg:` links, bare domains (common TLDs),
  e-mails, `@mention`, `#hashtag`, `$CASHTAG`, `/command`, `` `code` `` and
  ```` ```pre``` ````, plus digits/`_`/over-long/mixed-script words and
  ALL-CAPS acronyms (Quill extra). `SpellBackend` trait; `SpellChecker`
  (`Send + Sync`) adds a per-word cache, session ignores, app-level learned
  words; `shift_misspellings` carries underlines across an edit;
  `is_typing_word` picks the debounce; `spelling_languages` mirrors
  tdesktop's system-language → dictionary mapping. The old embedded
  English wordlist is now `WordlistBackend` (non-macOS fallback; also
  accepts contractions via their stem).
- `src/ui/spellcheck_mac.rs`: `NSSpellChecker` backend (objc2-app-kit
  `NSSpellChecker`, objc2-foundation `NSLocale`/`NSRange` features). With
  "Automatic by Language" the languages are the macOS preferred
  languages that have a dictionary (first attempt used
  `userPreferredLanguages`, which lists every installed dictionary — "Teh"
  passed as Czech); a word is correct if any language of its script
  accepts it. Calls are serialized by a mutex and wrapped in autorelease
  pools; checks run on background threads like tdesktop's `crl::async`.
- `src/ui/spellcheck_ui.rs`: red wavy underline painted with
  `Window::paint_underline` on a canvas overlaid on the kit Textarea, rects
  from `TextareaState::range_to_bounds` (wrapped words split per line,
  clipped to the input). Each input event shifts the underlines and
  schedules a background re-check: 700 ms while typing a word, 40 ms
  after space/punctuation/deletion/paste; stale results are dropped.
  Right-click (`Textarea::context_menu`, native menu): suggestions first,
  "Add to Dictionary", "Ignore" (or "Remove from Dictionary" on a learned
  word), then Cut / Copy / Paste / Select All. Suggestion items carry their
  range (`SpellingReplace` etc. in `actions.rs`, `no_json` actions handled
  at the root) and replace via select + `replace` so it is undoable.
- Ignores now last until quit (tdesktop tag-0 behaviour) instead of being
  cleared on send/chat switch. The on/off setting is unchanged; turning it
  off clears underlines immediately. Words previously added to
  `spellcheck_words.json` are still honoured on every platform; on macOS new
  ones go to the system dictionary.
- Removed: the "ABC n" badge and the corrections panel.

### Fix: right-click crash (review round 1)

Live test: right-clicking a misspelled word aborted the app (`cannot read
InputBaseState<TextareaMode> while it is already being updated`). The kit
calls a Textarea's `context_menu` builder from `handle_right_click_menu`
via `cx.defer_in`, i.e. still inside the input entity's update, and the
builder read the composer state. Now the builder returns an empty menu
(`NativeMenu::show` ignores it) and `deferred_input_menu` builds and shows
the real one at the click position with `Window::defer`, after that update.
UI integration test `right_click_menu_reads_the_input_after_its_update`
(gpui-kit test harness: type, right-click, assert the builder read the
text) — it fails with the same panic when the read is made synchronous
again. Runs with `cargo test --features demo-capture --bin quill
spellcheck_ui` (needs gpui-kit `test-support`). Audited the other reads:
the underline canvas paints as a sibling of the Textarea and the action
handlers run from menu dispatch, neither inside the input's update.

### Not done / differences

- No per-word language recognition (tdesktop uses CLD3); a word passes if
  any preferred language of its script accepts it.
- No keyboard "show spelling menu" shortcut; no `Remove` for words learned
  in other apps unless they are under the caret (same as tdesktop).
- Linux/Windows still use the English wordlist (no hunspell).

### Verification

- 16 core unit tests in `spellcheck.rs` (segmentation, skip rules, scripts,
  language mapping, cache, learn/ignore, edit shifting, typing detection).
- `quill-tools/gate.sh`: GATE OK. `cargo clippy --features ui --bin quill
  -D warnings` has no findings in touched files (pre-existing 1.98 lints
  elsewhere).
- In-process captures (macOS, system NSSpellChecker):
  `docs/screenshots/ready-spellcheck.png`, `ready-spellcheck-panel.png`
  (multi-line draft: Teh / speling / recieve / tomorow underlined; URL,
  @mention, #hashtag, /command and code span not), `ready-spellcheck-toggle.png`.
  The native context menu cannot be captured in-process; it was not
  exercised interactively.
