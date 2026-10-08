# codex:local-passcode — local passcode lock (gap-audit batch 15)

Closes gaps.md batch 15 item 25 (set/change/remove, lock screen, auto-lock, lock icon, Touch ID, tray "Lock") and the "Local passcode + auto-lock + Touch ID" and "Idle detection drives auto-lock" lines. Reference: tdesktop `settings_local_passcode.cpp`, `boxes/auto_lock_box.cpp`, `window_lock_widgets.cpp`, `core/application.cpp` (`lockByPasscode`, `checkAutoLock`), `settings.h` (`passcodeCanTry`).

## Security design

Core: `src/passcode.rs` (no GPUI; Linux CI tests it). UI: `src/ui/passcode.rs`, `src/ui/system_unlock.rs`.

```
passcode --PBKDF2-HMAC-SHA512, 210 000 rounds, 16-byte random salt--> KEK (32 B)
KEK --XChaCha20-Poly1305 (AAD "quill-passcode-master-v1")--> wrapped master key   (passcode.json)
master key --XChaCha20-Poly1305 (AAD = account id)--> wrapped TDLib database key  (accounts/<id>/db-encryption.wrapped)
```

* The passcode is never stored, logged or sent. `passcode.json` holds the KDF name and rounds, the salt, the AEAD-wrapped master key, the auto-lock time, the Touch ID preference and the failed-attempt counter. There is no separate password hash: the AEAD tag of the wrapped master key is the passcode check, so an attacker with the file has exactly one thing to brute-force, at 210 000 SHA-512 rounds per guess.
* A random 32-byte master key (not the passcode key) wraps each account's database key. Changing the passcode re-wraps only the master key, in one atomic file write (temp file, fsync, rename), so a crash cannot leave a mix of old and new wrappings.
* **TDLib cannot start until unlock.** While a passcode is set the database key is removed from the OS secret store (Keychain item, Linux key file, Windows DPAPI file) and exists only wrapped. `live_secret_store()` returns a `PasscodeStore` around the OS store: `get` is `Locked` until the master key is in memory, so `prepare_connect` fails closed and, at launch, the app does not connect at all — it shows the lock screen and starts the connection after the first successful unlock (`PasscodeUi::deferred_connect`). This is tdesktop's behavior (the app starts locked).
* **Migration is rollback-safe** (`passcode::enable`/`disable`). Enable: wrap every account key, write and read back each wrapped file, write the config, and only then delete the OS-store copies; any failure removes what was written and leaves the OS store untouched. Disable: put every key back in the OS store first, then delete the wrapped files and the config. A crash between steps leaves a state `PasscodeStore::get` still resolves (a key missing from the wrapped files falls back to the OS store).
* **Rate limiting** is tdesktop's `passcodeCanTry`: three free attempts, then 5, 10, 15, 20, 25, then 30 s between attempts. The counter and time of the last failure are persisted in `passcode.json`, so restarting the app does not reset it (tdesktop keeps it in memory). The KDF cost, not the limiter, is the defense against someone reading the files offline.
* **Locking** (button, Cmd/Ctrl+Shift+L, tray, auto-lock) drops the master key from memory, covers the window with an opaque overlay, closes the media/story viewers and context menus, hides kit dialogs, moves focus to the passcode field, stops marking history read, and reports the account offline. Notifications raised while locked show "Quill / You have a new message" (or "N new messages"), no sender and no text (`QueuedNotification::for_locked_display`).
* **Log out on the lock screen**: after the master key is gone the databases cannot be opened, so (as in tdesktop) "Log out" asks for confirmation, then deletes the local data of every account on this device and turns the passcode off. It does not terminate the server-side sessions (the key is not available); they stay listed in Active Sessions on other devices.

### Threat model

Protects against: someone with the unlocked-session machine briefly (lock screen, auto-lock), someone who copies the app data directory or the Keychain/key file without the passcode (the TDLib database is encrypted with a key that is not in the OS store), and online guessing at the lock screen.

Does not protect against: malware or a debugger running as the user while the app is unlocked (the master key and TDLib key are in process memory; the master key is dropped on lock, but the live TDLib client keeps its own copy until the process ends), a stolen unlocked process memory image, or weak passcodes (a 4-digit passcode falls to offline guessing; the UI does not require length, like tdesktop).

### What is encrypted

Encrypted with the passcode-wrapped key: the TDLib database (`accounts/<id>/tdlib`, SQLite and binlog, encrypted by TDLib with the database key — unchanged from before the passcode; the passcode only changes where the key lives). Not encrypted by this change: TDLib's downloaded files (`files/`), Quill's thumbnails and exports, preferences, and the decrypted media scratch folder. Those files were never encrypted and are protected only by file permissions/full-disk encryption. tdesktop has the same split (it encrypts its own storage, not the cache folder's media).

## Auto-lock

`passcode::autolock_due`/`IdleTracker` (pure, tested). Presets 1 min, 5 min, 1 h (default), 5 h and a custom `H:MM` (tdesktop `AutoLockBox`). A one-second app timer reads the idle time and locks when it reaches the setting. Idle source: macOS `CGEventSourceSecondsSinceLastEventType` and Windows `GetLastInputInfo` give OS-wide idle ("Auto-lock if away for"). Linux has no portable idle API without compositor-specific protocols (X11 XScreenSaver, Wayland `ext-idle-notify`); it falls back to input to the Quill window and the setting reads "Auto-lock if inactive for" (tdesktop's own label for that mode). A suspend (the timer not running for over 5 s) counts as idle time, so a laptop that slept past the timeout locks on wake (tdesktop's `kAutoLockTimeoutLateMs`).

## Touch ID / system authentication

macOS only: `LAContext` with `LAPolicyDeviceOwnerAuthentication` (Touch ID, Apple Watch or the account password), shown as a switch in the passcode dialog and a button on the lock screen. As in tdesktop ("enter your passcode before you can use Touch ID") the first unlock after launch needs the passcode; the master key is then kept in process memory across locks so the prompt can unlock again. Nothing new is written to the Keychain or disk. The prompt is an app-level gate, not a hardware-bound key release: it protects against a person at the keyboard, not against code running in the process. Windows Hello and a Linux polkit prompt have no backend yet (`system_unlock::available()` is false, the switch is hidden).

## Shortcut deviation

tdesktop locks with Ctrl/Cmd+L. Quill already uses Cmd/Ctrl+L for "Focus composer" (rebindable), so lock is **Cmd/Ctrl+Shift+L** (fixed binding, listed in the shortcuts reference, also in the app menu on macOS).

## Not done

Windows Hello and Linux system unlock; per-account passcodes; encrypting TDLib's file cache; terminating server sessions on lock-screen logout; biometric-bound key storage (Secure Enclave).

## Tests

`passcode::tests` (PBKDF2 published vector, determinism, key-wrap round trip, tamper/wrong-account/wrong-key rejection, rate-limit schedule and persistence across "restarts", enable/disable migration with a memory secret store, rollback with a locked store, change re-wraps only the master key, passcode absent from disk, auto-lock timer and suspend logic, `H:MM` parsing), `notify::tests::locked_notifications_reveal_no_sender_or_text`, `tray` menu routing. Visual: `QUILL_DEMO_THEME=light|dark QUILL_DEMO_CAPTURE=out.png quill --screenshot-demo ready-passcode-settings|ready-passcode-create|ready-lock-screen <dir>` (fixtures never touch the Keychain or the data folder).
