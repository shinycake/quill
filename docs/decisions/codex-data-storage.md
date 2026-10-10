## codex/data-storage (2026-10-09)

Closes the data-storage cluster: two storage items that were already built
get verified and claimed, and the spelling dictionary manager is new.

### Storage limits and per-type clearing (already in main, verified)

What Telegram Desktop does (`settings/sections/settings_local_storage*`,
`boxes/local_storage_box.cpp`): the local storage box lists the cache by
type with a clear action, shows a total size limit and a "clear files older
than" limit, and reports how much space was freed.

What Quill has (Batch 6, `src/ui/dialogs/local_storage.rs`,
`src/storage_limits.rs`, `Connect::clear_storage`, `set_storage_limits`):

- Usage by type with a checkbox per type, "Clear selected", "Clear all"
  and a per-chat "Clear". Each sends `optimizeStorage` with the chosen
  `file_types` or `chat_ids`, and the answer's statistics become "{size}
  freed on your device!".
- "Total size limit" and "Clear files older than" set TDLib's storage
  optimizer options (`storage_max_files_size`,
  `storage_max_time_from_last_access`, `storage_max_file_count`,
  `use_storage_optimizer`) and read them back from `updateOption`.

Checked this round: the request shape (new test
`optimize_storage_carries_file_types_and_chats`, next to the existing
`everything` test), the option mapping (`storage_limits` tests) and the
type list against `schema/td_api.tl`. Claimed: `data-storage-limits`,
`data-clear-per-type`. The download folder item (`data-download-path`) is
already checked in the README, so nothing was done there.

### Spelling dictionary manager (new)

What Telegram Desktop does (`boxes/dictionaries_manager.cpp`): a list of
spell-checker languages with a filter box. Each row has a switch and a
state line (download size, progress, "Enabled"). Switching an uninstalled
language on downloads it, switching a downloading one off cancels it, and
the context menu removes a downloaded dictionary. tdesktop shows it only
where it owns the dictionaries.

What changed:

- `src/spell_catalog.rs` (core, pure): a 57-language catalog with install
  codes, display names and download sizes; row state and status text;
  the filter; `install` (validates the pair with `spellbook`, writes
  temp files, renames, `.dic` last) and `remove` (catalog codes only, so a
  path can't leave the folder); `next_chosen` for the switches.
- `src/spell_download.rs`: blocking `ureq` download with progress and
  cancel, HTTPS only, one pinned host, no redirects, 40 MB cap per file.
- Source: tdesktop's dictionaries are Telegram-hosted zips served over
  MTProto, which TDLib can't fetch. Quill downloads the Hunspell `.aff` and
  `.dic` from a pinned commit of the open `wooorm/dictionaries` collection
  (each dictionary keeps its own license; the UI says so). The catalog
  leaves out dictionaries over 16 MB and languages the engine has no script
  rule for.
- Downloads land in `<app data>/dictionaries`, which is now searched first
  on Linux and Windows. The engine needed no change: a downloaded
  dictionary appears in `available`, and the existing language picks apply.
- UI (`src/ui/spell_dictionaries.rs`): Appearance, Spelling, "Manage
  dictionaries" opens the filter and list. A "Remove" button replaces
  tdesktop's context menu, so it is reachable by keyboard and screen
  reader. System dictionaries are never removable.
- Where it shows: only when Quill owns the dictionaries (Hunspell engine
  or none). macOS (NSSpellChecker) and the Windows system checker keep
  their language lists in the OS, as in tdesktop.

### Verified

- Unit tests: catalog invariants, URLs, install and remove in temp dirs
  (rejected downloads leave nothing behind; the Hunspell engine reads an
  installed dictionary), language switching, rows and filter, URL
  allowlist, progress flags.
- Demo capture `ready-dictionaries` (English fixture rows for enabled,
  installed, downloading, failed and available), viewed.
- Not run: a real download (the test machine is macOS, where the manager
  is hidden), and Linux or Windows desktops.
