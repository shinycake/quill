# Stability sweep round 2: small fixes

Audit: `round2-stability.md`, findings 4, 5, 8, 9, 10, 14. Finding 7 was
checked first and skipped (see below). No tdesktop behavior is involved;
these are robustness fixes.

| # | Change |
|---|--------|
| 4 | `folder_archive.rs`: the zip writer refuses more than 65,535 entries and names over 65,535 bytes with the existing "too big" error, instead of wrapping `as u16` into a corrupt end-of-central-directory record. Zip64 was not added: a 65,536-file folder is rare enough that the clear error is the cheaper honest answer. |
| 5 | `media_tools::stop_capture` calls `libc::kill(pid, SIGINT)` on Unix; `update_install` checks the parent with `kill(pid, 0)` (`EPERM` counts as alive, an id that does not fit `i32` as gone). No `kill` binary on PATH is needed (Flatpak, minimal images). |
| 8 | `single_instance.rs` on Windows: the pipe name hashes the canonicalized root, lower-cased, with `/` folded to `\`, the `\\?\` prefix and a trailing separator removed (`windows_root_key`, tested on every platform). The acquire lock is now a named mutex (`Local\quill-<hash>.ipc.lock`, 10 s wait, abandoned counts as acquired) taken and dropped on the same thread inside `acquire`, matching the Unix `flock`. |
| 9 | `export/account/folder_acl.rs`: the export folder gets a protected DACL (no inheritance; full control for the owner and SYSTEM) via `SetNamedSecurityInfoW`, best effort because FAT/exFAT volumes refuse ACLs. Needed the `Win32_Security_Authorization` windows-sys feature. The doc comment now names both platforms. |
| 10 | `update_install.rs` on Windows: a null `OpenProcess` handle only means "parent gone" for `ERROR_INVALID_PARAMETER` (87, `open_failure_means_gone`). Other failures (access denied, elevated parent) retry every 200 ms for up to 30 s, then report "App did not exit". |
| 14 | `checklist::next_task_ids` uses saturating arithmetic and a checked count; `reaction_who` computes the remainder with saturating subtraction and reads "{names} and {count} more" from the new key `qll_chat_reactors_and_more`. |

Finding 7 (`stale_history_requests`) is already fixed on main: `src/state/requests.rs` clears the set in `invalidate_account`, with a test in `tests/lost_requests.rs`.

## Verification

- Unit tests: 65,535 entries accepted, 65,536 and a 70,000-byte name refused;
  SIGINT stop of a `sleep` child finishes in under 5 s; liveness of self,
  a missing pid and `u32::MAX`; the Windows error classifier; the Windows
  root key folding; checklist id overflow; reaction tooltip overflow.
- Not run locally: the `cfg(windows)` code (mutex, DACL, OpenProcess retry).
  It compiles on the Windows CI job of the `cursor/` branch; behavior on a
  real Windows machine is unverified.
