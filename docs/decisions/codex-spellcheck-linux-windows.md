## codex/spellcheck-linux-windows (2026-10-08)

Spellcheck backends for Linux and Windows, plus a fix for the macOS
`NSSpellServer findMisspelledWordInString timed out` log. Extends
`codex-spellcheck-native.md`.

### What Telegram Desktop does (read from source)

- `lib_spellcheck/spellcheck/platform/linux/spellcheck_linux.cpp` +
  `linux_enchant.cpp`: enchant (dlopen'ed, optional) over the system
  hunspell/aspell/hspell dictionaries. The system-locale dictionary is
  first, then up to ~10 others; a word is correct when any validator of its
  script accepts it; suggestions come from the first dictionary of the
  word's script that has any (max 5).
- `platform/win/spellcheck_win.cpp`: `ISpellCheckerFactory` /
  `ISpellChecker` (Windows 8+) on one dedicated COM thread (not
  thread-safe). Languages: the folders in `%APPDATA%\Microsoft\Spelling`,
  else the UI languages, filtered by `IsSupported`. A word is correct when
  any checker reports no error or an error whose corrective action is not
  GET_SUGGESTIONS/REPLACE. Persian is skipped (API bugs). Hunspell is the
  fallback below Windows 8.
- `platform/mac/spellcheck_mac.mm`: `NSSpellChecker` calls come from
  `crl::async` threads (no dedicated queue).

### Choices

- **Linux: `spellbook` (pure Rust Hunspell-compatible, MPL-2.0, 0.4.2,
  published 2026-06, used by Helix) instead of dlopen(libhunspell).** No
  runtime library to find, nothing to allowlist in
  `scripts/check-bundle-elf.sh` / `linux-package.sh`, and the same code
  works on every OS, so it is also the Windows fallback when ISpellChecker
  has no language. Only the `.aff`/`.dic` files are external. MPL-2.0 is
  file-level copyleft; we link it unmodified (listed in `THIRD_PARTY.md`).
  Non-UTF-8 dictionaries (ISO-8859-x, KOI8-R...) are decoded via the `.aff`
  `SET` line with `encoding_rs` (already in the lock file).
- `src/spell_dict.rs` (core, pure, tested): standard dictionary dirs
  (`$XDG_DATA_HOME/hunspell`, `$XDG_DATA_DIRS/hunspell`,
  `/usr/share/hunspell`, `/usr/share/myspell[/dicts]`, Flatpak), discovery
  (`<code>.dic` + `.aff`, user dir wins, `hyph_`/`th_` ignored, symlinks
  followed), locale detection from `LANGUAGE`/`LC_ALL`/`LC_MESSAGES`/`LANG`,
  `pick_dictionaries` (explicit picks, else tdesktop's `spelling_languages`
  mapping), `HunspellBackend` (dictionaries parsed lazily on the first
  background check), and `select_backend`: OS checker, else Hunspell, else
  **nothing**.
- **No dictionary => spellcheck off.** The embedded English wordlist is no
  longer used on Linux/Windows at runtime (it stays for tests and the
  screenshot fixtures). The Appearance row says why and how to install a
  dictionary. This drops the previous English-only underline on Linux
  without dictionaries, as required.
- `src/spell_win.rs` (`cfg(windows)`, in the library so
  `cargo check --target x86_64-pc-windows-msvc --no-default-features`
  covers it) uses the `windows` 0.62 crate (windows-sys has no COM
  interfaces). Own COM thread like tdesktop. Learned words stay in Quill's
  list (`spellcheck_words.json`) rather than the shared Windows
  dictionary: ISpellChecker can't report added words, so "Remove from
  Dictionary" would not know what to undo (tdesktop keeps a shadow list
  for the same reason).
- Settings: `spellcheck_languages.json` (`SpellcheckLanguages`). Where
  Quill owns the language list (Linux dictionaries, Windows spelling
  languages) Appearance -> Spelling shows chips: "Automatic" (system
  locale) plus one per available dictionary; picking rebuilds the engine
  and re-checks the draft. macOS keeps just the toggle (the system owns
  languages).

### macOS timeout fix

`NSSpellChecker` was called from gpui background-pool threads (serialized
by a mutex, but from many different threads, with tag 0, and the first
call for each language paid the dictionary load while the UI was already
asking). All traffic now goes through one named serial worker thread
(`quill-spell`) that uses one `uniqueSpellDocumentTag`, and each language's
dictionary is warmed up on that thread at startup. Callers (already
background tasks, so the UI thread never waits) block on a reply channel;
a dead worker yields "correct" (no underline). No other behavior change.
The warning is AppKit-internal and timing dependent, so it can't be proven
gone by a unit test; see the PR for the live check.

### Verified where

- Core unit tests (`spell_dict`): dir order/dedup, discovery with temp
  dirs, user-dir-wins, locale env parsing, language picking, backend
  fallback chain, Hunspell check/suggest/affix rules via the fixtures in
  `tests/fixtures/hunspell` (own minimal `.aff/.dic`), legacy charset,
  missing dictionaries => off, tokenization parity between Hunspell and
  wordlist backends.
- `cargo check --target x86_64-pc-windows-msvc --no-default-features`
  compiles `spell_win.rs`. ISpellChecker behavior and the Windows/Linux
  settings row were not run on a real Windows/Linux desktop.
- macOS: gate (fmt, clippy, core + UI tests); the timeout fix needs a live
  run.
