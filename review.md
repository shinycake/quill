CHANGES

# Review of PR 1310 (`claude/sweep-b10`, bundle B10) against main

Reviewed head: `e95e696`. Read against `sweep-briefs/rules.md` and `sweep-briefs/bundles/B10.md` on `claude/sweep-briefs`, plus the PR text on `claude/prtext-b10`.

The PR breaks the single-instance guard (finding 1 below). A frozen Quill now lets a second copy start on the same profile. That alone blocks the merge. Two items stay open that the brief counts as defects: the ack timeout and Linux policy routing. N22 and N21 are only partly closed, and their tests do not prove the change they claim.

## Item by item

| Item | Verdict | Notes |
| --- | --- | --- |
| N19 | Fixed at root | `ticks.rs` restores `network_seen` on a send error. The test fails without the fix. |
| N20 | Partly fixed | The Linux namespace test is real. The Windows test only proves that registration worked (see 6). |
| N21 | Fixed in code, not proven | `NotifyIpInterfaceChange`, `NotifyUnicastIpAddressChange` and `NotifyRouteChange2` with `AF_UNSPEC` are correct. No test sees an IPv6 event (see 6). |
| N22 | Partly fixed | The Linux key reads only the main table and ignores the destination (see 3). Two networks that share address, gateway and interface still look the same (see 4). |
| N23 | Not a defect, accepted | The evidence in PR.md holds: `is_sweepable` does not list `SetParameters`, and `sweep_stale_requests` runs only when Ready or Updating. The scratch test was not committed (see 8). |
| L07 | Done, wrong layer | The detail lives only in the derived `auth_view`, not in state (see 5). |
| robustness-9 | Regressed | See 1, 2 and 7. |

## Findings

### 1. P0: a frozen owner lets a second Quill start on the same profile

`src/single_instance.rs` `serve_while`: when the UI does not beat within `UI_ANSWER` (2.5 s), `serve` returns and drops the connection. The launcher in `forward` is still inside its 3 s `recv_timeout`. Its `read_line` gets EOF and `ack.trim() == "ok"` is false, so it returns `Ok(Ok(false))`, which maps to `Forward::NoOwner` ("a dying owner counts as gone"). `acquire` then unlinks the live owner's socket (Unix) and binds its own listener. The result is `Acquired::Primary`, not `TimedOut`, so `Launch::OwnerStuck` and its dialog never happen. Two processes then open the same TDLib database.

The PR's own test `an_owner_whose_window_is_frozen_leaves_the_launch_unanswered` asserts that the ack is `""`, which is this EOF. It never runs the client side.

Proof. I ran two scratch tests (not pushed) on `e95e696` with `cargo test --no-default-features --lib`:
- `forward()` against `serve_while(.., UI_ANSWER)` with a started, silent heartbeat printed `forward after 2.501581362s: NoOwner`.
- `UI.start()`, then a first `acquire` and a second `acquire` on one endpoint printed `first: Primary`, then `second: Ok(Primary)`.

Fix: never close a stuck launch's connection early. Keep the stream open with no ack, so the launcher reaches its own timeout and gets `Unresponsive`. Better, send an explicit `stuck\n` line and map it to `Forward::Unresponsive` in `forward`. Then the result does not depend on how the two timers race. Add a test that drives `acquire` twice against a silent heartbeat and expects `ErrorKind::TimedOut`.

### 2. P1 (stays open, defect): a 3 s ack against tdesktop's 30 s main-thread check

`ACK_TIMEOUT` is 3 s and `UI_ANSWER` is 2.5 s. tdesktop allows 30 s (`sandbox.cpp:53`). A UI thread that is only busy for 2.5 s (a big chat list load, a slow disk, a debugger) reads as hung. Even with finding 1 fixed, the user then sees "not responding" while the link that `on_launch` already queued opens moments later in the copy that was called stuck. The PR names this as open. The brief says what stays open is a defect.

Fix: use a two-stage protocol so that delivery and liveness are separate.
- The owner writes `queued\n` as soon as `on_launch` returns. The launcher waits up to 3 s for it, as now.
- The owner then writes `ok\n` once the UI beats, or `stuck\n` after 30 s (a new `UI_ANSWER`, which matches tdesktop).
- The launcher waits up to `UI_ANSWER` plus a margin for the second line. Only `stuck\n` or that timeout gives `Launch::OwnerStuck`.
- The `HEADER` version goes to `quill-ipc 2`, and an old owner's single `ok\n` still counts as delivered.

Test with a fake heartbeat and an injected clock, not a sleep: a beat at 29 s acks, and no beat gives `stuck`.

### 3. P1 (stays open, defect): Linux ignores policy routing and the destination

`src/net_watch/link.rs` Linux `backend::of` ignores `_to`. It takes the lowest-metric default route of the interface from `/proc/net/route` and `/proc/net/ipv6_route`, which show only the main table. VPNs (`wg-quick` table 51820 with fwmark rules), NetworkManager's per-connection tables and split tunnels with a more specific route to 149.154.0.0/16 all give a gateway that is not the one Telegram traffic uses. A change in those tables never moves the key.

