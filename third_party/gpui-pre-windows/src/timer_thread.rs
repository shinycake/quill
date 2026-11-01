// Added by the Quill project (2026). See third_party/gpui-pre-windows/QUILL-CHANGES.md.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use gpui::RunnableVariant;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_FAILED};
use windows::Win32::Media::{timeBeginPeriod, timeEndPeriod};
use windows::Win32::System::Threading::{
    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, CreateEventW, CreateWaitableTimerExW, INFINITE,
    SetEvent, SetWaitableTimer, TIMER_ALL_ACCESS, WaitForMultipleObjects, WaitForSingleObject,
};
use windows::core::PCWSTR;

use crate::dispatcher::WindowsDispatcher;
use crate::timer_queue::{TimerQueue, Wait};

/// Without a high-resolution timer, waits shorter than this hold the system
/// timer at 1 ms; longer (idle) waits leave it alone.
const SHORT_WAIT: Duration = Duration::from_millis(100);

struct Handle(HANDLE);

// SAFETY: an event or waitable timer handle names a kernel object that any
// thread of the process may signal, set or wait on.
unsafe impl Send for Handle {}
// SAFETY: as above; the kernel serializes operations on the object.
unsafe impl Sync for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful Create* call and is
        // closed only here, once.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

struct State {
    timers: TimerQueue<RunnableVariant>,
    stopped: bool,
}

struct Shared {
    state: Mutex<State>,
    wake: Handle,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn wake(&self) {
        // SAFETY: `wake` is a live auto-reset event owned by `self`.
        unsafe {
            let _ = SetEvent(self.wake.0);
        }
    }
}

/// One thread that fires every `dispatch_after` timer from a deadline heap,
/// sleeping on a high-resolution waitable timer instead of the 15.6 ms
/// system tick that thread-pool timers are rounded to.
pub(crate) struct TimerThread {
    shared: Arc<Shared>,
}

impl TimerThread {
    pub(crate) fn start() -> Option<Self> {
        // SAFETY: no security attributes and no name; the result is checked.
        let wake = Handle(unsafe { CreateEventW(None, false, false, PCWSTR::null()) }.ok()?);
        let (timer, high_resolution) = waitable_timer()?;
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                timers: TimerQueue::new(),
                stopped: false,
            }),
            wake,
        });
        let thread_shared = shared.clone();
        std::thread::Builder::new()
            .name("Timer".to_owned())
            .spawn(move || run(&thread_shared, &timer, high_resolution))
            .ok()?;
        Some(Self { shared })
    }

    pub(crate) fn schedule(&self, duration: Duration, runnable: RunnableVariant) {
        let Some(deadline) = Instant::now().checked_add(duration) else {
            // Never due. Dropping it would cancel the task and make the next
            // poll of its awaiter panic.
            std::mem::forget(runnable);
            return;
        };
        let earliest = self.shared.lock().timers.push(deadline, runnable);
        if earliest {
            self.shared.wake();
        }
    }
}

impl Drop for TimerThread {
    fn drop(&mut self) {
        self.shared.lock().stopped = true;
        self.shared.wake();
    }
}

fn waitable_timer() -> Option<(Handle, bool)> {
    let create = |flags| {
        // SAFETY: no security attributes and no name; the result is checked.
        unsafe { CreateWaitableTimerExW(None, PCWSTR::null(), flags, TIMER_ALL_ACCESS.0) }
            .ok()
            .map(Handle)
    };
    // The flag exists from Windows 10 1803; older systems reject it.
    if let Some(timer) = create(CREATE_WAITABLE_TIMER_HIGH_RESOLUTION) {
        return Some((timer, true));
    }
    create(0).map(|timer| (timer, false))
}

fn run(shared: &Shared, timer: &Handle, high_resolution: bool) {
    loop {
        let (due, wait) = {
            let mut state = shared.lock();
            // A pending runnable holds the dispatcher that owns `TimerThread`,
            // so nothing is queued by the time it drops.
            if state.stopped {
                return;
            }
            let now = Instant::now();
            (state.timers.pop_due(now), state.timers.wait(now))
        };
        if !due.is_empty() {
            due.into_iter().for_each(WindowsDispatcher::execute_runnable);
            continue;
        }
        match wait {
            Wait::Forever => {
                // SAFETY: `wake` is a live event owned by `shared`.
                let result = unsafe { WaitForSingleObject(shared.wake.0, INFINITE) };
                if result == WAIT_FAILED {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            Wait::For(duration) => {
                let _resolution =
                    (!high_resolution && duration < SHORT_WAIT).then(SystemResolution::hold);
                wait_for(shared, timer, duration);
            }
        }
    }
}

fn wait_for(shared: &Shared, timer: &Handle, duration: Duration) {
    // A negative due time is relative, in 100 ns units.
    let ticks = i64::try_from(duration.as_nanos() / 100).unwrap_or(i64::MAX);
    let due = -ticks.max(1);
    // SAFETY: `timer` is a live waitable timer and `due` outlives the call;
    // no completion routine is passed.
    let armed = unsafe { SetWaitableTimer(timer.0, &due, 0, None, None, false) }.is_ok();
    let result = if armed {
        // SAFETY: both handles are live for the whole wait.
        unsafe { WaitForMultipleObjects(&[shared.wake.0, timer.0], false, INFINITE) }
    } else {
        let millis = u32::try_from(duration.as_millis()).unwrap_or(INFINITE - 1);
        // SAFETY: `wake` is a live event owned by `shared`.
        unsafe { WaitForSingleObject(shared.wake.0, millis.max(1)) }
    };
    if result == WAIT_FAILED {
        std::thread::sleep(duration.min(Duration::from_millis(1)));
    }
}

/// Holds the system timer at 1 ms until dropped.
struct SystemResolution;

impl SystemResolution {
    fn hold() -> Self {
        // SAFETY: always safe; paired with `timeEndPeriod` in `drop`.
        unsafe {
            timeBeginPeriod(1);
        }
        Self
    }
}

impl Drop for SystemResolution {
    fn drop(&mut self) {
        // SAFETY: matches the `timeBeginPeriod(1)` in `hold`.
        unsafe {
            timeEndPeriod(1);
        }
    }
}
