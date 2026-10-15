# Passkeys on Windows through webauthn.dll

Parity item `settings-passkeys`, PR 3 of the build plan in
`quill-tools/designs/passkeys.md` (PR 1 `passkeys-ceremony.md`, PR 2
`passkey-sign-in.md`). The item stays open until the hybrid (phone) PR
lands, so there is no parity fragment.

## What Telegram Desktop does

`platform/win/webauthn_win.cpp`:

- `Resolve`: `SafeLoadLibrary(L"webauthn.dll")`, then `LOAD_SYMBOL` for
  `WebAuthNIsUserVerifyingPlatformAuthenticatorAvailable`,
  `WebAuthNAuthenticatorMakeCredential`, `WebAuthNFreeCredentialAttestation`,
  `WebAuthNAuthenticatorGetAssertion` and `WebAuthNFreeAssertion`. Native
  WebAuthn is used only when all five resolve. No runtime API version check:
  every `dwVersion` is the `_CURRENT_VERSION` of the SDK it was built with.
- `RegisterViaNative`: RP and user entity info, one COSE parameter per
  `pubKeyCredParams` entry, `WEBAUTHN_CLIENT_DATA` over the same
  `clientDataJSON` as the libfido2 path (SHA-256), and the options
  `dwTimeoutMilliseconds = timeout`, `ATTACHMENT_ANY`,
  `bRequireResidentKey = FALSE`, `bPreferResidentKey = TRUE` (when the SDK
  has it), `UV_PREFERRED`, `ATTESTATION_CONVEYANCE_PREFERENCE_NONE`, and a
  `CoCreateGuid` cancellation id that is never used to cancel.
- `LoginViaNative`: rp id, client data, the allow list, the same
  timeout, and `UV` = `REQUIRED` for `required`, `PREFERRED` for
  `preferred`, `DISCOURAGED` for anything else.
- The parent is the active window's `HWND`, brought to the front with
  `SetForegroundWindow` + `SetFocus`. The call blocks the UI thread; tdesktop
  shows nothing of its own while the system dialog is up.
- `NTE_USER_CANCELLED` or `HRESULT_FROM_WIN32(ERROR_CANCELLED)` is a
  cancel. Any other failure logs `hr 0x...` and shows a toast:
  `lng_passkey_error_register` / `lng_passkey_error_login`.
- Results are copied out (credential id + attestation object; credential
  id, authenticator data, signature, user handle) and freed with the
  matching `WebAuthNFree*`.

## What Quill does

- `src/webauthn/windows.rs` (`#[cfg(windows)]`): `webauthn.dll` loaded
  once with `LoadLibraryExW(.., LOAD_LIBRARY_SEARCH_SYSTEM32)` and
  `GetProcAddress`, never freed. The five tdesktop symbols plus
  `WebAuthNGetApiVersionNumber` are required; `WebAuthNGetCancellationId`
  and `WebAuthNCancelCurrentOperation` are optional. Struct types and
  constants come from `windows-sys` (`Win32_Networking_WindowsWebServices`
  feature added; no new crates, `Cargo.lock` and `THIRD_PARTY_LICENSES.md`
  unchanged). Nothing links `webauthn.dll` at load time, so Quill starts on
  a Windows without it.
- Options exactly as tdesktop above, for both calls.
- The call runs on the ceremony's worker thread (`Ceremony::spawn_system`),
  with Quill's main window `HWND` (from GPUI's raw window handle) as parent,
  raised and focused on the UI thread first as tdesktop does.
- `src/webauthn/windows_plan.rs` (every OS): structure versions per API
  version, the `HRESULT` mapping, the sign-in UV mapping, UTF-16 strings,
  COSE algorithms, checked lengths and the copy-out helper, with tests
  that run on macOS and Linux. `windows.rs` asserts at compile time that
  its constants equal the SDK's.
- Seam: `webauthn::supported()` (and `passkeys::webauthn_supported()`) is
  now a runtime function: macOS/Linux as before, Windows when the DLL
  resolves and its API version is at least 1. `start_register` /
  `start_login` pick the backend; the dialog no longer names `UsbKeys`.
- `UserVerification` gained `Unspecified` (missing or unknown value),
  so the Windows path can tell `preferred` from "anything else" as
  tdesktop does. The USB path treats both as before (omit `uv`).
