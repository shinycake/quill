//! The media viewer's HDR state for the current picture file: what the
//! background decode found, the frame it made, and whether the window is
//! in extended-range mode for it.

use super::HdrPixels;
use crate::ui::QuillApp;
use gpui_kit::*;
use quill::lang::{keys, tr};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Long edges are rounded up to this step, so a window resize re-decodes
/// only when it crosses one.
const EDGE_STEP: u32 = 512;

/// How often the display headroom is re-read while an HDR frame shows (it
/// changes with brightness, power and which screen the window is on).
const HEADROOM_POLL: Duration = Duration::from_secs(1);

/// The faster polls right after an HDR frame appears, while the screen
/// raises its headroom for it.
const RAMP_POLL: Duration = Duration::from_millis(100);
const RAMP_TICKS: u32 = 10;

/// What the viewer toolbar says about the picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HdrBadge {
    /// Shown with its HDR highlights.
    Shown,
    /// The file has HDR data, but this display (or this view of it) shows
    /// the standard-range picture.
    StandardRange,
}

#[derive(Default)]
enum Slot {
    #[default]
    Idle,
    Decoding,
    /// No HDR data in the file.
    Sdr,
    /// HDR data that this display (or platform) does not show.
    StandardRange,
    Hdr(HdrPixels),
}

/// What the background job found for one key.
enum Outcome {
    Sdr,
    StandardRange,
    Hdr(HdrPixels),
}

#[derive(Default)]
pub(crate) struct HdrViewer {
    /// Path, rounded long edge and "the screen can show HDR" of the
    /// current (or last) decode.
    key: Option<(PathBuf, u32, bool)>,
    /// Bumped by `reset` and every new decode; a result for an older token
    /// is dropped.
    token: u64,
    slot: Slot,
    /// The window's layer is in extended-range mode.
    edr_on: bool,
    /// The renderer could not draw in extended range and left it, for good.
    edr_lost: bool,
    /// The headroom last attached to the frame.
    headroom: f32,
    polling: bool,
    /// A decode is running for `key`.
    in_flight: bool,
}

impl HdrViewer {
    /// Forget the current picture (viewer step, close, item deleted).
    pub(crate) fn reset(&mut self) {
        self.key = None;
        self.in_flight = false;
        self.token = self.token.wrapping_add(1);
        self.slot = Slot::Idle;
    }

    pub(crate) fn badge(&self) -> Option<HdrBadge> {
        match self.slot {
            Slot::Hdr(_) if self.edr_lost => Some(HdrBadge::StandardRange),
            Slot::Hdr(_) => Some(HdrBadge::Shown),
            Slot::StandardRange => Some(HdrBadge::StandardRange),
            Slot::Idle | Slot::Decoding | Slot::Sdr => None,
        }
    }

    /// Whether a larger decode would add detail: the frame is as big as
    /// was asked (`edge`), not cut down to the picture's own size or the
    /// pixel cap.
    fn can_grow(&self, edge: u32) -> bool {
        match &self.slot {
            Slot::Hdr(pixels) => pixels.size.0.max(pixels.size.1) >= edge,
            _ => false,
        }
    }

    fn has_frame(&self) -> bool {
        matches!(self.slot, Slot::Hdr(_))
    }

    /// Tests: the headroom the shown frame is tone-mapped into.
    #[cfg(test)]
    #[cfg(all(target_os = "macos", feature = "demo-capture"))]
    pub(crate) fn frame_headroom(&self) -> Option<f32> {
        match &self.slot {
            Slot::Hdr(pixels) => Some(pixels.frame.headroom()),
            _ => None,
        }
    }

    /// Recorded for tests: whether the window was last switched to EDR.
    #[cfg(test)]
    #[cfg(all(target_os = "macos", feature = "demo-capture"))]
    pub(crate) fn edr_on(&self) -> bool {
        self.edr_on
    }
}

/// `long_edge` rounded up to `EDGE_STEP`.
fn bucket(long_edge: f32) -> u32 {
    let edge = long_edge.max(1.0).ceil() as u32;
    edge.div_ceil(EDGE_STEP).saturating_mul(EDGE_STEP)
}

