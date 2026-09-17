# Credentials for Telegram (not in this tree)

`TELEGRAM_API_ID` and `TELEGRAM_API_HASH` may be present in the process environment or a gitignored local env file. When both load successfully **and** a `tdjson` library is available, Quill starts a live client (`LiveTdJson`), sends `setTdlibParameters`, and advances authorization updates (typically to `WaitPhoneNumber`). Credentials alone do **not** imply a signed-in session — phone / code / 2FA are still interactive. Do not paste secrets into source, fixtures, CI, or chat. Never commit credentials. Never log `api_hash`, phone numbers, or codes.

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

## tdjson (required for live connect)

After credentials load, Quill also needs the official `tdjson` shared library:

1. `QUILL_TDJSON_PATH` pointing at `libtdjson.dylib` / `libtdjson.so` / `tdjson.dll`, or
2. The library bundled next to the executable (see `docs/native-bundle.md`).

Homebrew prefixes are **never** searched. If credentials are present but tdjson is missing, the UI shows an install/build halt instead of pretending to be signed in.

## Export examples (Mac / Linux)

```bash
export TELEGRAM_API_ID=12345678
export TELEGRAM_API_HASH=your_hash_here
# after scripts/build-tdlib.sh:
export QUILL_TDJSON_PATH=$PWD/native/prefix/lib/libtdjson.dylib   # or .so
```

Or write a local file (already gitignored):

```bash
cat > quill.local.env <<'ENV'
TELEGRAM_API_ID=12345678
TELEGRAM_API_HASH=your_hash_here
ENV
```

Do **not** borrow another client's `api_id`/`api_hash`. Do **not** copy sample IDs from tutorials.

## Headless connect smoke

`--connect-smoke` on the `quill` binary does **not** open a GPUI window. It loads credentials from the environment / gitignored `.env` / `quill.local.env`, **requires** `QUILL_TDJSON_PATH` pointing at a real `libtdjson` file, starts `start_live_connect`, and ingests updates until `WaitPhoneNumber` (or another terminal auth state / clear blocker). Timeout is 30 seconds.

```bash
export QUILL_TDJSON_PATH=$PWD/native/prefix/lib/libtdjson.so   # or .dylib
# TELEGRAM_API_ID / TELEGRAM_API_HASH already in env or quill.local.env
cargo run --no-default-features -- --connect-smoke
# UI feature may be enabled; the flag still skips GPUI:
cargo run --features ui -- --connect-smoke
```

Prints **one** redacted line to stdout, for example:

- `SMOKE_OK wait-phone`
- `SMOKE_BLOCKED missing-tdjson`
- `SMOKE_FAIL timeout`

Exit status is 0 only on `SMOKE_OK …`. Nothing in that line is an `api_hash`, phone number, code, or password.

Other values typed into the app (never committed):

- Test account phone (`setAuthenticationPhoneNumber` when auth is WaitPhoneNumber)
- SMS / Telegram verification code (`checkAuthenticationCode` when auth is WaitCode)
- 2FA password if enabled (`checkAuthenticationPassword` when auth is WaitPassword; Keychain / Linux file store is only for the TDLib **database** key)

Optional later (not required for personal Mac runs): Apple Developer ID, notarization credentials, a dedicated test chat with a second account. There is **no** App Store / notarization / distribution pipeline; ad-hoc Apple Developer signing only if needed on Idan's personal Mac.

Quill still will not enable channels/bots until sponsored-content handling exists.
