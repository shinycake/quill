# Stale libtdjson aborting on a newer database

On 2026-10-21 a local `dist/Quill.app` aborted at launch (SIGABRT) on the
owner's account. TDLib's log ended in `JsonBuilder.cpp:103` while sending
`updateOption ton_wallet_card_background_gifts` with the value
`PlushPepe\xFFResistanceDog`.

## Cause

That bundle carried a TDLib 1.8.68 `libtdjson` (`commit_hash` a94b83d…), but
the account's database had already been opened by 1.8.69 (the installed
2026.1021.3340 release). 1.8.69 stores `ton_wallet_card_background_gifts` as
an internal option, joining the gift names with a `0xFF` byte, and never sends
it to clients (`OptionManager::is_internal_option`). 1.8.68 doesn't know the
name, so when it loads saved options at startup it sends the value as an
`optionValueString`. The JSON interface checks every string for UTF-8 and
aborts on that byte.

Released builds are not affected: each one bundles the pinned TDLib, and the
pin (1.8.69) treats the option as internal. tdlib/td master after the pin has
no change to this path.

## What changed

- Patch to TDLib (`native/patches/tdlib-quill-takeout-contacts.patch`, the
  single pinned patch every platform's build applies):
  `OptionManager::get_option_value_object` returns `optionValueEmpty` for a
  string option that isn't valid UTF-8. This build no longer aborts when it
  meets a database from a newer TDLib that adds another `0xFF`-joined option,
  for example after a user reinstalls an older Quill.
- Quill refuses a `libtdjson` whose `version` differs from
  `pins::TDLIB_CMAKE_VERSION` before any client opens the database
  (`TdJson::check_pinned_version`, called from `TdHub::load_secured_api`).
  Connecting stops with `ConnectBlocker::TdjsonVersion` and a message that
  says to rebuild TDLib. A stale library in a local bundle now shows that
  message instead of aborting or reading the database with the wrong schema.

## How verified

- The installed 2026.1021.3340 app (TDLib 1.8.69) launched on the owner's
  account and stayed up.
- The patched TDLib was built with `scripts/build-tdlib.sh` (its tree diff
  matched the patch) and launched on the owner's account.
- CI builds the patched library on macOS, Windows and Linux x86_64/arm64; the
  TDLib caches are keyed on `native/patches/*`.
