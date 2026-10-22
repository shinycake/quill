# A shared "not supported in Quill" notice for the wallet and TON features

Date: 2026-10-21. Branch: `wallet-not-supported-notice`.

The owner decided Quill will not build the deferred wallet features
(`wallet-decrypt-comments`, `wallet-create-import`, `wallet-secret-backup`,
`wallet-send`, `wallet-tonconnect-sessions`, `wallet-onramp`,
`wallet-gift-withdraw-card`). Instead, every place where one of them shows
up opens one shared notice: Quill doesn't support this yet, with a button
that opens https://telegram.org/apps in the system browser.

## What Telegram Desktop does

Telegram Desktop has a real wallet (`SourceFiles/wallet/`: send, receive,
add funds, keys and backup, TON Connect, create and import). It reaches it
from these places, all of which Quill now covers:

| tdesktop | where |
| --- | --- |
| "Money" in the main menu (`window_main_menu.cpp`, `lng_wallet_menu`) | Settings row, already in Quill (opens the read-only Money page) |
| Send, Receive, Add Funds, Keys & Backup, Create / Import (`wallet_panel.cpp`) | Money page |
| Encrypted transfer comments (`history_view_gram_transfer.cpp`, `wallet_comment.cpp`) | transfer card, transaction details |
| `tg://sendgrams`, `t.me/sendgrams`, `tg://ton`, TON Connect start params (`local_url_handlers.cpp`, `ResolveSendGrams`, `OpenTonConnectQuery`) | links |
| TON address entity opens the send box (`click_handler_types.cpp`, `Wallet::OpenAddressEntity`) | address menu |
| "Click to review" on a pending TON Connect request (`history_item.cpp`, `lng_action_ton_connect_review`) | service message |
| "Send Money" and the balance pill on a profile (`info_profile_balance_pill.cpp`, `lng_wallet_profile_send_money`, `lng_wallet_profile_balance`) | profile |
| "Move to My Wallet" (`transfer_gift_box.cpp`, `lng_gift_transfer_to_wallet`) | gift transfer picker |
| Send {name} on a collectible (`lng_wallet_collectible_send`) | collectible details |

## Inventory: what Quill did before, and does now

| Surface | Before | Now |
| --- | --- | --- |
| `tg://sendgrams` / `t.me/sendgrams` (`internalLinkTypeTonWalletTransfer`) | one-line banner with hard-coded English | shared notice (wallet off: "Wallet is currently unavailable.", as iOS) |
| TON Connect links (`internalLinkTypeTonConnect`) | same banner | shared notice |
| Money page: Send, Receive, Add Funds, Keys & Backup | absent; a static grey note | four buttons, each opens the notice |
| Money page with no wallet (create / import) | text "You don't have a wallet yet.", no way on | "Create a New Wallet" opens the notice |
| Encrypted comment on a transfer card | blurred placeholder plus a note, not clickable | placeholder is a button that opens the notice |
| Encrypted comment in transaction details | placeholder plus a hint line | same button; the hint line is gone |
| Collectible details (Send) | nothing | "Send {name}" opens the notice |
| "Your first Grams!" hint | text only | adds Send, opens the notice |
| TON address menu (message link popup and message context menu) | Copy Address, View in Explorer; the code said Send Money is never offered | adds Send Money (wallet on only), opens the notice |
| Pending TON Connect request message | one service line | adds "Review request", opens the notice; answered requests get nothing |
| Profile with a public wallet address | address row, copy, explorer | adds the card balance (`userFullInfo.ton_wallet_card`) and Send Money |
| Gift transfer picker, unlocked gift, wallet on | "Send via Blockchain" went straight to the Fragment flow | the row reads "Move to My Wallet" (as tdesktop) and opens the notice; with the wallet off nothing changes |
| Chat menu "Send Money" (`window_peer_menu.cpp` `AddSendMoneyAction`, in the profile actions) | absent | adds Send Money after Send a Gift in the private chat's menu, for a non-bot user with a known wallet address and the wallet on; opens the notice |
| Profile balance pill opening the card view (`info_profile_top_bar.cpp`) | n/a | not mirrored: Quill shows the balance as text under the address; a conscious choice |
| Encrypted comment and "Review request" while the wallet is off | n/a | not clickable / hidden, like every other surface |
| Gift already in a TON wallet | "Action Locked" step with Fragment's page | unchanged: that is tdesktop's own `ShowActionLocked` |

