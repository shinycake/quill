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

## Every paid send is confirmed

`PaidGuard` (`src/paid_messages_guard.rs`) sits in the sender, the last stop
before TDLib, and screens `sendMessage`, `sendMessageAlbum`,
`forwardMessages` and `sendInlineQueryResultMessage`. So stickers, GIFs,
voice and video notes, media, polls, contacts, forwards and bot results are
covered, not only text. A request to a chat with a price goes out only when
the price is trusted; otherwise it is held and the Confirm payment box
opens. Pay releases it, Cancel drops it. A composer text send still opens
the box first so the text stays in the field on Cancel.

`messageSendOptions.paid_message_star_count` is the total for the request
(the schema: "the number of Stars the user agreed to pay to send the
messages"; albums and forwards are several messages under one options
object). The guard writes price x messages, and the box shows that total,
like tdesktop.

"Don't ask me again" follows tdesktop's `markPeerTrustedPayForMessage`: per
chat, per account, stored on disk (`accounts/<key>/paid_trust.json`), and a
stored price covers prices up to it. The entry is dropped when the user
stops charging.

## Paid supergroups and direct messages

`supergroupFullInfo.outgoing_paid_message_star_count` gates a channel's
direct-messages group (and any paid supergroup) for non-admin senders: the
composer shows the price and sends are screened like private chats. The
channel's own direct-messages price is read from `updateSupergroup` and
`getSupergroup`; opening the Direct Messages box asks for it
(`getChat` first when the group's chat is not loaded).

## Safety

Nothing carries a price the user has not agreed to. The charge choice starts
only with Save.

## Verification

Unit tests (gating, trust file round trip, guard totals), driver tests for
held, cancelled and released sends, persistence, paid supergroups and the
price fetch, demo UI tests, and temporary captures (not committed). Not
verified against a live account: no paid message was sent and no real
setting changed.
