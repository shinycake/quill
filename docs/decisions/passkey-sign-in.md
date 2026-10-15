# Passkeys: signing in with a security key (PR 2 of 4)

Parity id `settings-passkeys`. The design is `quill-tools/designs/passkeys.md`,
build plan step 2. PR 1 (`docs/decisions/passkeys-ceremony.md`) built the
ceremony, the CTAP2 client and "Create Passkey". This PR adds the sign-in
half on macOS and Linux. Windows stays off until PR 3, and QR (hybrid) comes
in PR 4. No parity fragment yet: the item is only done after PR 4.

## What tdesktop does

Read from `intro/intro_qr.cpp`, `data/components/passkeys.cpp`,
`main/main_app_config.cpp` and `webauthn/webauthn_common.cpp`.

- **The link.** `QrWidget::setupPasskeyLink` adds "Log in using passkey"
  (`lng_intro_qr_passkey`), centred, 1.5 lines under "Log in using phone
  number". It is added only when `appConfig().settingsDisplayPasskeys()`
  (`settings_display_passkeys`, default false) and
  `Platform::WebAuthn::IsSupported()` are both true. The widget watches
  `appConfig().value()` and adds the link once the config allows it.
- **The app config before login.** `AppConfig` sends `help.getAppConfig` as
  soon as the account's MTP instance exists, signed in or not, so the value
  is known on the intro screens.
- **The click.** `InitPasskeyLogin` (`auth.initPasskeyLogin`, TDLib
  `getAuthenticationPasskeyParameters`). The options are kept with their
  time and reused while younger than their own `timeout`. A failed init has
  no handler, so nothing is shown.
- **The ceremony.** `Platform::WebAuthn::Login`. On macOS and Linux
  unsigned builds this is libfido2's `RunLoginCeremony`: every key in turn,
  `fido_dev_get_assert`, and **assertion 0** is taken (`fido_assert_*_ptr(assert,
  0)`). Nothing matches the user handle against anything; the request
  options have no user. Errors are toasts: no device, "This security key has
  no passkey for your account.", PIN blocked, or "Couldn't sign in with the
  security key." for anything else. A cancel ends quietly.
- **The result.** If the user handle is empty, nothing happens.
  `FinishPasskeyLogin` splits the handle as UTF-8 on `:`. Unless there are
  exactly two parts and the first is a data centre id in `1..kDcShift`, it
  fails with `PASSKEY_HANDLE_INVALID`. Otherwise it sends
  `auth.finishPasskeyLogin` (TDLib `checkAuthenticationPasskey`) with the
  credential id as base64url without padding.
- **The answer.** Success finishes the login. `SESSION_PASSWORD_NEEDED`
  goes to the password step. Any other error drops the cached options and
  shows the error type in the step's error label (`showError(error)`), for
  example `PASSKEY_CREDENTIAL_NOT_FOUND`.

## What Quill does now

- **Core** (`src/passkeys/login.rs`):
  - the two requests;
  - tdesktop's user-handle check;
  - `LoginState`: the cached options, the options waiting for the dialog,
    the in-flight check and the error line.
- **TDLib fields** (`td_api.tl:11738`): `credential_id` is a base64url
  string (no padding), `client_data` is the clientDataJSON text, and
  `authenticator_data`, `signature` and `user_handle` are `bytes`, which
  TDLib's JSON takes as standard base64.
- **App config while signed out.** `getApplicationConfig` "can be called
  before authorization" (`td_api.tl:16789`). The driver sends it once when
  the authorization state is `WaitPhoneNumber` or
  `WaitOtherDeviceConfirmation` (`connect/passkeys_login.rs`). The existing
  app-config reducer sets `display_allowed`, and the link appears when the
  answer arrives, as in tdesktop.
- **Driver** (`ConnectDriver::begin_login`, `finish_login`):
  - Only in the states where TDLib accepts `checkAuthenticationPasskey`
    without a pending query.
  - Options are reused while fresh, as in tdesktop.
  - `SESSION_PASSWORD_NEEDED` needs no code in Quill. TDLib handles it in
    `AuthManager` (it fetches the password state and moves to
    `authorizationStateWaitPassword`), and Quill's password screen follows
    the state.
- **Errors.** `TdError` gains `server_type`: the TDLib message, kept only
  when it is an upper-case error constant (`[A-Z][A-Z0-9_]*`, at most 64
  bytes). Phone numbers, codes and free text never match, so the existing
  rule that error text is not stored still holds for anything secret. A
  failed parameters or check request forgets the options and shows
  `server_type` under the link in the danger colour.