Checked and left alone, because Quill already shows something readable or
nothing appears:

- `messageTonWalletTransfer`: card plus service line (`wallet-transfer-card`).
- `messageTonConnectRequest`: service line (accepted, declined, pending).
- `messageGiftedGrams` and `messageStakeDice`: service lines
  (`service_text/actions.rs`, tests in `service_text_tests`). Not
  "unsupported".
- Chat-list, reply and notification previews (`pushMessageContentTon*`).
- TON addresses in messages and Instant View pages: highlighted, copyable.
- Gram Balance screen, top up and "Collect via Fragment", gift value box,
  gift export through Fragment: these open Fragment in the browser, they do
  not need a Quill wallet.
- Sending a stake dice (`inputMessageStakeDice`, `emoji_stake_box.cpp`) and
  the "Money" item of the chat attach menu: Quill never offered either, so
  nothing shows up to fail.
- Wallet protection in the passcode screens and the wallet warning on log
  out (`lng_sure_logout_wallet_*`): they only exist because tdesktop stores
  keys; Quill stores none.
- `internalLinkTypeOauth` with `request_ton_wallet_access` ("Connect
  Wallet" login): Quill handles no OAuth links at all today; it falls to the
  ordinary unknown-link path. That is a general OAuth gap, not a wallet one.
- The old custodial wallet bot (`isOldWalletBot`): an ordinary bot here.

## What changed

- `src/ton_wallet/notice.rs`: `WalletSurface` (which surface asked) and
  `TonWalletState::notice`. Open or closed is state; the dialog is a pure
  function of it.
- `src/ton_wallet/links.rs`: `wallet_link` returns `Notice(surface)` or
  `Unavailable`; the chats reducer sets the notice (no banner string left).
- The dialog carries the "Money" title only for actions on the Money page; elsewhere it has no title.
- `src/ui/wallet_notice.rs`: the one kit `Dialog` (registered with
  `register_dialogs!`, priority 3790 so it opens over the Money page and the
  gift picker), `notice_button` for the surfaces, and
  `show_wallet_notice` / `dismiss_wallet_notice`. The button calls
  `cx.open_url`, which the test platform records (`opened_url()`).
- One notice string `qll_money_wallet_not_supported`, one button label
  `qll_money_get_telegram_apps`, plus tdesktop's own `lng_wallet_*` and
  `lng_gift_transfer_to_wallet` keys for the entry labels.
- `UserProfileExtras::ton_wallet_card_balance` (parsed in
  `envelope_types.rs`, which is a hotspot, see the commit trailer).

## How it was verified

- Core tests: the link reducer opens the notice and sends only the three
  lookups; `wallet_link`; the card balance parse.
- UI tests (`wallet_notice_tests.rs`): every surface above opens the notice;
  OK opens nothing; the button hands `https://telegram.org/apps` to the
  platform; the Money page is still open under the notice and returns.
- Light and dark captures of the Money page, the notice over it, the chat
  surfaces, the profile, the address menu and the gift picker, read back
  by eye.
- Not verified: real accounts (the owner has no wallet-enabled account for
  tests), so no live `ton_wallet_card` payload was seen; the parse follows
  the pinned schema.

## README

The README is not edited here. The seven deferred ids are proposed as
checked items with the note "not planned in Quill; opens a notice pointing
to the official apps"; see the PR description for the lines and the parity
counting convention.
