# Mini apps: invoices and phone sharing

## What tdesktop does

- `web_app_open_invoice` (`WebViewInstance::botHandleInvoice`) starts a
  `CheckoutProcess` for the slug. When it ends the panel gets
  `invoice_closed` with `paid` (payment finished, checkout closes by itself),
  `failed` (last send failed, then closed), `pending` (verification step
  started, then closed) or `cancelled`.
- `web_app_request_phone` (`Panel::requestPhone`, `botSharePhone`) shows a
  popup over the web view (title "Phone number", "Do you want to share your
  phone number with this bot?", Share / Cancel). Share unblocks the bot first
  when blocked, then `shareContact` sends the own contact into the bot's chat.
  The app gets `phone_requested` with `sent` or `cancelled`.

## What changed

- `src/ui/web_app_ui/invoice.rs`: the event queues a render action (needs the
  window), brings Quill in front, starts the existing invoice checkout
  (`open_invoice_link`) and watches the payment state. A pure `InvoiceWatch`
  maps paid / sending / error note / dialog closed to the four outcomes.
  Paid closes the checkout like tdesktop.
- `src/ui/web_app_ui/phone.rs`: the prompt is the helper's native popup, so it
  shows over the web view. `PhoneFlow` (Asking, Unblocking, Sending) is polled;
  the contact card goes out through `ConnectDriver::share_phone_with_bot`
  (`sendMessage` + `inputMessageContact`, purpose `SharePhoneWithMiniApp`).
- State: `web_apps.phone_share` one-shot, set when TDLib takes the message or
  refuses it.
- Removed the two "not supported yet" strings.

## Deviations

- Sent means TDLib accepted the message; tdesktop waits for the server ack of
  `messages.sendMedia`.
- The checkout is Quill's payment dialog in the main window (no separate
  checkout window); an invoice request while another checkout is open answers
  `failed`.
- The mini app window is not hidden during a card checkout (tdesktop hides the
  panel for the real form).

## Verified

Driver tests (recording sender), pure state tests, GPUI UI tests per flow,
`/verify` capture of the checkout in light and dark. No live mini app, so the
helper popup and a real payment were not exercised.
