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

/// What the background decode is working on: the path, the rounded long
/// edge and "the screen can show HDR".
type DecodeKey = (PathBuf, u32, bool);

/// The HDR frame on screen, with the headroom last attached to it.
struct Shown {
    pixels: HdrPixels,
    /// 0.0 until the first draw attaches one.
    headroom: f32,
}

/// The viewer's decode of the current picture file, one value. It leaves
/// `Decoding` when the matching answer arrives or `reset` runs; an answer
/// for another key is dropped.
#[derive(Default)]
enum Decode {
    #[default]
    Idle,
    /// A decode is running for `key`. A bigger re-decode keeps showing the
    /// current frame meanwhile.
    Decoding {
        key: DecodeKey,
        showing: Option<Shown>,
    },
    /// No HDR data in the file.
    Sdr(DecodeKey),
    /// HDR data that this display (or platform) does not show.
    StandardRange(DecodeKey),
    Hdr {
        key: DecodeKey,
        shown: Shown,
    },
}

/// What the background job found for one key.
enum Outcome {
    Sdr,
    StandardRange,
    Hdr(HdrPixels),
}

/// The window's layer against the viewer. `Lost`: the renderer could not
/// draw in extended range and left it, so frames aim at standard range
/// until the viewer shows no HDR frame (on Windows the check runs again
/// after that; on macOS the loss is for good).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Edr {
    #[default]
    Off,
    On,
    /// Lost, with the window's layer still in extended-range mode.
    Lost,
    /// Lost for good (not Windows), with the layer back in standard range.
    LostOff,
}

impl Edr {
    fn is_lost(self) -> bool {
        matches!(self, Self::Lost | Self::LostOff)
    }

    /// The state once the window's layer is back in standard range.
    fn layer_off(self) -> Self {
        if self.is_lost() && !cfg!(windows) {
            Self::LostOff
        } else {
            Self::Off
        }
    }

    fn window_on(self) -> bool {
        matches!(self, Self::On | Self::Lost)
    }
}

/// The display headroom re-read while an HDR frame shows.
#[derive(Default)]
enum HeadroomPoll {
    #[default]
    Off,
    /// The task ends itself (and this stage with it) when no frame shows.
    Running { _task: Task<()> },
}

#[derive(Default)]
pub(crate) struct HdrViewer {
    decode: Decode,
    edr: Edr,
    poll: HeadroomPoll,
}

impl Decode {
    fn key(&self) -> Option<&DecodeKey> {
        match self {
            Self::Idle => None,
            Self::Decoding { key, .. }
            | Self::Sdr(key)
            | Self::StandardRange(key)
            | Self::Hdr { key, .. } => Some(key),
        }
    }

    fn frame(&self) -> Option<&Shown> {
        match self {
            Self::Hdr { shown, .. } => Some(shown),
            Self::Decoding { showing, .. } => showing.as_ref(),
            _ => None,
        }
    }

    fn frame_mut(&mut self) -> Option<&mut Shown> {
        match self {
            Self::Hdr { shown, .. } => Some(shown),
            Self::Decoding { showing, .. } => showing.as_mut(),
            _ => None,
        }
    }

    fn is_decoding(&self) -> bool {
        matches!(self, Self::Decoding { .. })
    }
}

impl HdrViewer {
    /// Forget the current picture (viewer step, close, item deleted).
    pub(crate) fn reset(&mut self) {
        self.decode = Decode::Idle;
    }

    pub(crate) fn badge(&self) -> Option<HdrBadge> {
        match &self.decode {
            Decode::Hdr { .. } if self.edr.is_lost() => Some(HdrBadge::StandardRange),
            Decode::Hdr { .. } => Some(HdrBadge::Shown),
            Decode::StandardRange(_) => Some(HdrBadge::StandardRange),
            Decode::Idle | Decode::Decoding { .. } | Decode::Sdr(_) => None,
        }
    }

