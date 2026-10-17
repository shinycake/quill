# sweep-ui-small-round2 — four small UI fixes from the round 2 audit

## 1. Forum topic pane: loading, failed, empty

Telegram Desktop keeps its loading placeholder until the first slice of a
topic arrives and shows the empty hint only afterwards. Quill showed "No
messages in this topic" for a topic with no store entry, so every topic open
flashed a false empty state and a failed load looked like an empty topic.

- `Session::topic_history_loading` / `topic_history_load_failed`
  (`src/state/session_topic_load.rs`) derive the state from the topic store,
  the in-flight `GetTopicHistory` requests and a new failure set
  (`ThreadsState::topic_load_failed`), the same way `history_loading` and
  `history_load_failed` do for the main history. Nothing is stored that
  could disagree with the sources.
- The failure set is filled by an error answer and by the pending-request
  sweep, and cleared by a landed page, Retry, or opening the topic again.
- The pane renders the shared `history_skeleton()` while loading, a centred
  "Couldn't load messages · Retry" row on failure and the old placeholder
  only for a topic whose empty page arrived. The main history's Retry row got
  `flex_1` so it centres in the pane too.
- `fetch_topic_history` stops re-sending after a failure until Retry
  (`retry_topic_history`), like `fetch_history_for` does.

## 2. Next/previous chat keys

`step_open_chat` (Alt+Up/Down, Ctrl+Tab, Ctrl+PgUp/PgDn) now scrolls the list
to the opened row with `ScrollStrategy::Nearest`, as the keyboard selection in
`chat_list_keys.rs` does.

tdesktop (`Dialogs::InnerWidget::chatListEntryAfter/Before`, the
`ChatNext`/`ChatPrevious` handlers) returns no row past the last or before the
first one and does nothing then. Quill wrapped; it now stops at the ends. This
supersedes the wrap sentence in `codex-chat-nav-keys.md`. The step arithmetic
is the pure `step_target`.

## 3. Icon-only controls

The group call bar's mute and leave buttons and the invite row got a button
role, an accessible name, tab focus and a tooltip (mute uses "Mute"/"Unmute").
Scan of the other named files: the stage buttons and the pinned tile's
full-screen button in `group_call_tiles.rs`, the quick reply edit/delete
buttons and the passkey row menu had a label but no tooltip; they have one
now (the passkey menu label no longer hard-codes English). `earn/join.rs`,
`earn/starref_boxes.rs` and `premium_ui/promo.rs` have no icon-only
clickables (their icons sit beside visible text).

## 4. Topic tab strip

- Labels are capped at 180 px with an ellipsis; names that may be cut get a
  tooltip with the full name (`subsection_tabs_fit::label_may_be_cut`).
- The selected tab is scrolled into view once per change, with the folder
  strip's `needs_reveal` rule, so a topic opened from a link, the keyboard or
  the topic list is not left off screen and the user's own scrolling is not
  undone.
- No wheel code: GPUI maps a vertical wheel to x on a div that only scrolls
  along x (`Overflow::Scroll` branch of the scroll listener), so the strip
  already scrolls with a mouse wheel.

## How verified

State and pure-function unit tests (`state/tests/topic_load.rs`,
`ui/chat_list/step_open.rs`, `ui/subsection_tabs_fit.rs`) and screenshot demos
`ready-topic-loading`, `ready-topic-failed`, `ready-topic-empty`, captured in
light and dark with the verify skill; the group call bar tooltip by hover.
