# codex/status-notes — status notes become a transient toast

- `status_note` (766 call sites) rendered as a permanent footer row. It shifted the whole
  layout whenever a note appeared or cleared, and kept routine notes ("editing", "searching
  GIFs…", "attachment cleared") on screen indefinitely.
- Live mode now shows the latest note as a floating pill above the composer. It doesn't shift
  layout or capture clicks. Confirmations stay up 3 s, failures 6 s ("fail", "could not",
  "can't", "cannot", "error", "offline"). A changed note restarts the timer. An expired,
  unchanged note is cleared, so the same message set again later (a repeated failure) shows
  again.
- Screenshot demos keep the fixed footer: their captions describe the fixture, and the
  capture layouts depend on it.
- The call sites are unchanged. Moving routine notes to silence and errors to kit
  notifications can happen incrementally.