    /// Whether a larger decode would add detail: the frame is as big as
    /// was asked (`edge`), not cut down to the picture's own size or the
    /// pixel cap.
    fn can_grow(&self, edge: u32) -> bool {
        self.decode
            .frame()
            .is_some_and(|shown| shown.pixels.size.0.max(shown.pixels.size.1) >= edge)
    }

    fn has_frame(&self) -> bool {
        self.decode.frame().is_some()
    }

    /// The headroom the frame is drawn for on a display reporting
    /// `headroom`: back in 8-bit (`Edr::Lost`) it aims at standard range,
    /// whatever the display says. The draw and the headroom poll both read
    /// it, so they agree on when it changed.
    fn display_headroom(&self, headroom: super::Headroom) -> f32 {
        if self.edr.is_lost() {
            super::Headroom::SDR.effective()
        } else {
            headroom.effective()
        }
    }

    /// Tests: the headroom the shown frame is tone-mapped into.
    #[cfg(test)]
    #[cfg(all(target_os = "macos", feature = "demo-capture"))]
    pub(crate) fn frame_headroom(&self) -> Option<f32> {
        match &self.decode {
            Decode::Hdr { shown, .. } => Some(shown.pixels.frame.headroom()),
            _ => None,
        }
    }

    /// Recorded for tests: whether the window was last switched to EDR.
    #[cfg(test)]
    #[cfg(all(target_os = "macos", feature = "demo-capture"))]
    pub(crate) fn edr_on(&self) -> bool {
        self.edr.window_on()
    }

    /// Tests: a bigger decode of the shown frame is running, as a zoom
    /// starts it. Returns whether a frame was showing.
    #[cfg(test)]
    #[cfg(all(target_os = "macos", feature = "demo-capture"))]
    pub(crate) fn regrow_for_test(&mut self) -> bool {
        let Decode::Hdr { key, shown } = std::mem::take(&mut self.decode) else {
            return false;
        };
        self.decode = Decode::Decoding {
            key,
            showing: Some(shown),
        };
        true
    }

