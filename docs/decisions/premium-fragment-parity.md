# Premium purchase in the app and Fragment link-outs

Branch: `premium-fragment-parity`

## What tdesktop does

- `StartPremiumPayment` (`settings/settings_premium.cpp`): reads
  `premium_bot_username` and `premium_invoice_slug` from the app config. With
  a bot username it opens the bot and sends `/start <ref>` (`ref` is the
  feature the promo page was opened from, e.g. `double_limits`). Without one
  it opens the invoice link as an in-app payment form.
- Fragment is whichever `whitelisted_domains` entry contains "fragment";
  collectible numbers are the `fragment_prefixes` (default `888`).
- A tap on a collectible username or phone number asks
  `getCollectibleItemInfo` and shows `CollectibleInfoBox` (owner, purchase
  date, TON price with fiat, Learn More, Copy). `COLLECTIBLE_NOT_FOUND` copies
  the link instead.
- The phone menu of a collectible number shows a note with a Fragment link; a
  username that is taken but for sale links to Fragment.

## What changed

- The promo subscribe button runs the same bot / invoice routing
  (`src/premium_hub/purchase.rs`, `src/ui/premium_ui/purchase.rs`). The bot
  chat is opened in the app and `/start <ref>` is sent automatically; the
  "never takes payment" copy is gone. App config is requested when the promo
  page opens.
- Collectible box (`src/collectible_info.rs`,
  `src/ui/collectible_info_ui.rs`) for user phone numbers, user usernames and
  supergroup usernames, with the `getCollectibleItemInfo` plumbing and the
  `COLLECTIBLE_NOT_FOUND` fallback.
- Fragment link and prefixes from the app config (`src/premium_hub/fragment.rs`);
  the phone-menu note and the purchasable-username line use it.

## Verified

- Unit tests: routing, config parsing, feature refs, collectible parsing and
  formatting.
- Driver tests with the recording sender: app config to the bot start, the
  `getPaymentForm` request for `inputInvoiceName`, and the collectible lookup
  (request type, answer, `COLLECTIBLE_NOT_FOUND`).
- GPUI UI tests: subscribe closes the promo dialog and arms the auto start /
  invoice; the collectible box follows the lookup.
- `gate.sh`.

## Not matched, and why

- Gram balance screen ("Top-up via Fragment", withdraw via Fragment): Quill
  has no TON balance screen or `getTonTransactions` plumbing.
- Gift value box Fragment tooltips: no gift value box or
  `getUpgradedGiftValueInfo` in Quill.
- Gift transfer "Send via Blockchain", "Manage with Fragment / Open Fragment"
  and the "Action Locked" box: Quill's transfer has no recipient picker and no
  password step for `getUpgradedGiftWithdrawalUrl`.
- Bot "public links" description link: tdesktop shows it in the bot edit box;
  Quill's bot editing goes through BotFather screens.

Already matching before this change: the sign-in Fragment code hints and the
low-TON box.
