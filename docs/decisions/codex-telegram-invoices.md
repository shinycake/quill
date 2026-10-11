# Checkout for any invoice

Branch `codex/telegram-invoices`. Completes `parity:premium-send-gift` and
`parity:deeplink-invoice`. Also wires Stars top-up and the payment for a new
giveaway, but those README items need more (see the end).

## What Telegram Desktop does

- `payments/payments_form.h`: one `InvoiceId` variant per thing that can be
  paid: a message, a slug (invoice link), Premium gift codes and giveaways,
  Stars (`InvoiceCredits`) and a Star gift. `CheckoutProcess::Start` takes any
  of them and runs the same form: fetch the form, optional order info and
  shipping, credentials, pay. Money forms use the bot payment form with saved
  or new credentials; Stars forms are paid from the balance.
- `settings/settings_credits_graphics.cpp`: a pack in the "Buy more Stars"
  list starts the checkout with an `InvoiceCredits`.
- `boxes/star_gift_box.cpp` `SendGift`: a Premium option (with the message)
  starts the checkout with a Premium gift invoice for the one user.
- `info/channel_statistics/boosts/create_giveaway_box.cpp`: a prepaid giveaway
  asks "are you sure" and applies it; a new one starts the checkout with the
  chosen option.
- `core/local_url_handlers.cpp` `ShowInvoice`: an invoice link starts the
  checkout with the slug.

## What changed

TDLib 1.8.68 has one `InputInvoice` (`inputInvoiceMessage`,
`inputInvoiceName`, `inputInvoiceTelegram` with a `TelegramPaymentPurpose`),
and the same value goes into `getPaymentForm`, `validateOrderInfo` and
`sendPaymentForm`.

- `src/payment_invoice.rs` (new, core): `InputInvoice` and
  `TelegramPaymentPurpose` (all seven purposes) with their JSON.
  `src/payment_invoice/builders.rs`: the invoice each entry point builds
  (`stars_pack_invoice`, `premium_gift_invoice`, `giveaway_invoice`,
  `invoice_link_invoice`).
- The three request builders take `&InputInvoice`. `PaymentRequest` keeps the
  invoice instead of a chat and message, so validate and send reuse what the
  form was fetched with. `ConnectDriver::send_payment_form_request` takes any
  invoice; only a message invoice is checked against the chat history.
- `src/ui/invoice_checkout.rs` (new): `start_invoice_checkout` (spinner
  dialog, then the existing checkout), and the entry points: Buy button,
  invoice link, Stars pack, Premium gift option, new giveaway. It also has the
  body for a Stars form (`paymentFormTypeStars`): product, price, balance, Pay
  sends `sendPaymentForm` with null credentials. Before, a Stars form was
  declined. Star subscriptions still are.
- Call sites: the pack rows in `premium_ui/star_packs.rs`, the Premium rows in
  `gift_send_ui.rs` (with the gift message field, only for a user other than
  you), the Start button in `giveaway_create_ui.rs` for a new giveaway, and
  `DeepLinkUi::Invoice` in the deep link router (a nameless link says it is
  broken).
- The checkout locks after a successful payment (`form_paid`), so the same
  form cannot be paid twice. A failed form fetch now shows its reason in the
  dialog instead of closing it (an unknown invoice link used to leave
  nothing on screen).
- The giveaway form hides while the checkout is open and returns when it
  closes; the draft is kept.

Nothing is paid by opening the checkout. `sendPaymentForm` is sent only from
the Pay button.

## Not done

- `parity:premium-stars-balance` needs "send stars" (buying Stars for another
  user, `getStarGiftPaymentOptions` with `telegramPaymentPurposeGiftedStars`).
  The purpose serializes and is tested, but there is no entry point yet.
- `parity:premium-giveaways` still lacks specific-user gift codes
  (`telegramPaymentPurposePremiumGiftCodes`, also serialized and tested but
  not wired) and the country picker.
- tdesktop also pays a Premium gift with Stars (`byStars`); Quill pays money
  only.

## How it was verified

- Unit tests: request JSON for every `InputInvoice` and purpose; the same
  invoice in the form, validate and send requests; the builders for each
  entry point (pack, Premium gift to a user, a channel and yourself, Premium
  and Stars giveaway, unavailable choices, invoice slug); the deep link route
  for an invoice; driver tests with recorded JSON (invoice link and Stars
  pack ask for their form, a message invoice still needs a real message, a
  Stars form is paid with null credentials only on request, a payment locks
  the form and a new form unlocks it).
- No real payment was sent or attempted. The new dialogs were not run against
  a live account.
