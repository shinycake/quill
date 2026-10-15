# Passkeys in Privacy and Security (management only)

## What tdesktop does

- `data/components/passkeys.*` keeps the account's passkeys. `requestList`
  sends `account.getPasskeys` when the list was never asked for or was asked
  for more than 5 s ago (`kRequestTimeout`). `deletePasskey` sends
  `account.deletePasskey`; on success it forgets the list (`listKnown =
  false`), reloads it and closes the confirmation. A failed delete does
  nothing visible. `canRegister` needs `Platform::WebAuthn::IsSupported()`.
  `possible()` is the app config switch `settings_display_passkeys`
  (default off).
- `settings_privacy_security.cpp`: when `possible()`, a "Passkeys" row after
  Local passcode. Its value is "Loading..." until the list is known, then the
  single passkey's name, the count, or "Off". It shows when WebAuthn is
  supported or the list is not empty. A click does nothing while loading,
  opens `PasskeysNoneBox` without passkeys, else the Passkeys section.
- `settings_passkeys.cpp`: the section has a header (the `passkeys` Lottie and
  "Manage your passkey, stored safely in the cloud service you choose."), one
  row per passkey (the software's custom emoji or the permissions icon, the
  name or "Passkey", and "Last used {date}" or "Created {date}" with
  `{date}` = "{day} at {time}"), each with a ⋮ menu holding Delete. Delete opens
  a confirm box: title "Delete Passkey", "Once deleted, {emoji} **name**
  can't be used to log in." (or the unnamed wording), a blank line, "Don't
  forget to remove it from your password manager too.", buttons Cancel and
  Delete (attention style). Below the list: "Add Passkey" (only when
  `canRegister`) and "Your passkey is stored securely in your password
  manager. Learn more >", whose link opens `PasskeysNoneBox`.
- `PasskeysNoneBox`: icon, "Protect your account", "Log in safely and keep
  your account secure.", three rows (Create a Passkey / Log in with face
  recognition / Store Passkey securely, each with an about line) and a wide
  button: "Create Passkey", or "Unsupported" (disabled, faded) without
  WebAuthn.

## What changed

- `src/passkeys/` (core): the `passkey` / `passkeys` types of TDLib 1.8.68,
  `getLoginPasskeys` and `removeLoginPasskey`, the cached list with the 5 s
  freshness rule, the row label / visibility / click target, the date line,
  the delete question, the app config switch, and `webauthn_supported()`,
  the one seam the WebAuthn work turns on (it is `false`).
- Settings domain: `SettingsPurpose::Passkeys`, `SettingsPayload::Passkeys`,
  the reducer in `state/domains/settings/passkeys.rs` (list answer, a
  removal's `ok`, failures), and `settings_display_passkeys` read from the
  existing `getApplicationConfig` answer.
- Driver `connect/passkeys.rs`: `request_passkeys` (also asks for the app
  config once), `remove_passkey` (one at a time, only for a passkey in the
  list), and an after-ingest hook that refetches after a removal and resolves
  the software icons with `getCustomEmojiStickers`.
- UI `ui/security/passkeys.rs`: the row in Settings > Privacy and security,
  the list, the delete confirmation and the about box as three registered
  kit dialogs. The confirmation and the box take priority over the list, so
  closing either shows the list again, as a tdesktop box closes back to its
  section. Opening Privacy and security asks for the list. A demo kind
  `ready-passkeys` (two passkeys; `QUILL_DEMO_PASSKEYS=privacy|about|delete`
  picks the surface).

With WebAuthn off, Quill behaves as tdesktop does on a platform without it:
the row shows only when the account already has passkeys, "Add Passkey" is
hidden, and the about box's button reads "Unsupported" and is disabled.

## Differences from tdesktop

- The header shows a kit icon in a tinted circle instead of the `passkeys`
  Lottie (Quill does not ship tdesktop's animations; the passcode lock screen
  does the same). The fallback row icon is Lucide `user-key`, the about rows
  use `user-key`, `scan-face` and `lock`.
- A failed list fetch or removal shows a one-line error (tdesktop shows
  nothing). The confirmation stays open after a failed removal, as in
  tdesktop, with the error under the question.
- The kit menu has no attention style, so Delete in the ⋮ menu is a normal
  item with the trash icon (the confirm button is the danger variant).
- Times are 24-hour like the rest of Quill.

## Waits for WebAuthn

Creating a passkey (`getPasskeyParameters`, the platform prompt,
`addLoginPasskey`) and signing in with one
(`getAuthenticationPasskeyParameters`, `checkAuthenticationPasskey`), the
`passkeys_account_passkeys_max` limit in `canRegister`, and the
`suggestedActionAddLoginPasskey` suggestion. The create button calls
`start_passkey_creation`, which is empty and unreachable until
`webauthn_supported()` returns true. The README item stays partial, so this
PR has no parity fragment.

## How it was verified

- Core tests (`passkeys::tests`): parsing, requests, row label / visibility /
  target, freshness, removal, date line, delete question, app config switch.
- Driver tests with the recording sender (`connect::tests::passkeys`): one
  list request while fresh, the app config switch reaching the row, removal
  waiting for `ok` then refetching, unknown ids not sent, errors shown
  without loops.
- GPUI UI tests (`ui::security::passkeys::tests`): list → Delete → confirm
  returns to a shorter list; Cancel keeps the passkey; Learn more opens the
  box with a disabled "Unsupported" button and closes back to the list; the
  Privacy row shows and opens the list; the row hides without passkeys.
- Demo captures of the list, the confirmation, the box and the Privacy row
  (checked by eye, not committed).

Not verified against a live account: the real `passkeys` answer and a real
removal.
