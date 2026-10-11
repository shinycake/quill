# Paid messages

Items: `parity:premium-paid-messages`, `parity:composer-paid-gates`,
`parity:admin-direct-messages`.

## What tdesktop does

- Messages privacy box (`boxes/edit_privacy_box.cpp`): Everybody, My
  Contacts and Premium Users, and "Charge for messages" with a Stars slider
  (1-99 by 1, then by 10 to 1000, then by 100 to the server maximum), the
  line "You will receive N% of the selected fee", and an Exceptions row
  ("Remove Fee") backed by the NoPaidMessages privacy list.
- Composer (`history_view_compose_controls.cpp`, `history_view_about_view.cpp`):
  a user who restricts new chats replaces the composer with a Premium prompt;
  a user who charges shows "{user} charges {amount} for each message".
- First paid send asks "Confirm payment" with "Don't ask me again"
  (`ShowSendPaidConfirm`); the trusted price is stored per chat and a new
  price asks again.
- A user who pays you gets a bar with Remove Fee; the box offers "Refund
  already paid N Stars" after reading the revenue.
- Channel Direct Messages box (`EditDirectMessagesPriceBox`): toggle plus a
  price slider that starts at Free.

## What changed

- `src/paid_messages.rs`: policy, slider steps, copy, send gate, trust,
  confirmation text, channel choice, per-account state
  (`payments.paid_messages`).
- `src/telegram/requests_paid_messages.rs`, `src/connect/paid_messages.rs`:
  `canSendMessageToUser`, `getPaidMessageRevenue`,
  `allowUnpaidMessagesFromUser`, `setChatDirectMessagesGroup`;
  `set_new_chat_settings` saves the price;
  `messageSendOptions.paid_message_star_count` now carries the agreed price.
- `PrivacySettingKey::AllowUnpaidMessages` reuses the exceptions list UI.
- Parsed: `userFullInfo.incoming_paid_message_star_count`,
  `supergroupFullInfo.can_enable_paid_messages`,
  `supergroup.paid_message_star_count`, the paid-message options. TDLib has
  no Stars rate option, so tdesktop's dollar estimate is left out.
- UI: `ui/paid_messages_{privacy,send,channel,state}.rs`. One-line hooks in
  `submit_composer`, `composer_send_options`, the composer column and the
  channel settings dialog.

## Safety

A send carries a price only after the confirmation or a trusted price.
Otherwise it carries 0 and TDLib refuses it, so no Stars move by accident
(stickers, GIFs and voice notes in a paid chat fail until a text send was
confirmed with "Don't ask me again"). The charge choice starts only with
Save. "Don't ask me again" lasts for the session.

## Verification

Unit tests (gating, trust, slider steps, request JSON, driver round trips,
rollback), demo UI tests, and temporary captures of the Messages editor,
the Premium bar, the charge line with the confirmation, and the Direct
Messages box (not committed). Not verified against a live account: no paid
message was sent and no real setting changed. Album and multi-part sends
pass the per-message price; whether TDLib expects a total is unconfirmed.
The channel direct-messages chat composer for non-admin senders is not
gated.
