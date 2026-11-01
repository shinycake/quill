# fix-final-b: what was fixed and what was not

Branch `claude/fix-final-b`, from `origin/sweep-all` at a619b43df.
`origin/calls-followups` (e38fdbf4) was already in sweep-all, so merging it
changed nothing. `origin/main` was already in as well.

The review is `review.md` on `claude/review-fix-rounds`. Sections 3 (B10),
4 (B9) and 5 (export quit) are covered here, plus the group call in no known
chat. Every new test was run without its fix and failed. The one exception is
the fsync before the rename, which no test can show (see below).

## Fixed

### B10

- **#1, an over-cap launch took over a live owner** (`src/single_instance.rs`).
  Past `MAX_CLIENT_THREADS`, the owner now writes `busy` before it closes. It
  does the same when a client thread cannot start. The launcher reads `busy` as
  `Unresponsive`, which `acquire` turns into `TimedOut` (`Launch::OwnerStuck`).
  Writing `busy` alone did not fix it: the owner closes before it reads, so the
  launcher's write failed with EPIPE and the launcher dropped the answer that
  was waiting. The launcher now reads the answer even when its write fails.
  Test: `a_launch_past_the_client_cap_is_told_the_owner_is_busy_and_does_not_take_over`.
  It fills the 8 slots against a heartbeat that never beats, on a clock that
  never moves, so no wait in the test depends on time. A 9th `acquire` must
  then get `TimedOut`. Without the fix it got `Ok(Primary)`, in 3 of 3 runs.
- **#7, the startup gap in the link inbox** (`src/ui/link_inbox.rs`). The
  heartbeat started in `install_link_inbox`, and the spawned task read `seen`
  only later. `answer_wakes` now reads the epoch first, then starts the
  heartbeat, then beats and runs one turn before its first sleep. A launch that
  marks before the start is answered at once, as before. Its activation or link
  is taken by that first turn, so it no longer waits up to 60 s for the next
  wake.
  Test: `a_launch_that_pokes_before_the_inbox_first_runs_is_answered_at_once`.
  The test for a poke during a turn was updated for the first turn.
