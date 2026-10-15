# Passkeys: creating one on a USB security key (PR 1 of 4)

Parity id `settings-passkeys`. The design is
`quill-tools/designs/passkeys.md`, and this PR is step 1 of its build plan:
the WebAuthn ceremony, run in-process over CTAP2/HID, plus "Create Passkey".
Signing in with a passkey, QR (hybrid) and Windows come in later PRs. No
parity fragment is written yet, because the item is only complete after PR 4.

## What tdesktop does

Read from `data/components/passkeys.cpp`, `data/data_passkey_deserialize.cpp`,
`webauthn/webauthn_common.cpp`, `webauthn/cable_box.cpp` and
`settings/sections/settings_passkeys.cpp`.

- **When creation is offered.** `canRegister` is
  `Platform::WebAuthn::IsSupported()` (true on macOS and Linux) and fewer
  passkeys than `passkeys_account_passkeys_max` (default 10). "Create
  Passkey" sits in `PasskeysNoneBox`. "Add Passkey" in the list opens that
  box.
- **The options.** `initRegistration` sends `account.initPasskeyRegistration`
  (TDLib `getPasskeyParameters`) and caches the answer for its own `timeout`.
  A second click inside that window reuses the cached options.
- **Parsing the options.** `DeserializeCreationOptions` reads
  `publicKey.{rp, user, challenge, pubKeyCredParams, timeout}`. `user.id` is
  standard base64; `challenge` is base64url.
- **The algorithm.** `PreferredAlgorithm` sends exactly one: ES256 (-7) if it
  is offered, otherwise the first EdDSA (-8) or RS256 (-257), otherwise -7.
- **clientDataJSON.** A `QJsonObject` written compactly, so the keys come out
  sorted:
  `{"challenge":"<b64url>","crossOrigin":false,"origin":"https://<rp>","type":"webauthn.create"}`.
- **The attestation.** Whatever the key returns, tdesktop sends
  `NoneAttestationObject(authData)`, which is `{"fmt":"none","attStmt":{},"authData":…}`.
- **makeCredential** sends `rk = true` and leaves `uv` out.
- **Boxes.**
  - Touch box: "Touch your security key", "Insert your security key, then
    touch it to continue.", and Cancel.
  - PIN box: "Security key PIN", then the about line or "Incorrect PIN.
    Please try again.", a password field with the placeholder "PIN", and
    Confirm and Cancel. An empty PIN gets a field error. No retries count is
    shown.
  - Closing either box cancels the ceremony.
- **Errors.** `MapError` turns PIN_REQUIRED into a PIN request, PIN_INVALID
  into the invalid PIN box, PIN_BLOCKED or PIN_AUTH_BLOCKED into
  "PIN blocked", NO_CREDENTIALS into "no credentials", and anything else into
  a generic error. For register, both of the last two read "Couldn't add a
  passkey…".
- **The result.** `registerPasskey` sends `account.registerPasskey` (TDLib
  `addLoginPasskey`). On success the passkey is appended to the list and the
  box closes, with no toast. A failure drops the cached options and shows
  nothing.

## What Quill does now

- **`src/webauthn/`** is new and independent of the UI.
  - `options.rs` parses the options the way tdesktop does and builds a
    byte-exact clientDataJSON.
  - `ctap2/` is a CTAP 2.1 client:
    - CTAPHID framing (INIT, CBOR, CANCEL, KEEPALIVE and ERROR, with
      reassembly).
    - getInfo.
    - clientPIN protocols 1 and 2, including `getPinUvAuthTokenUsingPinWithPermissions`
      (with the `mc`/`ga` permission and the rpId) and the plain
      `getPinToken` fallback.
    - makeCredential and getAssertion, with error mapping.
    - HID backends for macOS (IOKit `IOHIDManager`, with input reports
      collected on a private run-loop mode) and Linux (`/dev/hidraw*` found
      through sysfs report descriptors, usage page 0xF1D0).
  - Crypto is RustCrypto (`p256` ECDH, `aes` + `cbc`, `hmac`, `hkdf`,
    `sha2`) and CBOR is `ciborium`. There is no OpenSSL and no libfido2.
  - `ceremony.rs` runs one ceremony on a guarded worker thread
    (`spawn_guarded`). The UI polls it for events (need PIN, registered,
    signed in, failed). A PIN reaches the worker as `Zeroizing<String>`.
    Cancelling, or dropping the ceremony, sends CTAPHID_CANCEL.
- **Settings.**
  - "Create Passkey" in the "Protect your account" box calls
    `ConnectDriver::begin_register`. It uses cached options while they are
    fresh, otherwise `getPasskeyParameters`.
  - A 100 ms ticker (`src/ui/security/passkey_ceremony.rs`) starts the
    ceremony once the options arrive, follows its events, and sends the
    result with `finish_register` (`addLoginPasskey`).
  - The passkey joins the list and the box closes.
  - The button reads "Unsupported" and is disabled where WebAuthn is off
    (Windows for now), or when the passkey limit is reached.
