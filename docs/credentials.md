# Credentials required for live Telegram (not in this tree)

Live login is **disabled**. Do not paste secrets into source, fixtures, CI, or chat.

When Phase 1 live testing is authorized, the owner must supply:

| Item | How | Storage |
|---|---|---|
| `api_id` (int) | https://core.telegram.org/api/obtaining_api_id — register **this** app | env `QUILL_API_ID` or an untracked local file |
| `api_hash` (string) | same | env `QUILL_API_HASH` |
| Test account phone | owner's authorized test user | typed into the app; never committed |
| SMS/Telegram code | from Telegram | typed into the app |
| 2FA password if enabled | owner's | typed into the app; Keychain is only for the TDLib **database** key |

Do **not** borrow another client's `api_id`/`api_hash`. Do **not** copy sample IDs from tutorials.

Optional later (release, not Phase 1): Apple Developer ID, notarization credentials, a dedicated test chat with a second account.

Quill still will not enable channels/bots until sponsored-content handling exists.