- UI: on Windows the ceremony dialog opens on a new `System` sheet: title
  `lng_passkey_cable_register_title` / `_login_title` ("Add a passkey" /
  "Sign in with a passkey"), one row with the kit `Spinner` and
  "Waiting for Windows Security…" (`qll_passkey_windows_waiting`), and
  Cancel. A non-cancel failure closes the dialog and shows the tdesktop
  toast text through the status toast; a cancel just closes it.
  Demos: `QUILL_DEMO_PASSKEY_CEREMONY=windows` and
  `QUILL_DEMO_PASSKEY_LOGIN=windows`.

## Deviations from tdesktop, and why

- **A waiting row and Cancel.** tdesktop blocks its UI thread on the call;
  Quill never blocks the UI thread (worker rules), so the window stays live
  and shows what it is waiting for, as the design asks. Cancel calls
  `WebAuthNCancelCurrentOperation` with the id from
  `WebAuthNGetCancellationId` (the documented way to get one; tdesktop's
  `CoCreateGuid` id is never used). A Cancel pressed before the call starts
  stops it from starting.
- **Runtime struct versions.** tdesktop sends its SDK's current versions;
  Quill asks `WebAuthNGetApiVersionNumber` and sends
  `MAKE_CREDENTIAL_OPTIONS` v4 (API >= 3, with `bPreferResidentKey`) or v3
  (API 1-2, without it), and `GET_ASSERTION_OPTIONS` v4. On current Windows
  the request is the same as tdesktop's; an older `webauthn.dll` gets the
  fields it knows instead of a version it may reject.
- **DLL search.** `LOAD_LIBRARY_SEARCH_SYSTEM32` only. lib_base's
  `SafeLoadLibrary` is not in the local reference clone, so this is the
  closest safe equivalent rather than a verified copy.
- **No libfido2 fallback.** tdesktop's `IsSupported()` is true on Windows
  even without the DLL (libfido2 over HID), and without a Windows Hello
  platform authenticator it routes `RegisterKey`/`Login` to its caBLE box
  first. Quill has no Windows HID backend and no hybrid yet, so the seam is
  off without the DLL and the system dialog (which offers security keys and
  phones itself) is always used when it is there.
- **COSE parameter type.** Quill's options keep only `alg`, so every
  parameter is sent as `public-key`; tdesktop sends an empty type for a
  non-`public-key` entry. Telegram only sends `public-key`.
- **The brief's options.** The brief listed UV REQUIRED and resident key
  required; it also said to match tdesktop's options, which are UV
  PREFERRED, resident key preferred (not required) and attestation none.
  Quill follows tdesktop.

## How it was verified

- `cargo check` and `cargo clippy -D warnings` for
  `x86_64-pc-windows-msvc --no-default-features --lib --tests` on macOS:
  `windows.rs` and the rest of the WebAuthn code compile and are
  lint-clean (other, older files show Windows-only clippy findings that
  CI does not check).
- The UI build for Windows (`--features ui`) cannot be cross-checked from
  macOS: `ring` and `gpui-pre` build scripts need the MSVC toolchain. The
  Windows-only UI code is one function (`parent_window` in
  `passkey_ceremony.rs`), the same `raw_window_handle` pattern as
  `install_main_window_tray` in `main.rs`. The `windows` CI job
  (`package-windows.yml`: `cargo build --release --features ui`,
  `cargo test --no-default-features`) is the real build check.
- Unit tests: `windows_plan_tests.rs` (versions, HRESULT mapping, UV,
  strings, copies), `ceremony_tests.rs` (a system ceremony cancelled
  through its hook; its result delivered once), `options_tests.rs`
  (`preferred` vs unspecified), `passkeys/tests.rs` (the seam on every OS).
- GPUI UI tests: the waiting row has only Cancel and Cancel returns to the
  box; a Windows failure toasts "Couldn't add a passkey with the security
  key." / "Couldn't sign in with the security key." without an error sheet;
  a Windows cancel closes quietly.
- Demo capture of both waiting dialogs (register and sign-in) on macOS.
- `gate.sh` GATE OK, `check-hotspots.sh`, `check-file-size.sh`.

## Not verified

- No run on Windows: the real `webauthn.dll` calls, Windows Hello, a
  security key or a phone through the system dialog, the dialog appearing
  over Quill's window, and `WebAuthNCancelCurrentOperation` closing it.
  Manual check for the owner on a Windows PC: Settings > Privacy and
  Security > Passkeys > Create Passkey, then sign out and "Log in using
  passkey" on the QR screen; also press Cancel while Windows Security is
  open.
