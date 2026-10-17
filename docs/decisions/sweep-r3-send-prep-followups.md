# Send-prep follow-ups

Follow-ups from the third review of PR #992 (tdesktop has no equivalent: it
decodes media in-process and keeps no draft-clear flag).

- Draft clear after an ack: `ComposerUi::clear_draft_on_success` now holds a
  `DraftClear` (chat plus the markup that send took). `clear_after_send`
  assigns it on every send, so a later partial or typed-over send resets an
  earlier full send's flag. At the ack the stored draft is cleared only while
  it still equals what was sent. The "composer is idle" test applies to the
  open chat only; a chat left meanwhile is judged by its own stored draft.
- User-visible English from ffmpeg, xdg-mime and the link handler moved to
  lang keys (`qll_ops_video_*`, `qll_prefs_default_app_*`). Raw tool stderr and
  io errors go to the log (`eprintln!`), never into a toast or status line.
- macOS default-app row: `link_handler::state()` only reads the cache there.
  `link_handler::read_now()` does the LaunchServices lookup, called from
  `QuillApp::open_appearance` when the dialog opens and from the re-reads after
  "Make default". Other platforms keep the background refresh.

Verified: unit tests (flag scenario, `DraftClear::matches`), check-lang, and
the `ready-link-handler-row` demo in light and dark (row shows the localized
status, read once on open).
