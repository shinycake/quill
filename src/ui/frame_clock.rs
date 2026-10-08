//! One redraw clock for everything that animates in the window: custom
//! emoji, animated stickers and inline video. GPUI's own animated images
//! redraw the whole window on every display refresh (120 Hz on ProMotion),
//! relaying out the full tree each time; this clock redraws only as fast
//! as the content needs and stops when nothing animated rendered.

use super::app::QuillApp;
use gpui_kit::*;
use std::time::Duration;

impl QuillApp {
    /// Ask for another frame within `1 / fps` seconds. Animated content
    /// calls this each time it renders; the clock stops after a tick in
    /// which nothing asked.
    #[track_caller]
    pub(super) fn request_animation_tick(&self, fps: u32, cx: &mut Context<Self>) {
        // `QUILL_TRACE_TICKS=1`: log who keeps the clock running (once a
        // second per call site), to hunt idle redraws.
        if trace_ticks() {
            trace_caller(std::panic::Location::caller(), fps);
        }
        self.animation_demand
            .set(self.animation_demand.get().max(fps.clamp(1, 60)));
        if self.frame_clock_running.get() {
            return;
        }
        self.frame_clock_running.set(true);
        cx.spawn(async move |this, cx| {
            let mut fps = 30;
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs_f64(1.0 / f64::from(fps)))
                    .await;
                let next = this
                    .update(cx, |this, cx| {
                        let demand = this.animation_demand.replace(0);
                        if demand > 0 {
                            cx.notify();
                        }
                        demand
                    })
                    .unwrap_or(0);
                if next == 0 {
                    break;
                }
                fps = next;
            }
            let _ = this.update(cx, |this, _| this.frame_clock_running.set(false));
        })
        .detach();
    }
}

/// `QUILL_ASSUME_ACTIVE=1`: animate as if the window had focus, so CPU can
/// be measured on a demo window launched from a terminal (which macOS does
/// not bring to the front).
pub(super) fn assume_active() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("QUILL_ASSUME_ACTIVE").is_some())
}

fn trace_ticks() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("QUILL_TRACE_TICKS").is_some())
}

fn trace_caller(caller: &'static std::panic::Location<'static>, fps: u32) {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::time::Instant;
    thread_local! {
        static LAST: RefCell<HashMap<(&'static str, u32), Instant>> = RefCell::new(HashMap::new());
    }
    LAST.with(|last| {
        let mut last = last.borrow_mut();
        let key = (caller.file(), caller.line());
        if last
            .get(&key)
            .is_none_or(|at| at.elapsed() >= Duration::from_secs(1))
        {
            last.insert(key, Instant::now());
            eprintln!("tick: {}:{} @{fps}fps", caller.file(), caller.line());
        }
    });
}
