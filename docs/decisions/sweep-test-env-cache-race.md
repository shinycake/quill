# Test-environment flag cache keyed by app root

tdesktop reads the account's test mode from its own `Account` object, so no
process-wide cache exists to race. Quill keeps the active account's test flag
in memory so render paths never touch the store.

The cache was one global slot. Parallel tests (and any writer for another app
root) cleared or overwrote it between two reads, which failed
`cached_flag_follows_add_switch_and_remove` on Windows CI.

Changes:
- The cache is a map keyed by app root with a per-root epoch. `save_registry`,
  `mark_test_environment`, `delete_account_folder` and `wipe_local_data`
  invalidate only their own root.
- `active_is_test()` resolves the root with `safe_app_root()` and reads that
  root's entry instead of returning any cached value.
- Deleting an account folder and the passcode wipe now invalidate the flag, as
  the marker file goes with the folder.

Verified: two new unit tests (another root's writes do not clear the flag;
folder deletion forgets it); `test_environment` and `settings::` tests run 50
times under ten busy loops with default test threads, 0 failures.
