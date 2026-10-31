# update_install and media_tools tests: nothing shared with a child being started

## What was wrong
The flaky tests left over in `carrier-helper-test-flake.md` ("Found, fixed later") still failed
on main at 43f2ba439, under `--test-threads=8`, with no carrier test in the run. A temporary
build (not committed) printed what each failure hid. It showed three causes, two of them the
same sharing #1273 found. A child that another test thread starts holds a copy of every
descriptor of the test process from its fork until its exec. Close-on-exec closes them at the
exec, not at the fork, and Rust's `File` already sets it. All line numbers here are at
43f2ba439.

- **A lock that outlived its handle**: `claim.rs:343`, `:366` and `:396`, where a claim nobody
  holds still looked `Live`. The tests drop the handle that holds a claim's `flock` and expect the
  next `liveness` to find the claim free. A `flock` belongs to the open file, so it lasted until
  that child's exec. Every other `Claim` drop had the same gap: the tests' other drops,
  `drop_held`, and the stale claims `any_live` drops when it finds a live one. So did
  `liveness` itself. When it takes the lock and answers `Live` (a young claim nobody holds), its
  lock also ended only when the last copy of the file closed. With the claim tests alone (no test
  starts a child), 0 of 200 runs failed.
- **`ETXTBSY`**: `media_tools.rs:565`, plus the probe at `tests.rs:37` and `tests.rs:370` (#1273's
  runs; neither printed its error here). The tests wrote a script and then started it. A child
  forked during the write kept the write descriptor, and a file that is open for writing cannot
  be started. The temporary build printed
  `Os { code: 26, kind: ExecutableFileBusy, message: "Text file busy" }` for the media tools
  self-test.
- **A file read before it was written**: `tests.rs:318`, all 19 times `left: Some("")`. The
  fixture app records its arguments with `echo "$@" > started`. The test polled for that file
  every 20 ms and sometimes read it after the shell had created it but before it wrote.

A temporary stress test (not committed) ran each case 1000 times while 3 threads started
`/bin/true` in a loop. Each count is from two runs:

| Case | Bad outcomes in 1000 |
|---|---|
| Script written with `fs::write`, then started | 27 and 34 `ETXTBSY` |
| Same script copied by `cp`, then started | 0 and 0 |
| `flock` taken, handle dropped, lock tried again | 8 and 8 still held |
| `flock` taken, unlocked, then dropped | 0 and 0 |
| `liveness` answers `Live`, then is asked an hour later | 6 `Live` before the change, 0 after |

## What changed
- The media tools self-test starts checked-in stand-ins (`tests/fixtures/media-tools`).
- The update fixture apps are checked in (`tests/fixtures/update-install`). The probe test starts
  them where they are. A package needs a regular file inside it (the manifest refuses links), so
  `cp` copies the app there. The write descriptor lives in `cp`, which starts nothing, and closes
  when `cp` exits.
- The tests' starter (`start_directly`) waits for the app to exit, so `started` is complete by the
  time the helper returns. `started()` reads it once instead of polling for up to 2 s with 20 ms
  sleeps.
- The probe test no longer measures elapsed time. It used to wait 200 ms on `sleep 30` and assert
  that this took under 5 s. Now, with no time left, a child that cannot exit by itself (`cat` on
  an input the test holds open) is killed (`SIGKILL`) and reported as `TimedOut`. This test does
  not cover a limit that runs out during the wait, because that needs a clock. If
  `wait_with_timeout` stopped enforcing its deadline, the test would hang instead of failing. The
  CI step's 30-minute timeout bounds that, the same trade-off #1273 accepted.
- `Claim` unlocks its file when dropped (`impl Drop`), so every drop lets go at once, as
  `a_held_generation_is_not_swept` does by hand. The tests are unchanged. `release` deletes the
  claim before the drop unlocks it, so a start still never sees a claim unlocked that is about to
  go. `HELD` is a static and is never dropped. `File::unlock` is std on every platform, and a
  failed unlock (no locks on the file system) is ignored.
- `child_reap::kill_and_reap_returns_at_once_and_ends_the_child` asserted under 1 s and then started
  a `kill -0` child every 25 ms. It is now `kill_and_reap_kills_the_child_before_it_returns`: the
  child is a shell waiting for a line, and after the call the test closes the shell's input and
  reads its output to the end. The output is empty if the call killed the shell and `survived` if
  it did not, and the read ends either way. The "returns at once" check is gone because only a
  clock can show it: a killed child can be reaped within microseconds, so a version that waited
  for the reap would pass any other check. The reap is not checked either. `kill -0` succeeds on a
  zombie, and a `waitpid` from the test would compete with the reap thread for the child. Checked
  by breaking the code: with no kill at all, 60 of 60 runs failed with `survived` and none hung.
  Without `kill_and_reap`'s own kill, where the reap thread still kills a moment later, 1 of 60
  failed. On the final code, 0 of 60 runs failed alone and 0 of 60 with the tests above for load.
- `claim::liveness` unlocks before it answers `Live`. Production behavior is the same for this
  and for the `Drop`, because each lock used to end when its file closed, a moment later. This
  was not a production bug. In Quill, the extra holder could only make a helper's `lock_shared`
  wait until a child's exec, or make another start see the claim as live for that long, which is
  the safe answer.
- Each fixture app the tests start says it must exit on its own, because `start_directly` waits
  for it.

## How measured
All runs went through `slot-cargo`, with the test binary repeated by a cargo runner script:
`test --no-default-features --lib -- --test-threads=8 <filters>`. The `update_install` filter
covers the claim, helper, staged, swap and manifest tests.

| Filters | Runs | Before (43f2ba439) | After |
|---|---|---|---|
| `update_install media_tools` | 60 | 2 failed | 0 |
| Same, plus `proxy_carrier::helper child_reap` | 60 | 7 failed | 0 |
| Same, plus `proxy_carrier::helper child_reap` | 450 | — | 0 |

Before, alone: `claim.rs:366` and `tests.rs:318`. Before, loaded: `tests.rs:318` five times,
`claim.rs:343` twice and `claim.rs:396` once (one run failed twice). An earlier round, with
`update_install::claim update_install::tests media_tools` and the claim tests unlocking by hand
before `Drop` existed, went from 5 of 60 alone and 4 of 60 loaded to 0, 0 and 0 of 450.

The temporary build of the old code, with the same load, failed 21 of 150 runs (claim tests and
`tests.rs:318`). With `update_install::tests media_tools proxy_carrier::helper child_reap`, it
failed 21 of 450 runs: 19 at `tests.rs:318`, 1 in the probe and 1 in the media tools self-test.

## Left for later
- The touched tests still go through production time limits that poll with sleeps: `runs` waits
  5 s, polling every 10 ms (`media_tools.rs:27`, `:156-161`); `probe_binary` waits 5 s, polling
  every 50 ms (`wait_with_timeout`, `update_install.rs:280-296`); and `wait_for_parent` waits 30 s
  (`update_install.rs:739`). The tests pass a parent that has already exited, so
  `wait_for_parent` does not wait unless that pid is reused. A badly saturated runner could
  still trip the 5 s limits. Making these limits parameters, as #1273 did with `shutdown_wait`,
  is a follow-up.
- `media_tools::tests::graceful_stop_interrupts_the_child_without_a_kill_binary`
  (`media_tools.rs:444`) starts `sleep 30` and relies on `stop_capture`'s 8 s deadline, which
  polls every 25 ms (`media_tools.rs:426`). It is not touched here.