fn run(path: &Path, long_edge: u32, can_show: bool) -> Outcome {
    if !super::probe(path) {
        return Outcome::Sdr;
    }
    if !can_show {
        return Outcome::StandardRange;
    }
    match super::decode(path, long_edge) {
        Ok(Some(pixels)) => Outcome::Hdr(pixels),
        // Nothing above SDR white after all, or a failed decode: the SDR
        // picture is the whole story, so no tag.
        Ok(None) | Err(_) => Outcome::Sdr,
    }
}

impl QuillApp {
    /// The current picture file drawn with its HDR highlights, once the
    /// background decode has made a frame; `None` draws the SDR picture.
    /// `fit` is the media box in logical pixels.
    pub(crate) fn viewer_hdr_element(
        &mut self,
        path: &Path,
        fit: (f32, f32),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let headroom = super::headroom(window);
        let long_edge = bucket(fit.0.max(fit.1) * window.scale_factor());
        let can_show = headroom.can_show_hdr();
        let hdr = &mut self.viewer.hdr;
        let stale = match hdr.key.as_ref() {
            None => true,
            Some((old, _, show)) if old != path || *show != can_show => true,
            // A larger decode waits for the one in flight, so a zoom
            // gesture runs one decode at a time.
            Some((_, edge, _)) => !hdr.in_flight && *edge < long_edge && hdr.can_grow(*edge),
        };
        if stale {
            self.start_hdr_decode(path, long_edge, can_show, cx);
        }
        let hdr = &mut self.viewer.hdr;
        let Slot::Hdr(pixels) = &hdr.slot else {
            return None;
        };
        // Back in 8-bit, the frame is tone-mapped into standard range
        // instead of clipping its highlights.
        let display = if hdr.edr_lost {
            super::Headroom::SDR.effective()
        } else {
            headroom.effective()
        };
        if hdr.headroom != display {
            let param = super::tone_map::shader_headroom(display, pixels.peak);
            pixels.frame.set_headroom(param);
            hdr.headroom = display;
        }
        let element = frame_element(&pixels.frame, fit);
        self.poll_viewer_headroom(window, cx);
        element
    }

    /// The toolbar badge for the current picture. A rotated or flipped
    /// picture draws the SDR image, so it reads as standard range.
    pub(crate) fn viewer_hdr_badge(&self) -> Option<HdrBadge> {
        match self.viewer.hdr.badge()? {
            HdrBadge::Shown if !self.viewer.orientation.is_identity() => {
                Some(HdrBadge::StandardRange)
            }
            badge => Some(badge),
        }
    }

