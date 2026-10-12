# Business bot manage bar (`parity:chrome-business-bot-bar`)

## What tdesktop does

`history_view_contact_status.cpp`, `BusinessBotStatus`: when a connected
business bot manages a private chat, a bar sits at the top of the history
with the bot's photo, its name and a status line. The status reads "bot
manages this chat" when the bot can reply, "bot paused" while paused, and
"bot has access to this chat" when it can only read. A round button shows
"Stop" while the bot runs and "Start" while it is paused; it is hidden when
the bot can neither reply nor is paused. A "..." menu has "Manage bot"
(opens the bar's `manageUrl`) and "Remove bot from this chat" (worded
"Revoke access to this chat" when the toggle button is hidden). Pause and
resume go through `Chatbots::togglePaused`, removal through
`Chatbots::removeFrom`.

## What changed

- `src/business_bot_bar.rs`: `BusinessBotManageBar`, the parser, the status
  and button logic, and the two request builders
  (`toggleBusinessConnectedBotChatIsPaused`,
  `removeBusinessConnectedBotFromChat`, schema 1.8.68).
- Envelope: `chat.business_bot_manage_bar` rides on `UpdateNewChat`, and
  `updateChatBusinessBotManageBar` has its own `ChatsPayload` variant.
- State: `ChatsState::business_bot_bars` with `Session::business_bot_bar` /
  `set_business_bot_bar` (`src/state/business_bot_bar.rs`). Two new chats
  request purposes route failures into the existing status-note error.
- Driver: `src/connect/business_bot_bar.rs`. Pause/resume flips the bar at
  once, remove drops it at once; the server update has the last word.
- UI: `src/ui/business_bot_bar_ui.rs`, one line in `chat_top_bars` (after the
  contact-status bar). "Manage bot" follows tdesktop and opens the
  `manage_url` (a t.me link, routed in the app; other URLs go to the
  browser) rather than the Business settings page. Button wording is
  tdesktop's: "Start", not "Resume".

## How verified

- Unit tests: parse (empty, null, no bot), status and button states for
  running, paused and read-only bots, request JSON, and a driver test with
  recorded `updateNewChat` / `updateChatBusinessBotManageBar` JSON that
  checks the sent requests and the optimistic state.
- Visual: a temporary demo kind (not committed) rendered the bar for the
  running and paused states with `QUILL_DEMO_CAPTURE`. No real bot was
  paused or removed.
- Unverified: the avatar shows initials until the bot's photo is
  downloaded, and the open menu was not captured (static frames only).
