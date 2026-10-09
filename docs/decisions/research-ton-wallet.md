# Research: the Telegram TON wallet (Gram wallet), read-only first

Status: research only. No feature code. Priority: very low.
Date: 2026-10-09. Author: research pass for the owner's request to "document it as a parity thing"
and plan read-only support "with very careful legal considerations".

This is not legal advice. The legal section collects facts and public sources and turns them into
questions for a lawyer. Anything marked **(uncertain)** was not confirmed from a primary source.

## Summary

- **What it is.** In 2026 Telegram added a built-in, self-custodial wallet for Gram, the TON
  blockchain's native coin (renamed from Toncoin in mid-2026). Durov announced it in July 2026 and
  began a limited rollout on 2026-08-31. The older custodial "Wallet" bot was renamed "Walt" and
  is now a separate product.
- **TDLib.** The client API ships in **TDLib 1.8.68** (tdlib/td master `c15d3f5a`). Quill pins
  **1.8.67**, and none of the 104 new wallet, TON Connect, on-ramp and exchange-rate constructors exist there.
  `textEntityTypeTonAddress` is also new in 1.8.68. Every phase below therefore depends on a
  separate TDLib bump.
- **Custody model, from TDLib source.** TDLib never generates, derives or stores the wallet's
  private key. The client app holds the secret phrase, derives the keys, and builds and signs
  transfers itself: `sendTonWalletTransfer` takes already-serialized transfer bytes. The official
  iOS app keeps the secret encrypted on the device behind a Keychain item and a passcode session.
  An app that implements sending therefore *is* the key-holding wallet software. There is an
  optional **server-side backup**. When the user enables it, TDLib splits the secret phrase into
  three XOR shares and encrypts each one to a different Telegram data center's public key. Export
  later reassembles the shares, after a 2-step-verification password check if one is set.
- **Read-only is cheap and low-risk.** Wallet state (address, public key, balance), transactions,
  NFTs, other users' public addresses, exchange rates, and the two new message types can all be
  read without any secret. Quill can show all of them without holding keys or moving value.
- **Nothing in the API is marked "official apps only".** That is unlike Firebase auth or the iOS
  store links in the same schema. Instead, most wallet methods are gated on a server option,
  `getOption("can_use_ton_wallet")`. Whether Telegram sets that option for third-party `api_id`s
  is unknown **(uncertain)**. Quill users bring their own `api_id`.
- **Recommendation.** Phase 1 should be display only: render TON addresses as copyable entities,
  render wallet-transfer and TON Connect messages as plain cards, show public addresses on
  profiles, and optionally show the user's own address, balance and history. Quill must never
  hold keys, never sign or broadcast anything, and never open an on-ramp. Everything else waits
  for a security review and a legal review.

## 1. API inventory (TDLib 1.8.68, `td/generate/scheme/td_api.tl` @ c15d3f5a)

Line numbers refer to that file. "Gated" means the schema says
`can be used only if getOption("can_use_ton_wallet")`.

### 1a. Read-only, no secret needed

