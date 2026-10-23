# Country select box

## What tdesktop does

- Shipping address (`payments/ui/payments_field.cpp`, `Field::setupCountry`):
  the country field is a read-only row. Focusing it opens `Ui::CountrySelectBox`
  (`ui/boxes/country_select_box.cpp`, type `Countries`): search field over
  flag + name rows; choosing a row fills the field and moves on.
- Polls: tdesktop does have a country restriction (`boxes/create_poll_box.cpp`,
  "Limit by Country" toggle, channels only). "Allowed Countries" shows nothing,
  the one name, or "N countries", and opens `Ui::SelectCountriesBox` (the
  giveaway's multi-select: search with chips on top, a check per row, Save and
  Cancel). The limit is app config `poll_countries_max` (fallback 12); going past
  it shows the "You can choose up to N countries" toast.

## What changed

- New shared box in `src/ui/country_box/`, pure choice logic in
  `src/country_select.rs`. Rows come from `phone::search_countries` over the
  same `getCountries` list sign-in uses (`auth_state.countries`); opening the
  box calls `fetch_countries()` when sign-in never did.
- Checkout: the typed country code is a "Country: <name>" button; one pick
  closes the box (single mode). The box replaces the form body inside the
  checkout dialog (the shell shows one kit dialog at a time).
- Poll: the typed ISO textarea is an "Allowed Countries: <summary>" button;
  multi mode with chips, checkboxes, Save, Cancel and the limit toast. The
  limit is `poll_country_count_max` from `updateOption` (new
  `payments.poll_countries_max`), tdesktop's 12 until it arrives. Esc closes
  the box before its host.
- Giveaway: its form already picks countries from a list with checkboxes
  (`GiveawayPage::Countries`), not typed, so it was left as is. It is not on the
  shared box yet; `parity:premium-giveaways` is untouched.
- Deviation: tdesktop gates the poll country row behind a toggle and only for
  broadcast channels; Quill keeps its existing always-visible row.

## How verified

GPUI UI tests (`src/ui/country_box/tests.rs`), unit tests for the choice logic,
and `/verify` captures (`ready-country-poll`, `ready-country-payment`, light and
dark).
