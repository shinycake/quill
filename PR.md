Quill notices more network changes, explains a data folder it cannot open, and says when a running copy is frozen

Branch: `claude/sweep-b10`, bundle B10 (startup-net-2). One commit per item.

## What was wrong

- N19: `network_tick` stored the new reach before it sent `setNetworkType`. When the send failed, the next poll saw the same reach and said nothing, so TDLib stayed on the old network until the next change.
- N20: The netlink watcher and the Windows watcher had no test, so a broken subscription could ship without anyone noticing.
- N21: On Windows, `NotifyAddrChange` reports IPv4 address changes only. A change on IPv6 never woke the watcher.
- N22: The route key was only the local address toward Telegram. A move between two networks that give out the same address (two home routers on 192.168.0.x) read as no change, so TDLib kept dead connections.
- L07 (owner default: same card, with TDLib's message as a detail line): any `setTdlibParameters` refusal other than 401 showed the data folder line alone. Nothing told a read-only folder apart from a file in the way or a full disk.
- robustness-9 (owner default: UI liveness through a heartbeat): the single-instance listener sent its ack from its own thread. With the window frozen, a second launch heard "ok" and exited with nothing on screen.

## What changed

- N19 `src/connect/ticks.rs`: a failed send puts the old reach back, so the next poll sends the change again.
- N21 `src/net_watch/platform.rs`: Windows registers `NotifyIpInterfaceChange`, `NotifyUnicastIpAddressChange` and `NotifyRouteChange2` for `AF_UNSPEC`, which covers interfaces, addresses and routes on both IP versions. `Cargo.toml` turns on the `Win32_NetworkManagement_Ndis` and `Win32_Networking_WinSock` windows-sys features those calls need. The lock file does not change.
- N20 `src/net_watch/watcher_tests.rs` and `.github/workflows/ci.yml`:
  - Linux: the test runs itself again in a new user and network namespace, brings `lo` up, and checks three things: the watcher loop wakes, IPv4 and IPv6 `RTM_NEWADDR` events arrive, and a namespace with no way out reads as offline. It is `#[ignore]` because it needs user namespaces. The Linux CI job gets a step that turns off Ubuntu's AppArmor userns restriction and runs it.
  - Windows: a new `net-watch-windows` job runs the `net_watch` tests on `windows-latest`. Its test registers all three notifications with an initial call and waits for one of each kind.
  - The Linux loop is split into `subscribe` and `listen` so that the test drives the code that runs in the app.
- N22 `src/net_watch/link.rs` (new) and `src/net_watch/mod.rs`: `Path::Via` now holds the local address, the interface index and the default gateway. Where they come from:
  - Linux: `getifaddrs` with `/proc/net/route` and `/proc/net/ipv6_route`.
  - macOS: `getifaddrs` with the router of the primary service from `State:/Network/Global/IPv4` or `IPv6`.
  - Windows: `GetBestRoute2`.
- L07 `src/telegram/envelope/auth.rs`, `src/auth.rs`, `src/state/domains/auth/error.rs`: `TdError` gets a `database_detail` field. It is kept only for messages about a database, binlog or file, and every quoted part (the paths) is cut out. Other free text is still dropped. The startup card shows the detail on a line under the reason. A wrong key (401) is unchanged.
- robustness-9 `src/single_instance/heartbeat.rs` (new), `src/single_instance.rs`, `src/ui/link_inbox.rs`: the link inbox loop on the UI thread beats each time it wakes. Before it acks, `serve` pokes `wake::LINKS` and waits up to 2.5 s for a beat. Without a beat it leaves the launch unanswered. The second launch then times out at 3 s and shows the "not responding" dialog that main already has (`Launch::OwnerStuck`, `ui::startup_failure::tell`). Before the window exists and after the inbox loop ends, launches get an answer at once, as before. `main.rs` does not change.
- One extra commit removes a trailing blank line that rustfmt rejected in `src/net_watch/tests.rs`.

## Not a defect: N23

The finding says `setTdlibParameters` is swept at 60 s and leaves the card on Connecting. On main, and at the audit commit 5072e9662, that cannot happen:
- `RequestPurpose::is_sweepable` (`src/state/requests.rs`) does not list `SetParameters`, and `is_swept_without_error` lists only the group call mute toggle.
- `Session::sweep_stale_requests` (`src/state/session_history_window.rs`) returns early unless the connection is Ready or Updating, which TDLib does not reach before its parameters are accepted.
- A scratch test, not committed, kicked off a driver and ran `poll_ticks` one hour later with the sweep due, once with the connection Connecting and once Ready. Both times `SetParameters` was still pending and auth stayed `WaitTdlibParameters`.
- The only ways `SetParameters` settles as lost (408) are closing the client and a failed re-send after a flood wait. The 408 guard in `auth/error.rs` exists on purpose for the first. That guard has a test, `a_parameters_request_lost_at_close_is_no_database_failure`.

Nothing was changed for N23.

## What ran (Linux cloud container, as root)

- `cargo fmt --check`, plus `cargo clippy -- -D warnings` for `--features demo-capture --all-targets` and `--no-default-features --all-targets`: clean.
- `cargo clippy --target x86_64-pc-windows-gnu --no-default-features --lib --tests` (mingw installed for it): nothing in the changed files. The warnings it prints elsewhere are already on main.
- `cargo test --no-default-features --lib`: 6840 passed, 15 ignored, 1 failed. The failure is `settings::accounts::...::atomic_write_failure_keeps_old_file`, the chmod 0555 test that fails as root.
- `cargo test --no-default-features --lib net_watch -- --ignored`: the namespace test passes here. With the address groups removed from the netlink subscription, it fails.
- `cargo test --features demo-capture --bin quill link_inbox`: 7 passed.
- New tests:
  - N19 `a_change_that_could_not_be_sent_is_sent_on_the_next_poll` fails without the fix.
  - N22 `a_new_network_that_gives_the_same_address_moves_the_epoch`, plus parser tests for both `/proc` route tables.
  - L07: `a_refused_database_replaces_connecting_with_the_reason` now expects the detail line with no path in it, and `only_storage_messages_reach_the_card_and_without_their_paths` checks that a message with a phone number gives no detail.
  - robustness-9: `an_owner_whose_window_is_frozen_leaves_the_launch_unanswered`, `an_owner_whose_window_answers_acknowledges_the_launch` and `an_owner_with_no_window_yet_or_any_more_acknowledges_at_once` go over a real local socket.
- The real route lookup on this machine gave `Via(Hop { local: 192.0.2.2, link: Link { interface: 4, gateway: Some(192.0.2.1) } })`, which matches `/proc/net/route`.
- `scripts/check-hotspots.sh`, `check-file-size.sh` and `check-lang.py`: OK.

## What did not run

- macOS: nothing was compiled. The macOS part is the `router` function in `src/net_watch/link.rs` (`SCDynamicStoreCopyValue`, about 30 lines, built the same way as `system_proxy::platform::detect`). The shared `getifaddrs` code compiles and runs on Linux. A Mac check needs `cargo clippy --features demo-capture --all-targets`.
- Windows: the code was type-checked and linted for `x86_64-pc-windows-gnu` but never run. The new `net-watch-windows` CI job is its first run, and the `GetBestRoute2` lookup has no test that runs here.
- The IPv6 assertion in the namespace test was skipped: this container's kernel has no IPv6 (`/proc/sys/net/ipv6` is missing). GitHub's runners have IPv6, so CI runs it.
- The new CI steps have not run yet: nothing here can start GitHub Actions. If the AppArmor sysctl is missing on the runner, the step goes on, and the test then fails with a message that names the sysctl.
- No screenshot of the startup card with the detail line or of the frozen-window dialog. The card puts the detail in the existing body text after a newline.
- No numbers to measure: none of the findings gives one.

## What stays open

- The 2.5 s heartbeat wait sits under the existing 3 s ack timeout. A UI thread that is busy, not hung, for longer than that during a second launch now gets the "not responding" dialog where it used to get a silent exit. tdesktop's own main-thread check allows 30 s. Matching that would mean a longer `ACK_TIMEOUT`, which this bundle left alone.
- On Linux the gateway comes from the main routing table in `/proc`, so policy routing (VPN rules in other tables) is not taken into account. Two networks with the same address, the same gateway and the same interface still read as one network.