| Constructor | Line | Notes |
|---|---|---|
| `loadTonWalletState` → `updateTonWalletState(tonWalletState)` | 16155, 11395 | Gated. The state is `address`, `public_key`, `gram_amount` (balance in nano-units), `is_being_created`, `is_backup_enabled`, `can_enable_backup`, `can_export_phrase`. It contains no secret. TDLib caches it and reloads it about every hour (`next_wallet_state_reload_at_ = now + 3500`). |
| `getTonWalletTransactions(direction, offset, limit)` → `tonWalletTransactions` | 16145 | Gated. Each item has peer address, peer user id, peer domain, date, fee, state (`Succeeded{tx_hash}` / `Failed`) and type (`OnRampDeposit`, `Transfer`, `NftTransfer`, `KeyChange`). Transfer comments may be `is_comment_encrypted`, and decrypting them **needs the private key**, so read-only display has to show a placeholder. |
| `getTonWalletTransaction(id)`, `getTonWalletTransactionByMsgHash(hash)` | 16148, 16151 | Single-transaction lookups. |
| `getTonWalletNfts(offset, limit≤20)` → `tonNfts` | 16221 | Gated. Each NFT has collection and NFT address, owner, name, description, image, thumbnail, content, sticker, attributes and `extra` JSON. |
| `getUserTonWalletAddresses(user_ids)` → `userTonWalletAddresses` | 16165 | Gated. Returns the public wallet address and public key of other users. Users without a wallet are skipped. This is what makes "address on profile" possible. |
| `getAddressTonWallet(address)` → `userTonWalletAddress` | 16171 | Gated. Reverse lookup from an address to a user id (0 if unknown). TDLib validates the address checksum before sending. |
| `getCurrencyExchangeRates` → `currencyExchangeRates` | 16216 | ISO 4217 currency to USD rates. Not gated. Useful for showing a balance in fiat. |
| `loadTonWalletGaslessTransfersInfo` → `updateTonWalletGaslessTransfersInfo` | 16158, 11398 | Gated. Remaining free ("gasless") transfers per day and the relayer address. Only meaningful if Quill can send. |
| `checkWalletBotBalance` → `walletBotBalance` | 16161 | Gated. Whether the user still has a balance in the legacy custodial bot `t.me/walt`, plus a URL to open it. |
| `getTonConnectSessions` → `tonConnectSessions` | 16225 | Gated. Lists the dApp sessions (manifest name, URL and icon, state, creation date). Reading the list is harmless. **Disconnecting is an action** (see 1d). |
| `updateTonWalletTonConnectSession`, `updateTonWalletTonConnectSessionDisconnectRequired` | 11401, 11404 | Session updates. The second one *requires* the client to disconnect sessions "in a regular way", which a read-only client cannot do (see the open questions). |
| `getTonCenterStreamingApiUrl` | 16324 | A TON Center streaming URL, proxied by Telegram. |

### 1b. Secrets: secret phrase, backup, ownership proofs

| Constructor | Line | What it touches |
|---|---|---|
| `getTonWalletSecretPhrase(password)` → `Text` | 16175 | **Returns the plaintext secret phrase.** Works only when `can_export_phrase` is set, which means the server backup exists. TDLib checks the 2-step-verification password with SRP, fetches the three encrypted shares from three data centers, decrypts them with ephemeral keys, and XORs them together (`TonWalletManager.cpp` `do_get_ton_wallet_secret_phrase_with_parts`, `on_get_ton_wallet_secret_phrase_part`). |
| `enableTonWalletBackup(secret_phrase, proof)` | 16183 | **The client hands the plaintext phrase to TDLib.** TDLib pads it, splits it into three XOR shares, and encrypts each share to one of three "backup holder" data-center keys (`enable_ton_wallet_backup`). It also needs an ownership proof. |
| `disableTonWalletBackup(password)`, `disableTonWalletBackupWithProof(proof)` | 16187, 16191 | Removes the server backup. |
| `getTonWalletOwnershipProofChallenge` → `{payload, domain}` | 16178 | A challenge that the client must **sign with the private key** to build a `tonWalletOwnershipProof{public_key, timestamp, signature}`. |
| `replaceTonWallet(password, anchor_public_key, proof)` | 16213 | Swaps in another existing v6r2 wallet or rotates the private key. This is what produces the `KeyChange` transaction type. |
| `deleteTonWallet(password)` | 16207 | Deletes the current wallet and creates a new one. **Funds risk.** |

### 1c. Money movement

