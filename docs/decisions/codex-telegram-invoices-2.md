# Gift Stars, gift codes for chosen members, countries, Premium gifts with Stars

Branch `codex/telegram-invoices-2`. Follows `codex-telegram-invoices.md`.
Completes `parity:premium-stars-balance` and `parity:premium-giveaways`.

## What Telegram Desktop does

- **Send Stars** (`boxes/gift_credits_box.cpp`, `api/api_credits.cpp`): the
  Stars page has a "Send Stars" button. It opens a contact list (not
  yourself, no bots), then a box with the packs
  `payments.getStarsGiftOptions` lists for that user. A pack starts the
  checkout with a Stars invoice that names the user (`giftPeerId`).
- **Award specific users** (`create_giveaway_box.cpp`): a row picks members
  of the channel (up to `giveaway_add_peers_max`, 10 by default). With
  members chosen the random-giveaway sections (extra channels, who may take
  part, extra prize, date, show winners) are hidden. The invoice is
  `InvoicePremiumGiftCodeUsers` with one code per member; the price is the
  one-person price times the number of members
  (`PremiumGiftCodeOptions::optionsForGiveaway`).
- **Countries** (`SelectCountriesBox`): up to `giveaway_countries_max` (10)
  countries; none means all.
- **Premium gift with Stars** (`star_gift_box.cpp`): when a Premium option has
  a Stars price the gift box shows "Pay with Stars"; the invoice then uses
  the Stars currency and the Stars price.

## What changed

TDLib 1.8.68 maps these to `getStarGiftPaymentOptions` (answer
`starPaymentOptions`) with `telegramPaymentPurposeGiftedStars`,
`telegramPaymentPurposePremiumGiftCodes`, `giveawayParameters.country_codes`
and `telegramPaymentPurposePremiumGift` with `currency` "XTR" (the schema
documents that value).

- `src/stars_gift.rs`, `src/connect/stars_gift.rs` (new): state of the Gift
  Stars dialog, open/choose/close on the driver, and the request
  (`get_star_gift_payment_options`). The answer shares the
  `starPaymentOptions` parser with the pack list and is told apart by its
  request purpose (`GetStarGiftOptions`).
- `src/ui/stars_gift_ui.rs` (new): the dialog (contacts with search, then the
  packs), opened by "Gift Stars" on the Stars dialog. A pack opens the
  existing checkout with `gifted_stars_invoice`.
- `src/giveaway/award.rs` (new): `award_user_ids` and `country_codes` on the
  draft, their toggles and limits, the per-member gift code price and the
  duration list. `parameters_json` now carries the countries (for prepaid
  giveaways too).
- `src/payment_invoice/builders.rs`: `gifted_stars_invoice`,
  `premium_gift_stars_invoice`, and the gift codes branch of
  `giveaway_invoice`.
- `src/ui/giveaway_pick_ui.rs` (new): the members and countries pages inside
  the giveaway dialog (search, checkboxes, Done), the "Choose members" and
  "Choose countries" rows, and the random-only sections of the form, moved
  here from `giveaway_create_ui.rs` so they can be hidden when members are
  chosen.
- `gift_send_ui.rs`: "Pay with Stars instead" for Premium gifts when a Stars
  price exists, with the balance.
- The Stars dialog no longer says Stars are bought elsewhere.

Nothing is paid by choosing a pack, members or a country. Every payment
still goes through Pay in the checkout.

## Follow-up: paging, option limits, frequent contacts

- **All members, paged.** The members page no longer reuses the 200-member
  `Recent` cache. `src/giveaway/members.rs` (`AwardMembers`) and
  `src/connect/giveaway_members.rs` page `getSupergroupMembers` 100 at a
  time. A typed name restarts the list with
  `supergroupMembersFilterSearch` on the server. Each list has a generation
  carried by its requests (`PaymentsPurpose::GetAwardMembers`), so an answer
  to an older search is dropped. Bots and yourself are left out but still
  count towards the offset. The next page loads from a "Load more" button
  and from any scroll-wheel event on the list (the dialog's scroll container
  gives no scroll position to hook).
- **Limits from TDLib.** `GiveawayLimits` (`src/giveaway/limits.rs`) reads
  `giveaway_additional_chat_count_max` and `giveaway_country_count_max` from
  `updateOption`; the names are in the schema's `giveawayParameters` docs and
  TDLib's `OptionManager`. The constants (10) stay as the fallback. The
  members limit uses the chat-count option because tdesktop uses one
  `giveaway_add_peers_max` for both, and TDLib maps that config key to the
  chat-count option. The draft carries the limits, the form messages name
  them (`text_for`).
- **Frequent contacts.** The Gift Stars list starts with the people from
  `getTopChats` (`topChatCategoryUsers`, the search domain's existing
  `top_chats`), under "Frequent contacts", and leaves them out of "Contacts"
  until a search starts. Skipped when the account switched frequent contacts
  off, as in tdesktop (`CollectGiftFrequentUsers`).

## Not done

- tdesktop's Gift Stars list and the send-gift list also have a birthdays
  section; Quill does not.
- A very long member list still renders every loaded row (no row recycling).

## How it was verified

- Follow-up tests: option names and fallback, the form checking its limits,
  paging and stale-generation handling of the members list, and driver tests
  with recorded JSON (recent then search filter, offsets, bots and self
  skipped, older answer dropped, non-channel refused, `updateOption` limits,
  Gift Stars also asks for the top chats).
- Unit tests: request JSON for gifted Stars, the Stars-paid Premium gift,
  gift codes and countries in the parameters; toggles and limits; per-member
  price and months; driver tests with recorded JSON (contact gets their packs,
  the answer lands in the gift state and not in the pack list, yourself and a
  closed dialog are refused).
- Gate, hotspot and file-size checks. No payment was sent; the new dialogs
  were not run against a live account.