- **#4, router changes seen late or not at all** (`src/net_watch/`). There
  were three gaps:
  - macOS IPv6 read no router hardware address. The new `llinfo.rs` reads the
    IPv6 router's NDP entry from the routing table. It uses `sysctl
    {CTL_NET, PF_ROUTE, 0, AF_INET6, NET_RT_FLAGS, RTF_LLINFO}`, the call
    `ndp -a` makes, and clears the KAME scope from link-local addresses before
    it compares them. configd's `Router` value now has any `%zone` cut off
    before it is parsed. Test: `two_ipv6_routers_on_one_address_read_as_two_networks`
    builds a routing dump with the macOS layout by hand and parses it on Linux.
  - While a new gateway's hardware address was unresolved, `settled` kept the
    old one, so the change showed only once traffic made the system resolve it.
    `Tracker::gateways_to_resolve` now names each gateway whose address is carried over,
    once per loss, and `link::resolve` sends one empty UDP datagram to it on
    port 9 (Linux, macOS). That starts ARP or ND at once. On Linux the
    neighbour event that follows runs `network_changed` again, and the change
    is seen. On Windows, `link::of` already starts `ResolveIpNetEntry2`.
    Naming each gateway only once stops a router that never answers from
    turning every failed lookup's event into another lookup.
    Test: `a_new_router_on_the_same_address_is_looked_up_at_once_and_only_once`.
  - Windows `RESOLVING` was one flag for both IP versions. It is now
    `Resolving`, one flag per version, freed on drop (including when the
    thread never starts).
    Test: `an_ipv4_router_still_resolving_does_not_hold_up_the_ipv6_one`.
- **#6a** (`src/net_watch/watcher_tests.rs`). As asked, there is still no
  `wait_timeout`. The deterministic way this test can go deaf is a refused
  registration, because `keep_working` logs it and goes on without it. The
  test now asserts that all three registrations were made, so that case fails
  at once and says why. A registration that Windows accepted and then never
  calls back cannot fail any sooner. Windows gives no point after which every
  callback for a change has run, so anything else would be a time bound. That
  case still ends at the CI step's time limit, and the test's comment says so.

### B9

- **#1, a stale `pending_still`** (`src/ui/sticker_set_box/creator.rs`).
  `pick_existing_sticker` now clears both `pending_still` and `pending_video`
  at the top, so any new pick replaces one still decoding or downloading. The
  video case had the same gap: a pending video download opened the video
  editor over a later Tgs pick.
  Test: `a_still_decoding_behind_a_later_animated_pick_opens_no_editor`
  (covers both cases).
- **Replay intro race, no test** (`src/ui/first_run_ui.rs`). The write moved
  into `write_clip(dir, bytes, lock, write)`, and `intro_file` calls it with
  the real lock. Test:
  `a_replay_during_the_first_write_waits_and_reuses_the_written_clip`. It
  checks that the lock is held during the write. It also checks that a second
  call reuses the clip without writing, and that no `.part` file is left.
  Without the lock, the first check fails.

### Export quit

- **A held delete waited for any export of the account.** `WorkerLink` now has
  an `ExportId`. Each held delete records the export that copied the file and
  goes once that export stops or the chat leaves the screen
  (`ExportFiles::release_held`). `SettingsState::exporting` had no other
  caller, so it was removed.
  Test: `a_held_delete_goes_when_its_own_export_stops_while_another_still_runs`.
- **A delete answered with an error cleared the journal.** `ExportFiles::answer`
  keeps the entry when the `deleteFile` answer is an `error`, so the next
  start's check tries again.
  Test: `a_cache_copy_whose_delete_fails_stays_listed_for_the_next_start`.
  Whether TDLib really answers `deleteFile` with an error on a live database
  was not traced. The envelope does route every answer that carries the export
  `@extra` to this code, errors included (`parse_envelope`).
- **A `.part` file left after a crash, and no sync before the rename.** While a
  copy writes `<name>.part`, the path is listed in the account's journal
  (`export-leftovers/parts/<run>-<n>`). The entry goes when the copy is done.
  On Ready, `check_export_leftovers` first runs `Leftovers::sweep_parts`, which
  removes the parts that earlier runs listed. It only removes an absolute path
  ending in `.part`, never other files. `settle` now runs `sync_all` on the
  finished part before the rename, and on Unix syncs the folder after it.
  Test: `a_part_file_a_crash_left_in_the_export_folder_goes_on_the_next_start`.
  No test can cut the power, so the sync has no test.
- **Stale comment** at `src/connect/closing.rs` (`EXPORT_STOP_HOLD`): fixed.
- **Bound the re-queue of the macOS `answer_later` and the Windows quit-later
  handler.** The forks share a new `QuitAnswer::step(asked)` with
  `QUIT_ASKS_AGAIN = 100`, in `third_party/gpui-pre/src/platform.rs`. macOS
  counts the asks in a static. Windows carries the count in the message's
  `lparam`. After 100 asks that still get `Later`, the quit is cancelled, as a
  busy app would answer. The user can quit again. Without the bound, a modal
  panel that holds the app kept the main queue or message loop spinning until
  the panel closed. The three `QUILL-CHANGES.md` files were updated.
  Test: `an_os_quit_that_finds_the_app_busy_for_good_is_cancelled_after_bounded_asks`
  in `src/ui/export_quit_tests.rs`. It feeds the real `Later` answer of a
  borrowed app through `step` and expects a cancel after exactly 100 asks. The
  vendored crate's own tests do not build on their own, so the test lives in
  Quill.

### Group call in no known chat

`start_group_offer` (`src/connect/group_calls/join.rs`) made no instance when
`group_call_chat_id` was `None`. It then sent `joinVideoChat` with
`honest_no_device()` params: an empty payload and audio source 0. The user
showed up in the call but heard nothing. The instance is now made with chat 0.
The engine uses the chat only to key stream channels, and
`create_conference_call` already passes 0. With no engine, or an unavailable
one, the offer now fails through `fail_offer` and sends no join without media.
`honest_no_device` had no other caller, so it was removed.
Test: `a_voice_chat_in_no_known_chat_joins_with_an_instance_and_its_payload`.

## Not fixed, or not a defect

- **Export leftovers cleaned only when the account reaches Ready again.** This
  is not a defect in practice, for these reasons:
  - Every signed-in account gets a background TDLib client at startup
    (`ui/background_accounts`, `parity:notify-all-accounts`).
  - That client is the same `ConnectDriver`, so it runs `check_export_leftovers`
    when it becomes Ready (`connect/core/mod.rs`).
  - An account that never becomes Ready again is one that was signed out or
    whose session ended. TDLib then destroys its database and files, the cache
    included.
  - A removed account loses its whole folder, the journal included.

  An "account that is never opened again" therefore still gets cleaned on each
  start. No new test was added. The existing
  `a_file_an_earlier_run_left_in_the_cache_goes_on_the_next_start` covers the
  Ready path, which the background clients share.
- **B9 #3** needs a Windows GPU and was not run here:
  - a real device loss with a toast up (`dxcap -forcetdr`);
  - hardware-GPU numbers;
  - an explanation of the first toast's 47.7 ms.

  To settle the first-toast figure, repeat the run in prtext-b9-2's "Numbers"
  section with the order swapped: main first, then the branch, in one job,
  with `QUILL_DEBUG_NOTIFICATION_CUSTOM=20 QUILL_TRACE_TOASTS=1`. If the
  branch's first toast falls to about main's 13 ms, the gap was the cold
  runner.
- **macOS and Windows quit paths, real runs.** The bound above was
  type-checked for Windows only (see below). Nothing has quit a real Mac or
  logged off a real Windows session. These runs are still owed:
  - macOS: Dock Quit during an export, while a modal panel is open.
  - Windows: logoff during an export.
- **macOS IPv6 router read.** `llinfo::hardware` and the macOS `link.rs`
  branch were not compiled: there is no Apple SDK here. The parser runs on
  Linux, and the libc constants it uses are in libc 0.2.191 for macOS
  (`NET_RT_FLAGS`, `RTF_LLINFO`, `rt_msghdr`). After the empty-datagram
  resolve, macOS has no notification for a new NDP entry, so the new IPv6
  router is seen at the next configd event, not at once. On macOS IPv4,
  IPConfiguration writes `ARPResolvedHardwareAddress` when the link changes.

## Telegram Desktop behavior assumed

The tdesktop source is not in this environment, so these points come from
memory and were not checked:

- `core/sandbox.cpp` answers every local-socket client and has no client cap.
  The `busy` answer is Quill's own, for a cap tdesktop does not have.
- tdesktop builds a `GroupCall` for a peer it has resolved first. Only a
  conference has no peer. Making the instance without a chat follows Quill's
  conference path, not a tdesktop rule.
- tdesktop has no repeated ask for a quit. A quit it cannot accept while busy
  is cancelled, and the cancel after the bound matches that.

## What ran (Linux)

- `cargo fmt --check`: clean.
- `cargo clippy --locked --features demo-capture --all-targets -- -D warnings`
  and `cargo clippy --locked --no-default-features --all-targets -- -D warnings`:
  clean.
- `cargo test --features demo-capture --locked --lib -- single_instance
  net_watch export:: chat_state::export group_calls calls:: telegram::requests
  settings::deferred`: 1004 passed, 0 failed, 7 ignored.
- `cargo test --features demo-capture --locked --bin quill -- export_quit
  link_inbox sticker_set_box first_run closing_tests`: 66 passed, 0 failed.
- Each new test, run with its fix taken out: every one failed, as listed
  above. The over-cap test ran with the fix 5 times, all passing.
- `scripts/check-file-size.sh`: OK (limit 1000).
- `scripts/check-hotspots.sh origin/sweep-all`: OK. This branch touches no
  hotspot. Against the default `origin/main` base, the script flags
  `src/main.rs`, `src/ui/shell.rs` and `src/state/session_apply.rs`. Those
  changes come from sweep-all itself, not from this branch.
- `cargo check --target x86_64-pc-windows-gnu --features demo-capture
  --all-targets` (with MinGW installed): it compiles. It gives warnings only,
  none in a file this branch changed. `cargo clippy` for that target found
  nothing in this branch's files, after the test module was moved. Those
  warnings include the three Windows lints of review section 2 and the
  `layout.rs` `unused_mut`, which are out of this job's scope.
  `cargo check -p gpui-pre-windows` for that target: clean.

## Not run

- macOS: no build at all. `cargo check --target aarch64-apple-darwin` stops
  at `objc2-exception-helper`, which needs the Apple C toolchain. These were
  read but never compiled:
  - the macOS `answer_later` change;
  - `llinfo::hardware`;
  - the macOS `link.rs` branch.
- Windows: type-checked only, never run. In particular the
  `an_ipv6_address_added_on_loopback_wakes_the_watcher` test and the quit-later
  handler never ran.
- No real TDLib, no real device loss, no namespace tests, no full test suite.