| Constructor | Line | Notes |
|---|---|---|
| `sendTonWalletTransfer(user_id, address, amount, comment, is_comment_encrypted, sending_id, regular_transfer_data, gasless_transfer_data)` → `tonWalletTransferResult` | 16203 | The client passes **pre-built, signed** transfer bytes. TDLib only relays them and posts a local `messageTonWalletTransfer`. Minimums come from `ton_wallet_transfer_amount_min` and `ton_wallet_gasless_transfer_amount_min`, both 10^8 nano-units (0.1 Gram) by default. |
| `createUserTonWallet(user_id)` | 16168 | Creates a wallet address for another user, so that someone without a wallet can receive. Requires a positive balance. This is a server-side side effect, so it is not read-only. |
| `getOnRampProviders`, `getOnRampProviderBaseCurrencies`, `getOnRampPaymentAvailability`, `getOnRampPaymentLimits`, `getOnRampPaymentQuote` | 16275–16302 | Fiat-to-crypto purchase discovery. Availability returns the user's **country code and state** and whether purchase is allowed, so the server applies a jurisdiction check. |
| `createOnRampPaymentSession(...)` → `{session_id, expiration_date, url}` | 16316 | Returns an HTTPS URL "to be opened in a WebView to complete the purchase". The third-party provider handles payment and KYC. |
| `sendTonCenterApiRequest(endpoint, Get/Post)` | 16321 | A generic proxy to the TON Center v3 API. GET is read-only, but **POST can broadcast signed messages** (TON Center exposes a message-sending endpoint). Treat it as money movement. |

### 1d. TON Connect (connecting the wallet to dApps and mini apps)

`createTonConnectSession` (16230), `setTonConnectSessionWalletClientId`, `sendTonConnectSessionConnectResult`,
`getTonConnectSessionPendingRequests`, `getTonConnectDAppPendingRequests`, `claimTonConnectRequest`,
`answerTonConnectRequest` (16263), `getTonConnectSessionNextEventId`, `disconnectTonConnectSession` (16271).

- Telegram's servers act as the TON Connect bridge. Pending requests arrive as **service messages
  in chat 777000** (`tonConnectRequest.message_id`).
- `wallet_client_id` is an X25519 key "calculated from the TON wallet private key", and connecting
  answers a 104-byte challenge. **Every TON Connect write needs the private key.** Requests can ask
  for the address and for an ownership proof (`tonConnectConnectItemProof`), and request bodies
  carry transaction-signing RPCs.
- `claimTonConnectRequest` claims a request "for processing on this device". A client that claims
  a request and then cannot sign it would block the user's other devices. A read-only Quill must
  never claim.

### 1e. Links, message contents, entities, push, suggestions

| Constructor | Line | Read-only treatment |
|---|---|---|
| `textEntityTypeTonAddress` | 6074 | A TON address entity in message text, also returned by `getTextEntities`. It can be rendered and copied. |
| `richTextTonAddress(text, address)` | 4392 | The same, inside Instant View pages. |
| `messageTonWalletTransfer(sender_user_id, transaction_id, peer_address, amount, comment, is_comment_encrypted)` | 5870 | "A transfer with the TON wallet of the current user". Amount is signed (negative means outgoing). It can be rendered as a card with no actions. |
| `messageTonConnectRequest(session_id, state, dapp_name, topic, trace_id)` | 5878 | State is `Pending{expiration_date}`, `Accepted` or `Rejected`. It can be rendered as a card; accept and reject must not be offered. |
| `pushMessageContentTonWalletTransfer(sender_user_id, amount, comment)`, `pushMessageContentTonConnectRequest(dapp_name)` | 9036, 9040 | Notification text. The comment appears only "if it is public". |
| `internalLinkTypeTonWalletTransfer(receiver, gram_amount)` | 9987 | A "send Grams" link with a prefilled receiver (username or address) and amount. **Money movement.** |
| `internalLinkTypeTonConnect(version, dapp_client_id, connect_request, return_strategy, rpc_request, trace_id)` | 9982 | A universal TON Connect link. **Needs keys.** |
| `suggestedActionViewFirstTonWalletTransferHint` | 10381 | A one-time hint after the first incoming transfer. |
| `settingsSectionMyGrams` | 9655 | A settings deep link for the *Telegram-held* Gram balance (see below). |

**Not to be confused with.** 1.8.67 already has a separate, *custodial-by-Telegram* Gram balance:
`updateOwnedGramCount`, `getTonTransactions` and `tonTransaction*`. That balance comes from
Fragment deposits, suggested-post payments, gift resale and stake dice. Telegram Desktop shows it
as a "Currency" row in Settings. The existing parity item `premium-fragment-ton` covers it, and
this document does not change that item.

