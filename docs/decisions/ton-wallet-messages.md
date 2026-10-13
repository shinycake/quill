# TON wallet, phase 1: the message side (read-only)

Branch `ton-wallet-messages`. Follows `docs/decisions/research-ton-wallet.md`, phase 1, items 1-4.
TDLib is 1.8.68 (`schema/README.md`, commit `c15d3f5a`), which has every constructor used here.

## What the reference clients do

- **Telegram Desktop** (`~/Developer/Reference/tdesktop`, 2026-10-02) has no wallet: no TON address
  entity, no wallet messages. It has `TonAddressUrl` (`boxes/gift_premium_box.cpp:1380`), which
  appends an address to the `ton_blockchain_explorer_url` app-config value, default
  `https://tonviewer.com/`.
- **Telegram X** has nothing for the 1.8.68 wallet.
- **Telegram iOS** (public repo, read through the GitHub API) is the only reference, so this PR
  follows it:
  - `ChatControllerOpenTonAddressContextMenu.swift`: a tap or long press on a TON address opens a
    menu with *Send Money* (only when the wallet is available), *Copy Address* (toast "Address
    copied"), *View in Explorer* (`ton_blockchain_explorer_url` + "/" + address), and a
    *View Profile* row when `getUserAddresses` maps the address to a user.
    `ChatMessageTextBubbleContentNode` acts only on a 48-character address that
    `WalletContext.transferAddress` accepts.
  - `ServiceMessageStrings.swift`: a transfer reads "%1 sent you %2" / "You sent %1 %2" /
    "Someone sent you %2" (service chat), with the amount from `formatTonAmountText(…,
    maxDecimalPositions: 3)` and `Currency.Grams` ("%@ Gram" / "%@ Grams").
    A TON Connect request reads "Wallet request from X", "… accepted", "… declined" (hardcoded
    English in iOS, marked TODO). Pending requests show no expiry.
  - `ChatMessageTransferBubbleContentNode.swift`: under the service line, a 216 pt card with a
    "received" (blue `0x42b0ff`) or "sent" (green `0x00cf00`) ribbon, the signed amount
    ("+5 Grams", "−5 Grams"), the chat peer's name in uppercase ("UNKNOWN USER" in the service
    chat), and the peer address in groups of four (10 pt monospace). The public comment goes under
    the card. An encrypted comment shows as invisible-ink dust, 120 pt wide.
    Tapping the card opens the wallet's transaction screen.
  - `ChatListItemStrings.swift` / `incomingGramTransferPreview`: in the chat list, a received transfer
    reads "sent you 5 Grams" without the author ("Someone sent you …" in the service chat). The
    in-app notification uses the same text with a diamond before the amount.
  - Push strings: `MESSAGE_GRAM_TRANSFER` "%1 sent you %2" (": %3" with a comment),
    `MESSAGE_GRAM_TRANSFER_UNKNOWN` "You received %@", `MESSAGE_WALLET_TONCONNECT_REQUEST`
    "You have a TON Connect request" (" from %@").

## What changed

- New core module `src/ton_wallet/`:
  - `address.rs`: checksum validation (base64url, CRC16-XMODEM, as iOS), the explorer URL, and
    the app-config parser.
  - `amount.rs`: the iOS amount format.
  - `message.rs`: the `messageTonWalletTransfer` and `messageTonConnectRequest` models and
    one-line wording.
  - `push.rs`: the push-content texts.
  - `FORBIDDEN_METHODS`: the methods a read-only client must never send.
  - `TonWalletState`, a new `Session::ton_wallet` field, holds the explorer prefix.
- `textEntityTypeTonAddress` → `TextEntityKind::TonAddress` → `LinkTarget::TonAddress` (only for a
  valid address). It is drawn like other links. A click opens the link popup with *Copy Address*
  and *View in Explorer*. A right-click adds *Copy Address* and *View in Explorer* to the message
  menu.
