# Sweep round 3 leftovers

Small review follow-ups collected in one change.

## Video recorder and probe strings

`src/video/mod.rs`: the round-video probe errors ("video note must be square",
size, duration, "unsupported video"), `Couldn't start ffmpeg ({err})` and
`recorded clip unreadable ({err})` were English and showed raw error text.
They are now `qll_ops_video_*` keys. The raw error goes to `eprintln!`. The
playback code in the same file is untouched.

## Draft after a send (`clear_draft_after_send`) - no gap

`clear_draft_after_send` returns early when the session holds no draft for the
slot. That is correct, so nothing changed.

- Text sends (`inputMessageText`, rich messages, suggested posts) already pass
  `clear_draft: true` (`requests/messages/mod.rs`). TDLib then deletes the draft
  itself and emits `updateChatDraftMessage` with no draft. The session applies
  that update, so `draft_for` is `None` and there is nothing left to clear.
- Media sends have no `clear_draft` field in the schema. The draft stays in the
  session until `updateMessageSendSucceeded` queues a `draft_clears` entry and
  `finish_successful_sends` calls `clear_draft_after_send`. `draft_for` is
  `Some` there, so the `setChatDraftMessage(None)` request is sent.
- The session never forgets a draft on its own after a send: `draft_clears`
  only records the slot. A missing draft therefore means TDLib or an earlier
  debounced `setDraft(None)` already cleared it.
- tdesktop sets `clearDraft` on the send action and clears the local draft
  (`history_widget.cpp`, `api_sending.cpp`). The Quill split matches that.
- TDLib's `MessagesManager.cpp` is not vendored, so the TDLib half rests on the
  schema docs (`td_api.tl`, "Pass true to delete message draft in the chat")
  and the existing request tests.

## Smaller items

- Demos call `open_appearance()` instead of setting `appearance_open`.
- `flush_chat_order` no longer invalidates the folder orders: they depend on
  folder positions only, and every place that changes those already invalidates
  (`apply_position_fields`, `replace_main_list_from_positions`).
  `invalidate_folder_orders` had no other caller and is removed. The folder
  order cache tests pass unchanged.
- `playlist_manage.rs`: the suggested "save as" name goes through
  `export::names::safe_file_name`.
- `settings::save_window_state` was unused since the off-thread saver and is
  removed.
- Create-new race: `window_save.rs` writes a uniquely named temp file under a
  mutex, so it has none. The real one was in `selection_mode.rs`
  `unique_destination`, which checked `exists()` and copied later. It now
  claims the name with `create_new` and removes the empty claim if the copy
  fails.
- `request_errors.rs`, `ownership_types.rs`, `requests.rs` (`PasswordOp`
  labels), `auth.rs` (`flood_line`, `flood_notice`) and three sentences in
  `ui/ownership.rs` moved to `qll_ops_err_*` / `qll_ops_owner_*` keys. English
  output is byte-identical; durations use the existing `lng_days` / `lng_hours`
  plurals. The `src/ui/ownership.rs` entry left the lang baseline.

## Verification

Unit and gate runs; no visual change in English (same strings), so no capture.
