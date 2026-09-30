## Slice parity/platform-spellcheck (2026-09-30, loop 1)

**Scope:** `parity:platform-spellcheck` — spellcheck in the message composer.

- **Built:**
  - `src/spellcheck.rs`: pure-Rust engine, no system deps. Embedded
    frequency-ordered English wordlist (~48k words,
    `assets/spellcheck/en.txt`, from FrequencyWords en_50k); hash
    lookups for `is_correct`; Damerau-Levenshtein distance-1 edits
    (distance-2 fallback, hard-capped) filtered through the wordlist
    and ranked by frequency for `suggestions`. Only ASCII words are
    checked — other scripts, URLs, @mentions, #hashtags, digits and
    ALL-CAPS acronyms are never flagged.
  - UI: per-input-event cheap re-check (`check_words`, no suggestions);
    an "ABC n" badge appears in the composer input row while the draft
    has misspellings; clicking opens a corrections panel above the
    composer with up to 5 suggestions per word, Ignore (session-only,
    cleared on send/chat switch) and Add to dictionary (persisted to
    `spellcheck_words.json`). Applying a suggestion replaces the word
    in the draft and restores the cursor after it.
  - Settings: "Check spelling" toggle in the Appearance dialog
    (ChatPrefs.spellcheck_enabled, default on like Telegram Desktop;
    serde + manual Default agree).
  - Tests: 6 engine unit tests (dictionary load, typo flagging,
    non-English/noise skipping, suggestion ranking, byte ranges,
    custom words + ignores).
- **Key decisions (ponytail):**
  - No inline wavy underlines: the kit Textarea is a closed component
    with no decoration hooks — forking it for underlines was rejected
    as over-engineering. The badge + corrections panel is the full
    feature without kit surgery.
  - No hunspell/system dictionaries: nothing ships them on all three
    OSes and the VM has none; the embedded wordlist is portable and
    the whole engine is dependency-free.
  - Suggestions computed on panel open, never per keystroke.
- **Out of this slice:** non-English dictionaries (language picker for
    spellcheck); inline underline rendering if the kit ever supports it.