    /// Tests: a frame shows (a running decode keeps the old one up).
    #[cfg(test)]
    #[cfg(all(target_os = "macos", feature = "demo-capture"))]
    pub(crate) fn shows_frame(&self) -> bool {
        self.decode.frame().is_some()
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
        let stale = match hdr.decode.key() {
            None => true,
            Some((old, _, show)) if old != path || *show != can_show => true,
            // A larger decode waits for the one in flight, so a zoom
            // gesture runs one decode at a time.
            Some((_, edge, _)) => {
                !hdr.decode.is_decoding() && *edge < long_edge && hdr.can_grow(*edge)
            }
        };
        if stale {
            self.start_hdr_decode(path, long_edge, can_show, cx);
        }
        let hdr = &mut self.viewer.hdr;
        let display = hdr.display_headroom(headroom);
        // A bigger re-decode keeps drawing the frame it replaces.
        let shown = hdr.decode.frame_mut()?;
        // Back in 8-bit, the frame is tone-mapped into standard range
        // instead of clipping its highlights.
        if shown.headroom != display {
            let param = super::tone_map::shader_headroom(display, shown.pixels.peak);
            shown.pixels.frame.set_headroom(param);
            shown.headroom = display;
        }
        let element = frame_element(&shown.pixels.frame, fit);
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
        let key = (path.to_path_buf(), long_edge, can_show);
        let showing = match std::mem::take(&mut hdr.decode) {
            Decode::Hdr { shown, .. } => Some(shown),
            Decode::Decoding { showing, .. } => showing,
            _ => None,
        };
        hdr.decode = Decode::Decoding {
            key: key.clone(),
            showing,
        };
        let path = path.to_path_buf();
        let job = cx.background_spawn(async move { run(&path, long_edge, can_show) });
        self.spawn_scoped(cx, async move |this, cx| {
            let outcome = job.await;
            let _ = this.update(cx, |app, cx| {
                let hdr = &mut app.viewer.hdr;
                // An answer for a key that is no longer the one decoding
                // (reset, or a newer decode) is dropped.
                if !matches!(&hdr.decode, Decode::Decoding { key: now, .. } if *now == key) {
                    return;
                }
                hdr.decode = match outcome {
                    Outcome::Sdr => Decode::Sdr(key),
                    Outcome::StandardRange => Decode::StandardRange(key),
                    Outcome::Hdr(pixels) => Decode::Hdr {
                        key,
                        shown: Shown {
                            pixels,
                            headroom: 0.0,
                        },
                    },
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
        if matches!(self.viewer.hdr.poll, HeadroomPoll::Running { .. }) {
            return;
        }
        let task = self.spawn_scoped_in(window, cx, async move |this, cx| {
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
                    let hdr = &mut app.screen.viewer.hdr;
                    if !hdr.has_frame() || app.screen.viewer.state.current().is_none() {
                        hdr.poll = HeadroomPoll::Off;
                        return false;
                    }
                    let left_edr = hdr.edr == Edr::On && super::renderer_left_edr(window);
                    // The same value the draw aims at, so a screen that lost
                    // EDR (drawn at SDR) does not read as changed forever.
                    let drawn = hdr.decode.frame().map_or(0.0, |shown| shown.headroom);
                    if left_edr
                        || app.screen.viewer.orientation.is_identity()
                            && hdr.display_headroom(super::headroom(window)) != drawn
                    {
                        cx.notify();
                    }
                    true
                });
                if !matches!(keep, Ok(true)) {
                    break;
                }
            }
        });
        self.viewer.hdr.poll = HeadroomPoll::Running { _task: task };
    }

    /// The account on screen leaves: its viewer (and the record of the
    /// window's extended-range mode with it) goes, so the mode is switched
    /// off here, or the next screen's `want = false` would equal the new
    /// record and leave the layer in extended range.
    pub(crate) fn leave_viewer_edr(&mut self, window: &Window) {
        let hdr = &mut self.viewer.hdr;
        if hdr.edr.window_on() {
            super::set_window_edr(window, false);
        }
        hdr.edr = hdr.edr.layer_off();
    }

    /// Called every render: the window is in extended-range mode exactly
    /// while the viewer shows an HDR frame upright (a rotated or flipped
    /// picture draws the SDR image).
    pub(crate) fn sync_viewer_hdr(&mut self, window: &Window) {
        let want = self.viewer.state.current().is_some()
            && self.viewer.orientation.is_identity()
            && self.viewer.hdr.has_frame();
        let hdr = &mut self.viewer.hdr;
        if hdr.edr == Edr::On && super::renderer_left_edr(window) {
            hdr.edr = Edr::Lost;
        }
        match (want, hdr.edr) {
            (true, Edr::Off) => {
                super::set_window_edr(window, true);
                hdr.edr = Edr::On;
            }
            (true, Edr::LostOff) => {
                super::set_window_edr(window, true);
                hdr.edr = Edr::Lost;
            }
            (false, Edr::On | Edr::Lost) => {
                super::set_window_edr(window, false);
                hdr.edr = hdr.edr.layer_off();
            }
            _ => {}
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
    use super::{Edr, bucket};

    #[test]
    fn a_lost_layer_stays_lost_off_windows_once_switched_off() {
        let off = Edr::Lost.layer_off();
        if cfg!(windows) {
            assert_eq!(off, Edr::Off);
        } else {
            assert_eq!(off, Edr::LostOff);
            assert!(off.is_lost() && !off.window_on());
        }
        assert_eq!(Edr::On.layer_off(), Edr::Off);
    }

    #[test]
    fn long_edges_round_up_to_the_step() {
        assert_eq!(bucket(1.0), 512);
        assert_eq!(bucket(512.0), 512);
        assert_eq!(bucket(513.0), 1024);
        assert_eq!(bucket(2879.5), 3072);
    }
}
