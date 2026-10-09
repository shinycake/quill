# TDLib 1.8.68

Pin moved from 1.8.67 (`d1085f9cebc5a62379991ae1652673954f229c1f`) to 1.8.68
(`c15d3f5a5de6e3ba5839822c451152e5e18bb700`, tdlib/td master, "Update version
to 1.8.68", 2026-10-08). 257 upstream commits. tdlib/td stays read-only for us.

## Pin

- `src/pins.rs`: `TDLIB_GIT_COMMIT`, `TDLIB_CMAKE_VERSION`, `TD_API_TL_SHA256`
  (`cea6311c…a5a83eae5`), `TD_API_TL_BYTES` (1,152,505 → 1,191,756),
  `TD_API_TL_UPSTREAM_URL`. `scripts/build-tdlib.sh`,
  `scripts/build-tdlib-windows.ps1` and `src/telegram/requests/misc.rs`
  (`expected_runtime_label`) read the pin from `pins.rs`, so they needed
  no edit.
- `schema/td_api.tl` re-vendored with `scripts/vendor-td-schema.sh`;
  byte-identical to upstream (the `vendored_schema_equals_official_commit` test
  fetches and compares it). `schema/README.md`, `docs/build.md`,
  `THIRD_PARTY.md`, `ROADMAP.md` and the site's "what's missing" data updated.
- `native/patches/tdlib-quill-takeout-contacts.patch` still applies (hunks moved
  by 4–154 lines) and was regenerated from `git -C native/td diff` against the
  new commit, so `build-tdlib.sh`'s "tree diff == patch" check keeps holding on
  an already-patched tree. The patched TDLib compiles and links.
- Schema-line citations in older doc comments ("schema 1.8.67, line N") were
  left alone where the code did not change: they cite a specific file and stay
  accurate. Comments on code this PR touched cite 1.8.68.
- The README badge and toolchain line (`TDLib 1.8.67`) need the merge pipeline
  (feature PRs may not edit README.md).

## Schema diff

113 added, 2 removed, 6 changed constructors. 100 of the additions are TON
wallet, TON Connect and on-ramp, which Quill does not use.

| Change | Quill impact |
|---|---|
| `loadCommunityFullInfo` (answered `ok`, data via update) removed; `getCommunityFullInfo` returns `communityFullInfo` | **Adapted.** Builder, driver and purpose renamed (`get_community_full_info`, `RequestPurpose::GetCommunityFullInfo`); new `EnvelopePayload::CommunityFullInfo` for the direct answer, routed to the community through `PendingRequest::community_id`. `updateCommunityFullInfo` still applies when the pack changes. Without this, opening a community's info panel or chat-list mode would fail with "Unknown class". |
| `editCallbackQueryMessage` removed; `replyToCallbackQueryWithEphemeralMessage` (with `replace_callback_query_message`) | Not used: both are bot-only. Nothing to change. |
| `usernames` + `secondary_username` | Parser reads the fields it needs by name; the new field is ignored. Bots' secondary usernames are a bot-owner feature (`addBotSecondaryUsername`). |
| `animatedEmoji` + `is_regular_emoji` | Ignored by the parser; no behavior depends on it yet. |
| `themeSettings` + `has_outgoing_message_accent_color` | Quill does not parse `themeSettings` (cloud themes are not implemented). |
| `messageProperties` + `custom_emoji_ids` | Ignored; the menu reads its booleans by name. |
| `checkBotUsername` + `is_secondary` | Not called (bot creation is not implemented). |
| `setMessageSenderBotVerification` `custom_description` string → `formattedText` | Not called (bot-only). |

## Newly possible, and what this PR builds

TDLib 1.8.68 adds `setCommunityPermissions`, `setCommunityPhoto` and
`deleteCommunity`. `community` now feeds Quill's state with the viewer's
status and the member permission (`ParsedCommunity::is_owner`,
`can_change_info`, `can_ban_members`, `members_can_edit_chat_list`), which
mirror the checks in TDLib's `CommunityManager` (creator for delete,
`can_change_info` for name and photo, `can_restrict_members` /
`can_ban_members` for permissions).

