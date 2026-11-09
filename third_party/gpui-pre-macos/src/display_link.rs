// Modified by the Quill project (2026) from gpui-pre-macos 0.3.11 (Apache-2.0):
// a window's link runs only while frames are wanted, and the latest present time per
// display is kept for video pacing. See third_party/gpui-pre-macos/QUILL-CHANGES.md.
//! Frame pacing for macOS windows.
//!
//! Each window is paced by a `CAMetalDisplayLink` on its layer, which runs on
//! the main run loop and hands out the drawable of each frame. Quill runs the
//! link only while the display should pace frames (`link_pace`). A lone draw
//! comes from the frame waker through this module's dispatch source, with the
//! link stopped, and takes its drawable from the layer.

use anyhow::{Context as _, Result};
use core_graphics::display::CGDirectDisplayID;
use dispatch2::{
    _dispatch_source_type_data_add, DispatchObject, DispatchQueue, DispatchRetained, DispatchSource,
};
use gpui_apple::display_link::MetalFrameSource;
use objc2::MainThreadMarker;
use objc2_quartz_core::{CACurrentMediaTime, CAMetalDisplayLinkUpdate, CAMetalLayer};
use scheduler::Instant;
use std::{
    collections::BTreeMap,
    ffi::c_void,
    sync::{Mutex, PoisonError},
    time::Duration,
};

/// The latest present time per display, as an `Instant` and in host ticks,
/// and the display's refresh period.
type Timing = ((Instant, u64), Option<Duration>);

static TIMINGS: Mutex<BTreeMap<CGDirectDisplayID, Timing>> = Mutex::new(BTreeMap::new());

/// Quill: when the latest frame paced on `display_id` is shown, as an
/// `Instant` and in Mach host ticks, and the display's refresh period, read
/// under one lock so they belong together (`None` before a link first ran
/// there). Video players pull the picture due then; draw spacing can't tell
/// the refresh period apart from a player that skips frames on purpose.
pub fn present_timing(
    display_id: CGDirectDisplayID,
) -> (Option<(Instant, u64)>, Option<Duration>) {
    TIMINGS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&display_id)
        .map_or((None, None), |(present, period)| (Some(*present), *period))
}

/// Quill: record the present time of a link update on `display_id`.
pub(crate) fn note_present(
    display_id: CGDirectDisplayID,
    update: &CAMetalDisplayLinkUpdate,
    period: Option<Duration>,
) {
    // The vsync the frame must be ready for, one refresh ahead: what the
    // clock and players time a frame against. The presentation time adds
    // the compositor's latency, which no draw can shorten.
    let target = update.targetTimestamp();
    let ahead = target - CACurrentMediaTime();
    let Some(present) = present_instant(Instant::now(), ahead) else {
        return;
    };
    let frequency = objc2_core_video::CVGetHostClockFrequency();
    let ticks = (target * frequency) as u64;
    TIMINGS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(display_id, ((present, ticks), period));
}

fn present_instant(now: Instant, ahead_secs: f64) -> Option<Instant> {
    let offset = Duration::try_from_secs_f64(ahead_secs.abs()).ok()?;
    if ahead_secs >= 0.0 {
        now.checked_add(offset)
    } else {
        now.checked_sub(offset)
    }
}

/// A window's frame requests: its display link while one runs, and a
/// dispatch source that asks for a step on the main queue from any thread.
pub struct WindowFrameSource {
    frame_requests: DispatchRetained<DispatchSource>,
    link: Option<MetalFrameSource>,
}

impl WindowFrameSource {
    pub fn new(data: *mut c_void, callback: extern "C" fn(*mut c_void)) -> Self {
        let frame_requests = unsafe {
            let frame_requests = DispatchSource::new(
                &raw const _dispatch_source_type_data_add as *mut _,
                0,
                0,
                Some(DispatchQueue::main()),
            );
            frame_requests.set_context(data);
            frame_requests.set_event_handler_f(callback);
            // Resume before this source can ever be dropped: destroying a
            // suspended dispatch source is undefined behavior (#50875).
            frame_requests.resume();
            frame_requests
        };
        Self {
            frame_requests,
            link: None,
        }
    }

    pub fn start(
        &mut self,
        layer: &CAMetalLayer,
        main_thread: MainThreadMarker,
        callback: impl Fn(&CAMetalDisplayLinkUpdate) + 'static,
    ) -> Result<()> {
        self.stop();
        self.link = Some(
            MetalFrameSource::new(layer, main_thread, callback)
                .context("CAMetalDisplayLink is unavailable")?,
        );
        Ok(())
    }

    pub fn stop(&mut self) {
        self.link = None;
    }

    /// Quill: whether the display is pacing this window's frames.
    pub fn is_running(&self) -> bool {
        self.link.is_some()
    }

    /// Quill: the dispatch source that delivers this window's steps on the
    /// main queue; `merge_data(1)` on it requests one step now, from any
    /// thread.
    pub fn requests(&self) -> DispatchRetained<DispatchSource> {
        self.frame_requests.clone()
    }
}

impl Drop for WindowFrameSource {
    fn drop(&mut self) {
        self.stop();
        // Cancelling guarantees the event handler never runs again; its
        // context points at the window's native view, which may be
        // deallocated after this.
        self.frame_requests.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2::AnyThread;
    use objc2_quartz_core::CAMetalDisplayLink;

    #[test]
    fn a_renderer_draws_normally_after_its_display_link_is_invalidated() {
        if !MetalFrameSource::is_available() {
            return;
        }
        let mut renderer =
            crate::renderer::MetalRenderer::new(Default::default(), false, false).unwrap();
        renderer.update_drawable_size(gpui::size(gpui::DevicePixels(64), gpui::DevicePixels(64)));
        let layer = renderer.layer().expect("window renderer has a layer");
        // The link is never added to a run loop: attaching it to the layer is
        // the state under test.
        let link = CAMetalDisplayLink::initWithMetalLayer(CAMetalDisplayLink::alloc(), layer);
        renderer.set_display_link_attached(true);
        renderer.draw(&gpui::Scene::default());

        link.invalidate();
        renderer.set_display_link_attached(false);
        renderer.draw(&gpui::Scene::default());
    }

    #[test]
    fn present_times_can_fall_on_either_side_of_now() {
        let now = Instant::now();
        let ms = Duration::from_millis(8);
        assert_eq!(present_instant(now, 0.008), now.checked_add(ms));
        assert_eq!(present_instant(now, -0.008), now.checked_sub(ms));
        assert_eq!(present_instant(now, f64::NAN), None);
    }
}
