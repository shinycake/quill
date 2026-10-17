# Replies to tasks and poll options, option menu, "Sent by", Copy Phone

Round 3 of the composer gap list (C6, M1, M2, M4).

## What tdesktop does

- `history_view_context_menu.cpp` `AddReplyToMessageAction`: a right-click on a
  checklist task (link property `kTodoListItemIdProperty`) turns "Reply" into
  "Reply to Task" and sends `todoItemId` with the reply.
- `FillPollOptionPage`: right-click on a poll option opens the option's page:
  Retract vote (when voted), Reply to Option, Copy Option, Copy Option Link,
  Delete Item (creator, or own recent option), then the "added by" row.
- `FillContextMenu`: a shared contact offers "Copy Phone Number".
- `MakeMessageAuthorAction` (shown by `canLookupMessageAuthor`): "Loading..."
  then "Sent by {user}"; clicking opens the user's profile.

## What changed

- `ReplyItem` (`Message`, `Task(id)`, `PollOption(id)`) rides on
  `ComposerReplyTo` and `SendReply`; the request builders write
  `checklist_task_id` / `poll_option_id` (before they were always 0/""). Drafts
  store and restore it (`ChatDraft.item`, draft parse), so switching chats keeps
  the target.
- Right-click plumbing copies the link pattern: the task and option rows note
  `(position, ReplyItem)` on mouse down, `open_message_menu` takes it by menu
  position into `menu_item`. Rows live in `src/ui/message_menu_ui/item_rows.rs`.
- Menu over a task: "Reply to Task" replaces "Reply". Over an option: Reply to
  Option, Copy Option, Delete Item above the usual rows. Delete Item shows only
  when `getPollOptionProperties` answered `can_be_deleted` for that option
  (TDLib decides; tdesktop's creator / pollAnswerDeletePeriod rule is inside
  TDLib). `deletePollOption` is sent only when that answer is on hand.
- "Sent by {user}" under the date lines when `messageProperties.can_get_author`;
  `getMessageAuthor` runs with the other audience requests, the row is Loading,
  then the name (opens the profile), or nothing on failure.
- "Copy Phone Number" on a contact message.
- The reply bar shows the task or option text in place of the message preview.
- New text: tdesktop keys `lng_context_reply_to_task`,
  `lng_context_reply_to_poll_option`, `lng_context_copy_poll_option`,
  `lng_context_delete_poll_option`, `lng_context_sent_by`,
  `lng_profile_copy_phone`.

## History rendering touched (owned by another thread)

Only `poll_option_row` and `checklist_task_row` (`message_poll.rs`,
`message_checklist.rs`): a right mouse-down handler that notes which item was
pressed, plus `.test_support()` on the rows. No layout change. `info_row` and
the menu's `item!` macro got `.test_support()` so tests can find the rows.

## Skipped

- Retract vote inside the option menu: the message menu already has it.
- Copy Option Link: TDLib 1.8.68 has `can_get_link` but no option-link request
  to build the `?option=` link from.
- The "added by" row: `pollOption` in 1.8.68 does not carry the adder.
- Showing the task or option inside an incoming reply bubble: that is history
  rendering.

## Verified

- Unit and driver tests: request JSON, draft parse and round trip, gating of
  Delete Item, author Loading/Ready/Failed (`connect/tests/poll_option_menu.rs`).
- GPUI tests that right-click the real option and task rows and click the menu
  rows (`message_menu_ui/item_rows_tests.rs`).
- Captures in light and dark of the option, task, contact and author menus and
  the reply bar, from the `ready-item-menus` demo.
