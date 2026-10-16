# Reply bar and selections follow the source of truth

## What tdesktop does
`ComposeControls` / `FieldHeader` keep only ids for the reply and the edit.
The shown message is looked up each time; an edit refreshes the text
(`MessageUpdate::Flag::Edited`) and a destroyed message cancels the reply or
the edit (`itemRemoved`, `Flag::Destroyed`).

## What changed
- `ComposerReplyTo.preview` and `ComposerEdit.original_text` are gone. The bars
  read the message through `Session::reply_target` / `edit_target_text`
  (src/state/composer_targets.rs). A message that is not loaded shows "Loading…".
- `cancel_gone_composer_targets` (src/ui/composer_targets.rs) runs with
  `flush_notifications` and cancels an edit or reply whose message has a
  tombstone (or, for scheduled edits, left the scheduled list).
- Message selection: `QuillApp::forward_draft()` returns the picked ids that are
  still in the chat history. Counts, the forward bar, copy, report, unpin, save,
  download and delete read it. The delete and unpin confirm boxes re-check at
  confirm time. Toggles first drop stored ghost ids.
- Chat list: `selected_chat_ids()` is the checked ids that are in the rows the
  list shows (folder, Unread, community, archive); count and bulk actions use
  it. The folder/filter list code moved to `listed_main_chats` /
  `listed_archived_chats` so render and selection share it.

## How verified
Unit tests in src/state/tests/composer_targets.rs; GPUI tests in
src/ui/derived_state_tests.rs; demo captures (ready-reply, -edited, -deleted,
ready-selection, -deleted, ready-chats-select-all, -filtered) in light and dark.
