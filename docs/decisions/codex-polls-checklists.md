# B15: polls and checklists

## What Telegram Desktop does

Sources: `history/view/media/history_view_poll.cpp`, `history_view_todo_list.cpp`, `boxes/create_poll_box.cpp`, `boxes/edit_todo_list_box.cpp`, `history/view/history_view_corner_buttons.cpp`, `window/window_peer_menu.cpp`, `Resources/langs/lang.strings`.

- **Poll creation extras.** "Allow Adding Options", "Hide results" (results appear after the poll closes), "Restrict to Subscribers" (broadcast channels only) and "Limit Duration". The duration offers presets plus a "Custom" absolute date and time (`ChooseDateTimeBox`, at least a minute ahead, at most a year).
- **Add an option.** A participant of an open poll with `can_add_option` gets an "Add an Option" row. The text field rejects empty and duplicate text, and a failed request shows "Could not add the option. Please try again."
- **Hidden results.** While `can_see_results` is false the poll shows "Results will appear after the poll ends." instead of bars.
- **Poll stats and voters.** "Poll Stats" is available to the poll's creator or admin (`can_get_poll_vote_statistics`). Voters per option page in with "Show more (N)".
- **Unread poll votes.** There is no chat-row badge. A corner button in the history ("Jump to poll votes", with a count) walks the unread votes, and its context menu has "Read all poll votes" (`readAllChatPollVotes`).
- **Checklists.** A card with the title ("Checklist" or "Group Checklist"), "N of M completed" / "None of M completed", and one row per task with a check and the person who completed it. Tapping a row toggles it when marking is allowed. "Add a task" appends tasks. The composer ("New Checklist": title, task list, "Allow Others to Add Tasks", "Allow Others to Mark As Done") is offered only to Premium users, not in broadcast channels or secret chats (`PeerData::canCreateTodoLists`). Limits come from app config: title 32, task 64, 30 tasks. Messages: "Please enter a title.", "Please enter at least one task.", "You have added the maximum number of tasks."

## What changed in Quill

Polls (`src/poll.rs`, `src/telegram/envelope/message_poll.rs`, `src/telegram/requests/{polls,poll_extras}.rs`, `src/connect/{polls,checklists}.rs`, `src/ui/{message_poll,polls,checklists}.rs`):

- `Poll` now keeps `can_see_results`, `members_only`, `open_period`, `close_date`; `PollContent` keeps `can_add_option`.
- The creation dialog adds "Allow adding options" (regular polls), "Hide results until closed", "Subscribers only" (channels only) and "Close at a set date and time". The deadline reuses the schedule date picker (`DatePicker`, one year range) and is sent as `close_date`; it replaces the hours field, because TDLib takes either `open_period` or `close_date`. `PollDraft::validate_at` checks the deadline (at least 60 s ahead, at most 365 days).
- `addPollOption`: an "Add an Option" button on open polls with `can_add_option` opens an inline panel (validated by `validate_new_option`: non-empty, 100 characters, no case-insensitive duplicate, under the option cap). Failures set the status note.
- Hidden results render empty bars and the tdesktop caption. The meta line also shows "subscribers only" and "ends in 3h 20m / N days".
- "Poll Stats" in the message menu (`messageProperties.can_get_poll_vote_statistics`, new field on `MessageActions`) runs `getPollVoteStatistics` and shows the vote graph with the existing statistics sparkline row, above the per-option voter lists. Voter lists keep paging 50 at a time; the button now reads "Show more (N)".
- Unread poll votes: `Chat.unread_poll_vote_count` from `updateNewChat`, `updateChatUnreadPollVoteCount` and `updateMessageContainsUnreadPollVotes`. A third corner button (`UnreadJumpKind::PollVote`) jumps with `searchMessagesFilterUnreadPollVote` and its menu runs `readAllChatPollVotes`.

Checklists (`src/checklist.rs`, `src/telegram/envelope/message_checklist.rs`, `src/telegram/requests/checklists.rs`, `src/ui/{message_checklist,checklists}.rs`, `src/ui/dialogs/checklist.rs`):

- `messageChecklist` parses into `MessageContent::Checklist` (tasks, completer, completion date, the four capability flags). The chat-list preview shows the title.
- The card renders the kind label, progress bar, "N of M completed" and task rows with a done mark and "Name · date" under done tasks. Tapping a task calls `markChecklistTasksAsDone` (the same method un-marks; 1.8.67 has no separate "not done" request). "Add a task" opens the Add Tasks panel (`addChecklistTasks`, ids numbered after the highest existing id).
- "Checklist" in the attach menu opens the composer panel and sends `inputMessageChecklist`. It is hidden unless the account has Premium and the chat is not a channel or secret chat (`can_create_checklist`); mark and add also refuse without Premium and set the tdesktop "Only subscribers of Telegram Premium can …" note.
- Service rows for tasks done/added already existed (`messageChecklistTasksDone/Added`).

## Skipped

- Links and media in poll options (`polls-option-media`), "Retract vote" in the message menu, deleting a poll option (`deletePollOption`), editing a sent checklist (`editMessageChecklist`), replying to a single task or option. None are part of the B15 brief.
- Task/title text is plain: entities in `formattedText` are dropped when sending (Quill's other poll fields do the same).
- Limits (30 tasks, 32/64 characters, 100-character options) are tdesktop's defaults, not read from `getOption`.
- The date-time deadline is a picker, not tdesktop's preset menu plus "Custom".
- The stats dialog shows TDLib's graph through the existing sparkline renderer; async graphs show "Still processing".

## How it was verified

- Unit tests: poll helpers (hidden results, add-option validation, "ends in" buckets, deadline bounds), checklist logic and draft validation, request JSON shapes (`addPollOption`, `getPollVoteStatistics`, `markChecklistTasksAsDone`, `addChecklistTasks`, `inputMessageChecklist`, new `inputMessagePoll` fields), envelope parsing (checklist, poll fields), driver tests (`src/connect/tests/polls_checklists.rs`: guards, request bodies, error notes, Premium gating, unread-vote counters, jump and read-all).
- `bash /Users/idan/Developer/Projects/quill-tools/gate.sh` prints GATE OK.
- The `ready-poll` demo fixture gained an addable poll with a deadline, a hidden-results poll, a group checklist and an unread-poll-vote count; captured with `QUILL_DEMO_CAPTURE` (see the PR).
- Not verified against a live account (no account was used): server behavior of the new requests, Premium gating from the server side, and `getPollVoteStatistics` graph contents.