- **The dialog** is one kit dialog (priority 605, above the box) with three
  sheets: Key (tdesktop's touch box), PIN (masked kit `Input`, Enter or
  Confirm, Cancel) and Error ("Something went wrong", the reason, Close).
- **The limit** comes from the app config key `passkeys_account_passkeys_max`
  (default 10, as in tdesktop).
- **Strings.** All text is in `lang/en/passkeys.strings`, with tdesktop's
  `lng_` keys, so official translations apply.
- **Dev tool.** The hidden, read-only `--passkey-probe [login]` (from the
  previous commit) stays. It prints the server's WebAuthn options so the
  parser can be checked against the live format.

## Deviations from tdesktop, and why

- **Key-path errors show the Error sheet, where tdesktop shows toasts.**
  The design's dialog has an Error sheet, which is also where the QR (cable)
  path reports errors in tdesktop (`cable_box.cpp`, "Something went wrong").
  The text of each error is tdesktop's.
- **No "Passkey added" toast.** tdesktop has none; the box closing and the
  passkey appearing in the list are the confirmation.
- **The worker waits for a key.** It looks for a key every 500 ms until the
  options' timeout, instead of failing at once with "No security key
  found". tdesktop's touch box says "Insert your security key", so it waits
  as well.
- **The first key that opens is used for registration.** There is no
  key-choice sheet yet; that comes with QR in a later PR.
- **Failed `getPasskeyParameters` or `addLoginPasskey` show an error line in
  the box** ("Could not add the passkey: …"). tdesktop shows nothing. Quill
  shows the reason because otherwise a click on "Create Passkey" could
  silently do nothing, which the Design Guides do not allow ("State must be
  visible").
- **`user.id` and `challenge` accept either base64 alphabet,** with or
  without padding. This is a superset of tdesktop's decoding and changes no
  valid input.

## Platform support

- **macOS:** IOKit HID. No entitlement is needed for USB HID in an
  unsandboxed app.
- **Linux:** `/dev/hidraw*`. As with every browser, the user needs the usual
  FIDO udev rule (`70-u2f.rules` from libfido2 or systemd's
  `60-fido-id.rules` tag) for read/write access. Without it no key is found
  and the Error sheet says so.
- **Windows:** the seam is off (`webauthn::supported()` is false). The box
  shows "Unsupported", the same as tdesktop on a platform without WebAuthn.
  The Windows backend (`webauthn.dll`) is a later PR. The crate compiles
  there and `hid::open_all` returns no devices.

## How it was verified

- `src/webauthn/` has 27 tests. They use a scripted fake authenticator
  (`ctap2/fake.rs`) that speaks CTAPHID and CTAP2 and covers:
  - framing and reassembly;
  - getInfo;
  - makeCredential without a PIN, checking canonical CBOR keys;
  - PIN protocol 1 and 2 round trips (ECDH, encryption and HMAC);
  - PIN tokens with and without permissions, a wrong PIN, PIN required and
    PIN blocked;
  - getAssertion;
  - cancel and timeout while waiting for a touch;
  - the "none" attestation bytes;
  - credential-id bounds;
  - the ceremony's PIN flow, error mapping and the wait for a key.
- The fixtures are built from the CTAP 2.1 specification (command and
  status codes, parameter keys, COSE key layout). They are **not**
  captured from a real YubiKey.
- `options_tests.rs` checks clientDataJSON byte for byte against tdesktop's
  output for both create and get, and the request parser reads the same
  fields that the probed `getAuthenticationPasskeyParameters` answer has.
- Driver tests (`src/connect/tests/passkeys.rs`, with the recording sender)
  cover:
  - options fetched once and reused while fresh;
  - `addLoginPasskey` carrying standard-base64 attestation and the exact
    client data;
  - the passkey joining the list;
  - a failed add forgetting the options;
  - unreadable options producing an error and no ceremony.
- GPUI UI tests (`src/ui/security/passkey_ceremony_tests.rs`) cover:
  - Cancel on the Key sheet returning to the box;
  - the PIN sheet keeping an empty PIN, showing the invalid text and
    clearing the field after submitting;
  - the Error sheet with Close;
  - a cancelled ceremony closing quietly.
- The existing passkeys UI tests now follow the seam.
- Demo-capture screenshots were checked:
  - `ready-passkey-ceremony` with `QUILL_DEMO_PASSKEY_CEREMONY=key|pin|error`
    (also `pin-invalid`);
  - `ready-passkeys` with `QUILL_DEMO_PASSKEYS=about`, which shows "Create
    Passkey".

## Not verified

- **No physical security key was used.** Registration end to end against a
  real key, and against Telegram's servers, still needs a manual check on
  macOS and Linux.
- **`getPasskeyParameters` was not probed.** Reading it needs a signed-in
  account, and the owner's account and keychain were off limits. Only
  `getAuthenticationPasskeyParameters` was probed, on a signed-out
  throwaway database: it returned `challenge`, `rpId=telegram.org`, `timeout`
  and `userVerification`. The creation parser follows tdesktop's
  `DeserializeCreationOptions` field for field.
