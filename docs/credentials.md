# Credentials required for live Telegram (not in this tree)

Live login unlocks **only when** `TELEGRAM_API_ID` and `TELEGRAM_API_HASH` are present in the process environment or a gitignored local env file. Do not paste secrets into source, fixtures, CI, or chat. Never commit credentials.

## Exact names

| Item | Env name | Notes |
|---|---|---|
| `api_id` (int) | `TELEGRAM_API_ID` | From https://core.telegram.org/api/obtaining_api_id — register **this** app |
| `api_hash` (string) | `TELEGRAM_API_HASH` | Same registration page |

Optional aliases (used only if the matching `TELEGRAM_*` name is unset): `QUILL_API_ID`, `QUILL_API_HASH`.

Also accepted (gitignored; never commit):

- `.env`
- `quill.local.env`

Quill looks for those files under the crate root (`CARGO_MANIFEST_DIR`) and the process current working directory. Simple `KEY=VALUE` lines; `#` comments. No secrets belong in git.

## Export examples (Mac / Linux)

```bash
export TELEGRAM_API_ID=12345678
export TELEGRAM_API_HASH=your_hash_here
```

Or write a local file (already gitignored):

```bash
cat > quill.local.env <<'ENV'
TELEGRAM_API_ID=12345678
TELEGRAM_API_HASH=your_hash_here
ENV
```

Do **not** borrow another client's `api_id`/`api_hash`. Do **not** copy sample IDs from tutorials.

Other values typed into the app (never committed):

- Test account phone
- SMS / Telegram verification code
- 2FA password if enabled (Keychain is only for the TDLib **database** key)

Optional later (not required for personal Mac runs): Apple Developer ID, notarization credentials, a dedicated test chat with a second account. There is **no** App Store / notarization / distribution pipeline; ad-hoc Apple Developer signing only if needed on Idan's personal Mac.

Quill still will not enable channels/bots until sponsored-content handling exists.
