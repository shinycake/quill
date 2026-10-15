# Managed-bot request keyboard button

Checked against tdesktop `api/api_bot.cpp` (`ButtonType::CreateBot`),
`boxes/peers/create_managed_bot_box.cpp`, and TDLib a94b83d
(`keyboardButtonTypeRequestManagedBot`, `shareUsersWithBot`).

## What tdesktop does

- The button (MTP `requestPeerTypeCreateBot`) carries an id, a suggested
  name and a suggested username. Pressing it hides a single-use keyboard,
  then opens `CreateManagedBotBox` with the chat's bot as manager and the
  suggestions prefilled. `viaDeeplink` stays false.
- On success it sends `messages.sendBotRequestedPeer` for the button id with
  the new bot as the only peer, opens the new bot's chat and shows a toast.
- Errors (name taken, `BOT_CREATE_LIMIT_EXCEEDED`, flood) are shown in the box
  or a toast; the box stays open.

## What changed

- `KeyboardButtonType::RequestManagedBot` now carries
  `RequestManagedBotSpec { id, suggested_name, suggested_username }`.
- The button is enabled. It opens the existing create dialog
  (`ui/bot_create_dialog.rs`) prefilled, managed by the chat's bot, and
  calls `createBot ... via_link:false`. A one-time keyboard hides first.
- When `createBot` answers, Quill sends `shareUsersWithBot` with
  `keyboardButtonSourceMessage`, the button id and the new bot as the single
  shared user (`only_check:false`), then opens the new bot's chat as the link
  flow does. Failed creates reuse the dialog's error line.
- The tooltip text is a new `qll_misc_create_a_bot_this_bot` key; the
  "not supported" key is gone.

## How verified

- Unit tests: envelope parsing of the three fields
  (`tests_keyboards.rs`), `createBot` with `via_link:false`
  (`connect/tests/owned_bots.rs`); the `shareUsersWithBot` body is already
  covered by `share_requests_use_the_keyboard_button_source`.
- Full gate. No UI test: the create dialog has no existing UI test harness.
- Unverified: a live press against a real bot (no account in the cloud
  container); the `BOT_CREATE_LIMIT_EXCEEDED` text is the link flow's
  generic hint, not tdesktop's Premium-aware toast.
