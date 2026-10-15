# Gift value box

Part 2 of `quill-tools/designs/fragment-ton-flows.md` (parity item
`premium-fragment-ton`). No parity fragment: the item is done only once the
gift export PR also lands.

## What tdesktop does

- The unique gift box has a "Value" row (`lng_gift_unique_value`) when the
  gift carries an estimate (`unique->value`): "~$2,487.50" and a small
  "learn more" pill (`MakeUniqueGiftValueValue`,
  `boxes/gift_premium_box.cpp`). The pill sends
  `payments.getUniqueStarGiftValueInfo` once (a second click while it runs
  does nothing) and opens the value box with the answer; later clicks reopen
  it from what is known.
- `UniqueGiftValueBox` (`settings/settings_credits_graphics.cpp:2536`), no
  title: the gift sticker, the price in an accent pill
  (`FillAmountAndCurrency`, no "~"), an about line, the table, the
  availability links and OK.
- About line: when `lastSalePrice > averagePrice`, "This is the price at
  which **{gift #number}** was last sold on {Telegram|Fragment}."
  (`lng_gift_value_about_last`), otherwise "This is the average sale price of
  **{title}** gifts on Telegram and Fragment over the past month."
  (`_about_average`). The flag `is_value_average` is not read.
- Table (`AddUniqueGiftValueTable`): Initial Sale (date, when set), Initial
  Price "★ {stars} (~{amount})" (always), Last Sale (when set), Last Price
  with a change pill "+150%" (whole percents from 10 %, two decimals below,
  only with an initial price), Minimum Price and Average Price (when not
  zero) with a tooltip each. Dates are `lng_month_day_year`.
- Links "{count} {emoji} for sale on {platform} >"
  (`lng_gift_value_availability`): Telegram when listed there (opens the
  resale list, `ShowStarGiftResale`), Fragment when listed there (opens
  `fragment_url`).

## What changed

- `src/premium_hub/gift_value.rs`: `upgradedGiftValueInfo` parsing (negative
  amounts read as 0), the request, the lookup with stale-answer checks, the
  about-line choice, the table rows as data, price/date/percent formatting,
  the availability links. `Collectible` now reads `value_currency` and
  `value_amount`.
- Request plumbing: `PaymentsPurpose::GetGiftValueInfo`,
  `PaymentsPayload::GiftValue`, reducer in
  `state/domains/payments/gift_value.rs`, driver
  `connect/gift_value.rs` (`request_gift_value`). An answer or error lands
  only for the request in flight; asking about another gift drops the older
  one. The request is a read; nothing moves Stars, Grams or gifts.
- `src/ui/gift_value_ui.rs`: the Value row (in the received gift box and the
  resale buy box), and the kit `Dialog` (420 px): sticker, price pill,
  about text with the bold name, a kit `DescriptionList` table with a kit
  `Tag` for the change and `Tooltip`s on the info marks, the links as ghost
  `Button`s, OK.
- Strings in `lang/en/money.strings` under tdesktop's keys (including its
  `lng_gift_vlaue_average_price` spelling) plus `lng_month_day*`;
  `qll_gift_value_couldnt_load` for the error.
- Demo `ready-gift-value` with `QUILL_DEMO_GIFT_VALUE=box|average|row|loading|error`
  from the recorded fixtures in `tests/fixtures/gift_value/`.

## Deviations

- The design asked for the last-sale line only when the value is not
  average-only; tdesktop ignores `is_value_average`, so Quill does too.
- Prices use Quill's fiat formatter (`ton_wallet::fiat::format_fiat` with
  its currency signs) and the ISO exponent now shared as
  `currency_exponent`. For USD this matches tdesktop ("$2,487.50"); for some
  other currencies the sign differs ("JP¥" where tdesktop writes "¥") and
  tdesktop's per-currency separators are not applied.
- Quill shows one kit dialog at a time, so the value box covers the gift box
  instead of stacking on it; OK returns to the gift box. "for sale on
  Telegram" closes the value box and opens the resale list (tdesktop opens
  the list above the box).
- While the value loads, the pill is inert and a kit `Spinner` shows next to
  it; a failure shows in red under the row (tdesktop shows nothing).
- The Fragment link is offered only for an `https://` URL and opens through
  `link_policy`.
- The `{emoji}` in the links is the gift's sticker drawn small, not a custom
  emoji.

## How verified

- Unit tests (`premium_hub/gift_value_tests.rs`): parsing of the recorded
  answers and broken fields, request shape, price/date/percent formatting,
  about-line choice (last above average, Fragment vs Telegram, average
  only), table rows and tooltips, availability order and the https check,
  stale answers.
- Driver tests (`connect/tests/gift_value.rs`): request carries only the
  name; a duplicate click sends nothing; a stale answer and a stale error
  are dropped; the live answer opens the box; a known value reopens without
  a request; a failure is shown and a retry sends again.
- GPUI UI tests (`ui/gift_value_ui_tests.rs`): the box parts, OK closing it,
  the spinner while loading, reopening a known value, the error line, and
  the Telegram link opening the resale list.
- Screenshots of every demo view, compared against tdesktop's layout.

Not verified: the request against a live account (fixtures only), and the
Fragment link opening in a browser.
