# update_install and media_tools tests: nothing shared with a child being started

## What was wrong
The flaky tests left over in `carrier-helper-test-flake.md` ("Found, not fixed") still failed on
main at 43f2ba439, under `--test-threads=8`, with no carrier test in the run. A temporary build
(not committed) printed what each failure hid. It showed three causes, two of them the same
sharing #1273 found. A child that another test thread starts holds a copy of every descriptor of
the test process from its fork until its exec. Close-on-exec closes them at the exec, not at the
fork, and Rust's `File` already sets it.

- **A lock that outlived its handle**: `claim.rs:343`, `:366` and `:396`, where a claim nobody
  holds still looked `Live`. The tests drop the handle that holds a claim's `flock` and expect the
  next `liveness` to find the claim free. A `flock` belongs to the open file, so it lasted until
  that child's exec. `liveness` had the same gap itself. When it takes the lock and answers `Live`
  (a young claim nobody holds), its lock also ended only when the last copy of the file closed.
  With the claim tests alone (no test starts a child), 0 of 200 runs failed.
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
  not cover a limit that runs out during the wait, because that needs a clock.
- The claim tests unlock a claim before they drop it (`let_go`), as
  `a_held_generation_is_not_swept` does.
- `claim::liveness` unlocks before it answers `Live`. Production behavior is the same, because the
  lock it took used to end when the file closed a line later. This was not a production bug. In
  Quill, the extra holder could only make a helper's `lock_shared` wait until a child's exec, or
  make another start see the claim as live for that long, which is the safe answer.

## How measured
All runs went through `slot-cargo`, with the test binary repeated by a cargo runner script:
`test --no-default-features --lib -- --test-threads=8 <filters>`.

| Filters | Runs | Before (43f2ba439) | After |
|---|---|---|---|
| `update_install::claim update_install::tests media_tools` | 60 | 5 failed (`claim.rs:366` ×3, `:343`, `tests.rs:318`) | 0 |
| Same, plus `proxy_carrier::helper child_reap` | 60 | 4 failed (`claim.rs:366` ×4) | 0 |
| Same, plus `proxy_carrier::helper child_reap` | 450 | — | 0 |

The temporary build of the old code, with the same load, failed 21 of 150 runs (claim tests and
`tests.rs:318`). With `update_install::tests media_tools proxy_carrier::helper child_reap`, it
failed 21 of 450 runs: 19 at `tests.rs:318`, 1 in the probe and 1 in the media tools self-test.
