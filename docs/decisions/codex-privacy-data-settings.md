# Privacy keys, security and data settings (B13)

## What tdesktop does

- **Privacy keys** (`settings_privacy_controllers.cpp`, `edit_privacy_box.cpp`): each key has Everybody / My contacts / Nobody plus Always/Never exception lists. Phone number also has "Who can find me by my number" (Everybody / My contacts, shown while the number is Nobody). Groups & Channels offers a "Premium users" row in the Always list. Gifts offers "Mini Apps" (bots) in both lists, a gift-icon switch and five accepted-gift-type switches; changing them needs Premium. Voice messages needs Premium to restrict. "Messages" (new chats) is Everybody or Contacts and Premium users, plus a paid-message price. Exceptions take users and groups (chat-member rules).
- **Sessions**: "Terminate old sessions if inactive for" with 1 week / 1 / 3 / 6 / 12 months; a session info box (application, system, IP address, location with an accuracy note) with Terminate.
- **Sensitive content**: "Show 18+ Content" switch.
- **File open** (`data_document_resolver.cpp`, `core/mime_type.cpp`): before opening an executable, unknown or IP-revealing file a box asks, with "Remember for this file type" (or "Don't ask me again" for the IP warning). Settings has an editable extension whitelist and an IP reveal switch. Files from verified senders never warn.
- **Download path**: a folder and "Ask download path for each file".
- **Password check**: server suggestion "Do you still remember your password?" (`suggestedActionCheckPassword`).
- Network usage is not in tdesktop; the layout follows Telegram X (`TGNetworkStats`).

## What changed

- `PrivacySettingKey` gains bio, birthdate, saved music (`ShowProfileAudio`), find-by-phone, voice messages and gifts (`AutosaveGifts`). `PrivacyRuleDetail` now round-trips Premium, bots and chat-member rules instead of passing them through; the exception overlay edits them (users, groups, Premium users, Mini Apps) with tdesktop's per-key availability.
- Gifts (`setGiftSettings`, state from `userFullInfo.gift_settings`), Messages (`get/setNewChatPrivacySettings`, paid price preserved, rolled back on error), find-by-number, voice-message Premium gate.
- Sessions: `setInactiveSessionTtl` with the five tdesktop periods; details view with Terminate. Session `log_in_date` and `is_official_application` are now parsed.
- 18+ toggle via `setOption(ignore_sensitive_content_restrictions)`, shown only when `can_ignore_sensitive_content_restrictions` is true; it follows `updateOption`, never optimistic.
- `src/file_prefs.rs`: app-wide download folder, ask-each-time, whitelist and IP-reveal switch (`file_prefs.json`), plus a port of tdesktop's extension tables and `LauncherWouldWarn`. Opening a downloaded file or the viewer clip goes through a kit confirm dialog. The save-to-downloads action asks for a place when "ask" is on (GPUI save prompt, all three OSes); the folder picker is the GPUI directory prompt.
- Network usage (`getNetworkStatistics`, `resetNetworkStatistics`) per Mobile / Wi-Fi / Roaming / Other with category rows, call time and a confirmed reset, at the bottom of Data & Storage.
- Password check card in Privacy and the Two-Step Verification dialog: `getRecoveryEmailAddress(password)` verifies, "Done"/"Dismiss" send `hideSuggestedAction`.

## Skipped or already present

- **Rename this device**: no TDLib method in `td_api.tl` (the session-details README item stays open).
- **Per-type and per-chat clearing, storage limits**: already shipped in batch 6 (`ui/dialogs/local_storage.rs`, `optimizeStorage` with `file_types`/`chat_ids`); not re-done. Their README boxes look stale.
- Paid-message price editing, "Messages" exceptions (`AllowUnpaidMessages`) and call privacy exceptions are not built.
- The verified-sender exemption for file warnings is not applied: the open path has no message context.

## Verification

Gate (fmt, clippy, core and UI tests). New core tests cover rule round-trips, request shapes, envelope parsing, reducer/driver flows (rollback, TTL, options, password check) and the file classifier. Demo captures (English fixtures): `ready-privacy`, `ready-privacy-gifts`, `ready-session-details`, `ready-file-open-confirm`, viewed in-process. Not tested live (no account): server acceptance of the new rules and `getRecoveryEmailAddress` as a password check.