- `richTextTonAddress` in Instant View becomes the same entity. The link popup and tooltip now
  draw above the Instant View reader (`app_render/mod.rs`, `chat_window/body.rs`), so links
  clicked in the reader open their menu where it can be seen.
- `ServiceAction::TonWalletTransfer` / `ServiceAction::TonConnectRequest`, with their wording in
  `service_text/wallet.rs`. The transfer card is in `ui/ton_wallet_ui.rs`, hooked into
  `service_row.rs`.
- Chat list: `Session::service_text_for` (the chat-list path) uses the iOS "sent you …" form for a
  received transfer. History rows use the full line.
- Notifications: `notify::decide_notify` uses "sent you 💎5 Grams" for a received transfer. A TON
  Connect request notifies with its service line. Reply headers use `preview()`, which gives the
  service line with unknown names ("Someone sent you 5 Grams", "You sent 5 Grams").
- Explorer prefix: `getApplicationConfig` is asked once per session when a TON address menu opens
  (`ConnectDriver::fetch_ton_explorer_url`, purpose `GetTonExplorerUrl`). The `jsonValueObject`
  answer now parses as `GroupsPayload::ApplicationConfig { boosts_per_gift, ton_explorer_url }`.
  Boosts behave as before. Any app-config answer refreshes the explorer prefix. Until one
  arrives, or if it fails, tdesktop's `https://tonviewer.com/` is used.

## Read-only guarantees

- No wallet method is sent. The only request is `getApplicationConfig`. A unit test scans
  `src/telegram` and `src/connect` for every name in `FORBIDDEN_METHODS`.
- The cards have no actions. There is no *Send Money*, no transaction screen, and no decrypting.
  An encrypted comment is never printed, only the placeholder.

## Deviations from the iOS app (deliberate, read-only scope)

- No *Send Money* item, and no *View Profile* row in the address menu. The row needs
  `getUserTonWalletAddresses`, which is gated on `can_use_ton_wallet` and belongs to the
  profile-address item (`parity:wallet-profile-address`).
- A tap on the card does nothing. iOS opens the wallet transaction screen, which belongs to
  `parity:wallet-own-history`.
- The card uses Quill's service-card surface (the gift card's `secondary` fill). iOS uses a
  gradient image asset, which Quill does not have. The ribbon is a corner chip in iOS's two colors.
- The encrypted-comment placeholder uses Quill's spoiler dust surface. iOS uses invisible ink.
- An invalid address entity is plain text. iOS colors it but ignores taps.
- Pending TON Connect requests show no expiry, as in iOS. The brief asked for "pending with
  expiry", but no official client shows it, so none is invented. The parsed `expiration_date` is
  kept for later.
- Push contents: Quill takes notifications from `updateNewMessage` and never calls
  `processPushNotification`, so `pushMessageContentTonWalletTransfer` /
  `pushMessageContentTonConnectRequest` never reach it. `ton_wallet::push::push_text` words them
  as iOS does, for a future push path, and is unit-tested.

## Verification

- Unit tests (`src/ton_wallet/tests.rs`): amount format, checksum, explorer URL and config
  parsing, entity and rich-text parsing, link target and copy label, transfer and TON Connect
  wording, the encrypted comment, push texts, and the forbidden-method scan.
- Driver tests (`src/connect/tests/ton_wallet.rs`): `getApplicationConfig` is asked once and its
  explorer value applied, a failure keeps the default, chat-list previews are correct, and
  displaying wallet messages sends nothing.
- GPUI tests (`src/ui/ton_wallet_ui_tests.rs`): cards draw with the comment or the placeholder,
  and a TON address opens the popup with *View in Explorer* and copies the address.
- Visual check: demo capture with a temporary fixture (not committed) showing received, sent and
  encrypted transfers, the TON Connect line, and an address entity in a text message.

## Unverified

- Real server data: there is no wallet-enabled account to test with, so everything comes from
  fixtures that follow the schema.
- Whether the server sends `ton_blockchain_explorer_url` to third-party `api_id`s.
- The right-click message menu inside the Instant View reader still draws under the reader (it
  did before this PR). Left-click works.
