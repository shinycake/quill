# Received gift management and sending gifts

Branch `codex/gifts`. Covers `parity:premium-received-gifts` (done) and the
Stars half of `parity:premium-send-gift` (Premium gifts are listed, not paid).

## What Telegram Desktop does

- `info/peer_gifts/info_peer_gifts_widget.cpp`: a grid of the gifts on a
  profile with a tab per collection (`All` plus one per collection), pinned
  gifts at the top (`stargifts_pinned_to_top_limit`, 6 by default; pinning
  moves a gift to the end of the pins, hiding a pinned gift unpins it).
  Pinning is only offered in the unfiltered list.
- `info_peer_gifts_collections.cpp`: create, rename, delete and reorder
  collections (up to `stargifts_collections_limit`, names of 1-12 characters),
  add or remove gifts.
- `boxes/star_gift_box.cpp`, the gift view box: show or hide on the profile,
  pin, convert to Stars (`sellGift`), upgrade (`upgradeGift`, free when the
  sender prepaid it, otherwise `upgrade_star_count`, with a "keep original
  details" checkbox), transfer (`transferGift`, `transfer_star_count`,
  recipient picker, cooldown `next_transfer_date`), sell on the marketplace
  (`UniqueGiftSellBox`, `setGiftResalePrice`, price range from the resale
  limits, cooldown `next_resale_date`).
- `GiftBox` / `SendGiftBox`: a Premium section (months and prices) above a
  grid of Star gifts with limited supply ("N of M left"), sold-out and
  Premium-only states, a message, "Hide my name and message", an optional
  prepaid upgrade, the cost on the send button, and `sendGift`.

## What changed

- `src/gift_manage/` (pure core): the extra `receivedGift` fields
  (`GiftManageInfo`: transfer price and date, resale date, listing, prepaid
  upgrade, collection ids), the gates for each action (`pin_gate`,
  `upgrade_cost`, `transfer_gate`, `resale_gate`), the limits TDLib reports
  (`getOption`: pinned, collections, resale range; tdesktop's defaults until
  they arrive), collections, `AvailableGift` with its `SendGate`, Premium gift
  options, `GiftManageState` / `SendGiftState` (held in
  `PaymentsState::gifts_manage`).
- `src/telegram/requests_gifts.rs`: builders for `getReceivedGifts` with a
  collection, `setPinnedGifts`, `upgradeGift`, `transferGift`,
  `setGiftResalePrice`, the five collection requests, `getAvailableGifts`,
  `getPremiumGiftPaymentOptions`, `sendGift`. Every name checked against
  `schema/td_api.tl` (TDLib 1.8.68).
- `src/connect/gift_manage.rs`: one driver method per action. Each repeats the
  UI's rule, so a stale button cannot build a request the gift does not allow:
  own gifts only, one mutation at a time, pin only in the unfiltered list and
  below the limit, price taken from the gift (never from the UI), balance
  check when it is known, transfer only to a user or channel of the open chat
  (not yourself, not a bot or group), resale price inside the range.
  `sendGift` also needs the dialog open, a sendable gift, a message within
  `gift_text_length_max`, and no request in flight.
- Reducer (`state/domains/payments/gift_manage.rs`): new `PaymentsPurpose`
  variants, the answers (`giftCollections`, `giftCollection`,
  `upgradeGiftResult`, `availableGifts`, `premiumGiftPaymentOptions`), `ok`
  and error handling. Nothing is optimistic: a success marks the gifts and
  collections stale and the driver refetches them on the same ingest.
- UI (new modules): `ui/gift_manage_ui.rs` (collection tabs with new, rename
  and delete, and the handlers), `ui/gift_owner_actions.rs` (pin, upgrade,
  transfer, resale and collection chips under the existing show/hide and
  convert buttons), `ui/gift_send_ui.rs` (the send dialog, registered with
  `register_dialogs!`). The profile Gift button and the chat menu entry
  ("Send a gift") open the send dialog; the old collectible-by-name dialog is
  one button inside it.

## Spending Stars

`upgradeGift` (when not prepaid), `transferGift` (when the gift has a
transfer price) and `sendGift` spend Stars. Each is behind an inline panel
whose button names the amount ("Upgrade for 30 Stars", "Transfer for 25 Stars",
"Send for 50 Stars"), is disabled when a known balance is too low, and is
refused again in the driver. The send dialog never sends on selection. Nothing
was sent, sold, transferred or paid during development.

## Not done

- Premium gifts: the options are listed with prices and discounts, but paying
  is not available. Quill's checkout only handles `inputInvoiceMessage`;
  paying needs `inputInvoiceTelegram` with `telegramPaymentPurposePremiumGift`
  and a checkout that is not tied to a chat message. The dialog says so.
  `parity:premium-send-gift` stays unticked for that reason.
- Collection and in-collection gift reordering, TON prices for resale, gift
  auctions, "drop original details", crafting, and the upgrade preview table.
- Management of a channel's gifts (needs channel admin rights); only the
  signed-in user's own gifts are managed.
- Transfer picks the recipient from the open chat instead of a picker box.

## How verified

- Unit tests: `gift_manage::tests` (parse, every gate, limits, input helpers,
  send gate), `requests_gifts::tests` (JSON of every request),
  `connect::tests::gift_manage` (driver: requests, refusals, balance and
  ownership checks, answers and refetch).
- gate.sh: GATE OK (core 3682, ui 312). Hotspot and file-size checks pass.
- Visual check with a temporary in-process demo capture (not committed) of
  the grid with collection tabs, the gift page, the upgrade, transfer and
  resale panels, and the send dialog with a selected gift.
- Not verified: live TDLib answers (no real account used, no Stars moved), the
  shape of `upgradeGiftResult` on a real upgrade, the resale cooldown error.