    fn start_hdr_decode(
        &mut self,
        path: &Path,
        long_edge: u32,
        can_show: bool,
        cx: &mut Context<Self>,
    ) {
        let hdr = &mut self.viewer.hdr;
        hdr.token = hdr.token.wrapping_add(1);
        hdr.key = Some((path.to_path_buf(), long_edge, can_show));
        hdr.in_flight = true;
        // A bigger re-decode keeps showing the current frame meanwhile.
        if !hdr.has_frame() {
            hdr.slot = Slot::Decoding;
        }
        let token = hdr.token;
        let path = path.to_path_buf();
        let job = cx.background_spawn(async move { run(&path, long_edge, can_show) });
        self.spawn_scoped(cx, async move |this, cx| {
            let outcome = job.await;
            let _ = this.update(cx, |app, cx| {
                let hdr = &mut app.viewer.hdr;
                if hdr.token != token {
                    return;
                }
                hdr.in_flight = false;
                hdr.headroom = 0.0;
                hdr.slot = match outcome {
                    Outcome::Sdr => Slot::Sdr,
                    Outcome::StandardRange => Slot::StandardRange,
                    Outcome::Hdr(pixels) => Slot::Hdr(pixels),
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// While an HDR frame shows, re-read the display headroom (ten times a
    /// second at first, while the screen ramps into EDR, then once a
    /// second) and repaint only when it changed.
    fn poll_viewer_headroom(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.viewer.hdr.polling {
            return;
        }
        self.viewer.hdr.polling = true;
        self.spawn_scoped_in(window, cx, async move |this, cx| {
            let mut ticks = 0_u32;
            loop {
                ticks = ticks.saturating_add(1);
                let wait = if ticks <= RAMP_TICKS {
                    RAMP_POLL
                } else {
                    HEADROOM_POLL
                };
                cx.background_executor().timer(wait).await;
                let keep = this.update_in(cx, |app, window, cx| {
                    let hdr = &mut app.viewer.hdr;
                    if !hdr.has_frame() || app.viewer.state.current().is_none() {
                        hdr.polling = false;
                        return false;
                    }
                    let left_edr = hdr.edr_on && !hdr.edr_lost && super::renderer_left_edr(window);
                    if left_edr
                        || app.viewer.orientation.is_identity()
                            && super::headroom(window).effective() != hdr.headroom
                    {
                        cx.notify();
                    }
                    true
                });
                if !matches!(keep, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }

    /// Called every render: the window is in extended-range mode exactly
    /// while the viewer shows an HDR frame upright (a rotated or flipped
    /// picture draws the SDR image).
    pub(crate) fn sync_viewer_hdr(&mut self, window: &Window) {
        let want = self.viewer.state.current().is_some()
            && self.viewer.orientation.is_identity()
            && self.viewer.hdr.has_frame();
        let hdr = &mut self.viewer.hdr;
        if hdr.edr_on && super::renderer_left_edr(window) {
            hdr.edr_lost = true;
        }
        if want != hdr.edr_on {
            super::set_window_edr(window, want);
            hdr.edr_on = want;
        }
    }
}

/// The toolbar tag: "HDR", with a tooltip saying when this display shows
/// the standard-range picture instead.
pub(crate) fn badge_element(badge: HdrBadge) -> AnyElement {
    use gpui_kit::component::tag::Tag;
    use gpui_kit::component::tooltip::Tooltip;
    let tag = Tag::custom(
        gpui_kit::white().opacity(0.14),
        gpui_kit::white(),
        gpui_kit::white().opacity(0.0),
    )
    .child("HDR");
    match badge {
        HdrBadge::Shown => div()
            .id("media-viewer-hdr")
            .child(tag)
            .tooltip(|window, cx| {
                Tooltip::new(tr(keys::Q_REST_SHOWN_IN_HIGH_DYNAMIC_RANGE)).build(window, cx)
            })
            .into_any_element(),
        HdrBadge::StandardRange => div()
            .id("media-viewer-hdr")
            .opacity(0.6)
            .child(tag)
            .tooltip(|window, cx| {
                Tooltip::new(tr(keys::Q_REST_SHOWN_IN_STANDARD_RANGE_ON)).build(window, cx)
            })
            .into_any_element(),
    }
}

#[cfg(target_os = "macos")]
fn frame_element(frame: &super::Frame, (w, h): (f32, f32)) -> Option<AnyElement> {
    Some(
        gpui_kit::surface(frame.buffer())
            .w(px(w))
            .h(px(h))
            .object_fit(ObjectFit::Contain)
            .into_any_element(),
    )
}

#[cfg(windows)]
fn frame_element(frame: &super::Frame, (w, h): (f32, f32)) -> Option<AnyElement> {
    Some(
        gpui_kit::surface(frame.surface())
            .w(px(w))
            .h(px(h))
            .object_fit(ObjectFit::Contain)
            .into_any_element(),
    )
}

#[cfg(not(any(target_os = "macos", windows)))]
fn frame_element(frame: &super::Frame, _fit: (f32, f32)) -> Option<AnyElement> {
    match *frame {}
}

#[cfg(test)]
mod tests {
    use super::bucket;

    #[test]
    fn long_edges_round_up_to_the_step() {
        assert_eq!(bucket(1.0), 512);
        assert_eq!(bucket(512.0), 512);
        assert_eq!(bucket(513.0), 1024);
        assert_eq!(bucket(2879.5), 3072);
    }
}