Fix: ask the kernel the same question `ip route get 149.154.167.51` asks. Send `RTM_GETROUTE` with `RTA_DST = to` (and `RTM_F_FIB_MATCH` unset) on a `NETLINK_ROUTE` socket, then read `RTA_OIF`, `RTA_GATEWAY` (or `RTA_VIA`) and `RTA_PREFSRC` from the reply. This goes through `ip rule` like a real packet. `platform.rs` already opens a `NETLINK_ROUTE` socket, so the code to parse the reply sits next to it. That also removes both `/proc` parsers. Test it in the existing namespace test: add a rule and a second table with a different gateway, and assert the hop follows the rule.

### 4. P2 (stays open, defect): same address, gateway and interface still read as no change

The PR text says two networks with the same local address, gateway IP and interface still look like one network. Two home routers on 192.168.0.1 are the case N22 was about, so N22 is not fully closed.

Fix: add the gateway's link-layer address to `Link`. On Linux, read it with `RTM_GETNEIGH` for the gateway on `RTA_OIF`. On Windows, use `ResolveIpNetEntry2` or `GetIpNetEntry2`. On macOS, use a `sysctl` with `NET_RT_FLAGS` and `RTF_LLINFO`. Two routers with the same IP have different MACs. If the neighbour entry is not resolved yet, read it as `None` and look again on the next event, not as a change.

### 5. P2: the L07 detail lives only in the view, not in state

`state/domains/auth/error.rs` writes `self.auth_view = with_detail(view_for(&self.auth), ..)` after `set_auth`. The detail is not part of `AuthorizationState` or the session. Any later `set_auth` with the same state, or a language change that rebuilds the view, drops it. This breaks the rule that the UI is a pure function of one source of truth.

Fix: carry the detail in the state, for example `StartupFailure::DataFolder { detail: Option<String> }` (or a `startup_detail` field next to `auth`). Have `view_for` render it and delete `with_detail`.

Also: `database_error_detail` cuts every second segment around `"`. A path that itself contains a quote, or a message with an odd number of quotes, leaks part of the path. Return `None` when the quote count is odd, and add that case to the test.

### 6. P2: the N20 and N21 tests do not prove what they claim

- Windows: `interface_address_and_route_notifications_register_for_both_ip_versions` waits for the three `MibInitialNotification` calls that `initial = true` gives. That proves that the three registrations succeeded, not that an IPv6 change wakes the watcher. N21 is about the IPv6 change. The wait also has no bound: a missing callback hangs the job until the 10 min step timeout.

  Fix: pass the row's family and the notification type to the listener. Register with `initial = false`. Add an IPv6 address to the loopback (`New-NetIPAddress -InterfaceAlias "Loopback Pseudo-Interface 1" -IPAddress fd00::1`, since the runner is admin), wait on a `Condvar` with `wait_timeout` for an `AF_INET6` `MibAddInstance`, and remove the address after.
- Linux: the namespace test drives `listen` with a closure, not `watch()`, so it never checks that `network_changed` moves the epoch. Make it call the real `network_changed` path and assert that `latest().epoch` moved.
- The IPv6 assertion is skipped when `/proc/sys/net/ipv6` is missing. Make that skip fail when `CI` is set, so a runner without IPv6 cannot quietly pass.

### 7. P2: the heartbeat can miss a beat when a poke lands in the loop body

`src/ui/link_inbox.rs` beats right after the wake, then runs the body, and then reads `seen = LINKS.epoch()` at the top of the next turn. Suppose `serve` takes its `mark` after that beat, its `on_launch` and `poke` land while the body runs, and the body's `take_activation` already handles the activation. Then `seen` includes the poke and the loop sleeps 60 s. No beat comes, and the launch is judged stuck. With finding 1 still in place, that starts a second instance. The window is short, but a plain re-launch (activation, no link) reaches it, since `has_pending_links` is false and the wait stays at 60 s.

Fix: read `seen` before `UI.beat()`, so a poke after the beat always ends the next sleep at once.

### 8. P3: smaller items

- N23: commit the scratch test from PR.md (a `SetParameters` an hour old, sweep due, connection Ready, still pending) as a regression test. "Keep `setTdlibParameters` out of the 60 s sweep" is what the brief asked for, and today nothing pins it.
- Windows `subscribe`: if the second or third registration fails, the drop cancels the ones that worked, and Quill then has no network watcher at all. Before, it had `NotifyAddrChange`. Keep the registrations that succeeded, and only log the failures.
- Windows: one network change now fires dozens of callbacks across three registrations. Each one runs `route_now`, which does `GetBestRoute2` and a UDP connect twice. That cost is small, but coalesce the calls through a flag that `network_changed` clears, or document why the callbacks are not batched.
- CI: `sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0 || true` hides a failed sysctl until the test fails with an `expect` message. Drop `|| true` and check for the key first, so the step fails where the cause is.

## What I ran

- The two scratch tests in finding 1, on `e95e696`, Linux container. They were not pushed.
- Read every file in the diff, plus the call sites of `net_watch::latest`, `set_auth`, `sweep_stale` and `forward`.

## What I did not run

- I did not build for macOS or Windows. Findings 3 to 6 and 8 come from reading the code.
- I did not run the new CI jobs.
