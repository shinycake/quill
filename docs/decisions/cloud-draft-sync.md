# Cloud draft sync with other clients

## Problem

A draft typed in Telegram on another device showed in Quill's chat list but
never reached the open composer. Leaving the chat then saved the empty
composer (`flush_leaving_draft`, `setChatDraftMessage` with a null draft) and
deleted the draft everywhere.

The outgoing direction was checked against TDLib's log on a live account
(Saved Messages, 2026-10-22). Typing sends `setChatDraftMessage` one second
after the last key, and TDLib sends `messages.saveDraft` a second later.
The draft reached iOS.

## Decision

Follow Telegram Desktop's `HistoryWidget::applyCloudDraft`: when a cloud
draft changes for the slot the composer shows, the field takes it unless the
user is editing a message or has unsaved changes in that slot.

- `Session::set_remote_draft` bumps `remote_draft_revision` when a remote
  draft replaces the stored one. An echo of Quill's own save matches the
  stored draft and does not bump it.
- Each composer (the main window and every chat window) keeps the revision
  it last checked (`ComposerUi::remote_draft_seen`). `adopt_remote_draft`
  runs in the frame upkeep and in each chat window's render. When the
  revision moved and the composer differs from the slot's draft, it calls
  `restore_open_draft`.
- When the composer already shows the draft, nothing is restored, so the
  caret stays where it is.

The debounced save's timer moved from `ComposerUi`, which is swapped per
window, to `FrameUi`, which is app-wide like the driver's `pending_draft`.
With a chat window's state swapped in, the timer task could step that
window's timer and leave the real one marked as running. No later
keystroke would then start a save.

## Not done

tdesktop also ignores cloud drafts for a few seconds after its own save
(`kSkipCloudDraftsFor`). Quill relies on the dirty bit and the equality check
instead. A remote draft that arrives in the gap between a save and the next
keystroke replaces the composer, which is what tdesktop does past its window.
