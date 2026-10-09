# Atomic writes for prefs, registry and exports

## Problem
`accounts.json` was written with `std::fs::write` (truncate then write). A crash
mid-write left a truncated file; `load_registry` swallowed the parse error and
returned `[primary]`, and the next save permanently dropped every other account.

## What changed
- `write_json_atomic` (temp file, `sync_all`, rename) now also fsyncs the parent
  directory on Unix (best effort) and uses a per-process counter in the temp name.
  `std::fs::rename` replaces an existing target on Windows too (MoveFileExW with
  MOVEFILE_REPLACE_EXISTING), so no remove-then-rename step is needed.
- `save_registry`, `save_contact_prefs`, `save_badge_prefs` and
  `save_data_storage_prefs` go through it.
- `load_registry`: on a parse failure the bad file is renamed to
  `accounts.json.corrupt-<unix-ts>` (suffix `-N` if that exists, never
  overwritten) and a diagnostic goes to stderr, then defaults are used.
- `chat_export::write_export` writes to a temp file in the destination dir and
  renames at the end; the temp file is removed on failure.

## Audit of other `fs::write` calls under src/ (non-test)
Not converted, deliberately: `notify.rs` (regenerable icon cache),
`autostart.rs` (launcher file, not state), `composer.rs` (temp paste file),
`main.rs` (demo-capture marker/prefs, dev only), `connect/calls.rs` (user-chosen
log destination).

## Verification
Tests: corrupt and truncated registry produce a backup and defaults, repeated
corruption keeps all backups; atomic write replaces an existing file with no
leftover temp; failed writes (rename blocked, read-only dir on Unix) leave the
old file intact. The Windows rename behavior is by the std contract above; not
run on Windows.
