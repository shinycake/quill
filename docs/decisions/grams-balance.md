# Grams balance (Settings → My Grams)

PR 1 of the Fragment/TON design (`quill-tools/designs/fragment-ton-flows.md`
section 1). Part of parity item `premium-fragment-ton`; the gift value box
and the gift export flow are still to come, so the item is not ticked.

## What tdesktop does

- `settings_main.cpp`: a "My Grams" row (`lng_settings_currency`, TON icon)
  right after the Stars row, with the short balance as its label, shown
  while the Gram balance is not empty. `Credits::tonLoad` asks for the
  balance when Settings is built.
- `settings_credits.cpp` with `CreditsType::Ton`: title "Gram Balance",
  about "Offer Grams to submit post suggestions to channels on Telegram.",
  the "Top-up via Fragment" button opening
  `lng_suggest_low_ton_fragment_url` (`https://fragment.com/ads/topup`),
  "Your balance: {emoji} {amount}" with the balance as
  `FormatCreditsAmountDecimal`, the `≈$` line (`ToUsd`, 3 decimals, app
  config `ton_usd_rate`) only for a non-zero balance, and the divider text
  "You can top-up your Grams balance via Fragment."
- `BuildCurrencyWithdrawalSection`: only when the user's own Gram revenue
  (`EarnStatistics`) has an available balance: the amount, its `≈$` value,
  a Withdraw button (`Api::HandleWithdrawalButton`: 2-step password, then
  the Fragment URL) and "You can withdraw your Grams using Fragment."
- History: nothing until there are rows; the All / Incoming / Outgoing
  tabs only when both directions exist, otherwise the "All Transactions"
  title. Rows (`CreditsRow`): refund / pending / failed as the second line
  first, "Fragment" for Fragment entries, gift names for gift deals;
  amounts as `+12.50` with two cut decimals.

## What changed

- `src/premium_hub/ton.rs`: `tonTransactions` / `tonTransaction` parsing
  with every `TonTransactionType` (unknown ones become `Unsupported`),
  `gramRevenueStatistics` and `updateGramRevenueStatus`, the request
  builders (`getTonTransactions` limit 50, `getGramRevenueStatistics`),
  amount formatting and `GramsHub` (paging, stale-answer guard, tabs rule,
  Settings row rule). State lives in `PremiumHub::grams`.
- Plumbing: `PaymentsPurpose::Grams`, `PaymentsPayload::Grams`, reducer in
  `state/domains/payments/grams.rs`, driver in `connect/grams.rs` (probe
  once, open, more, tab switch).
- `src/ui/premium_ui/grams.rs`: the dialog in the Stars screen's layout
  with the currency switched (balance header, kit `TabBar`, rows with the
  peer photo / sticker / glyph, receipt with From/To/Recipient, Via
  Fragment, status, date, Transaction ID). A user or channel the session
  does not know yet shows a skeleton bar (never the raw id) while the poll
  loop's `grams_tick` asks `getUser` / `getChat` once per id; one TDLib
  refuses reads "Deleted Account" (`lng_deleted`).
- Rows are keyed by `TonTx::key`: the id, or `index-date` when the id is
  empty or repeats, so a row without an id is kept and opens its own
  receipt (it has no Transaction ID line). The Settings row is
  added after Stars in `navigation.rs`. Top-up goes through
  `link_policy::open_decision` and `open_message_url`.
- Strings in `lang/en/money.strings`: tdesktop keys where tdesktop has the
  text (`lng_credits_currency_summary_*`, `lng_credits_box_history_entry_*`,
  `lng_credits_summary_history_tab_*`, `lng_settings_currency`, …) and
  `qll_grams_*` for Stake, Payout, Transaction and the load error.
- Demo `ready-grams` with `QUILL_DEMO_GRAMS=overview|receipt|settings`,
  from the fixture `tests/fixtures/grams/ton_transactions.json`.

## Deviations

- The Settings row is "My Grams" as in tdesktop (the design said "Grams"),
  shown only for a non-zero balance (tdesktop), not for "or history".
- tdesktop works on MTProto entries and has no titles for some TDLib
  types: "Gram Top-Up" (`lng_credits_box_history_entry_currency_in`, which
  tdesktop defines) for a Fragment deposit, "Received Gift" for a gifted
  deposit, "Stake" / "Payout" / "Transaction" are Quill keys.
- No Withdraw button in the withdrawal section yet: it needs the 2-step
  password chain (`getGramWithdrawalUrl`), which the gift export PR builds
  for `getUpgradedGiftWithdrawalUrl`; the section shows the amount, the
  `≈$` value and the note. Withdrawal stays a follow-up of this item.
- Quill's Settings list is centered label buttons without icons, so the
  row shows "My Grams 💎 12.75" instead of an icon and a right-aligned
  label. The Gram sign is the 💎 glyph the Money page uses.
- The copied Transaction ID says so in the status line, not a toast.
- The balance's `≈$` line keeps 3 decimals on purpose: that is tdesktop's
  `ToUsd(.., 3)` for this screen (the withdrawal amount uses 2).
- The gift-sale commission (`commission_per_mille`,
  `commission_gram_amount`) is not shown and not parsed: tdesktop's
  receipt has no commission line for Gram entries (`lng_credits_commission`
  is only the bot affiliate title in `settings_credits_graphics.cpp`).
- The Settings balance probe runs from the poll loop while Settings is
  open, not from render. Settings is opened from five places
  (`navigation.rs`, `suggestions_block.rs`, `passkeys.rs`, `advanced_ui.rs`,
  `deep_link_routes.rs`) with no shared open hook, so the tick that already
  resolves names checks `settings.open` instead.
- History pages through a "Load more" button, as Quill's Stars history
  does. tdesktop's `CreditsController` loads 20 more rows on scroll up to
  100 and then shows "Show more"; scroll loading inside a kit `Dialog`
  would be new to Quill and should land for Stars and Grams together.

## How verified

- `gate.sh`: GATE OK.
- Unit tests (`premium_hub/ton_tests.rs`): every type from the fixture,
  an unknown type, rows without a type dropped and without an id kept,
  unique keys for empty and repeated ids, row titles/details/peers per
  type, amount formats, paging and tabs, request shapes (including
  `getUser`) against `schema/td_api.tl`, revenue parsing.
- Driver test (`connect/tests/grams.rs`): the tick probes only while
  Settings is open and once; open sends the page and the revenue request;
  unknown users and the channel are asked for once each, a `user` answer
  is stored, a refused `getChat` marks the chat missing; offset paging; a
  stale page and a stale error after tab switches are ignored, the live
  error stops loading; only reads are sent.
- UI tests (`premium_ui/grams_tests.rs`): the screen with balance,
  withdrawal and rows, tab switch, receipt and back, close; the Settings
  row only with a balance; an unknown buyer is a placeholder, then
  "Deleted Account" once TDLib refuses it.
- Screenshots of the screen, a receipt and Settings via `ready-grams`
  (the demo seeds the fixture's buyer, seller and channel names).

Not verified: a live account's history (no Grams on the test account was
used), Fragment opening in the browser (not clicked in tests).
