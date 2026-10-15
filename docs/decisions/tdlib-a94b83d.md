# TDLib a94b83d (still 1.8.68)

Pin moved from `c15d3f5a5de6e3ba5839822c451152e5e18bb700` to
`a94b83d67ab4c8397c6245e541b31a4131ea2f79` (tdlib/td master, 23 commits; CMake
version is still 1.8.68, no release tag). tdlib/td stays read-only for us.
Procedure: the "TDLib" section of `docs/dependency-updates.md`; precedent:
`docs/decisions/codex-tdlib-1.8.68.md`.

## Pin

- `src/pins.rs`: `TDLIB_GIT_COMMIT`, `TD_API_TL_SHA256`
  (`44b836e6…cc206be97`), `TD_API_TL_BYTES` (1,191,756 -> 1,192,133),
  `TD_API_TL_UPSTREAM_URL`. `TDLIB_CMAKE_VERSION` stays 1.8.68.
- `schema/td_api.tl` re-vendored with `scripts/vendor-td-schema.sh`;
  `schema/README.md` and `docs/build.md` updated. `THIRD_PARTY.md` carries only
  the version (1.8.68), so it needed no edit.
- `native/patches/tdlib-quill-takeout-contacts.patch` applied cleanly to the
  new commit with offsets (one hunk, `UserManager.cpp`, moved by 28 lines); it
  is regenerated from the tree diff against the new commit so it applies with no
  offsets and `build-tdlib.sh`'s "tree diff == patch" check keeps holding.
- The README badge is edited by the merge pipeline, not this PR.

## Schema diff

`td_api.tl` changed by documentation only (no constructor added, removed or
re-typed):

| Change | Quill impact |
|---|---|
| `gift.background` "may be null" | Quill does not parse gift backgrounds. Nothing to change. |
| `starTransactionTypeBotSubscriptionPurchase/Sale.product_info` "may be null" | `premium_hub::product_title` already reads through `text_of`, so null (or a missing key) yields the generic "Subscription" label. Regression test `null_product_info_falls_back_to_generic_titles` added. |
| `encryptedPassportElement.front_side` "may be null" | Passport elements are not parsed. |
| `usernames.secondary_username` wording | Field is not read. |
| `answerChatJoinRequestQuery` `result`/`url` mutually exclusive | Bot-only. |
| `shareUsersWithBot` also for `keyboardButtonTypeRequestManagedBot` | Bot-only. |

## Notable runtime commits

"Always send updateFile for merged files", "Fix sharing managed bots through
request-managed-bot buttons", "Keep recent inline bots unchanged when previewing
a message", "Allow getUpgradedGift for bots", and internal flag, IPAddress and
PCH cleanups. No Quill code depends on the changed behavior.

## Verification

- `vendored_schema_equals_official_commit` passes (fetches the official file at
  the pin and compares size and hash); `null_product_info_falls_back_to_generic_titles`
  passes.
- Patched TDLib built on Linux with `TDLIB_BUILD_TARGET="tdjson tdjson_static"
  JOBS=2 bash scripts/build-tdlib.sh` (the build script's patch check passed on
  the patched tree, so the regenerated patch equals the tree diff). Offline
  probe of the resulting `libtdjson.so` (ctypes, no network): `getOption("version")`
  = 1.8.68, `commit_hash` = a94b83d67ab4c8397c6245e541b31a4131ea2f79;
  `getQuillSavedContacts` is a known class (it answers "can't be executed
  synchronously", an unknown class answers "Unknown class").
- Linux gate (fmt, clippy both feature sets, tests, frame-idle, notification
  extras, cargo deny licenses, third-party licenses). The one known root-only
  failure, `atomic_write_failure_keeps_old_file`, is ignored.
- Not exercised before merge: macOS and Windows builds (both build scripts read
  the pin from `pins.rs`; CI caches are keyed on it), and a live connection.
