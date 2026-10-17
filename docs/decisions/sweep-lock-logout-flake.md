# Flaky "test scheduler is not deterministic" in background_accounts tests

## Symptom

`the_lock_screen_log_out_wipes_only_after_every_client_closed` (and other UI
tests) failed under load with "Detected activity on thread A, but test
scheduler is running on thread B. Your test is not deterministic."

## Root cause

`quill::wake` holds process-wide statics (`UPDATES`, `PASSCODE`, ...). Every
test app registers its tasks' wakers on them. A window-activation observer
(`app_demo.rs`) calls `poke_on_activation`, which woke the wakers of every
app in the process, including those of tests running concurrently on other
threads. Waking a GPUI foreground task schedules it on that test's scheduler
from the wrong thread, which the scheduler records as non-determinism and
raises when the owning test ends. It only shows when the activation of one test
overlaps another test's registered waiter, hence "under load".

## Fix

`wake::isolate_current_thread()` marks a thread as a test thread. Wakers
registered from a marked thread remember it, and a poke from a marked thread
wakes only wakers of its own thread. `QuillApp::new_with_demo` calls it under
`#[cfg(test)]`. The app never calls it: pokes from the TDLib thread, tray and
the single-instance listener still wake every waiter, so production
behavior is identical. Unmarked pokers (helper threads) still wake everything.

The other timing-sensitive tests share this cause when they fail with the
same message; they use the same apps and signals.