**Restrictions found in the schema comments.** None of the wallet methods say "official
applications only". The only gates are `can_use_ton_wallet` and per-state flags
(`can_export_phrase`, `can_enable_backup`, `is_backup_enabled`). In the request handlers
(`Requests.cpp`), TDLib itself only enforces `CHECK_IS_USER()`, so bots cannot use these methods.

## 2. What the official clients do

- **Telegram Desktop 7.2.10** (`~/Developer/Reference/tdesktop`) ships **no self-custodial
  wallet**. Its MTProto scheme has no `wallet.*` methods, no TON address entity and no
  wallet-transfer message. It has:
  - Number formatting for TON amounts (`ui/controls/ton_common.cpp`; a `style_wallet.h` include is
    commented out).
  - The Telegram-held Gram balance as a "Currency" settings row
    (`settings/sections/settings_main.cpp:542-557`; its search keywords include "wallet").
  - `TonAddressUrl` (`boxes/gift_premium_box.cpp:1380`), which appends an address to the
    `ton_blockchain_explorer_url` app-config value (default `https://tonviewer.com/`). It opens
    gift NFT owner and gift addresses.
- **Telegram X** (`~/Developer/Reference/tgx`) has only a commented-out drawer item left from the
  2019 wallet era (`navigation/DrawerController.java:538, 839`). It has no 1.8.68 wallet support.
- **Telegram iOS** (public repo, `TelegramMessenger/Telegram-iOS`, read via the GitHub API) ships
  the wallet:
  - `submodules/TelegramCore/Sources/TelegramEngine/Wallet/Wallet.swift` calls
    `wallet.getState`, `getUserAddresses`, `getTransactions`, `sendTransfer`, `getGaslessInfo`,
    `getExistingWaltBalance` and the TON API proxy.
  - `submodules/WalletContext/Sources/` covers backup, collectibles, fiat rates, gasless, TON
    Connect, Toncenter, transfers and storage.
  - `WalletSecurity.swift` wraps the wallet secret in a versioned envelope whose key lives in a
    **non-synchronizable Keychain item**, unlocked through a passcode session.

  This confirms the keys live on the device.
- **Telegram Android** (`DrKLO/Telegram`): GitHub code search found no wallet API usage
  **(uncertain: the code search index may lag; not verified further)**.

Takeaway: desktop parity today means *no* wallet. Mobile is the only reference for the full
wallet. Quill's phase 1 would already go beyond Telegram Desktop for the display-only pieces
(the entity and the two message types).

## 3. Legal and compliance considerations (facts and questions, not advice)

### 3a. Telegram's own terms

