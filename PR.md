Quill notices more network changes, explains a data folder it cannot open, and says when a running copy is frozen

Branch: `claude/sweep-b10`, bundle B10 (startup-net-2). One commit per item, then one commit per finding of the review on `claude/review-b10`.

## What was wrong

- N19: `network_tick` stored the new reach before it sent `setNetworkType`. When the send failed, the next poll saw the same reach and said nothing, so TDLib stayed on the old network until the next change.
- N20: The netlink watcher and the Windows watcher had no test, so a broken subscription could ship without anyone noticing.
- N21: On Windows, `NotifyAddrChange` reports IPv4 address changes only. A change on IPv6 never woke the watcher.
- N22: The route key was only the local address toward Telegram. A move between two networks that give out the same address (two home routers on 192.168.0.x) read as no change, so TDLib kept dead connections.
- L07 (owner default: same card, with TDLib's message as a detail line): any `setTdlibParameters` refusal other than 401 showed the data folder line alone. Nothing told a read-only folder apart from a file in the way or a full disk.
- robustness-9 (owner default: UI liveness through a heartbeat): the single-instance listener sent its ack from its own thread. With the window frozen, a second launch heard "ok" and exited with nothing on screen.

The review of the first version found these on top:

- Review 1: when the UI did not beat, the owner closed the launch's connection without an answer. The launcher read that as a dead owner, removed the live socket and started a second Quill on the same TDLib database.
- Review 2: the owner waited 2.5 s for its UI under a 3 s ack. A UI that was only busy read as hung, while the link it had already queued opened moments later.
- Review 3: on Linux the gateway came from the main table in `/proc`, so VPN tables, per-connection tables and split tunnels were not taken into account.
- Review 4: two networks with the same address, gateway and interface still read as one.
- Review 5: the L07 detail lived only in the derived view, so rebuilding the view dropped it. A quote inside a path let part of the path through.
- Review 6: the Windows test proved only that the registrations worked, and the Linux test never checked that the watcher moves the epoch.
- Review 7: a launch whose poke landed while the link inbox ran its turn was folded into the next sleep's epoch, so no beat came for 60 s.
- Review 8: no test pinned N23; one failed Windows registration dropped the other two; every Windows callback ran a route lookup; the CI sysctl hid its own failure.

## What changed

- N19 `src/connect/ticks.rs`: a failed send puts the old reach back, so the next poll sends the change again.
- N21 `src/net_watch/platform.rs`: Windows registers `NotifyIpInterfaceChange`, `NotifyUnicastIpAddressChange` and `NotifyRouteChange2` for `AF_UNSPEC`, which covers interfaces, addresses and routes on both IP versions. A refused registration is logged, and the others stay. Callbacks go through a coalescer (`src/net_watch/coalesce.rs`): a callback that lands while a lookup runs makes that run look once more, and never starts a second lookup beside it.
- N22 `src/net_watch/link.rs`, `src/net_watch/netlink.rs` (new) and `src/net_watch/mod.rs`: `Path::Via` holds the local address, the interface, the gateway and the gateway's link-layer address.
  - Linux: `RTM_GETROUTE` toward Telegram's address, which goes through the policy rules as a real packet does, then `RTM_GETNEIGH` for the gateway. The `/proc` parsers are gone. The watcher also hears neighbour news, but wakes only for a gateway's entry.
  - Windows: `GetBestRoute2`, then `GetIpNetEntry2`. Windows has no notification for its neighbour cache, so a missing entry is resolved with `ResolveIpNetEntry2` on a thread of its own, and the network is looked at again after.
  - macOS: the router of the primary service, and `ARPResolvedHardwareAddress` from that service's IPv4 key, which configd writes. The watcher now hears service IPv4 keys.
  - A link-layer address that is not resolved yet, or that the cache dropped, keeps the one seen before, so it is no change. A different one is.
- L07 `src/telegram/envelope/auth.rs`, `src/auth.rs`, `src/state/domains/auth/error.rs`: `TdError` gets a `database_detail` field, kept only for messages about a database, binlog or file. Everything from the first quote to the last (the path) is cut out, and a message with an odd number of quotes gives no detail. `StartupFailure::DataFolder { detail }` carries it in state, and `view_for` puts it on a line under the reason. A wrong key (401) is unchanged.
- robustness-9 `src/single_instance.rs`, `src/single_instance/heartbeat.rs` (new), `src/ui/link_inbox.rs`:
  - The link inbox loop on the UI thread beats each time it wakes. It reads the epoch for its next sleep before the beat, so a launch that pokes while the loop works always wakes it again.
  - The protocol is `quill-ipc 2`. The owner answers `queued` once it holds the launch, then `ok` when the UI beats, or `stuck` after 30 s, tdesktop's main-thread check (`sandbox.cpp:53`). The launcher waits 3 s for `queued`, then for the verdict. Only `stuck`, or no verdict in 33 s, gives the "not responding" dialog (`Launch::OwnerStuck`). The owner never closes a launch unanswered.
  - Once it hears `queued`, a launch drops the election lock, so later launches do not wait behind it (the Windows lock gives up after 10 s). An owner that queues and then closes is retried once, then called stuck.
  - An owner from before this change drops the new header, so the launcher tries `quill-ipc 1` and takes its single `ok`. A `quill-ipc 1` launch gets a single `ok`.
  - Before the window exists and after the inbox loop ends, launches get `ok` at once. `main.rs` does not change.
- N20 tests and `.github/workflows/ci.yml`: see "What ran". The Linux CI step writes the AppArmor sysctl only when the kernel has the key, and a failed write fails the step. A new `net-watch-windows` job runs the `net_watch` tests on `windows-latest`.
- Docs: `docs/decisions/codex-deep-links-single-instance.md` and `docs/decisions/agent-access.md` describe the new protocol.

## Not a defect: N23

The finding says `setTdlibParameters` is swept at 60 s and leaves the card on Connecting. On main, and at the audit commit 5072e9662, that cannot happen:
- `RequestPurpose::is_sweepable` (`src/state/requests.rs`) does not list `SetParameters`, and `is_swept_without_error` lists only the group call mute toggle.
- `Session::sweep_stale_requests` (`src/state/session_history_window.rs`) returns early unless the connection is Ready or Updating, which TDLib does not reach before its parameters are accepted.
- The only ways `SetParameters` settles as lost (408) are closing the client and a failed re-send after a flood wait. The 408 guard in `auth/error.rs` exists on purpose for the first, and `a_parameters_request_lost_at_close_is_no_database_failure` tests it.

The test `the_request_sweep_leaves_the_parameters_request_waiting` now pins this. It runs the ticks an hour after kickoff with the sweep due, once Connecting and once Ready, and checks that the request is still pending and the card still says Connecting. It fails when `SetParameters` is made sweepable. No code changed for N23.

## What ran (Linux cloud container, as root)

- `cargo fmt --check`, plus `cargo clippy -- -D warnings` for `--features demo-capture --all-targets` and `--no-default-features --all-targets`: clean.
- `cargo clippy --target x86_64-pc-windows-gnu --no-default-features --lib --tests` (mingw installed for it): nothing in the changed files. The warnings it prints elsewhere are already on main.
- `cargo test --no-default-features --lib`: 6851 passed, 17 ignored, 1 failed. The failure is `settings::accounts::...::atomic_write_failure_keeps_old_file`, the chmod 0555 test that fails as root.
- `cargo test --no-default-features --lib net_watch -- --ignored`: both namespace tests pass here.
- `cargo test --features demo-capture --bin quill link_inbox` (8 passed) and `connect_ui` (5 passed).
- `scripts/check-hotspots.sh`, `check-file-size.sh` and `check-lang.py`: OK.
- Each new test was checked against the code without its fix:
  - N19 `a_change_that_could_not_be_sent_is_sent_on_the_next_poll` fails without the fix.
  - Review 1 `a_launch_against_a_frozen_owner_is_told_so_and_does_not_take_over` drives `acquire` twice against a silent heartbeat. Without the `stuck` answer it gets `Ok(Primary)`, the second owner the review found.
  - Review 2: `an_owner_whose_window_answers_at_29_seconds_acknowledges_the_launch` and `an_owner_whose_window_is_silent_for_30_seconds_says_it_is_stuck` move a fake clock by hand. Neither sleeps. `a_launch_reaches_an_older_quill_that_answers_ok_alone`, `a_launch_from_an_older_quill_gets_its_single_ok` and `an_owner_that_keeps_closing_after_it_queued_is_called_stuck` cover the rest of the protocol.
  - Review 7 `a_launch_that_pokes_while_the_inbox_works_ends_the_next_sleep` drives the inbox loop with a turn that pokes mid-way. With the old order it sees 1 turn, not 2.
  - Review 3: in a fresh user and network namespace, a bridge gets a main-table default via 192.0.2.1, table 100 via 192.0.2.9, and a rule for 149.154.0.0/16. The hop is 192.0.2.9. The old `/proc` lookup gave 192.0.2.1.
  - Review 4: the same namespace test adds a neighbour entry for the gateway and checks the hop picks it up. `a_router_on_the_same_address_with_another_link_address_moves_the_epoch` checks that an unresolved entry is no change and a different one is.
  - Review 5: `a_refused_database_replaces_connecting_with_the_reason` derives the view again from state and expects the same detail. `only_storage_messages_reach_the_card_and_without_their_paths` covers quotes inside a path and an odd quote count.
  - Review 6: the namespace test runs the watcher's own handler on the queued events and checks that the tracker's epoch moved. With the handler made a no-op, it fails.
  - Review 8: `a_refused_notification_keeps_the_ones_that_registered`, `calls_during_a_run_make_one_more_run_not_one_each` and `a_run_that_panics_does_not_stop_later_ones`.

## What did not run

- macOS: nothing was compiled. The macOS code is `backend::of` in `src/net_watch/link.rs` (`SCDynamicStoreCopyValue` for the global and service IPv4 keys, about 40 lines) and one more notification pattern in `src/net_watch/platform.rs`. A Mac check needs `cargo clippy --features demo-capture --all-targets` and `cargo test --lib net_watch`.
- Windows: the code was type-checked and linted for `x86_64-pc-windows-gnu`, never run. The new test `an_ipv6_address_added_on_loopback_wakes_the_watcher` adds `fd00::5175:1` to the loopback interface (index 1), which needs an administrator, and waits for the `AF_INET6` address notification. The CI step's 10 minute limit ends the wait if nothing comes. `GetIpNetEntry2` and `ResolveIpNetEntry2` have no test that runs here.
- The IPv6 assertion in the namespace test was skipped: this container's kernel has no IPv6. On CI the skip now fails the test, since `CI` is set there.
- The new CI steps have not run yet: nothing here can start GitHub Actions.
- No screenshot of the startup card with the detail line or of the frozen-window dialog. The card puts the detail in the existing body text after a newline.
- Numbers: none of the findings gives one to measure. How many Windows callbacks one network change fires, before and after the coalescer, could not be measured without Windows.

## What stays open

Nothing from the bundle or the review. Two limits of the platforms remain, and each falls back to the behavior before this change:
- macOS records the router's link-layer address for IPv4 only. On a network with IPv6 alone, two routers with the same address, gateway and interface still read as one.
- A gateway whose link-layer address is not resolved at the moment of a change reads as the same network until it is resolved. Linux hears that from its neighbour events, Windows resolves it in the background, and macOS hears the service key change when configd records it.
