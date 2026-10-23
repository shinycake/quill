# Payment card form

## What tdesktop does

- `payments/ui/payments_edit_card.cpp` (`EditCard`): Card Number, then
  MM / YY and CVC side by side, Cardholder name when the provider asks,
  a "Billing Information" title, Country and Zip Code when asked, and
  "Save Information for future use" when the form can save credentials
  or only lacks a password. Done and Cancel at the bottom; the panel title
  is "New Card", with " (Test)" on a test invoice.
- `payments/ui/payments_field.cpp`: each field reformats as it is typed.
  The number groups 4-4-4-4 (Amex 4-6-5, 14-digit Diners 4-6-4), the
  expiry becomes MM/YY, CVC takes 3 digits (4 for Amex and unknown
  brands), the holder name is upper case. A complete field moves focus to
  the next one; Enter on an invalid field shows its error, on a valid one
  activates the next field, and on the last one presses Done. Backspace
  and Delete step over a separator; Backspace at the front of a field
  goes back to the previous one. The country field opens the country box
  when activated and moves on once a country is picked.
- `payments/payments_form.cpp` (`validateCard`): local checks in field
  order, then Stripe (`stripe/stripe_api_client.cpp`, form-encoded
  `card[...]` to `api.stripe.com/v1/tokens`, Stripe-Version 2015-10-12)
  or Smart Glocal (`smartglocal/smartglocal_api_client.cpp`, JSON to the
  form's tokenize URL when it is a `*.smart-glocal.com/cds/v1/tokenize/card`
  URL, else the default host). The answer becomes `NewCredentials`:
  `{"id":…,"type":"card"}` (Stripe) or `{"token":…,"type":"card"}`
  (Smart Glocal), titled "visa *4242". `sendPaymentForm` sends it as
  `inputCredentialsNew`, with `allow_save` only when the user ticked Save
  and `can_save_credentials` is true. Ticking Save without being able to
  save and with `need_password` asks to turn on Two-Step Verification.
- Provider errors (`CheckoutProcess::handleError`): number, CVC and
  expiry errors mark their field; CardDeclined and ProcessingError are
  toasts; anything else is "Stripe Error: <id>" / "SmartGlocal Error: <id>".
- `paymentProviderOther` and 3-D Secure (`paymentResult.verification_url`)
  show the provider's page in an embedded webview inside the panel. The
  webview hands the credentials back through `payment_form_submit`. When no
  webview is available, verification opens the URL in the browser and
  closes the panel.

## What changed

- `src/payment_card/`: a port of the validator, formatter, Stripe and
  Smart Glocal request builders and answer parsers, behind a `Transport`
  trait. The live transport is `ureq` over rustls (HTTPS only, no
  redirects, 60 s timeout, 64 KiB answer cap) and runs on the background
  executor; tests and screenshot demos use recorded answers and never
  reach the network.
- `PaymentProviderKind` now keeps the Stripe publishable key and
  `need_*` flags and the Smart Glocal token and URL, instead of a name.
- The checkout's payment method list is tdesktop's choice box inline:
  "New Card...", the saved cards, then the card just entered. "New Card..."
  opens the card form over the checkout (the kit shows one dialog at a
  time, so it replaces the checkout body, like the country box does). Pay
  with no card chosen opens the same entry. The pasted-token radio, field
  and copy are gone.
- The billing country reuses the shared country box from #1131
  (`CountryHost::PaymentCard`), starting on the account's phone country.
- Esc on the card form returns to the checkout; Esc in its country box
  returns to the card form.
- A Backspace or Delete that changes nothing (at the start or end of a
  field) leaves no snapshot behind, so later typing is never read as a
  delete.

## Where Quill differs, and why

- `paymentProviderOther` and 3-D Secure open the provider's page in the OS
  browser (the brief asked for Quill's existing behavior). After a 3-D
  Secure redirect the checkout closes, which is tdesktop's no-webview
  fallback. For `paymentProviderOther` this means the provider's page
  cannot hand credentials back to Quill: the buyer completes payment on the
  provider's site only if the provider supports that. Quill already has a
  webview host for mini apps (`crates/webview-host`,
  `docs/decisions/codex-miniapp-webview.md`); moving these two pages into
  it, with `payment_form_submit` wired to `inputCredentialsNew`, is the
  follow-up that closes the gap.
- The Two-Step Verification prompt is an inline panel under the payment
  method list, not a separate box, because the checkout is already a kit
  dialog. Pay stays disabled while it shows, as tdesktop's modal box
  blocks the panel. "Continue" opens Quill's Two-Step Verification dialog
  over the checkout (the kit shows one dialog at a time, so it takes the
  checkout's place); the checkout and the entered card come back when it
  closes. A password set there lets the card be saved: the form's
  `can_save_credentials` counts the account's password state, as
  tdesktop's `setHasPassword` does. Picking another method drops the
  prompt; picking the new card again asks again while it still cannot be
  saved (`chooseSavedMethod`).
- Provider refusals ("Your card was declined.", "Stripe Error: <id>")
  show as a kit `Alert` at the bottom of the card form, cleared by the next
  Done or its close button. tdesktop shows a toast inside the payment
  panel; Quill's window toasts (the kit `Notification` and the status
  toast) paint under kit dialogs, which draw in a deferred layer, so a toast
  would be hidden behind the checkout.
- Errors from an unreadable answer use tdesktop's ids ("InvalidJson",
  "InvalidJsonRoot", "InvalidTokenJson"). Qt's JSON parse error code is
  not appended, since serde reports errors differently.
- HTTP failures map to tdesktop's "RequestError<n>" with the
  `QNetworkReply::NetworkError` number Qt gives that status, so the toast
  text matches.

## Card data

- Nothing logs, persists or puts card data in a status note; `Debug`
  prints neither the card nor the token data.
- Wiped on drop: `CardDetails`, the request body and the Stripe and
  Smart Glocal bodies it is built from, every copy the field formatter and
  the validator make (`FieldText`, its char buffers, digit strings) and the
  card form's reads of its fields (`Zeroizing`). These are built at their
  final size, so a growing `String` never leaves an unwiped buffer behind.
- Not wiped: the text inside the kit `InputState` fields (the kit owns
  the rope and its undo history) and whatever `ureq` and rustls copy while
  sending. The form is dropped as soon as the card is tokenized or
  cancelled, and the fields are never written anywhere else.
