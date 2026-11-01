# Changes made by the Quill project

This directory is blocking 1.7.0 from crates.io (Apache-2.0 OR MIT), the
thread pool behind `blocking::unblock` that zbus, smol, async-fs and oo7 use
on Linux, used through `[patch.crates-io]` in Quill's `Cargo.toml`. Commit
`8dcaa9a` added the unmodified registry copy (without `.cargo-ok`,
`.cargo_vcs_info.json` and `Cargo.toml.orig`), so
`git diff 8dcaa9a -- third_party/blocking` shows every change. Changed code is
marked `QUILL`.

## Changes

`src/lib.rs` (sweep bundle B13, N49): a pool thread with nothing to run waits
500 ms for a task and then is meant to stop. 1.7.0 joined that test and the
"more threads than the limit" test with `&&`, so a thread in a pool under its
limit (500 by default) never stopped: after the first `unblock` the
`blocking-1` thread woke twice a second for the life of the process. The two
reasons are joined with `||` again (`should_stop`), as the comments next to
them say and as 1.6 behaved for the timeout. The pool starts a thread again
on the next `unblock`.

Measured with a program that calls `unblock` once and reads
`voluntary_ctxt_switches` from `/proc/self/task/*/status`: before, `blocking-1`
switched 19 times in 10 s and stayed; after, the thread is gone 1 s after its
task. Test: `an_idle_thread_under_the_limit_stops_after_the_timeout`, run with
`scripts/test-blocking-pool.sh` (the crate's own tests pass too).

Measured in Quill too: the `ready-chats` demo window idle under Xvfb with
Mesa lavapipe and a private session bus (zbus connected), counted with
`scripts/ci-idle-wakeups.py` over 60 s and `/proc/<pid>/task` read every
second. With 1.7.0's `&&`: one `blocking-1` thread, which woke 120 times
(2 a second), all threads 139 wakeups. With `||`: no `blocking-*` thread at
any sample, so none was started in the 60 s, all threads 19 wakeups. An idle
Quill calls `unblock` too rarely for the restarted thread to cost anything.
Not measured: an active session, where TDLib and the network keep zbus and
async-fs busier.
