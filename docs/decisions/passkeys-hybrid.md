# Passkeys from a phone (hybrid / caBLE v2) on macOS and Linux

Parity item `settings-passkeys`, PR 4 (the last one) of the build plan in
`quill-tools/designs/passkeys.md`. PR 1 is `passkeys-ceremony.md`, PR 2
`passkey-sign-in.md`, PR 3 `passkeys-windows.md`. With this PR every step
of the plan is in, so `parity-fragments/passkeys-hybrid.txt` declares the
item.

## What Telegram Desktop does

`webauthn/cable_core.cpp`, `cable_tunnel.cpp`, `cable_ceremony.cpp`,
`cable_box.cpp`, `cable_scanner_mac.mm`, `cable_scanner_linux.cpp`, and
`webauthn_common.cpp` `RegisterViaCable` / `LoginViaCable`:

- On macOS and Linux every passkey ceremony starts as a phone ceremony.
  It makes a P-256 identity key and a 16-byte secret, then builds the QR
  text: `FIDO:/` followed by the CBOR map {0: compressed public key,
  1: secret, 2: 2 assigned tunnel domains, 3: unix time, 4: false,
  5: "mc" or "ga"}, in 7-byte little-endian chunks of 17 digits each.
- It starts a BLE scan and waits up to 1.5 s to learn whether Bluetooth
  works, then shows the box. With Bluetooth it shows the QR sheet.
  Without it, the same sheet says "Bluetooth is unavailable…" and offers
  only the security key.
- Each advertisement under FDE2 or FFF9 is decrypted with the EID key,
  `HKDF(secret, info=1)`: AES-256 on one block and a 4-byte HMAC tag.
  The plaintext gives the nonce, the routing id and the tunnel domain
  (0 `cable.ua5v.com`, 1 `cable.auth.com`, 256 and up hashed to a generated
  name).
- It opens `wss://<domain>/cable/connect/<routing>/<tunnel id>` with the
  `fido.cable` subprotocol. It then runs Noise
  KNpsk0 over P-256 with AES-GCM and SHA-256: the PSK is
  `HKDF(secret, salt=EID plaintext, info=3)` and the prologue is `[1]`.
  Messages are padded to 32 bytes.
- The phone's first message is CBOR {1: getInfo, 3: features}. A plain
  CBOR first message means protocol revision 1, which puts a type byte
  on every frame. The desktop sends makeCredential (all algorithms,
  `rk` and `uv`) or getAssertion (`up` and `uv`, the allow list). It
  skips Update frames, and builds a `none` attestation object from the
  returned authData.
- Sheets: QR → "Connecting to your phone…" → "Continue on your phone" →
  done, or "Something went wrong" with "Couldn't connect to your
  phone.\nPlease try again." The ceremony times out after the options'
  timeout, or 180 s if they give none. "Use a security key" either
  toasts "No security key found…" or ends the phone ceremony and runs
  the libfido2 one (the touch box).

## What Quill does

`src/webauthn/hybrid/` does the same steps, behind the `Ceremony` seam
from PR 1, and reuses the CTAP2 answer parsing from PR 1
(`parse_made_credential`, `parse_got_assertion`):

- `qr.rs`, `eid.rs`, `crypto.rs`: the QR payload and digits, the HKDF
  derivations, advert decryption, and tunnel domains. All of it uses
  RustCrypto (`p256`, `aes`, `aes-gcm`, `hkdf`, `hmac`, `sha2`).
- `noise.rs`: Noise KNpsk0 over P-256 written in-tree (under 300
  lines). `snow` has no P-256, which caBLE requires.
- `tunnel.rs`: tungstenite over rustls (ring, webpki roots) on a plain
  TCP stream with read slices, so cancel and the deadline are checked
  between reads. No OpenSSL. `messages.rs` holds the post-handshake
  framing for revisions 0 and 1.
- `ble_macos.rs`: a `CBCentralManager` on its own serial dispatch queue
  scans with duplicates allowed. `ble_linux.rs`: BlueZ over D-Bus with
  `zbus`, which is pure Rust, so there is no libdbus or GLib to link.
  It takes the first adapter, sets a discovery filter for LE with
  duplicates, and reads `ServiceData` from known devices, from
  `InterfacesAdded` and from `PropertiesChanged`.
- `flow.rs`: the ceremony on its worker thread. The dialog gets
  `CeremonyEvent::Hybrid { qr }`, `PhoneConnecting`, `PhoneContinue`,
  `UseKey` and `NoKey`. "Use a security key" is
  `Ceremony::use_security_key`. When a key is plugged in, the phone
  ceremony ends and the PR 1 USB ceremony runs on the same worker.