- **The dialog.** It is PR 1's ceremony dialog with a mode
  (`CeremonyMode::Login`).
  - Same Key and PIN sheets: tdesktop's login path uses the same touch and
    PIN boxes.
  - The worker runs `Ceremony::login`, and a `SignedIn` assertion closes the
    dialog and goes to `finish_login`.
  - Failures use tdesktop's login texts. `lng_passkey_error_no_credentials`
    and `lng_passkey_error_login` are new in `lang/en/passkeys.strings`.
  - If the account signs in another way while the dialog is open (the QR
    code was scanned), the ceremony is cancelled and the dialog closes.
- **The link** (`ui/security/passkey_login.rs`): a ghost `Button`, the same
  style as the screen's other in-app commands, under the QR caption. It is
  disabled and loading while the options or the check are on their way or
  the dialog is open.
- **Multiple assertions.** The CTAP2 client keeps the first assertion of
  `authenticatorGetAssertion` and does not call `getNextAssertion`. That is
  libfido2's assertion 0, which is what tdesktop takes. The brief asked to
  "pick the one whose user handle matches, else the first". tdesktop has
  nothing to match against, so Quill takes the first.

## Deviations from tdesktop, and why

- **Where the link sits.** Quill's QR screen has no "Log in using phone
  number" link. Its QR state is reached from the phone screen. So the
  passkey link sits directly under the QR caption, centred, where tdesktop's
  phone link would be.
- **Ceremony errors use the dialog's Error sheet, not toasts.** This is
  the same deviation PR 1 made for registration, with tdesktop's texts.
- **A failed `getAuthenticationPasskeyParameters`, or options that cannot be
  read, show a line under the link.** tdesktop shows nothing, which leaves a
  click that does nothing. The Design Guides do not allow that ("State must
  be visible"). The line is the server's error type, or
  "Could not sign in with the passkey: unreadable parameters".
- **No data centre switch in Quill.** tdesktop calls `setMainDcId(userDc)`
  and sends `auth.finishPasskeyLogin` to that DC. With TDLib the client
  sends `checkAuthenticationPasskey` and TDLib handles routing. The handle
  check itself is kept, so a malformed handle fails the same way.

## How it was verified

- `src/passkeys/login_tests.rs`:
  - each field's encoding in `checkAuthenticationPasskey`;
  - the parameters request;
  - the user-handle rule (good and bad cases);
  - option freshness and reset on failure.
- `src/telegram/envelope/auth.rs`: `server_type` keeps error constants and
  drops phone numbers, codes, free text and over-long strings.
- Driver tests (`src/connect/tests/passkeys_login.rs`, recording sender):
  - the app config is asked for once while signed out;
  - the link follows `settings_display_passkeys`;
  - the options start the ceremony and are reused while fresh;
  - the assertion becomes `checkAuthenticationPasskey` with the right
    base64 fields, one at a time;
  - `ok`, then `authorizationStateWaitPassword`, reaches the password state;
  - a refused check shows `PASSKEY_CREDENTIAL_NOT_FOUND` and forgets the
    options;
  - an invalid handle sends nothing and shows `PASSKEY_HANDLE_INVALID`;
  - unreadable or failed options are shown;
  - nothing is offered once signed in.
- GPUI UI tests (`src/ui/security/passkey_login_tests.rs`, signed-out
  `wait-qr-passkey` demo):
  - the link follows the switch;
  - the server error shows under it;
  - the sign-in dialog shows the Key sheet;
  - NoCredentials and other failures map to tdesktop's login texts;
  - Close, Cancel and a `SignedIn` assertion close the dialog.
- Demo capture: `wait-qr-passkey`, with `QUILL_DEMO_PASSKEY_LOGIN` unset,
  `key`, `pin` and `server-error`. The captures were checked by eye:
  the link under the QR caption, the Key sheet over the QR screen, and the
  error line.

## Not verified

- **No physical security key and no real sign-in.** The full path (key,
  then `checkAuthenticationPasskey`, then Ready or the password screen)
  needs a manual run with a key that holds a Telegram passkey. That key can
  only be made with PR 1's "Create Passkey", whose server side is not
  probed either.
- **The live app config while signed out was not probed.** Whether
  `settings_display_passkeys` is true for Quill's `api_id` before login is
  unknown. The flag only hides the link; nothing else depends on it.
  `getAuthenticationPasskeyParameters` signed out is known to answer
  (`challenge`, `rpId=telegram.org`, `timeout`, `userVerification`, from
  the hidden `--passkey-probe login`).
- **TDLib's DC routing for a passkey on another data centre** is TDLib's,
  and was not tried.
