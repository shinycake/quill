# Giveaways (codex/giveaways)

Parity item `parity:premium-giveaways`, "Create and view giveaways". Partly
done: viewing and starting prepaid giveaways work; paying for a new one does
not. No parity fragment is added.

## What tdesktop does

- `boxes/gift_premium_box.cpp` `ResolveGiveawayInfo` / `GiveawayInfoBox`:
  the Learn more button under a giveaway message asks for the giveaway info
  and shows "About this giveaway" (or "Giveaway ended"): who sponsors it,
  prizes, who is picked and when, the user's own status (taking part, not
  eligible and why, won, did not win), a refund notice, and "View my prize"
  for a Premium winner.
- `info/channel_statistics/boosts/create_giveaway_box.cpp`: "Boosts via
  Gifts" form. Type (Premium, Stars, or award specific users), quantity,
  duration with prices, extra channels, countries, only new members,
  additional prize, end date, show winners. A new giveaway goes to the
  checkout; a prepaid one asks "are you sure" and calls the prepaid launch.
- `info_boosts_inner_widget.cpp`: "Get Boosts via Gifts" button and the
  "Prepaid giveaways" list on the boosts page.

## What changed

- Core: `telegram/envelope/giveaway.rs` (info, options, prepaid giveaways,
  message summary), `giveaway/` (draft, checks, `giveawayParameters` JSON,
  the box wording), request builders, `payments.giveaway` state, connect
  driver methods (`request_giveaway_info`, `request_giveaway_options`,
  `launch_prepaid_giveaway`). `chatBoostStatus` now keeps its prepaid
  giveaways. Giveaway cards carry a `GiveawaySummary`.
- UI (new files): `giveaway_info_ui.rs` (box, Learn more button),
  `giveaway_create_ui.rs` (form), `giveaway_boosts_ui.rs` (button and
  prepaid list on the boosts page), `giveaway_state.rs`. One line each in
  `service_row.rs` and `group_links_boosts_ui.rs`.
- Nothing is sent without a click: the info box and the price options only
  read, and `launchPrepaidGiveaway` goes out only from the confirmation
  step.

## Not done, and why

- Paying for a new giveaway. Quill's checkout is built on invoice messages
  (`inputInvoiceMessage`); a giveaway needs `inputInvoiceTelegram` with a
  payment purpose and a card flow. The form shows the price and says so;
  its Start button stays disabled for a new giveaway.
- Award specific users (gift codes), country list: tdesktop has them, Quill
  has no country list or user picker for it yet. Countries stay empty
  (all countries).
- Limits (extra chats 10, 7 days, 4 boosts per Premium) use tdesktop's
  fallbacks; Quill does not read the app config yet.
- The button is always shown on channel and group boosts pages; tdesktop
  hides it when `giveaway_gifts_purchase_available` is false.
- "You won" is also shown when TDLib says `is_winner` without a code or
  Stars (tdesktop would say "you didn't win").

## Verified

- Unit tests: parsing (`giveawayInfo*`, options, prepaid, message
  summaries), draft checks and parameters JSON, the box wording, request
  JSON, and driver flows (info, options, launch only on the explicit call,
  never `getPaymentForm`/`sendPaymentForm`).
- Visual check with a temporary demo (not committed): the form, a prepaid
  form and the ended-giveaway box render.
- Not verified against a live account; nothing was paid or launched.
