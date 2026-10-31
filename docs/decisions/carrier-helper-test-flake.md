# Carrier host tests: checked-in stand-ins, no waits on the clock

## What was wrong
CI (`linux-fmt-clippy-test`, core tests) failed on carrier host tests three times in two days,
each at an open: `left: Err(Terminated)`, `right: Ok(())`.
`a_closed_helper_that_exits_ends_the_wait` (run 41082203667),
`a_closed_carrier_reports_no_failure` (41165787964) and
`a_missing_web_view_library_turns_hosting_off` (41179402096). The host maps any spawn error to
`Terminated` (`src/proxy_carrier/helper.rs`, `open`), so the helper never started.

The tests wrote each stand-in script at run time (`std::fs::write`, then `chmod 755`) and the
host started it right after. Every test runs on a thread of the same process, and others start
children at the same time. A child forked while the script's write descriptor is open gets a
copy of it and keeps it until its own `exec` closes it (close-on-exec). Starting a file that
something still has open for writing fails with `ETXTBSY` ("Text file busy").

Evidence:
- A temporary stress test (not committed) wrote a script and started it 1000 times, through
  `spawn` and through `HelperCarrierHost::open`, while 3 threads started `/bin/true` in a loop.
  Two runs: 9 and 8 spawns failed with `Os { code: 26, kind: ExecutableFileBusy }`, and 23 and 6
  opens returned `Err(Terminated)`. The same test on the checked-in script: 0 and 0.
- The unchanged test binary, filters `proxy_carrier::helper::tests update_install media_tools
  child_reap`, `--test-threads=8`, two loops of 60 runs side by side: 11 of 120 runs failed in a
  carrier test, every one `open == Err(Terminated)` right after its script was written (6
  different tests).

This is not a production bug: the real helper is installed with Quill and never written while
Quill runs, and a failed start is already `Terminated`, which TDLib retries. No retry was added.

The same inheritance broke `data_folder::tests::a_held_generation_is_not_swept`: it drops the
file holding a generation's lock and expects the next sweep to remove the generation. A `flock`
belongs to the open file, which a child being started on another thread shares until its exec,
so the sweep still found it held. With the carrier tests in the same run
(`proxy_carrier::helper`, 60 runs) it failed 10 times before this change and 13 after the
stand-in change alone; with the data folder tests alone, 0 of 200.

## What changed
- The stand-ins are checked-in scripts with their executable bit, in
  `tests/fixtures/carrier-helper` (`stays-up.sh`, `missing-library.sh`, `says-unsupported.sh`,
  `posts.sh`, `stuck.sh`). No test writes a file it then starts, so no descriptor can be open
  for writing.
- Waits are on the host's own events: the next failure is a blocking `recv` on the host's
  failure channel (was a 10 s poll with 20 ms sleeps); the page message is a blocking `recv`
  on the relay (was `recv_timeout(10 s)`), with the host's copy of the relay dropped so only
  the reader holds it. The failure wait drops the host's own sender too, so if no report
  comes it ends, and the test fails, once the carrier's threads have ended: the reader at
  the helper's exit, the writer at its first failed write or at the latest at its 60 s start
  watchdog. Checked by making `Shared::fail` send nothing: the three tests that wait for a
  failure panicked with "carrier threads ended without reporting a failure" after 60 s
  (their writers had sent the open, so the start watchdog ended them) instead of hanging. The relay wait
  ends at the latest at the writer's 15 s silence watchdog, which kills a helper that said
  `ready` and nothing since.
- The close tests no longer sleep 100 ms "to let it start" or poll `exiting()` for 2 s. They
  close, then collect every failure until the carrier's threads end (the host's sender is
  dropped, so the channel closes with the last thread's). That is exact: removing the
  `closing` flag from `close` and `stop` makes both close tests fail on every run with
  `[(5, Terminated)]`.
- `posts.sh` waits for the close instead of `sleep 5`, so its shutdown no longer sits out the
  3.5 s ceiling.
- `the_stand_in_helper_stays_up_until_closed` is gone. It checked that a fixture had not exited
  300 ms in, which no wait can prove; `stays-up.sh` is checked in and exits only on the close
  command or the end of its input, which the host sends only when it closes.
- `a_held_generation_is_not_swept` unlocks the file before dropping it. An unlock releases the
  lock of the open file, whoever else shares it.