- **Telegram API Terms of Service** (<https://core.telegram.org/api/terms>) have no wallet,
  crypto or payments clause. The relevant parts:
  - Users' privacy must be guarded "with utmost care", and apps must follow the Security
    Guidelines (§1.1; <https://core.telegram.org/mtproto/security_guidelines>).
  - Apps must not take actions on the user's behalf without the user's knowledge and consent
    (§1.4).
  - Branding rules (§2.2–2.4).
  - All monetization must be disclosed (§3.2).
  - Telegram notifies the developer of a breach, and if it is not fixed within 10 days it may
    revoke API access and contact app stores (§4.1–4.2).
- **Grams Wallet Terms of Service** (<https://telegram.org/tos/wallet>):
  - Last updated **2019-10-08**, offered by Telegram FZ-LLC.
  - Users must be 18 or older, and use is barred for sanctioned countries and individuals.
  - The terms say Telegram does not access or store keys, backup phrases or passwords, and cannot
    recover lost credentials.
  - Liability is capped at USD 10, under English law with LCIA arbitration.

  **(Uncertain)** This document predates the 2026 Gram wallet, and it is unclear whether it
  governs it. Its statement that Telegram stores no backup phrases also sits awkwardly next to the
  1.8.68 opt-in backup, which places encrypted shares of the phrase on Telegram's data centers.
  No 2026 wallet terms were found.
- **Mini App blockchain guidelines** (<https://core.telegram.org/bots/blockchain-guidelines>)
  apply to Mini Apps, not to clients. They require TON-only assets and TON Connect for wallet
  interactions, and forbid forking or modifying the TON Connect SDK. They matter only if Quill ever
  acts as a TON Connect wallet for mini apps.
- **Server gate.** `can_use_ton_wallet` is set by the server. Telegram may decline to enable the
  wallet for unofficial `api_id`s, may enable it, or may change its mind later **(uncertain)**.

### 3b. Who holds the keys (custody)

- **From the TDLib source.** TDLib does not create or keep keys. The app does. TDLib persists no
  phrase: the phrase passes through memory only for `enableTonWalletBackup` and
  `getTonWalletSecretPhrase`. If Quill implemented sending, Quill would be the self-custodial
  wallet software running on the user's machine.
- **Grey area: the server backup.** With backup on, three Telegram data centers each hold one
  encrypted XOR share. One center alone learns nothing, but Telegram's infrastructure as a whole
  can serve all three shares to an authenticated session that passes the password check.
  `replaceTonWallet` and the `KeyChange` transaction type show the key can be rotated through
  Telegram's API. Whether either of these gives Telegram "control" in a regulatory sense is a
  question for counsel. For a read-only Quill it is moot, because Quill would never touch these
  methods.
- **US.** FinCEN guidance FIN-2019-G001 (2019-05-09), §4.2.1, treats providers of unhosted
  (software) wallets as not money transmitters, because the user, not the provider, controls the
  value
  (<https://www.fincen.gov/resources/statutes-regulations/guidance/application-fincens-regulations-certain-business-models>;
  summary: <https://www.coincenter.org/new-tornado-cash-indictments-seem-to-run-counter-to-fincen-guidance/>).
  The guidance is interpretive, and state money-transmitter laws are separate.
- **EU.** MiCA (Regulation (EU) 2023/1114) defines custody as safekeeping or controlling crypto-
  assets, or the means of access such as private keys, *on behalf of clients* (Art. 3(1)(17)).
  Recital 83 says providers of non-custodial hardware or software wallets should not fall within
  scope (<https://cms.law/en/esp/legal-updates/safeguarding-the-digital-vault-custody-and-administration-of-crypto-assets-under-the-new-mica-regulation>,
  <https://dfns.co/article/custodial-or-non-custodial-under-micar>). These are secondary sources;
  verify against the Official Journal text (OJ L 150, 9.6.2023).

### 3c. What read-only avoids, and what later phases would trigger

| Activity | Likely exposure (to be confirmed by counsel) |
|---|---|
| Show addresses, balances, transactions, NFTs, transfer and TON Connect message cards | Quill holds no keys, moves no value and offers no exchange. The open points are accuracy (fiat conversions and balances should say where they come from and may be stale) and privacy (showing another user's address links their identity to public on-chain history, although Telegram serves that data). |
| Open a blockchain explorer link (`ton_blockchain_explorer_url`) | It links out to a third party, as Telegram Desktop already does for gift NFTs. |
| Hold a secret phrase or private key; sign; send (`sendTonWalletTransfer`, TON Connect answers) | Quill becomes self-custodial wallet software: likely outside FinCEN money transmission (§4.2.1) and outside MiCA custody (recital 83), *if* nobody but the user controls keys. It also brings a duty of care for key security and **consumer-protection** expectations (irreversible transfers, wrong-address warnings, fee disclosure). OFAC's 2021 virtual-currency guidance lists wallet providers among the businesses it addresses (<https://home.treasury.gov/system/files/126/virtual_currency_guidance_brochure.pdf>; summary <https://www.cooley.com/news/insight/2021/2021-10-29-ofac-sanctions-compliance-guidance-virtual-currency-industry>). Whether an open-source desktop client must screen addresses or block jurisdictions is an open question. |
| On-ramp (`createOnRampPaymentSession`) | The third-party provider does KYC, payment and its own licensing, and Telegram's availability check applies a country and state gate. Quill would still be **promoting and referring users to buy crypto**. In the UK, cryptoasset financial promotions have had to be made or approved by an authorised or registered firm, with mandatory risk warnings, since 2023-10-08 (FCA PS23/6; <https://www.fca.org.uk/publications/good-poor-practice/firms-preparations-cryptoasset-financial-promotions-regime/printable/print>). Similar marketing rules may apply elsewhere **(uncertain)**. |
| Server backup and phrase export | Quill would display or relay the most sensitive secret the user has. This adds no new regulatory category, but the security bar and liability are the highest here. |
| Distribution through app stores (not done today; Quill ships on GitHub Releases) | Apple App Review 3.1.5(i) allows wallet apps only from developers enrolled as an organization (<https://developer.apple.com/app-store/review/guidelines/>). Google Play's crypto exchange and software wallet policy requires local licences in listed countries, but the Help Center says non-custodial wallets are out of scope (<https://support.google.com/googleplay/android-developer/answer/16329703>). A Financial Features declaration is still needed. |

### 3d. Security obligations, if anything beyond read-only is ever built

- **Never persist the phrase in plaintext.** Use the OS secret store: the macOS Keychain
  (non-synchronizable, like the iOS app), Windows DPAPI or Credential Manager, and the Linux Secret
  Service. Linux has no universal secret store, and Quill's cross-platform rule requires parity
  on all three platforms.
- **Zeroize secrets in memory.** Keep them out of `Debug`, `tracing` and panic output. Quill's
  TDLib request and response logging must redact `getTonWalletSecretPhrase` results and the
  `enableTonWalletBackup` input. TDLib's own INFO log prints the backup-share token, though not
  the phrase.
- **Clipboard.** Mark copies as concealed (macOS `org.nspasteboard.ConcealedType`; Windows
  `ExcludeClipboardContentFromMonitorProcessing` and clipboard-history exclusion) and auto-clear
  them. Linux has no standard equivalent.
- **Screenshots and screen sharing.** Exclude phrase screens from capture (macOS
  `NSWindow.sharingType = .none`; Windows `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`).
  Linux again has no equivalent. Quill's in-process `demo-capture` feature must never render
  wallet secret screens.
- **Confirmation UX** for irreversible transfers: show the full address, the amount and fee in
  both Gram and fiat, and a warning for a first-time recipient. This also satisfies API ToS §1.4
  (no actions without the user's knowledge and consent).
- **Phishing.** `internalLinkTypeTonWalletTransfer` and TON Connect links can be sent by anyone.
  They must never auto-fill and send.

### 3e. Open questions for counsel

1. Does Telegram permit unofficial clients (third-party `api_id`s) to use the wallet API? Will
   `can_use_ton_wallet` be set for them, and is there or will there be a wallet-specific term
   beyond the API ToS?
2. Do the 2019 Grams Wallet terms govern the 2026 Gram wallet? If they do, how do they square
   with the opt-in server backup?
3. Does showing a user's own balance, history and fiat value in a third-party client create any
   duty (accuracy disclaimers, financial-information rules) in the EU, UK or US?
4. Does showing *other* users' wallet addresses on their profiles raise GDPR or other privacy
   issues for Quill, given that Telegram serves the data and the user can presumably control it?
   **(uncertain: no privacy setting for this was found in the schema.)**
5. If Quill later holds keys and signs transfers, does it stay a "non-custodial software
   provider" under FinCEN §4.2.1, state money-transmitter laws and MiCA recital 83, given that
   the backup and key-rotation paths run through Telegram?
6. Sanctions: what is expected of a free, open-source desktop wallet front-end, for example
   address screening, geo-blocking, or relying on Telegram's server checks?
7. On-ramp: would a "Buy" button that hands off to Telegram's on-ramp providers count as a
   financial promotion (UK) or marketing (EU and elsewhere)? Does it need a risk warning or an
   authorised approver?
8. Who carries liability for lost funds caused by a Quill bug, given that the project's MIT
   license disclaims warranty? Is a separate wallet disclaimer or terms of use needed?
9. App-store distribution, if it happens later: under Apple 3.1.5(i), who is the "organization"
   developer for an open-source project?

## 4. Phased plan

### Prerequisite

Bump TDLib 1.8.67 → 1.8.68 as its own task, with the usual schema re-vendor and regression pass.
Phase 1 has no dependencies beyond that.

### Phase 1: read-only display (planned)

Guardrails, all of which are hard requirements:

- **No keys.** Never call `getTonWalletSecretPhrase`, `enableTonWalletBackup`,
  `getTonWalletOwnershipProofChallenge`, `replaceTonWallet` or `deleteTonWallet`. Never store or
  accept a secret phrase.
- **No sending.** Never call `sendTonWalletTransfer`, `createUserTonWallet`, any TON Connect write
  (`create*`, `set*`, `send*`, `claim*`, `answer*`, `disconnect*`), any on-ramp method, or
  `sendTonCenterApiRequest` with POST.
- **Links do not act.** `internalLinkTypeTonWalletTransfer` and `internalLinkTypeTonConnect` show a
  short notice that wallet actions are available in Telegram's official apps. They never prefill
  or perform anything.
- **Cards have no buttons that act.** Allowed actions are copy-address and open-in-explorer only.
- **Gate on the option.** Call wallet getters only when `can_use_ton_wallet` is true. Otherwise
  hide the wallet UI. Entities and message cards still render, because they arrive with messages.
- **Logs.** Wallet objects are not logged above `debug`. Addresses and amounts never appear in
  crash reports.

Items:

1. **TON address entity.** Render `textEntityTypeTonAddress` like other interactive entities, with
   left-click and right-click "Copy Address" (mirroring `LinkTarget::BankCard` and "Copy Card
   Number" in `src/ui/entity_links.rs`) and "Open in Explorer" (`ton_blockchain_explorer_url`, as
   Telegram Desktop does for gifts). Also render `richTextTonAddress` in Instant View
   (`src/rich.rs`).
2. **Wallet transfer card.** Render `messageTonWalletTransfer` showing direction, amount in Gram
   with the fiat value next to it, the peer (user name if known, otherwise the shortened address
   with copy), and the public comment. An encrypted comment shows as "Encrypted comment" and is
   never decrypted.
3. **TON Connect request card.** Render `messageTonConnectRequest` showing the dApp name, the
   topic, and the state (pending until a given time, accepted, rejected, or expired). It has no
   accept or reject buttons. The card says the request must be handled in an official Telegram
   app.
4. **Previews and notifications.** Chat-list, reply and notification previews for both contents,
   and push texts from `pushMessageContentTonWalletTransfer` and
   `pushMessageContentTonConnectRequest`.
5. **Public address on profiles.** On user profiles, show the wallet address from
   `getUserTonWalletAddresses` with copy. Fetch it on demand when the profile opens, and never
   prefetch in bulk.
6. **Own wallet overview (optional).** A read-only Settings page showing the address (copy, QR,
   explorer), the balance from `updateTonWalletState` with fiat from `getCurrencyExchangeRates`,
   and the backup *status* as a flag only. A clear note says Quill cannot send, back up or
   restore.
7. **Own history and NFTs (optional).** Paged `getTonWalletTransactions` and `getTonWalletNfts`,
   view-only.
8. **Wallet links.** TON transfer and TON Connect links get the notice described in the
   guardrails.
9. **First-transfer hint.** Show `suggestedActionViewFirstTonWalletTransferHint` as an
   informational hint and dismiss it via `hideSuggestedAction`.

Verification for phase 1:

- Unit tests that map each new constructor to its view model.
- A test that fails if a forbidden method name appears in the request builders.
- Demo-capture screenshots with fixture data only, and no real addresses in captures.

### Phase 2 and later: only after a security review and a legal review (deferred)

- Receive flow polish (QR sharing). This is low risk but sits next to the send UI.
- Decrypting encrypted comments (needs the private key).
- Creating or importing a wallet, holding the secret phrase locally, and backup enable, disable
  and export.
- Sending Grams to a user or address, gasless transfers, and creating a wallet for a recipient.
- TON Connect: connecting to dApps and mini apps, approving or rejecting requests, managing and
  disconnecting sessions (including handling `updateTonWalletTonConnectSessionDisconnectRequired`).
- Buying Grams through on-ramp providers.
- Replacing, rotating or deleting the wallet.

### Never, without an explicit security and legal sign-off

- Generate, import, store, display, copy, log or transmit a secret phrase or private key.
- Call `getTonWalletSecretPhrase` or `enableTonWalletBackup`, or relay a phrase anywhere.
- Sign anything: transfers, ownership proofs, TON Connect challenges or responses.
- Call `sendTonWalletTransfer`, `createUserTonWallet`, `deleteTonWallet` or `replaceTonWallet`.
- Call `claimTonConnectRequest` (it would steal the request from the user's official device).
- Open an on-ramp purchase session or show a "Buy" call to action.
- Send a POST through `sendTonCenterApiRequest`.
- Act on a `tg://`/`t.me` TON transfer or TON Connect link beyond showing a notice.
- Present fiat values as advice, or add any yield, staking, swap or trading feature.
- Include wallet screens with real data in captures, screenshots or crash reports.

## 5. Parity dashboard

The README's Status section gains `### TON wallet (low priority)` with ids `parity:wallet-*`.
Phase-1 items are tagged "(planned: phase 1, read-only)", and the rest are tagged "(deferred:
needs security & legal review)". All items start unchecked.

## Sources

Primary:

- TDLib schema: <https://github.com/tdlib/td/blob/c15d3f5a5de6e3ba5839822c451152e5e18bb700/td/generate/scheme/td_api.tl>
- TDLib wallet implementation: <https://github.com/tdlib/td/blob/c15d3f5a5de6e3ba5839822c451152e5e18bb700/td/telegram/TonWalletManager.cpp>
- Telegram iOS wallet code: <https://github.com/TelegramMessenger/Telegram-iOS/tree/master/submodules/WalletContext/Sources>
- Telegram API Terms of Service: <https://core.telegram.org/api/terms>
- Telegram security guidelines: <https://core.telegram.org/mtproto/security_guidelines>
- Grams Wallet Terms of Service (2019): <https://telegram.org/tos/wallet>
- Mini App blockchain guidelines: <https://core.telegram.org/bots/blockchain-guidelines>
- FinCEN FIN-2019-G001: <https://www.fincen.gov/resources/statutes-regulations/guidance/application-fincens-regulations-certain-business-models>
- OFAC virtual currency guidance: <https://home.treasury.gov/system/files/126/virtual_currency_guidance_brochure.pdf>
- Apple App Review Guidelines 3.1.5: <https://developer.apple.com/app-store/review/guidelines/>
- Google Play crypto exchange and software wallet policy: <https://support.google.com/googleplay/android-developer/answer/16329703>

Secondary (verify before relying on them):

- Cointelegraph, July 2026 announcement: <https://cointelegraph.com/news/telegram-launch-native-self-custody-gram-wallet-for-over-1-billion-users-durov>
- Rollout and rename of the custodial bot to Walt: <https://cryptocurrencyhelp.com/news/telegram-rolls-out-gram-wallet-walt/>
- Limited rollout, September 2026: <https://www.crowdfundinsider.com/2026/09/304524-telegram-opens-gram-wallet-to-limited-group-ahead-of-broader-userbase-rollout/>
- MiCA custody commentary: <https://cms.law/en/esp/legal-updates/safeguarding-the-digital-vault-custody-and-administration-of-crypto-assets-under-the-new-mica-regulation>, <https://dfns.co/article/custodial-or-non-custodial-under-micar>
- FinCEN §4.2.1 commentary: <https://www.coincenter.org/new-tornado-cash-indictments-seem-to-run-counter-to-fincen-guidance/>
- OFAC guidance summary: <https://www.cooley.com/news/insight/2021/2021-10-29-ofac-sanctions-compliance-guidance-virtual-currency-industry>
- FCA crypto promotions: <https://www.fca.org.uk/publications/good-poor-practice/firms-preparations-cryptoasset-financial-promotions-regime/printable/print>
