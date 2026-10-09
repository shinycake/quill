// Modified by the Quill project (2026) from gpui-pre-windows 0.3.8 (Apache-2.0):
// adds `FrameGate`, which lets the vsync thread sleep while every window is idle.
// See third_party/gpui-pre-windows/QUILL-CHANGES.md.
use std::{
    sync::LazyLock,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use gpui::FrameRequestSource;
use gpui_util::ResultExt;
use parking_lot::{Condvar, Mutex, RwLock};
use smallvec::SmallVec;

use windows::Win32::{
    Foundation::HWND,
    Graphics::Dwm::{DWM_TIMING_INFO, DwmFlush, DwmGetCompositionTimingInfo},
    System::Performance::QueryPerformanceFrequency,
};

static QPC_TICKS_PER_SECOND: LazyLock<u64> = LazyLock::new(|| {
    let mut frequency = 0;
    // On systems that run Windows XP or later, the function will always succeed and
    // will thus never return zero.
    unsafe { QueryPerformanceFrequency(&mut frequency).unwrap() };
    frequency as u64
});

const VSYNC_INTERVAL_THRESHOLD: Duration = Duration::from_millis(1);
const DEFAULT_VSYNC_INTERVAL: Duration = Duration::from_micros(16_666); // ~60Hz

pub(crate) struct VSyncProvider {
    interval: Duration,
    f: Box<dyn Fn() -> bool>,
}

impl VSyncProvider {
    pub(crate) fn new() -> Self {
        let interval = get_dwm_interval()
            .context("Failed to get DWM interval")
            .log_err()
            .unwrap_or(DEFAULT_VSYNC_INTERVAL);
        let f = Box::new(|| unsafe { DwmFlush().is_ok() });
        Self { interval, f }
    }

    pub(crate) fn wait_for_vsync(&self) -> FrameRequestSource {
        let vsync_start = Instant::now();
        let wait_succeeded = (self.f)();
        let elapsed = vsync_start.elapsed();
        // DwmFlush and DCompositionWaitForCompositorClock returns very early
        // instead of waiting until vblank when the monitor goes to sleep or is
        // unplugged (nothing to present due to desktop occlusion). We use 1ms as
        // a threshold for the duration of the wait functions and fallback to
        // Sleep() if it returns before that. This could happen during normal
        // operation for the first call after the vsync thread becomes non-idle,
        // but it shouldn't happen often.
        if !wait_succeeded || elapsed < VSYNC_INTERVAL_THRESHOLD {
            log::trace!("VSyncProvider::wait_for_vsync() took less time than expected");
            std::thread::sleep(self.interval);
            FrameRequestSource::LocalSchedule
        } else {
            FrameRequestSource::NativeCallback
        }
    }
}

fn get_dwm_interval() -> Result<Duration> {
    let mut timing_info = DWM_TIMING_INFO {
        cbSize: std::mem::size_of::<DWM_TIMING_INFO>() as u32,
        ..Default::default()
    };
    unsafe { DwmGetCompositionTimingInfo(HWND::default(), &mut timing_info) }?;
    let interval = retrieve_duration(timing_info.qpcRefreshPeriod, *QPC_TICKS_PER_SECOND);
    // Check for interval values that are impossibly low. A 29 microsecond
    // interval was seen (from a qpcRefreshPeriod of 60).
    if interval < VSYNC_INTERVAL_THRESHOLD {
        Ok(retrieve_duration(
            timing_info.rateRefresh.uiDenominator as u64,
            timing_info.rateRefresh.uiNumerator as u64,
        ))
    } else {
        Ok(interval)
    }
}

/// Quill: which windows' vsync invalidations are parked (see `frame_idle`).
///
/// Upstream's vsync thread invalidates every window on every vblank, so each
/// visible window gets a `WM_PAINT` and a GPUI frame request 60–144 times a
/// second even when nothing changes. A window that stays idle parks here; the
/// vsync thread skips parked windows (except for a heartbeat invalidation
/// every `PARKED_HEARTBEAT`) and sleeps on the condition variable while
/// every window is parked. The window's frame waker unparks it.
///
/// Lock order: the vsync thread takes `parked`, then the window list's read
/// lock. The UI thread never takes `parked` while holding the window list.
pub(crate) struct FrameGate {
    parked: Mutex<Vec<isize>>,
    unparked: Condvar,
}

pub(crate) static FRAME_GATE: FrameGate = FrameGate {
    parked: parking_lot::const_mutex(Vec::new()),
    unparked: Condvar::new(),
};

fn hwnd_key(hwnd: HWND) -> isize {
    hwnd.0 as isize
}

impl FrameGate {
    /// The window's frames are idle: stop invalidating it every vblank.
    pub(crate) fn park(&self, hwnd: HWND) {
        let key = hwnd_key(hwnd);
        let mut parked = self.parked.lock();
        if !parked.contains(&key) {
            parked.push(key);
        }
    }

    /// The window wants frames again (or is new, in case a closed window
    /// with the same handle was parked): invalidate it every vblank, and
    /// wake the vsync thread if it sleeps.
    pub(crate) fn unpark(&self, hwnd: HWND) {
        let key = hwnd_key(hwnd);
        self.parked.lock().retain(|parked| *parked != key);
        self.unparked.notify_all();
    }

    /// The window is gone.
    pub(crate) fn forget(&self, hwnd: HWND) {
        let key = hwnd_key(hwnd);
        self.parked.lock().retain(|parked| *parked != key);
    }

    /// Blocks the vsync thread while every window is parked, until a window
    /// unparks or `heartbeat_at`. Returns `false` once the window list is
    /// gone (the platform was dropped). `raw` gives each list entry's handle.
    pub(crate) fn wait_for_frames<W>(
        &self,
        all_windows: &std::sync::Weak<RwLock<SmallVec<[W; 4]>>>,
        raw: impl Fn(&W) -> HWND,
        heartbeat_at: Instant,
    ) -> bool {
        let mut parked = self.parked.lock();
        loop {
            let Some(windows) = all_windows.upgrade() else {
                return false;
            };
            let any_awake = windows
                .read()
                .iter()
                .any(|window| !parked.contains(&hwnd_key(raw(window))));
            drop(windows);
            let now = Instant::now();
            if any_awake || now >= heartbeat_at {
                return true;
            }
            self.unparked.wait_for(&mut parked, heartbeat_at - now);
        }
    }

    /// The windows the vsync thread skips on this vblank. Taken before the
    /// window list's read lock, so the vsync thread never holds both.
    pub(crate) fn parked_windows(&self) -> ParkedWindows {
        ParkedWindows(self.parked.lock().iter().copied().collect())
    }
}

/// Quill: a snapshot of [`FrameGate`]'s parked windows.
#[derive(Default)]
pub(crate) struct ParkedWindows(SmallVec<[isize; 4]>);

impl ParkedWindows {
    pub(crate) fn contains(&self, hwnd: HWND) -> bool {
        self.0.contains(&hwnd_key(hwnd))
    }
}

#[inline]
fn retrieve_duration(counts: u64, ticks_per_second: u64) -> Duration {
    let ticks_per_microsecond = ticks_per_second / 1_000_000;
    Duration::from_micros(counts / ticks_per_microsecond)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_compositor_wait_reports_native_callback() {
        let provider = VSyncProvider {
            interval: Duration::ZERO,
            f: Box::new(|| {
                std::thread::sleep(VSYNC_INTERVAL_THRESHOLD * 2);
                true
            }),
        };

        assert_eq!(
            provider.wait_for_vsync(),
            FrameRequestSource::NativeCallback
        );
    }

    #[test]
    fn failed_compositor_wait_reports_local_schedule_even_after_threshold() {
        let provider = VSyncProvider {
            interval: Duration::ZERO,
            f: Box::new(|| {
                std::thread::sleep(VSYNC_INTERVAL_THRESHOLD * 2);
                false
            }),
        };

        assert_eq!(provider.wait_for_vsync(), FrameRequestSource::LocalSchedule);
    }
}