- UI (`src/ui/security/passkey_hybrid.rs`): the Choose sheet has the
  about line, a 260 px quartile QR on a white rounded card with the
  64 px Quill mark on a white rounded square in the centre, the
  Bluetooth hint, the "Use a security key" pill, and the key hint.
  Connecting and Continue show the passkey icon. The phone sheets and
  the error sheet have one full-width Cancel (Close on the error). The
  dialog stays hidden during the Bluetooth probe (`CeremonySheet::Probing`).
- Strings are tdesktop's `lng_passkey_cable_*` and
  `lng_passkey_method_key`, with tdesktop's English.
- The macOS bundle gets `NSBluetoothAlwaysUsageDescription`
  (`scripts/macos-package-smoke.sh`).

New crates: `aes-gcm` (with `aead`, `ctr`, `ghash`, `polyval`,
`universal-hash`), `tungstenite` (with `data-encoding`, `sha1`),
`objc2-core-bluetooth`, plus `async-io` / `futures-lite` on Linux (both
were already in the tree through zbus). All are MIT or Apache-2.0;
`THIRD_PARTY_LICENSES.md` is regenerated.

## Deviations from tdesktop, and why

- tungstenite rejects a server that does not echo the `fido.cable`
  subprotocol, while tdesktop's socket does not check it. The tunnel
  servers do echo it, so this changes nothing in practice.
- When a phone's getAssertion answer leaves out the credential and the
  allow list had exactly one id, Quill uses that id. This is what CTAP2
  says an authenticator may assume. tdesktop sends an empty id, which
  the server then rejects.
- macOS terminates an app that touches CoreBluetooth without
  `NSBluetoothAlwaysUsageDescription`. Quill checks the running
  bundle's Info.plist first, and a bare binary (for example `cargo
  run`) reads as "Bluetooth unavailable" instead of crashing.
- The Linux scan uses zbus instead of GLib's D-Bus. It makes the same
  calls in the same order.
- CBOR from the phone is decoded with a nesting limit of 16 and must be
  consumed in full. This follows the never-crash rule.
- "No security key found" is the status toast Quill already uses for
  other passkey errors.
- The error sheet now uses the same full-width Close as tdesktop's
  cable box, for security-key errors too. Before, it was a
  right-aligned button.
- The Connecting and Continue sheets show the kit `UserKey` icon in the
  header circle that the passkeys box already uses, not the `passkeys`
  Lottie. Quill has no Lottie player for UI icons.
- Windows is unchanged: phones come from the Windows Security dialog,
  as in tdesktop.

## How it was verified

- The vectors come from tdesktop itself. A C++ harness built against a
  scratch copy of tdesktop's `cable_core.cpp` (with OpenSSL) produced
  `tests/fixtures/cable/tdesktop-vectors.txt`. The Rust code matches it
  byte for byte: QR text and digits, HKDF outputs, advert decryption,
  tunnel domains, the makeCredential and getAssertion requests, the
  Noise handshake message and transcript hash, and padded frames.
- `cargo test --no-default-features --lib webauthn::hybrid` runs 29
  tests. They include full register and sign-in ceremonies against a
  scripted fake phone over a real localhost websocket (revision 0 and
  1, Update frames, phone errors, a phone without CTAP). They also
  cover Bluetooth off, a scanner that never answers, no scanner, the
  security key with and without a key plugged in, the timeout, an
  unreachable tunnel, and cancel while connecting.
- GPUI UI tests (`passkey_hybrid_tests.rs`): the probe hides the dialog,
  the QR sheet and its parts, Bluetooth off, the pill with `NoKey` and
  `UseKey`, Connecting → Continue → the phone error and Close, and a
  failure during the probe as a toast.
- Demo captures (`QUILL_DEMO_PASSKEY_CEREMONY=choose|bluetooth-off|
  connecting|continue|phone-error`) were checked in light and dark. The
  QR in the `choose` capture, Quill mark included, decodes with
  CoreImage's QR detector.
- The BlueZ code was built and tested in a scratch crate on macOS. A
  real D-Bus-encoded `PropertiesChanged` body parses, and with no system
  bus the scanner reports "unavailable". Linux CI lints it with the UI
  features.

## Not verified

- No phone was paired, and no real tunnel server (`cable.ua5v.com`,
  `cable.auth.com`) was contacted: nothing ran against the owner's
  account. The first end-to-end run with an iPhone or Android phone
  needs a signed Quill.app on macOS, or a Linux machine with BlueZ.
- The CoreBluetooth scanner has not received a real advertisement. The
  bare test binary has no Info.plist, so it never starts the scan.
- The BlueZ scanner has not run against a real `bluetoothd`.
