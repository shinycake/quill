# App name in UI copy

## What tdesktop does

Its strings say "Telegram" for both the service and the client ("Telegram will
call you" is the service; "Quit Telegram" is the app). Quill borrowed some of
that copy.

## What changed

- Audited every string literal in `src/` and `crates/`, the packaging scripts
  (macOS usage descriptions, Linux desktop entries, package READMEs) and the
  landing page. Nearly all "Telegram" uses already mean the service, accounts,
  Premium, links or the official apps and were kept. Two strings meant this app:
  - "Telegram default tone" -> "Quill default tone" (notification sound picker)
  - demo unsupported-link dialog: "newer version of Telegram. Please update your
    app" -> "newer version of Quill. Please update Quill"
- `scripts/check-app-name.sh` fails on "Telegram Desktop" and a short list of
  app-meaning phrases on non-comment lines in `src/` and `crates/`. Legitimate
  uses go in `scripts/app-name-allowlist.txt` (`path|text`). It runs in the
  `linux-fmt-clippy-test` CI job. `gate.sh` lives outside the repo and does not
  call repo scripts, so it is not wired there.
- One-line copy rule added to the three agent definitions.

## Verified

`tests/app_name_check.rs` runs the script on fixture trees (bad, good,
allowlisted) and on `src/`. Gate: GATE OK.
