# Grams balance (Settings → My Grams)

PR 1 of the Fragment/TON design (`quill-tools/designs/fragment-ton-flows.md`
section 1). Part of parity item `premium-fragment-ton`. The withdrawal
(Collect, below) came in a follow-up PR. With the gift export (#819) and
the gift value box (#824), that PR completes the item and declares it in
`parity-fragments/grams-withdraw.txt`.

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
  the big balance button and "You can withdraw your Grams using Fragment."
  The button's label is `lng_channel_earn_balance_button` ("Collect"), and
  `withdrawalEnabled` is hard-coded to `true`: no minimum or disabled
  state on the client.
- `Api::HandleWithdrawalButton` (`api/api_earn.cpp`) with a
  `currencyReceiver` (the user, not a channel):
  - It reloads the cloud password state when the button is built. A click
    sends `payments.getStarsRevenueWithdrawalUrl` (flag `ton`) with
    `inputCheckPasswordEmpty`. `AMOUNT_TOO_SMALL` toasts "The amount is too
    small to withdraw."; `PrePasswordErrorBox` opens the security check
    ("You can withdraw only if you have:") for a missing or too fresh
    password; anything else opens the password box.
  - The password box (`PasscodeBox`) is titled
    `lng_bot_earn_balance_password_title` ("Two-step verification") with
    `lng_bot_earn_balance_password_description` ("Please enter your
    password to collect.") and Submit. With the password the same request
    returns a URL, which tdesktop opens (`UrlClickHandler::Open`) and closes
    the box. `AMOUNT_TOO_SMALL` closes the box with the toast; the box's
    own errors (wrong password, flood, ...) show under the field; anything
    else is a toast.
  - TDLib's version of the request is `getGramWithdrawalUrl password:string
    = HttpUrl`.
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
  `link_policy::open_decision` and `open_link`.
- Strings in `lang/en/money.strings`: tdesktop keys where tdesktop has the
  text (`lng_credits_currency_summary_*`, `lng_credits_box_history_entry_*`,
  `lng_credits_summary_history_tab_*`, `lng_settings_currency`, …) and
  `qll_grams_*` for Stake, Payout, Transaction and the load error.
- Demo `ready-grams` with
  `QUILL_DEMO_GRAMS=overview|receipt|settings|withdraw|withdraw-check`,
  from the fixture `tests/fixtures/grams/ton_transactions.json`.

### Collect (the withdrawal)

- The withdrawal section has the Collect button (kit `Button`, primary,
  full width) between the `≈$` line and the note, always enabled as in
  tdesktop.
- `src/premium_hub/ton_withdraw.rs`: `GramWithdraw` (the password box with
  `sending` and the last error, or the security check) and the request
  builder `get_gram_withdrawal_url`. `GramsHub` gains `withdraw`,
  `withdraw_url` and `withdraw_too_small` (both one-shot, drained by the
  UI).
- Driver (`connect/grams.rs`):
  - `start_gram_withdrawal` (Collect) only works while the screen is open
    with an available amount. It reloads the password state and opens the
    password box, or the security check when the known state has no
    password. It sends no withdrawal request.
  - `submit_gram_withdrawal` sends `getGramWithdrawalUrl` only from the
    idle password box, with a typed password, while the screen is open. A
    second Submit while the first is out is refused. The box keeps the
    request's `@extra` (never the password). Only the answer or error of
    that request counts, so a late reply to a try that was cancelled is
    not taken as the answer to a newer one.
  - `cancel_gram_withdrawal` closes the box; a URL that still arrives for
    it is dropped. Closing the screen closes the box too.
- Reducer (`state/domains/payments/grams.rs`): the `httpUrl` answer closes
  the box and hands the URL to the UI. A fresh `passwordState` with no
  password turns the idle box into the security check, as the gift export
  does.
- Errors: `AMOUNT_TOO_SMALL` closes the box and shows "The amount is too
  small to withdraw." as a toast. Everything else goes through the mapping
  the gift export already used, now shared as `password_check_failure`
  (`state/domains/payments/gift_manage.rs`): wrong or changed password,
  expired check and flood wait under the field; a missing or too fresh
  password opens the security check; anything else shows under the field.
- UI: the password box and the security check are now one shared piece,
  `src/ui/fragment_password.rs` (`PasswordBox`, `SecurityCheck`,
  `export_error_text`, `keep_password_field`, `recover_cloud_password`,
  `open_fragment_url`). The gift export's views use it too, with the same
  element ids and text as before. `src/ui/premium_ui/grams_withdraw.rs`
  holds the button, the actions, the frame upkeep and the dialog (priority
  3804, shown over the Grams screen, which shows again when it closes).
- The URL must be `https`; anything else is refused with "couldn't load
  the page" in the status line. When `link_policy` says `Open`, it opens
  in the browser. When it says `Confirm` (a look-alike address), the
  existing "Open this link?" box (`entity_links`) asks first. The gift
  export's URL goes through the same path.
- The password is read from the masked field once on Submit, handed to
  the one request and the field is emptied at once (Enter does the same).
  The field is also emptied whenever the box is not on screen. It is never
  kept in state or logged.
- Strings: `lng_channel_earn_balance_button`,
  `lng_bot_earn_balance_password_title` and
  `lng_channel_earn_out_amount_too_small` with tdesktop's text.

## Deviations

- The Settings row is "My Grams" as in tdesktop (the design said "Grams"),
  shown only for a non-zero balance (tdesktop), not for "or history".
- tdesktop works on MTProto entries and has no titles for some TDLib
  types: "Gram Top-Up" (`lng_credits_box_history_entry_currency_in`, which
  tdesktop defines) for a Fragment deposit, "Received Gift" for a gifted
  deposit, "Stake" / "Payout" / "Transaction" are Quill keys.
- Collect sends no password-less first request. tdesktop's click sends
  the withdrawal request with an empty password and routes on its error.
  Quill sends nothing until the password is submitted (TDLib answers an
  empty password locally anyway, see `gift-export.md`). The click routes
  on the password state instead: no password set opens the security
  check, otherwise the password box. A password that is too fresh is
  only found once it is submitted, and then opens the security check.
- Errors that tdesktop toasts while the box stays open show under the
  field, as in the gift export, so they are not hidden behind the box.
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
- Collect, driver tests (`connect/tests/grams_withdraw.rs`, recording
  sender, recorded JSON):
  - Collect is refused before the screen has an available amount. It
    sends only `getPasswordState`, never `getGramWithdrawalUrl`.
  - A "no password" state, late or already known, opens the security
    check.
  - The request is refused before Collect, with an empty password, after
    the screen closed and while one is out. Its exact shape is
    `{"@type","@extra","password"}`.
  - The URL answer is handed over, and dropped after Cancel. After
    Cancel and a new Submit, the first try's error and URL are both
    dropped, and only the second try's URL is handed over.
  - Every error lands as listed above, `AMOUNT_TOO_SMALL` included.
  - The password never appears in the Grams state's `Debug`.
- Unit tests (`ton_tests.rs`): the routing of `GramWithdraw` and the
  request against `schema/td_api.tl`. The gift export's tests still pass
  on the shared mapping and views.
- UI tests (`premium_ui/grams_tests.rs`):
  - The Collect button is in the section and opens the box over the
    screen. The field takes the keyboard, and Enter submits and empties it.
  - An error keeps the box open. Cancel closes it, empties the field and
    shows the screen again. The security check also shows over the screen.
  - A non-https URL is not opened. A look-alike https address opens the
    "Open this link?" box instead of the browser. Fragment's https URL is
    opened once (the test platform's `opened_url`).
- Demo screenshots of the section with Collect, the password box and the
  security check (`QUILL_DEMO_GRAMS=overview|withdraw|withdraw-check`),
  checked by eye.

Not verified: a live account's history (no Grams on the test account was
used), and the real `getGramWithdrawalUrl` answers and Fragment's
withdrawal page. Nothing was run against a live account: no password was
entered and no withdrawal URL was requested from Telegram.
