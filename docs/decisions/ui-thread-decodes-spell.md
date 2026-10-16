# UI thread: spell engine startup and image decodes

Quality sweep findings S-04 and S-05. No tdesktop behavior changes: tdesktop
builds its spellchecker on a worker thread and decodes images with Qt off the
main path as well; this just stops Quill from stalling its window.

## S-04 spell engine

Before: `QuillApp` construction built the platform engine inline. On Windows
that waited on the COM thread with an unbounded `recv`; the macOS worker was
also awaited with `recv`, and a language change rebuilt the engine on the UI
thread.

After:
- Startup uses the embedded wordlist plus the persisted app words
  (`provisional_spellchecker`) and `start_spell_engine` builds the real
  engine on `cx.background_spawn`, then swaps it in (app words carried over,
  underlines re-checked). A language change does the same. A newer build
  replaces a pending one. Demo fixtures keep the synchronous engine so
  captures stay deterministic.
- `spell_win` / `spellcheck_mac` waits are bounded: 750 ms for suggestions and
  learn/ignore calls from menus (fall back to no suggestions), 5 s for the
  background `is_correct`, 15 s for Windows COM init. No verdict still means
  "do not underline".

Verified: UI tests with a builder that sleeps (construction/start returns at
once, old engine answers, swap lands after the task, words learned meanwhile
survive). Windows `ISpellChecker` changes are compile-reviewed only (no
Windows machine here); the macOS worker was not exercised against a stuck
spelling server.

## S-05 image decodes

`Copy Image`, the composer photo editor, "Edit Image" on a message edit and
the sticker-set image picker called `image::open` on the UI thread (no size
limit, no panic guard, no HEIC/AVIF/JXL). They now run
`image_decode::decode_file` (full size) on a background task through
`QuillApp::decode_image_then` / `run_off_ui_then` (`src/ui/image_open.rs`) and
act when it resolves, with the existing error text on failure. If another
editor opened while a decode ran, the late result does not replace it.

Verified: UI tests for each flow (nothing decoded on the calling stack,
editor opens after the task, failure notes) and a HEIC fixture opening in the
editor and copying as PNG (macOS and Windows only; Linux needs system
libheif).
