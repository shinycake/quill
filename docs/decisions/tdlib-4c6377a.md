# TDLib 4c6377a (still 1.8.69)

Pin moved from `0e88653dcbf4e44c73cb655a05b58cd749a91169` to
`4c6377aa7386c39217878a9a56d581922ad37ca4` (tdlib/td master, 28 commits; CMake
version is still 1.8.69, no release tag). tdlib/td stays read-only for us.
Procedure: the "TDLib" section of `docs/dependency-updates.md`; precedent:
`docs/decisions/tdlib-a94b83d.md`.

## Pin

- `src/pins.rs`: `TDLIB_GIT_COMMIT` and `TD_API_TL_UPSTREAM_URL`.
  `TD_API_TL_SHA256` and `TD_API_TL_BYTES` are unchanged: `td_api.tl` is
  byte-identical at both commits, so `schema/td_api.tl` is not re-vendored.
- `schema/README.md` and `docs/build.md` updated.
- `native/patches/tdlib-quill-takeout-contacts.patch` applied cleanly; it is
  regenerated from the tree diff (two `index` lines and one hunk offset in
  `UserManager.cpp`) so `build-tdlib.sh`'s "tree diff == patch" check holds on
  a rebuild.

## Notable runtime commits

| Commit | Quill impact |
|---|---|
| Preserve removal of already displayed edited notifications | Fixes notification groups after edits; Quill reads `updateNotificationGroup` as before. |
| Fix updating of replied count after replied message change | Reply counters in threads stay right without a reload. |
| Don't lose promises after an empty one in `on_get_monoforum_topic` | Monoforum topic loads no longer hang. |
| Move pinned topics to `OrderedTopics`, use it in `SavedMessagesManager` | Internal reorder of Saved Messages topic ordering; same updates. |
| Remove empty media blocks from rich messages | Fewer empty blocks to skip. |
| Fix `UploadEphemeralMessageContentCallback` | Upload path fix. |
| MSVC 14.51 internal compiler error in `is_reserved_ipv4` | Keeps the Windows build working on the newest MSVC. |

The rest is container and helper cleanup (`td::reverse`, `FlatHashMap`,
automatic string flags, tests). No API change, so no Quill code changes.

## Verification

- `vendored_schema_equals_official_commit` passes against the new pin.
- Patched TDLib built on macOS with `TDLIB_BUILD_TARGET="tdjson tdjson_static"
  bash scripts/build-tdlib.sh`. Offline probe of `libtdjson.dylib` (ctypes, no
  network): `getOption("version")` = 1.8.69, `commit_hash` =
  4c6377aa7386c39217878a9a56d581922ad37ca4; `getQuillSavedContacts` is a known
  class ("can't be executed synchronously"; an unknown class answers "Unknown
  class").
- macOS gate (fmt, both clippies, lang checks, licenses, core and UI tests).
- Not exercised before merge: Linux and Windows TDLib builds (they run in the
  packaging workflows after merge and read the pin from `pins.rs`), and a live
  connection.
