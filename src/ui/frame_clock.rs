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
    pub(super) fn request_animation_tick(&self, fps: u32, cx: &mut Context<Self>) {
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
