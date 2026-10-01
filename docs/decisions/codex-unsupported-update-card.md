# Unsupported message update card

Messages Quill cannot render show a readable placeholder and a Get latest Quill button that opens the existing, fixed official GitHub Releases page. Raw API constructor names no longer appear inside the conversation bubble. The card says a newer release may support the message, without promising that updating will recover it. Downloading and self-installing are separately tracked updater items; this action opens the release/download page and never silently replaces the app.

TDLib's four messageExpired* constructors show an expiry notice without an update action. The renderer uses the effective content, including ephemeral overrides, and performs no account request or content download for the placeholder.

Validation: formatting and macOS UI compilation passed. The reusable native AX smoke in ready-unsupported-message mode verifies the named card/button, an expired-media notice with no second update action, and absence of raw API constructor names. The owned-window screenshot was visually inspected. The fixture injects an unknown message constructor and an expired photo without a live Telegram account. The button's fixed release URL was reviewed; external browser activation was not exercised by the smoke.
