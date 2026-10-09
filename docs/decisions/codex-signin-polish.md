# Sign-in polish

## What tdesktop does

- Phone step (`intro_phone.cpp`): a country button opens `CountrySelectBox` (search, "Country not found"), the code and number are separate fields grouped by server-supplied per-country patterns (`Countries::Groups`), the default country comes from the IP (`nearest`/`getCountryCode`). Submit is refused client-side when fewer than two digits are present (`lng_bad_phone`). `PHONE_NUMBER_INVALID` shows `lng_bad_phone`, a flood shows `lng_flood_error`, `PHONE_NUMBER_FLOOD` shows `lng_error_phone_flood` in a box, `PHONE_NUMBER_BANNED` opens the banned box (text, OK, Help). Help opens a prefilled `mailto:` to `login@stel.com` (subject/body with number, app and OS version).
- Code step (`intro_code.cpp`): the description names the channel (`lng_code_desc`, `lng_code_from_telegram`), a call countdown ("Telegram will call you in m:ss", h:mm:ss from an hour) turns into the call request, "Send code via SMS" when the code came through Telegram, and going back to the phone step corrects a wrong number.
- Email step (`intro_widget.cpp`): "Reset your account / email" with the wait period and the pending state.
- Accounts (`Main::Domain::maxAccounts`): three, plus one per Premium account, capped at six.

## What changed in Quill

- `src/phone.rs` (pure): `getCountries` rows, longest-calling-code match (so `1876` beats `1`), preferred country for shared codes, as-you-type international grouping with a built-in pattern table (TDLib's `countryInfo` has no patterns, so unknown codes fall back to groups of three), paste normalisation (`+`, `00`, spaces, dashes, Unicode digits, E.164's 15-digit cap), a separator-aware backspace, per-country length validation (`PhoneCheck`) and word-prefix country search (name, English name or `+code`; hidden countries skipped).
- One phone field holds the whole international number (instead of tdesktop's two fields), so a pasted full number selects the country on its own. The country button shows flag, name and code; picking a country writes `+<code>`. The IP default (`getCountryCode`, a `text` answer) prefills `+<code>` until the user types. Continue sends the E.164 number only when it validates; the server stays the authority.
- Code step: `authorizationStateWaitCode.code_info` now keeps `type`, `next_type` and `timeout` (`CodeDelivery`). The description names the channel. The resend button is built from `next_type` ("Send code via SMS in 0:42" / "Telegram will call you in 0:42" while the server timeout runs, then enabled) and is hidden when `next_type` is null (TDLib would refuse). A 1 s redraw runs only while a countdown is on screen. "Wrong number?" re-opens the phone form and re-sends `setAuthenticationPhoneNumber`, which the schema allows from `authorizationStateWaitCode` with no query in flight.
- Email step: `email_address_reset_state` is parsed. `emailAddressResetStateAvailable` offers `resetAuthenticationEmailAddress` (only then); `Pending` shows the remaining time; `TASK_ALREADY_EXISTS` reads "an email reset is already pending".
- Errors: `PHONE_NUMBER_BANNED`, `PHONE_NUMBER_INVALID`, `PHONE_NUMBER_FLOOD` and `TASK_ALREADY_EXISTS` get their own `ErrorClass` (the raw message is still dropped). Banned shows an in-card box with OK and Help (prefilled mailto, tdesktop's text). Flood waits show "Too many tries. Please try again in 0:42." counting down live and lock the submit buttons until it reaches zero.
- Accounts: `signin::max_accounts` (3, +1 per Premium, cap 6). Only the connected account's Premium state is known, so Premium counts as at most one. Add account is refused with an explanatory line and the dialog carries the note.
- Link a desktop device: the Sessions dialog already had the camera scanner and the explicit consent step (`confirmQrCodeAuthentication`). It is macOS-only, so a "Link device from pasted link" button now reads a `tg://login?token=...` link from the clipboard on every platform, validates it with the existing `is_device_login_qr` and goes through the same consent step. tdesktop has no such entry (only phones scan).

## Skipped

- `getPhoneNumberInfo` / `getPhoneNumberInfoSync`: local formatting and validation cover the as-you-type need without a round trip, and the sync call is not exposed through the driver. Revisit if per-country patterns are needed beyond the built-in table.
- Flash/missed-call and Firebase/Fragment delivery get a description and a generic "Send the code again"; no dedicated flows.

## How verified

- Unit tests: `phone` (digits, longest code, shared-code preference, grouping at each keystroke, pasted forms, 15-digit cap, backspace over a separator, per-country validation, search, `getCountries` parsing), `signin` (countdown formats, resend lock/unlock and labels by `next_type`, no resend for null `next_type`, flood countdown, email reset states, banned mailto encoding, account limits) and driver tests (delivery parsing, wrong-number resend, one-shot country fetch and cache, email reset gating and `TASK_ALREADY_EXISTS`, error classification).
- Demo captures (English fixtures, fake `+1 555 010 0199`): `wait-phone-country`, `wait-phone-formatted`, `wait-code-resend`, `wait-phone-banned` (`docs/screenshots/ready-auth-*.png`).
- Not verified: live TDLib answers for `getCountries`/`getCountryCode`/`resetAuthenticationEmailAddress` (no real account was used or signed in), the clipboard link flow on Linux/Windows, flag glyph rendering on Windows (emoji flags show as letters there).
