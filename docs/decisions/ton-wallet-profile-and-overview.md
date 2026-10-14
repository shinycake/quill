# TON wallet, phase 1: the profile and own-wallet side (read-only)

Branch `ton-wallet-profile-and-overview`. Follows `docs/decisions/research-ton-wallet.md`, phase 1,
items 5 to 9, and builds on `docs/decisions/ton-wallet-messages.md` (#749).

## What the reference clients do

- **Telegram Desktop** has no wallet UI: no wallet address on profiles, no wallet settings page,
  no wallet links. Its only related piece is `TonAddressUrl` (the `ton_blockchain_explorer_url`
  app-config prefix), which Quill already uses for "View in Explorer".
- **Telegram iOS** (public repo, read through the GitHub API) is the only reference, so this PR
  follows it, minus everything that acts on the wallet:
  - `PeerInfoSettingsItems`: a Settings row "Money", shown only when the wallet configuration is
    available, with the balance on the right once it is above 0.01 Gram.
  - `WalletScreen`: the balance in Gram with its fiat value under it, then Transactions and
    Collectibles tabs. The fiat value is `ton_usd_rate` (app config) times the
    `getCurrencyExchangeRates` rate for the chosen currency (`WalletFiatRates.swift`,
    `TonFormat.swift`), USD by default, picked from the iOS currency list.
  - `WalletTransactionItemComponent`: a row shows the user's name, the domain or the shortened
    address, a subtitle ("Incoming transfer", "Deposit", "Withdrawal", "Failed", ...) and a signed
    amount.
  - `WalletTransactionScreen` / `WalletNftScreen`: the details of one transaction or collectible,
    with "View in Explorer" (`walletTransactionExplorerUrl`: the explorer prefix,
    `transaction/` and the hash in hex).
  - `WalletFirstGramsScreen` (`checkFirstGramsOnAppear`): "Your first Grams!" over the first
    incoming transfer's details while the server still suggests
    `suggestedActionViewFirstTonWalletTransferHint`; dismissing it hides the suggestion.
  - `OpenResolvedUrl.swift` (`.sendGrams`): a wallet link opens the Send or TON Connect screen, or
    says "Wallet is currently unavailable." when the wallet is off.
  - A transfer card in a chat opens the wallet's transaction screen.

## What changed

- `src/ton_wallet/`:
  - `own.rs`: `OwnWallet` (address, balance, `is_being_created`, `is_backup_enabled`; the public
    key is not kept), `WalletTx` / `TxKind` / `TxPage`, `WalletNft` / `NftPage`, and the iOS row
    wording.
  - `fiat.rs`: the iOS currency list and symbols, exchange-rate and `ton_usd_rate` parsing, and the
    fiat formula and format.
  - `links.rs`: the notice for `internalLinkTypeTonWalletTransfer` and `internalLinkTypeTonConnect`.
  - `payload.rs`: the wallet request purposes and TDLib answers (they ride in the payments domain,
    as gift management does).
  - `READ_ONLY_METHODS`, the wallet methods Quill may send, and `TonWalletState` grows the option,
    rates, own wallet, Money page and profile addresses. The fiat currency is kept per account in
    `ton_wallet_prefs.json`, as iOS stores `selectedFiatCurrency`.
- `can_use_ton_wallet` (`updateOption`) drives everything. While it is false no wallet getter is
  sent, the Settings row, Money page and profile row do not show, and turning it off drops what was
  loaded.
- Profile (`parity:wallet-profile-address`): opening a user's info panel sends
  `getUserTonWalletAddresses` for that one user, once per session, never for yourself and never in
  bulk. A user with a wallet gets a "Wallet address" row (grouped by four, monospace). A click
  copies it; right-click offers Copy Address and View in Explorer.
- Settings → Money (`parity:wallet-own-overview`, `parity:wallet-own-history`): opening it sends
  `loadTonWalletState`, `getCurrencyExchangeRates`, the app config (explorer and `ton_usd_rate`)
  and the first page of `getTonWalletTransactions`. The page shows the balance and fiat value,
  the address with Copy Address and View in Explorer, "Encrypted Backup: On/Off" as a status only,
  the currency picker, a note that Quill cannot send, receive or back up, and the Transactions and
  Collectibles tabs. Collectibles load on first view (`getTonWalletNfts`, 20 per page). Both lists
  page with "Load more". A row opens its details. An encrypted comment is drawn as spoiler dust
  with "Encrypted comment. Open it in an official Telegram app to read it."; it is never decrypted.
- A transfer card in chat opens its transaction on the Money page, through the history if it is
  loaded or `getTonWalletTransaction` if not. With the wallet off the card stays inert, as in #749.
- Links (`parity:wallet-links-notice`): a link TDLib resolves to a TON transfer or TON Connect type
  shows a notice ("Quill can't send Grams. ..." / "Quill can't connect your wallet to apps. ...",
  or iOS's "Wallet is currently unavailable." when the wallet is off). Nothing is prefilled or
  sent. `t.me/sendgrams` is added to the reserved `t.me` paths so it reaches TDLib instead of a
  username lookup.
- First-transfer hint (`parity:wallet-first-transfer-hint`): as in iOS, "Your first Grams!" comes
  up over the first incoming Gram transfer's details while TDLib suggests the action. It shows the
  current Gram rate. "Got it" or closing the page sends `hideSuggestedAction`.

## Read-only guarantees

- The only wallet methods built are `READ_ONLY_METHODS`: `loadTonWalletState`,
  `getTonWalletTransactions`, `getTonWalletTransaction`, `getTonWalletNfts`,
  `getUserTonWalletAddresses`, `getCurrencyExchangeRates`. None needs a key, and none is in
  `FORBIDDEN_METHODS`. `getApplicationConfig` and `hideSuggestedAction` are not wallet methods.
- The `FORBIDDEN_METHODS` scan now also covers `src/ton_wallet` and `src/state/domains/payments`
  (skipping the two files that list the names on purpose). A second test checks the request
  builders produce only read-only methods, and a third that the two lists never overlap.
- No Send, Receive QR, Add Funds, Buy, Trade or Keys & Backup actions. The backup is a status line.

## Deviations from the iOS app (deliberate, read-only scope)

- The Money page is a dialog with Quill's settings layout (gpui-kit Design Guides), not iOS's
  full-screen gradient header. It is built from kit components: `TabBar` (underline) for the tabs,
  `ListItem` rows for transactions and collectibles, `DescriptionList` for the backup status, fee,
  date and NFT attributes, and `Button`s for the address actions.
- No Send, Receive, Add Funds or Trade buttons; the read-only note stands in their place.
  "Your first Grams!" drops its Send and Trade buttons for the same reason.
- No QR code for the own address. The research doc lists QR under the overview, but the iOS QR
  sits in the Receive flow, which is phase 2.
- The encrypted comment uses Quill's spoiler dust (as #749), not iOS's invisible ink.
- iOS fetches the profile address as part of its peer info; Quill asks only when the profile
  opens, as the research doc requires.

## Verification

- Unit tests (`src/ton_wallet/own_tests.rs`): state, transaction, NFT and page parsing; iOS row
  wording; the fiat formula; the option gate; paging; the transaction explorer URL; link notices;
  the read-only and forbidden lists; the prefs round trip.
- Driver tests (`src/connect/tests/ton_wallet_own.rs`, recording sender): nothing is sent while
  the option is off; Money loads state, rates and history once; collectibles load on first view;
  the profile address is asked once and never for yourself; a user without a wallet settles to no
  row; a transfer card opens its transaction and the hint, and "Got it" hides the suggestion; a
  failed lookup says so; wallet links show the notice.
- GPUI tests (`src/ui/ton_wallet_money_tests.rs`): the Money page with balance, history and
  collectibles; the profile row copies the address; a transfer card opens the transaction and the
  hint.
- Visual check: demo capture with a temporary fixture (not committed, fixture addresses only) of
  the overview, a transfer with a public comment, one with an encrypted comment, a collectible, the
  hint, and the profile row.

## Unverified

- Real server data: there is no wallet-enabled account, and whether the server sets
  `can_use_ton_wallet` for third-party `api_id`s is unknown. Everything is from schema-shaped
  fixtures.
- TDLib (`LinkManager.cpp`) resolves `tg://sendgrams`, `t.me/sendgrams?to=…` and
  `t.me/sendgrams?startapp=tonconnect-…` (TON transfer and TON Connect) and `tc://…` (TON
  Connect). Quill's deep-link pipeline accepts only `tg:` and `t.me` links, so the first three get
  the notice. A `tc://` link is not a Telegram link shape for Quill: the OS does not hand it to
  Quill and a click on one in a message is refused (not `http(s)`), so nothing happens either way.
- Whether the server sends `ton_usd_rate` to third-party clients. Without it the fiat line shows
  "—".
