# Stars, gifts and Premium (read-only parity)

Branch `codex/stars-gifts-premium`. Strictly read-only: no purchase, payment,
transfer or gift sending exists in this slice, by design (financial actions).

## What Telegram Desktop does

- `settings/settings_credits*`: a Stars balance header with a transaction
  list (All / Incoming / Outgoing tabs) and a details box per entry (amount,
  title, counterparty, date, transaction id). It also sells Stars; Quill does
  not.
- `info/peer_gifts/*`: a grid of received gifts on a profile (sticker, price or
  collectible name, pinned/hidden markers) and a details box (sender, date,
  message). The owner can show or hide a gift and convert it to Stars.
- `history_view_service_box` / `gift_premium_box`: gift, collectible,
  Premium, Stars, gift-code and giveaway service messages draw as a centered
  card with the sticker, a title and the sender's message.
- `settings/settings_premium*`: the features list with a subscribe button.

## What changed

- `src/premium_hub.rs` (pure core): `StarAmount`, `StarTx` (title, detail and
  counterparty mapped from every `starTransactionType*`), `ReceivedGift`,
  `PremiumInfo`/`PremiumStateInfo`, and the `PremiumHub` state held in
  `Session::hub` (the Subscriptions-dialog pattern: reducer owns all state).
- `src/telegram/requests_premium.rs`: `getStarTransactions` (direction omitted
  for All), `getReceivedGifts`, `toggleGiftIsSaved`, `sellGift`,
  `getPremiumFeatures`, `getPremiumState`. Only methods present in
  `schema/td_api.tl`.
- `src/connect/premium_hub.rs`: driver methods with guards. Pages are tied to
  the newest request id so a slow answer for an old filter or owner is
  dropped. The two mutations are refused unless the open list is the signed-in
  user's own and (for conversion) the gift is convertible. Both refetch from
  the server on `ok` (never optimistic).
- Reducer: new `RequestPurpose`s, `updateOwnedStarCount`, error paths that
  release the mutation lock and surface the reason.
- Chat cards: `ServiceAction::WithCard` wraps the existing action (the one-line
  wording and chat-list preview are untouched) with a `GiftCard` for
  `messageGift`, `messageUpgradedGift`, `messageRefundedUpgradedGift`,
  `messageGiftedPremium`, `messageGiftedStars`, `messagePremiumGiftCode`,
  `messageGiveaway`, `messageGiveawayWinners`, `messageGiveawayPrizeStars`.
  The redeemable gift code is never stored in the card. The card sits under the
  pill in `service_message_row`; a "View my gifts" button appears for gifts
  sent to the user. Card stickers join the visible-file download list.
- UI (`src/ui/premium_ui.rs`): Stars dialog (balance, tabs, rows, details,
  copy id, load more), Received gifts dialog (grid, details; show/hide and
  convert only for own gifts, convert behind an inline confirmation), Premium
  dialog (features, limits, state text, "Get Premium" opens
  `https://t.me/PremiumBot`, as the folder-limit box does). Entry points:
  Settings (Stars, My gifts, Telegram Premium) and the chat "..." menu
  ("Received gifts" for private chats and channels).

## Not done (deliberately)

Buying or sending Stars/gifts/Premium, gift upgrades, transfers, resale,
pinning and collections, giveaway creation, gift-code redemption, in-app Premium
purchase. Gift pinning and collections were left out because
`toggleGiftIsSaved`/`sellGift` were the only own-gift mutations in scope.
The profile panel does not yet have a Gifts tab; the entry is the chat menu.

## How verified

- Unit tests: parsers and request shapes (`premium_hub`, `requests_premium`,
  `gift_card`) and reducer tests in `src/state/tests/premium_hub.rs`.
- Demo capture with English fixtures and fake names, looked at:
  `ready-stars`, `ready-gifts` (`QUILL_DEMO_GIFT_DETAILS=1` for the details
  box), `ready-premium`, `ready-gift-cards`.
- Not verified: live TDLib answers (no real account used), the real `sellGift`
  conversion, sticker download timing for gift grids.
