# Bank card info when tapping a card number

## What tdesktop does
A bank card number entity calls `getBankCardInfo`; the menu shows the issuer
title, "Copy Card Number" and one entry per `bankCardActionOpenUrl`.

## What changed
- `getBankCardInfo` request, `PaymentsPurpose::GetBankCardInfo`,
  `PaymentsPayload::BankCardInfo` (`bankCardInfo` parse), result in
  `payments.bank_card` (`BankCardLookup`); a failure clears the info.
- `ConnectDriver::request_bank_card_info` sends it on click.
- `src/ui/entity_links.rs`: the card popup shows the issuer title and the
  action links around "Copy Card Number"; with no answer or an error it
  stays the plain copy menu.

## Verified
Envelope parse test, reducer test (own request only, error clears), gate.
Not verified against a live TDLib account.
