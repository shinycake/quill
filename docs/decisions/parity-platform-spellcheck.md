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
  - Tests: 8 engine unit tests (dictionary load, typo flagging,
    non-English/noise skipping, suggestion ranking, byte ranges,
    custom words + ignores, hyphen/apostrophe edits, capitalization).
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

## PR #236 MUST_FIX follow-up

- The Appearance switch immediately rechecks the current draft on both
  transitions: OFF clears misspellings and suggestions and closes the panel;
  ON restores the badge without another keystroke.
- Distance-1 replacement/insertion includes apostrophe and hyphen, with a
  regression proving `wellknown` can suggest embedded `well-known`.
- Panel labels and applied corrections share capitalization matching:
  `Teh` → `The`, ALL-CAPS → ALL-CAPS, lowercase → dictionary case.
- Screenshot fixtures `ready-spellcheck`, `ready-spellcheck-panel`, and
  `ready-spellcheck-toggle` are wired through CLI parsing, ready markers,
  ReadyChats seeding, and the capture script. Badge/panel fixtures explicitly
  enable spellcheck, sync the draft, and refresh panel suggestions. Spelling
  appears near the top of Appearance so its switch is visible. Captured under
  Xvfb to `docs/screenshots/ready-spellcheck.png`,
  `ready-spellcheck-panel.png`, and `ready-spellcheck-toggle.png`.
- Tip-scoped `macos-ui-build` has not succeeded yet. Mac UI proof is **not
  claimed on this tip**; the earlier PR-body green claim referenced the wrong
  SHA and is retracted here.
- This English wordlist + badge/panel UX is a kit workaround, not Telegram
  Desktop system-spellcheck parity. The badge counts the engine's capped list
  (32); the panel displays at most 8 words.

Local verification for this follow-up (`TMPDIR=/tmp/quill-tmp-wt-236`):
`cargo fmt --check`, `cargo clippy --no-default-features --all-targets
--locked -- -D warnings`, 8 spellcheck unit tests, and the full
`--no-default-features --locked` suite (1219 lib + integration tests, 0
failed). Linux `cargo build --features ui --locked` passed for screenshot
capture — this is **not** tip-scoped Mac UI proof.
