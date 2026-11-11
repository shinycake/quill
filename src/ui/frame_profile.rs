//! Developer frame profiling (cargo feature `frame-profiler`, off by default).
//!
//! With `QUILL_FRAME_PROFILE=<path>` set, the first frame turns on GPUI's
//! profiler tracing and its detailed debug frame overlay, and every two
//! seconds the window's frame histograms (draw time, dirty-to-present,
//! present interval while animating) are written to `<path>` as percentile
//! lines. Pair it with Instruments' Time Profiler on a release build with
//! line tables to see which code a slow frame spent its time in.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::*;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// The profiler's one stage. `Off` without `QUILL_FRAME_PROFILE`; `Armed`
/// until the first frame turns tracing on; then `Running`, writing a report
/// every two seconds. The process ends it.
enum Profiler {
    Off,
    Armed { path: String },
    Running { path: String, last_write: Instant },
}

static STATE: OnceLock<std::sync::Mutex<Profiler>> = OnceLock::new();

fn state() -> &'static std::sync::Mutex<Profiler> {
    STATE.get_or_init(|| {
        std::sync::Mutex::new(match std::env::var("QUILL_FRAME_PROFILE") {
            Ok(path) if !path.is_empty() => Profiler::Armed { path },
            _ => Profiler::Off,
        })
    })
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
        let Ok(mut stage) = state().lock() else {
            return;
        };
        if let Profiler::Armed { path } = &mut *stage {
            let path = std::mem::take(path);
            profiler::set_trace_enabled(true);
            window.set_debug_frame_overlay_mode(DebugFrameOverlayMode::Full);
            *stage = Profiler::Running {
                path,
                last_write: Instant::now(),
            };
        }
        let Profiler::Running { path, last_write } = &mut *stage else {
            return;
        };
        if last_write.elapsed() < Duration::from_secs(2) {
            return;
        }
        *last_write = Instant::now();
        let frames = window.frame_duration_snapshot();
        let input = window.input_latency_snapshot();
        let report = [
            line!("draw", frames.draw_duration_histogram),
            line!("dirty_to_present", frames.dirty_to_present_histogram),
            line!("present_interval", frames.present_interval_histogram),
            line!("input_latency", input.latency_histogram),
        ]
        .concat();
        let _ = std::fs::write(&*path, report);
    }
}
