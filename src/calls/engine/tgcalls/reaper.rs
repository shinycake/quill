//! Ending a call off the UI thread. `qc_p2p_destroy` and `qc_group_destroy`
//! wait for tgcalls with no time limit (docs/decisions/tgcalls-engine.md),
//! and the engine runs on the connect driver, which runs on the UI thread.
//! So the engine does only what must be true at once (no callback or feeder
//! reaches the call, its sound stops) and hands the rest here. One thread
//! runs teardowns in the order they were handed over.
//!
//! A process that exits during a teardown crashes in static destructors, so
//! quit waits for this thread ([`await_call_teardown`]) and, if it is stuck
//! past that, leaves through `_exit`.
use crate::locks::LockExt;
use crate::thread_guard::{ffi_boundary, spawn_guarded};
use std::sync::mpsc::{SendError, Sender, channel};
use std::sync::{Arc, Condvar, Mutex, Once, OnceLock, PoisonError};
use std::time::Instant;

type Job = Box<dyn FnOnce() + Send>;

#[derive(Default)]
struct Backlog {
    count: Mutex<usize>,
    empty: Condvar,
}

impl Backlog {
    fn add(&self) {
        *self.count.lock_or_recover() += 1;
    }

    fn finish(&self) {
        let mut count = self.count.lock_or_recover();
        *count = count.saturating_sub(1);
        if *count == 0 {
            self.empty.notify_all();
        }
    }

    fn is_empty(&self) -> bool {
        *self.count.lock_or_recover() == 0
    }

    fn wait_empty(&self, deadline: Instant) -> bool {
        let mut count = self.count.lock_or_recover();
        while *count > 0 {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            count = self
                .empty
                .wait_timeout(count, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        true
    }
}

struct Reaper {
    jobs: Sender<Job>,
    backlog: Arc<Backlog>,
}

impl Reaper {
    fn start() -> Self {
        let (jobs, queue) = channel::<Job>();
        let backlog = Arc::new(Backlog::default());
        let finished = Arc::clone(&backlog);
        let _ = spawn_guarded("quill-call-reaper", move || {
            for job in queue {
                ffi_boundary((), job);
                finished.finish();
            }
        });
        Self { jobs, backlog }
    }

    fn hand_off(&self, job: Job) {
        self.backlog.add();
        if let Err(SendError(no_thread_took_it)) = self.jobs.send(job) {
            ffi_boundary((), no_thread_took_it);
            self.backlog.finish();
        }
    }
}

static REAPER: OnceLock<Reaper> = OnceLock::new();
static EXIT_HOOK: Once = Once::new();

pub(super) fn hand_off(job: impl FnOnce() + Send + 'static) {
    REAPER.get_or_init(Reaper::start).hand_off(Box::new(job));
}

/// Quit: waits for teardowns in progress until `deadline`. One still running
/// at exit makes the process leave through `_exit`. Blocks at most until
/// `deadline`, and not at all if no call ever ended.
pub fn await_call_teardown(deadline: Instant) {
    let Some(reaper) = REAPER.get() else {
        return;
    };
    // Registered here, the last moment before exit: `atexit` hooks and
    // static destructors run in reverse order, so this one runs first.
    EXIT_HOOK.call_once(|| {
        // SAFETY: `leave_if_stuck` is a plain `extern "C" fn()` that catches
        // every panic. A failed registration leaves exit as before.
        let _ = unsafe { libc::atexit(leave_if_stuck) };
    });
    reaper.backlog.wait_empty(deadline);
}

/// The exit status of a process that left with a teardown still running.
/// `atexit` hooks never learn the status `exit` was given, so a quit that
/// meant to fail must not be reported as success, nor a stuck one as clean.
const STUCK_TEARDOWN_STATUS: i32 = 70;

/// Runs inside `exit`, so it never calls `std::thread::current()` or prints:
/// on macOS that ends the process silently once `exit` has torn down thread
/// locals.
extern "C" fn leave_if_stuck() {
    ffi_boundary((), || {
        if REAPER
            .get()
            .is_some_and(|reaper| !reaper.backlog.is_empty())
        {
            // SAFETY: ends the process at once. Registered last, this runs
            // first, so every exit hook registered before it is given up
            // with the static destructors the running teardown would crash
            // in: TDLib's and ffmpeg's log flushes among them, and stdio
            // buffers are not flushed.
            unsafe { libc::_exit(STUCK_TEARDOWN_STATUS) };
        }
    });
}

#[cfg(test)]
pub(super) fn finish_teardowns_before_test_exit() {
    if let Some(reaper) = REAPER.get() {
        let far = Instant::now() + std::time::Duration::from_secs(60);
        assert!(reaper.backlog.wait_empty(far), "a teardown never finished");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::thread::{self, ThreadId};

    #[test]
    fn a_stuck_teardown_never_holds_up_the_caller() {
        let reaper = Reaper::start();
        let (release, stuck) = channel::<()>();
        reaper.hand_off(Box::new(move || {
            let _ = stuck.recv();
        }));
        assert!(!reaper.backlog.wait_empty(Instant::now()));
        assert!(!reaper.backlog.is_empty(), "exit would use _exit");
        drop(release);
        let far = Instant::now() + std::time::Duration::from_secs(60);
        assert!(reaper.backlog.wait_empty(far));
        assert!(reaper.backlog.is_empty());
    }

    #[test]
    fn teardowns_run_in_order_on_one_other_thread() {
        let reaper = Reaper::start();
        let (done, finished) = channel::<(usize, ThreadId)>();
        for order in 0..3 {
            let done = done.clone();
            reaper.hand_off(Box::new(move || {
                let _ = done.send((order, thread::current().id()));
            }));
        }
        drop(done);
        let ran: Vec<(usize, ThreadId)> = finished.iter().collect();
        assert_eq!(
            ran.iter().map(|job| job.0).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert!(ran.iter().all(|job| job.1 == ran[0].1));
        assert_ne!(ran[0].1, thread::current().id());
    }

    #[test]
    fn a_quit_with_a_stuck_teardown_does_not_report_success() {
        if std::env::var_os("QUILL_TEST_STUCK_TEARDOWN_CHILD").is_some() {
            exit_with_a_stuck_teardown();
        }
        let exe = std::env::current_exe().expect("test executable");
        let status = std::process::Command::new(exe)
            .env("QUILL_TEST_STUCK_TEARDOWN_CHILD", "1")
            .args([
                "--exact",
                "calls::engine::tgcalls::reaper::tests::a_quit_with_a_stuck_teardown_does_not_report_success",
            ])
            .output()
            .expect("spawn child test")
            .status;
        assert_eq!(status.code(), Some(STUCK_TEARDOWN_STATUS));
    }

    #[expect(
        clippy::exit,
        reason = "the child reports through its exit status, as a quit does"
    )]
    fn exit_with_a_stuck_teardown() -> ! {
        let (_held, stuck) = channel::<()>();
        hand_off(move || {
            let _ = stuck.recv();
        });
        await_call_teardown(Instant::now());
        std::process::exit(0);
    }

    #[test]
    fn a_panicking_teardown_leaves_the_rest_running() {
        let reaper = Reaper::start();
        let (done, finished) = channel::<()>();
        reaper.hand_off(Box::new(|| panic!("teardown failed")));
        reaper.hand_off(Box::new(move || {
            let _ = done.send(());
        }));
        assert_eq!(finished.recv(), Ok(()));
        let far = Instant::now() + std::time::Duration::from_secs(60);
        assert!(reaper.backlog.wait_empty(far));
    }
}
