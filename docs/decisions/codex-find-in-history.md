# Find in history (gap-audit batch 11)

Branch `codex/find-in-history`. Closes: jump to date, "From:" member search,
"N of M" with paging, global search filters.

## Verified gaps
Quill had prev/next in-chat search over one 50-hit page, no sender filter,
no calendar, a "N of loaded" counter, and community chips only in global
search (`ui/search_ui.rs`, `telegram/requests/chat_list.rs`).

## Decisions

- **Calendar needs a media filter for highlights.** TDLib's
  `getChatMessageCalendar` rejects `searchMessagesFilterEmpty` with 400
  (`MessagesManager::get_dialog_message_calendar`), so a plain history
  calendar cannot show "days with messages". The box highlights days only
  when an in-chat media tab (Media/Links/Files/Music/Voice) is active, and
  says so in its hint; any day is pickable regardless.
- **Day jumps use the day's first message.** A highlighted day carries its
  first message id (`messageCalendarDay.message`). Any other day asks
  `getChatMessageByDate(local midnight - 1s)` (last message before the day),
  loads the usual around-window and settles on the next message
  (`DateJumpMode::Next`). A 404 (date precedes the chat) jumps to the oldest
  message (`DateJumpMode::Oldest`, target `FIRST_MESSAGE_ID`).
- **Entry points:** calendar button in the search bar, and the history date
  pills (floating and in-row), like tdesktop.
- **"From:"** only in basic groups and non-channel supergroups. The picker
  reuses the search field as the member filter (`searchChatMembers`, like
  tdesktop's `dialogs_search_from_controllers`); the choice is sent as
  `sender_id` and a sender or media tab searches with an empty query.
- **"N of M"** uses `total_count`; walking older prefetches the next page
  (`SearchChatMessagesMore`, appended, never re-jumps).
- **Global filters.** Chat type (All/Private/Groups/Channels, tdesktop
  `lng_search_filter_*`) via `chat_type_filter`; content tabs and a date
  window (past week/month/year -> `min_date`) follow Telegram's mobile
  clients — tdesktop has no media tabs in global search. A community pick
  still wins over chat type (TDLib takes one chat-type filter). Not done:
  archive/non-archive toggle, custom date range, public posts tab.

## Tests
Request shapes (sender, by-date, calendar, filtered `searchMessages`),
`messageCalendar` parse, calendar/paging unit tests, and recorded-JSON driver
tests in `src/connect/tests/find_in_history.rs`.

Demo kinds: `ready-jump-date`, `ready-search-from`,
`ready-search-from-hits`, `ready-search-filters`.
