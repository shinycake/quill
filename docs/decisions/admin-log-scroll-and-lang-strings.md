# Recent actions scroll loading and the last hard-coded strings

## What tdesktop does

`history/admin_log/history_admin_log_inner.cpp` has no buttons. `checkPreloadMore` asks for the next page when the view is within a few viewport heights of the top (older events) or bottom (newer ones). A timer (`kNewEventsCheckInterval`, 5 s) calls `requestNewEvents`, so new events appear without any action. The list is chronological, newest at the bottom.

## What changed

- `src/ui/event_log.rs`: the Refresh and Load more buttons are gone. The events sit in a scrolling list (`event-log-list`, tracked `ScrollHandle` in `AdminUi`). A wheel scroll near the end calls the existing deduped `fetch_chat_event_log_more`; while older pages exist the list ends in a kit `Spinner` row. Failed first loads keep their Retry button; filters, admin chips and search are unchanged.
- Live updates: `start_event_log_poll` runs one timer task while the Recent actions page is open. Every 5 s, when the window is active and the list is at the top, `poll_chat_event_log` sends the newest page (`GroupsPurpose::PollChatEventLog`). The reducer merges it by event id into the loaded page and leaves `has_more` alone. A poll answer that arrives after a filter change cleared the page is dropped.
- Hard-coded text: every UI string literal the checker found, plus the notes in toggle_chat_pin, chat_list, mute_menu, secret_chats, stickers_ui and the single-word labels, now goes through `tr(keys::...)`. tdesktop keys and wording are used where tdesktop has the text, `qll_leftovers_*` otherwise (`lang/en/leftovers.strings`). `scripts/check-lang.py` also counts a literal handed straight to `.child/.label/.tooltip/.placeholder/.aria_label/.title/.description` or `StatusNote::*`, so a one-word label cannot slip back in.

## Deviations

- Order stays newest first (TDLib order, existing rows), so older events load at the bottom and new ones appear at the top, not tdesktop's chronological list. Live updates wait while the reader is scrolled down so rows do not jump.
- `src/ui/message_text/mod.rs` keeps one sample-text literal (a fixture body, not UI).

## Verified

`ready-event-log` demo (light, dark, end of list with spinner), state and driver tests for the poll, `check-lang.py`.
