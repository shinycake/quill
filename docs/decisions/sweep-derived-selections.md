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
- Message selection: `QuillApp::forward_draft()` returns the picked ids that were
  not deleted (`!Session::message_deleted`, a tombstone). Unloaded is not
  deleted: the window trim (`history_trim.rs`) and `reset_window` drop rows
  without tombstones, and tdesktop `HistoryInner::itemRemoved` erases a pick on
  destroy, not on unload. Counts, the forward bar, copy, report, unpin, save,
  download and delete read it. The delete and unpin confirm boxes re-check at
  confirm time. Toggles first drop deleted ids only.
- Chat list: `selected_chat_ids()` is the checked ids that are in the rows the
  list shows (archived or left chats drop out); count and bulk actions use it.
  The list code moved to `listed_main_chats` / `listed_archived_chats`.
  One rule for the view: the selection belongs to the folder/category it was
  made in. Every folder or category change goes through
  `set_chat_list_view`, which ends the selection (tdesktop
  `InnerWidget::changeOpenedFolder` calls `clearSelection`; tdesktop has no
  persistent chat multi-select to restore). All -> Unread -> All no longer
  re-enters select mode. `selecting_chats` and the per-row `rows_selecting`
  share one predicate (`checked_among`); the render passes the chats it already
  built to `selected_among_listed`, so the list is built once per frame.
- Send time: `submit_composer`, `submit_rich_composer`, stickers and voice/video
  notes re-check the targets. A reply to a tombstoned id is dropped and the text
  is sent without it (tdesktop cancels the reply when the item is removed). An
  edit of a gone message is cancelled and nothing is sent; `submit_edit` checks
  too. `begin_edit` refuses while the message is not loaded (it would start an
  empty field that could blank the message); `edit_target_text` also looks in the
  open thread and topic histories.

## How verified
Unit tests in src/state/tests/composer_targets.rs; GPUI tests in
src/ui/derived_state_tests.rs; demo captures (ready-reply, -edited, -deleted,
ready-selection, -deleted, ready-chats-select-all, -filtered) in light and dark.
Tests: a_selection_survives_the_window_trim_and_a_window_reset (state),
changing_the_filter_or_folder_ends_the_chat_selection,
sending_drops_a_reply_to_a_message_deleted_since_the_last_frame,
submitting_an_edit_of_a_deleted_message_sends_nothing,
an_edit_of_a_message_that_is_not_loaded_does_not_begin.