- The shutdown tests no longer measure elapsed time against the real 3.5 s ceiling (they
  asserted under 200 ms, under 1 s, and between the ceiling and the ceiling + 500 ms). The
  host takes its ceiling from a `shutdown_wait` field, always `SHUTDOWN_WAIT` outside tests,
  and its private `stop` (what `shutdown` runs) returns what the wait came to: `NotAtAll` (no
  helper was exiting), `UntilAllGone` or `GaveUp`. `close` likewise takes the grace it gives
  a helper before killing it from a `close_grace` field, always `CLOSE_GRACE`.
  - Nothing running: `NotAtAll`, decided before any wait, so this test shows there was
    nothing to wait for, not that no wait happened.
  - A helper that ignores the close, with a ceiling of zero: `GaveUp`. With `stop` waiting
    without a ceiling the test fails with `UntilAllGone`. It covers only a ceiling already
    reached: `stop` gives up before it waits at all, and `stays-up.sh` would give the same.
    A ceiling that runs out during the wait is not covered; that needs a clock.
  - A closed helper that leaves on the close, with a ceiling and a close grace of a day:
    `UntilAllGone`. Neither can end the wait, so it ended because the helper left. The old
    version of that test killed the helper first, which left `close` nothing to wait for,
    the same case as nothing running.
- The late teardown no longer checks that the new host's claim and sweep finished inside the
  old host's ceiling. The old host has a ceiling of a day and a helper that ignores the close,
  so its teardown cannot end until the test kills that helper (`libc::kill` on its pid, not a
  `kill` child process), after the new host's sweep. The sweep may run before the teardown starts or while it waits; either way the old
  folder is still held.
- `stuck.sh` loops on `kill -0 $PPID` with 1 s sleeps instead of `exec sleep 30`, so it never
  leaves by itself while the test process lives (a 30 s sleep would end a slow run's
  teardown), and leaves within a second of the test process if a test fails before killing
  it.
- `without_a_folder_of_its_own_no_carrier_opens` checked the 10 s claim backoff by opening
  again at once. It now checks the backoff ends at least 10 s after the failed claim, and
  that an open before the backoff ends is refused, with the backoff set a day ahead.
- Production behavior is the same: the fields hold the constants, and `stop` computes the
  outcome next to the loop it already ran.

A test that hangs now fails the CI step: "tests (core + replay)" has `timeout-minutes: 30`
(it takes about 3 minutes on a warm cache) instead of GitHub's 6-hour default for the job.

One bounded wait in production code remains, and
`a_missing_web_view_library_turns_hosting_off` relies on it: a reader that sees the
helper's output end waits up to 500 ms (`EXIT_WAIT`) for the exit status, to tell a missing
library (127) from a crash. The output ends only when the helper's own descriptors close on
exit, just before it becomes reapable, so on Linux the gap is microseconds; a 500 ms stall
there would turn the expected `Unsupported` into `Terminated`.

## How measured
All through `slot-cargo` on the final code, the binary run repeatedly by a cargo runner
script (`--config target.<triple>.runner`):
- `test --no-default-features --lib proxy_carrier::helper -- --test-threads=8`: 60 runs, all
  passed (24 tests each, 0.03 s per run; the suite took 3.5 s before). Before the review
  fixes (`next_failure` closing its channel, `close_grace`), 100 runs, all passed.
- The same with `update_install media_tools child_reap` in each run for load: 60 runs, no
  carrier test failed, twice (before and after the review fixes). Each time 8 runs failed in
  those other tests (`update_install` claim and `tests.rs:318`, `media_tools.rs:565`; see
  below).
- Before the shutdown rewrite, the stand-in change alone: 12 runs of the filter through
  `slot-cargo`, and 120 runs of a copy of that binary with the same load filters, had no
  carrier host failure.

## Found, fixed later
`update_install` has its own flaky tests under parallel load, with no carrier test in the run:
the pre-change binary, `update_install::claim update_install::tests`, `--test-threads=8`, failed
32 of 80 runs (`claim.rs:343`, `:366`, `:396`, `tests.rs:318`, `:370`). Its fixtures are also
written and then started (`src/update_install/tests.rs` `script`), as is
`media_tools::tests::a_tool_that_exits_with_an_error_or_hangs_fails_the_self_test`, which
failed at `assert!(runs(&script("good", "exit 0")))` in 2 of 60 loaded runs here. Fixed in
`fix-script-test-flakes.md`: the same sharing, of a claim's lock and of written scripts, plus a
fixture file read before it was written.
