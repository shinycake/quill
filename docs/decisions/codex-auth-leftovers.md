# Auth leftovers

## What tdesktop does

- "Link Desktop Device" (`intro_qr.cpp`, `lng_intro_qr_*`) is only the new-device side: tdesktop shows the QR and the phone scans it. Desktop has no screen that shows a QR for another device to log in as this account, and the Telegram protocol has no such request.
- Code step (`intro_code.cpp`, `intro_step.cpp`): Fragment codes get the title "Enter code", the text "Get the code for {phone} in the Anonymous Numbers section on Fragment." and an "Open Fragment" button. Flash call, missed call and Firebase are logged as unsupported (`bad(...)`); tdesktop has no screen for them.
- Premium-gated login: tdesktop has no such state (MTProto only, no store purchase).

## What changed

- `authorizationStateWaitPremiumPurchase` now parses `premium_day_count` and the support address and subject. The screen explains the requirement and the included days, offers "Sign in with QR code" (TDLib allows that switch from this state) and "Email Telegram support" when the address passes a strict mailbox check. There is no purchase flow: Quill never calls `checkAuthenticationPremiumPurchase` or `setAuthenticationPremiumPurchaseTransaction`. The old `UnsupportedHalt` is gone for this state (`AuthAction::PremiumRequired`).
- `CodeDelivery` carries a `CodeDetail` (flash-call pattern, missed-call prefix and digit count, Fragment URL) and is no longer `Copy`. Per type:
  - Flash call: the code is the number that called; the screen shows the pattern.
  - Missed call: the code is the last N digits of the calling number; the screen shows the prefix and N.
  - Fragment: tdesktop's title and text, plus an "Open Fragment" button. Only plain `https://` URLs without whitespace are kept.
  - Firebase (Android and iOS): the screen says device verification needs the official apps and points to resend or QR sign-in. Quill never calls `sendAuthenticationFirebaseSms` because it has no device token, and the Firebase parameters and receipt are dropped at parse time.
  - Resend labels exist for a `next_type` of call, flash call, missed call and Fragment.

## Skipped

- `auth-qr-authorize-other` stays unchecked. tdesktop has nothing to match, and the README wording (a QR shown by a signed-in device) is not something Telegram offers. Quill has the camera scan on macOS and the pasted-link confirmation from `codex-signin-polish.md`, which authorize another device the way the protocol allows.
- Real Firebase verification and the premium purchase need the official mobile apps.

## How verified

- Unit tests: `signin` (descriptions, title, Fragment URL, resend labels, premium explainer, mailbox check), `auth` (premium view and QR switch), `connect::tests::auth_leftovers` (parsing of each type, https-only Fragment URL, no Firebase secrets kept, premium fields).
- Demo captures with English fixtures and the fake `+1 555 010 0199`: `wait-code-flash`, `wait-code-missed`, `wait-code-fragment`, `wait-code-firebase`, `wait-premium` (`docs/screenshots/ready-auth-*.png`). All were looked at.
- Not verified: live TDLib deliveries of these types (no real account was used), and opening the browser or mail client from the buttons.
