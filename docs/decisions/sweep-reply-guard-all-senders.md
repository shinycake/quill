# Every sender takes the reply through one guard

## What tdesktop does
`ComposeControls` / `FieldHeader` keep the reply as an id and cancel it when the
message is destroyed (`itemRemoved`, `Flag::Destroyed`). Nothing that sends can
see a reply to a removed item. When an edit cannot start, tdesktop shows the
`lng_edit_error` toast ("You can't edit this message.").

## What changed
- `ComposerUi.pending_reply` is private. The ways to touch it are `reply()`
  (display and bookkeeping), `has_reply`, `set_reply` and `take_reply`.
- Senders read the reply with `QuillApp::reply_for_send(cx)` or
  `send_reply_for(chat_id, cx)` (src/ui/composer_targets.rs). Both drop a reply
  whose message was deleted before returning it. `cancel_gone_reply` is private
  to that module, so a new sender has no unguarded way to read the reply: it
  does not compile. This replaces the per-site `cancel_gone_reply` calls from
  #901 and covers the senders that had none: GIF/animation, polls, checklists,
  the share dialogs (contact, location, dice, venue), the rich composer, the
  attachment/album snapshots, and voice and video notes.
- `Session::message_deleted` answers for the places `reply_target` looks in:
  the chat history, the open thread and the topic histories of the chat. The
  delete update now records the tombstone in the open thread and in every topic
  history of the chat, not only the chat history, so a stale page cannot bring
  the row back there either.
- `begin_edit` on a message whose text is not loaded shows the
  `lng_edit_error` toast instead of refusing silently.

## Findings that were wrong or already fine
- Inline-bot results (`connect/bots.rs`) never read `pending_reply`: the driver
  call sends `reply_to: None`. There is nothing to guard. Carrying the reply
  into an inline result is a separate feature, not done here.
- A delete update always tombstones the chat history, whichever histories are
  loaded, so the thread/topic gap needed no behaviour fix beyond making the
  predicate and the writers consistent.

## How verified
Tests: `a_gif_drops_a_reply_to_a_deleted_message_and_is_still_sent`,
`a_sticker_drops_a_reply_to_a_deleted_message_and_is_still_sent`,
`shared_content_drops_a_reply_to_a_deleted_message`,
`a_poll_drops_a_reply_to_a_deleted_message`,
`a_checklist_drops_a_reply_to_a_deleted_message`,
`a_reply_to_a_message_that_is_there_passes_through` (src/ui/reply_guard_tests.rs);
`a_delete_is_a_tombstone_in_the_open_thread_and_every_topic_of_the_chat`,
`a_message_deleted_only_in_the_thread_or_topic_it_was_found_in_is_gone`
(src/state/tests/composer_targets.rs); the not-loaded edit test now checks the
toast. Demo capture of the toast in light and dark.
