# TDLib 1.8.69

Pin moved from `a94b83d67ab4c8397c6245e541b31a4131ea2f79` (1.8.68) to
`0e88653dcbf4e44c73cb655a05b58cd749a91169` (tdlib/td master, "Update version
to 1.8.69", layer 232). 110 upstream commits. tdlib/td stays read-only for us.
Procedure: the "TDLib" section of `docs/dependency-updates.md`; precedent:
`docs/decisions/codex-tdlib-1.8.68.md`.

## What tdesktop does

The reference clone (`~/Developer/Reference/tdesktop`, 2026-10-02) predates
layer 232. It still sends `payments.getStarGiftWithdrawalUrl` with an SRP
password check, and "Send via Blockchain" first tries an empty password to
learn whether a password exists and is old enough. Layer 232 replaces that
argument with `tmp_password`, so every client now gets a temporary password
(`account.getTmpPassword`) before asking for the export URL. TDLib 1.8.69
mirrors this: `getUpgradedGiftWithdrawalUrl` lost its `password` and refuses
to run until `createTemporaryPassword` has succeeded.

## What changed

Pin: `src/pins.rs` (`TDLIB_GIT_COMMIT`, `TDLIB_CMAKE_VERSION`,
`TD_API_TL_SHA256` `d8c53056…dcb6a2e9`, `TD_API_TL_BYTES` 1,192,133 to
1,216,190, `TD_API_TL_UPSTREAM_URL`), `schema/td_api.tl` re-vendored with
`scripts/vendor-td-schema.sh`, `schema/README.md`, `docs/build.md`,
`THIRD_PARTY.md`, `ROADMAP.md` and the site's TDLib version. The build
scripts and `expected_runtime_label` read the pin from `pins.rs`. The
takeout patch applied with offsets only (2 to 44 lines) and was regenerated
from `git -C native/td diff`. The README badge is left to the merge pipeline.

Schema diff: 41 added, 1 removed, 19 changed constructors.

| Change | Quill impact |
|---|---|
| `getUpgradedGiftWithdrawalUrl` lost `password`; needs a temporary password | **Adapted.** Submit in the password box now sends `createTemporaryPassword` (60 s, the minimum) with purpose `GiftExportTempPassword`. Its `temporaryPasswordState` answer sets a one-shot `GiftManageState::export_unlocked`, and the driver's after-ingest `continue_gift_export` sends `getUpgradedGiftWithdrawalUrl` with only the gift id. Errors from either request go through the existing `password_check_failure` mapping, so a wrong password, a missing one or a too fresh one land where they did before. The "Open Fragment" probe drops its empty password. TDLib now refuses the probe locally ("Temporary password required"), which still opens the password box, or the security check when the reloaded password state says no password is set. A too fresh password is now reported on Submit, not on the probe. |
| `storyAreaPosition` renamed `areaPosition` (`storyArea`, `inputStoryArea`) | **Adapted.** The story composer builds `areaPosition`. The parser reads the fields by name, so incoming areas were unaffected. Fixtures and comments follow the rename. |
| `textQuote` / `inputTextQuote` + `rich_texts` | `input_text_quote_json` sends the formatting a quote keeps and, for a quote cut from a rich message, one `RichText` per quoted text (see `parity-msg-quote-rich-text.md`). Incoming quotes and drafts read `rich_texts` when `text` is empty, as TDLib sends a quote from a rich message. Fixtures carry it. |
| `internalLinkTypePublicChat` / `UserPhoneNumber` / `UserToken` + `unpaid_messages_token` | Ignored; links route as before. Fixtures carry the field. |
| `getUserLink` + three flags, `userLink` + `can_get_permanent_link` | Not called or parsed. |
| `acceptOauthRequest` + `ton_connect_connect_data`, `oauthLinkInfo` + TON fields | Not called or parsed (OAuth login requests are not implemented). |
| `userFullInfo` + `ton_wallet_card`, `share_theme`; `supergroupFullInfo` + `share_theme` | Parsers read by name; new fields ignored. |
| `messageGift` / `receivedGift` + `can_be_deleted`; `upgradedGift` + `can_be_ton_wallet_card_background` | Parsers read by name; new fields ignored. |
| `getTonWalletSecretPhrase` returns `tonWalletSecretPhrase`; `tonNft` + `gift` | `getTonWalletSecretPhrase` stays in the wallet's forbidden list; `tonNft.gift` is ignored. |

The rest of the additions (web proxy carrier, profile share themes, TON wallet
cards, `deleteGift`, `withdrawUpgradedGift`, `displayUpgradedGift`,
`applyUnpaidMessagesToken`, `messageUnpaidMessagesAllowedByLink`) are new
features and are not built here.

## How verified

- `vendored_schema_equals_official_commit` passes; the vendored file is
  byte-identical to `td/generate/scheme/td_api.tl` in the clone at the pin.
- `connect/tests/gift_export.rs` covers the new export sequence: no URL request
  before the temporary password, the URL request carries no password, a
  second Submit sends nothing, the password never reaches the session, errors
  from either step land in the password box or the security check, and the
  probe's new TDLib error still opens the right step.
- Patched TDLib built on this Mac (M1 Pro) with
  `TDLIB_BUILD_TARGET="tdjson tdjson_static" bash scripts/build-tdlib.sh`:
  9 min 44 s wall, 1,799 s user. The script's "tree diff == patch" check
  passed. `libtdjson.dylib` is 41,388,568 B (1.8.68 at `c15d3f5`: 40,858,664 B).
- Offline probe of that dylib (ctypes, no network, no keychain):
  `getOption("version")` = 1.8.69, `commit_hash` = 0e88653…;
  `createTemporaryPassword`, the new `getUpgradedGiftWithdrawalUrl`,
  `getQuillSavedContacts`, a `postStory` with `areaPosition` and a
  `sendMessage` with `rich_texts` parse as requests; a client reaches
  `authorizationStateWaitTdlibParameters` and closes cleanly. `td_execute`
  does not check nested object types, so the probe proves only the top-level
  method names.
- Gate (`quill-tools/gate.sh`): fmt, clippy for both feature sets, core and UI
  tests, licenses.
- Not run: a live export (it needs an account with an unlocked collectible and
  would open Fragment), `--connect-smoke` (it opens the real data root and
  Keychain item), and the Windows and Linux builds (both read the pin from
  `pins.rs`; CI caches are keyed on it).

## For follow-up work

New in 1.8.69 and tracked as README parity items: `gifts-delete`,
`paid-messages-free-links` (including rendering `messageUnpaidMessagesAllowedByLink`),
`profile-share-theme`, `data-proxy-web`, `msg-quote-rich-text` and
`wallet-gift-withdraw-card`. TDLib now expires its own caches of user photos,
recommendations, invite link info, common chats and public chat search results, so
Quill needs no eviction of its own for those.
