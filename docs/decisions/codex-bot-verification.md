# Bot verification (codex/bot-verification)

Parity item `parity:bots-verification-badge`, "Bot verification badges and
verify via bot".

## What tdesktop does

- `ui/unread_badge.cpp` `PeerBadge::drawVerified`: the verifying bot's
  custom emoji (scaled to 0.88 of the emoji size) is drawn before the name,
  not after it. Used in chat rows (`dialogs_layout.cpp`,
  `dialogs_inner_widget.cpp`), the chat header
  (`history_view_top_bar_widget.cpp`) and the profile name
  (`info_profile_badge.cpp`, `BadgeType::BotVerified`).
- `info/profile/info_profile_actions.cpp` `addBotVerify`: the verification
  description appears as a text separator under the details. It is hidden
  when the description is empty and for a bot with a main app.
- `boxes/peers/verify_peers_box.cpp` `MakeVerifyPeersBox`: a chat list for
  users and channels. A row of an account the bot already verified asks to
  remove it; any other row asks to verify it, with a description field only
  when `verifierSettings.canModifyDescription` is set (limit 70 characters
  by default). It is reached from the bot's edit screen
  (`edit_peer_info_box.cpp`), only when `botInfo->verifierSettings` exists.
- Tdesktop does not draw the icon in message bubbles next to sender names, so
  Quill does not either.

## What changed

- `src/bot_verification.rs` (new): parse `botVerification` and
  `botVerificationParameters`, which icon to show (`leading_icon`), the
  profile description rules, "already verified by this bot", the description
  to send, and the JSON for `setMessageSenderBotVerification` and
  `removeMessageSenderBotVerification`.
- `VerificationStatus` keeps `bot_verification_icon_custom_emoji_id` as
  `bot_icon` (users and supergroups). `UserProfileExtras` and
  `SupergroupFullAdmin` keep `bot_verification` from the full info.
  `BotInfo` keeps `verification_parameters`, which TDLib sends to the bot's
  owner only. `SupergroupFullAdmin` lost `Copy` because it now holds text.
- `src/connect/bot_verification.rs`: driver calls `set_bot_verification` and
  `remove_bot_verification`, with two request purposes whose failures land in
  the status line.
- UI: `ui/bot_verify_icon.rs` draws the icon before the title in chat rows,
  the chat header and the user and channel profiles, and the description row
  on the profile. `ui/bot_verify_dialog.rs` is the "Verify accounts" dialog,
  opened from the bot's profile when the bot has verification parameters. The
  icon uses the existing custom emoji stills; its id is added to the ids the
  chat list already downloads.
- Hotspot: `envelope_types.rs` gained the two `bot_verification` fields.

## How verified

- Unit tests: parsing of both objects, icon choice, profile description
  rules, already-verified detection, description limits, request JSON
  (`bot_verification::tests`), and driver tests for the two requests, the icon
  reaching `chat_bot_verification_icon`, the full-info description and
  owner-only parameters (`connect::tests::bot_verification`).
- `gate.sh` passes.
- Not verified: the rendering on screen and the dialog against a live
  account. No real account was verified or unverified.

## Not done

- A user opened from a group member list shows the icon only when its
  emoji sticker was downloaded for the chat list. The profile does not fetch
  it separately.
- The dialog lists chats already loaded, as the share picker does, with no
  server search.