- Builders, drivers (rights-gated, deduped per community) and state for all
  three. A confirmed `deleteCommunity` drops the community and its pack;
  inaccessible communities (`have_access = false`) are no longer listed in the
  hub. Refusals go to a new one-shot `Session::community_error` that the poll
  loop drains into the status note (this also fixes `setCommunityName` errors,
  which were swallowed before).
- Community info panel: a "Members can edit the chat list" switch (needs
  `can_ban_members`; follows `updateCommunity`, not optimistic) and a
  destructive "Delete community" row (owner only) behind the standard confirm
  dialog. "Edit name" is now shown only with `can_change_info`.
- `setCommunityPhoto` has a builder and a driver but no UI: Quill does not
  render community photos yet (`community.photo` / `communityFullInfo.photo`
  are not parsed), and a setter without a visible result is not worth shipping.
  Next step is avatar rendering for communities, then the photo prompt.

Still blocked in 1.8.68 (no method): adding an existing chat to a community
(`parity:communities-add-chat`), toggling a chat's hidden state
(`parity:communities-chat-visibility`), and community administrator rights
(`parity:communities-admin-rights`). No README checklist item covers
permissions/photo/delete, so this PR declares no parity fragment.

## CI

Every TDLib cache is keyed on `src/pins.rs`, so this PR runs cold TDLib builds
on Linux, Windows and macOS. One fix was needed before pushing: the Windows job
restored its incremental work tree (`native/td` + `native/build`) with the
prefix-only restore key `tdlib-windows-mt-work-`, which would have brought back
a `native/td` checked out at the old commit, and `build-tdlib-windows.ps1`
refuses that ("is d1085f9…, expected c15d3f5…"). The work-tree key now includes
`hashFiles('src/pins.rs', 'native/patches/*')`, so a pin bump starts cold and a
retry within the same pin still resumes.

## Measurements (M1 Pro, local)

| | 1.8.67 | 1.8.68 |
|---|---|---|
| `libtdjson.dylib` (Release, LTO, with the Quill patch) | 39,847,912 B | 40,858,664 B (+1,010,752 B, +2.5%) |
| Cold `build-tdlib.sh` (`TDLIB_BUILD_TARGET="tdjson tdjson_static"`, all cores, cargo builds running alongside) | n/a | 563 s wall, 1,769 s user |

Offline load probe of the new dylib (ctypes, no network, no keychain):
`getOption("version")` = 1.8.68, `commit_hash` = c15d3f5…; `getCommunityFullInfo`
and the patched `getQuillSavedContacts` are known classes; `loadCommunityFullInfo`
is "Unknown class"; a client reaches `authorizationStateWaitTdlibParameters` and
closes cleanly. `--connect-smoke` was not run: it opens the real Quill data root
and Keychain item on this Mac, i.e. a live account.

## Watching for the next release

`.github/workflows/tdlib-watch.yml` (Mondays 08:00 UTC, plus manual dispatch)
runs `scripts/tdlib-watch.py`: it reads the pin from `src/pins.rs`, reads
tdlib/td master's HEAD and `CMakeLists.txt` version through the GitHub API with
`github.token`, and when upstream's version is newer, writes a "TDLib X.Y.Z
available" issue with the compare link, the commit count and the td_api.tl
constructor diff (added / removed / changed signature, TON wallet / TON Connect
/ on-ramp items collapsed into a count). It reuses one open issue (label
`tdlib-update`, falling back to the fixed title pattern) and edits it instead of
opening another. Permissions: `contents: read`, `issues: write`. Run against
the old pin, the script reproduces this PR's diff (113 / 2 / 6, 100 TON items).
