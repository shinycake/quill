//! Developer frame profiling (cargo feature `frame-profiler`, off by default).
//!
//! With `QUILL_FRAME_PROFILE=<path>` set, the first frame turns on GPUI's
//! profiler tracing and its detailed debug frame overlay, and every two
//! seconds the window's frame histograms (draw time, dirty-to-present,
//! present interval while animating) are written to `<path>` as percentile
//! lines. Pair it with Instruments' Time Profiler on a release build with
//! line tables to see which code a slow frame spent its time in.

use super::app::QuillApp;
use gpui_kit::*;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

struct State {
    path: String,
    last_write: Instant,
    started: bool,
}

static STATE: OnceLock<std::sync::Mutex<State>> = OnceLock::new();

fn state() -> Option<&'static std::sync::Mutex<State>> {
    STATE
        .get_or_init(|| {
            std::sync::Mutex::new(State {
                path: std::env::var("QUILL_FRAME_PROFILE").unwrap_or_default(),
                last_write: Instant::now(),
                started: false,
            })
        })
        .into()
}

// The histograms are `hdrhistogram::Histogram`, which Quill doesn't name;
// a macro reads them without adding the crate.
macro_rules! line {
    ($name:expr, $h:expr) => {{
        let h = &$h;
        let ms = |q: f64| h.value_at_quantile(q) as f64 / 1_000_000.0;
        format!(
            "{:<18} n={:<6} p50={:>6.2}ms p90={:>6.2}ms p99={:>6.2}ms max={:>7.2}ms\n",
            $name,
            h.len(),
            ms(0.50),
            ms(0.90),
            ms(0.99),
            h.max() as f64 / 1_000_000.0
        )
    }};
}

impl QuillApp {
    /// Called once per frame from `frame_upkeep`.
    pub(super) fn frame_profile_tick(&mut self, window: &mut Window) {
        let Some(lock) = state() else { return };
        let Ok(mut st) = lock.lock() else { return };
        if st.path.is_empty() {
            return;
        }
        if !st.started {
            st.started = true;
            profiler::set_trace_enabled(true);
            window.set_debug_frame_overlay_mode(DebugFrameOverlayMode::Full);
        }
        if st.last_write.elapsed() < Duration::from_secs(2) {
            return;
        }
        st.last_write = Instant::now();
        let frames = window.frame_duration_snapshot();
        let input = window.input_latency_snapshot();
        let report = [
            line!("draw", frames.draw_duration_histogram),
            line!("dirty_to_present", frames.dirty_to_present_histogram),
            line!("present_interval", frames.present_interval_histogram),
            line!("input_latency", input.latency_histogram),
        ]
        .concat();
        let _ = std::fs::write(&st.path, report);
    }
}
