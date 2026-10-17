# Typed status notes

Quality sweep, round 2, UI/UX findings 2, 3 and 14.

## What was wrong

- `status_note_is_toast` and `status_toast_duration` decided from English
  substrings ("fail", "copied", "link"). With any other interface language
  the copy, save and failure toasts never appeared.
- About 75 call sites wrote `format!("... failed: {err:?}")`, so the toast
  showed a Rust Debug dump (`Td { code: 400, ... }`), clipped at 520 px.
- A few notes were literal English ("copied", the sign-in progress lines,
  "could not load Premium").
- Dead notes ("replying", "editing", "pinned", "unpinned", "selection
  cleared") shared the single note slot with real ones and overwrote an
  unread failure.

## What Telegram Desktop does

`Ui::Toast::Show` is called per action and is unconditional: the call site
decides that something is worth a toast, never the text. Errors are shown as
the localized RPC error (`lng_flood_error` and friends) or a short sentence,
never a struct dump. Progress that the screen already shows (a sending
clock) has no toast.

## What changed

- `src/ui/status_note.rs`: `StatusNote { text, kind }` with
  `NoteKind::{Info, Error, Silent}`. Toast visibility
  (`kind != Silent && !text.is_empty()`) and duration (Error 6 s, otherwise
  3 s) come from the kind. `status_note_is_toast` and
  `status_toast_duration` are gone.
- `ConnectionUi` keeps the note private. Every write goes through
  `set_note(StatusNote)`, so a new untyped assignment does not compile.
  Reads are `status_note()` (text), `note()` and `clear_note()`.
- A `Silent` note never replaces a note the toast is still showing. Progress
  chatter cannot wipe an unread failure; the toast's own timer clears it.
  The five dead notes from finding 14 were removed outright.
- `src/ui/user_error.rs`: `user_error(&err)` maps an error to a localized
  sentence and writes the raw error to the debug log (`debug_log`). It
  covers `ConnectSendError` (send failed, caption/text over the limit) and
  `TdError` (flood wait with seconds, not found, not allowed, server error,
  generic). New text lives in `lang/en/notes.strings`; the flood fallback
  reuses tdesktop's `lng_flood_error` and the copied toast uses
  `lng_text_copied`, so those get every language from Telegram's pack.
- All 60 `{err:?}` status notes now call `user_error`. The localized
  strings replace the literal English in `copy_inline_text.rs`,
  `selection_mode.rs`, `group_calls.rs`, `auth_ui.rs` and
  `premium_ui/open_premium.rs`.
- `scripts/check-lang.py` fails on a status note built from a Debug format.

## How the call sites were converted

About 1,250 writes in 277 files were rewritten by a script, not by hand. The
script wrapped each text leaf (`tr(..)`, `tr_with(..)`, `format!(..)`, a
literal) in `StatusNote::info|error|silent`. It picked the kind by running
the old English-substring rules once on the English text, so English
behavior is unchanged and other languages now match it. Notes that arrive
pre-built (session drains such as `message_link_error`, helper functions)
were typed by hand: errors and refusals are `Error`, confirmations `Info`,
progress `Silent`.

## Not covered

- The text of notes produced in the lib crate (session error strings, the
  hidden experimental-option toasts) is still English. They are typed now,
  so they show in every language, but they are not translated.
- `dialog.error` strings that interpolate `{err:?}` (folder dialogs, photo
  source note) are not status notes and were left alone.
- Silent progress literals such as "muted" and "N messages selected" stay
  English; they are never shown while a live window is open.

## How verified

Unit tests: the kind drives visibility and duration for a German note;
`user_error` maps flood wait, 404, 401, 5xx and the send errors; no mapped
text contains a Debug dump. `ConnectionUi` tests check that a German failure
still toasts and that a silent note cannot wipe it. The toast needs a live
TDLib connection, so the captures use two demos
(`ready-status-toast-info`, `ready-status-toast-error`) with
`QUILL_DEMO_SHOW_TOAST=1`, light and dark. `scripts/check-lang.py` also
fails on a Debug dump in a status note.

The script that did the conversion was a throwaway and is not committed.
