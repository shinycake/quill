//! HDR stills: picture files whose HDR rendition (a gain map, or a PQ /
//! HLG colour space) shows at full brightness in the media viewer.
//!
//! macOS decodes through ImageIO into a half-float GPU buffer and switches
//! the window's layer to an extended-range drawable while one shows
//! (`macos.rs`). Windows decodes gain-map JPEGs and HDR JPEG XL on the CPU
//! (`cpu`) and switches the swap chain to scRGB while one shows on an HDR
//! display (`windows.rs`). Linux shows the SDR picture: GPUI's wgpu
//! renderer has no HDR surface; every entry point here answers "SDR" there.
#![deny(clippy::indexing_slicing)]

#[cfg(any(windows, test))]
mod cpu;
mod half_float;
#[cfg(target_os = "macos")]
mod macos;
mod probe_cache;
#[cfg(test)]
#[cfg(target_os = "macos")]
mod tests;
mod tone_map;
mod viewer;
#[cfg(test)]
#[cfg(all(target_os = "macos", feature = "demo-capture"))]
mod viewer_tests;
#[cfg(windows)]
mod windows;

pub(crate) use probe_cache::has_hdr;
#[cfg(test)]
#[cfg(all(target_os = "macos", feature = "demo-capture"))]
use viewer::HdrBadge;
pub(crate) use viewer::{HdrViewer, badge_element};

use crate::ui::image_decode::DecodeError;
use std::path::Path;

/// The largest HDR frame the viewer decodes, in pixels (8 bytes each: a
/// 24 Mpx frame is 192 MB of GPU memory, held only while it shows).
#[cfg(any(target_os = "macos", windows, test))]
pub(crate) const HDR_MAX_PIXELS: u64 = 24_000_000;

/// How far above SDR white a display can go (1.0 = no headroom).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Headroom {
    /// What the screen shows right now (it drops with brightness and
    /// power settings).
    pub current: f32,
    /// The most the screen could show.
    pub potential: f32,
}

impl Headroom {
    pub(crate) const SDR: Self = Self {
        current: 1.0,
        potential: 1.0,
    };

    /// The headroom to draw for. A screen reports 1.0 until EDR content is
    /// on it and then ramps up, so a frame drawn meanwhile aims at what
    /// the screen can reach rather than squashing its highlights.
    pub(crate) fn effective(self) -> f32 {
        if self.current <= 1.0 && self.potential > 1.0 {
            self.potential
        } else {
            self.current.max(1.0)
        }
    }

    /// Whether this screen can show anything above SDR white.
    pub(crate) fn can_show_hdr(self) -> bool {
        self.potential > 1.0
    }
}

/// A decoded HDR still (macOS and Windows make them).
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
pub(crate) struct HdrPixels {
    pub frame: Frame,
    pub size: (u32, u32),
    /// The brightest channel, in multiples of SDR white.
    pub peak: f32,
}

#[cfg(target_os = "macos")]
pub(crate) use macos::Frame;
#[cfg(windows)]
pub(crate) use windows::Frame;

/// No HDR frames exist on Linux.
#[cfg(not(any(target_os = "macos", windows)))]
pub(crate) enum Frame {}

#[cfg(not(any(target_os = "macos", windows)))]
impl Frame {
    pub(crate) fn set_headroom(&self, _headroom: f32) {
        match *self {}
    }
}

/// Decode `path` to its HDR rendition, long edge at most `long_edge`.
/// `Ok(None)` when the picture has nothing above SDR white (or on a
/// platform without an HDR path).
pub(crate) fn decode(path: &Path, long_edge: u32) -> Result<Option<HdrPixels>, DecodeError> {
    #[cfg(target_os = "macos")]
    return macos::decode(path, long_edge);
    #[cfg(windows)]
    return windows::decode(path, long_edge);
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = (path, long_edge);
        Ok(None)
    }
}

/// Whether the file carries HDR data (metadata only, no pixel decode).
pub(crate) fn probe(path: &Path) -> bool {
    probe_file(path).unwrap_or(false)
}

/// As `probe`, with `None` when the file can't be opened as a picture.
fn probe_file(path: &Path) -> Option<bool> {
    #[cfg(target_os = "macos")]
    return macos::probe(path);
    #[cfg(windows)]
    return windows::probe(path);
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = path;
        Some(false)
    }
}

#[cfg(test)]
thread_local! {
    /// Test windows have no screen; tests set the headroom they want.
    static TEST_HEADROOM: std::cell::Cell<Option<Headroom>> = const { std::cell::Cell::new(None) };
    static TEST_LEFT_EDR: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Tests: the renderer reads as having left extended range.
#[cfg(test)]
#[cfg(all(target_os = "macos", feature = "demo-capture"))]
pub(crate) fn set_test_renderer_left_edr(left: bool) {
    TEST_LEFT_EDR.with(|cell| cell.set(left));
}

#[cfg(test)]
#[cfg(all(target_os = "macos", feature = "demo-capture"))]
pub(crate) fn set_test_headroom(headroom: Option<Headroom>) {
    TEST_HEADROOM.with(|cell| cell.set(headroom));
}

/// The headroom of the screen `window` is on.
pub(crate) fn headroom(window: &gpui_kit::Window) -> Headroom {
    #[cfg(test)]
    if let Some(headroom) = TEST_HEADROOM.with(std::cell::Cell::get) {
        return headroom;
    }
    #[cfg(target_os = "macos")]
    return macos::headroom(window);
    #[cfg(windows)]
    return windows::headroom(window);
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = window;
        Headroom::SDR
    }
}

/// Switch `window` to (or back from) an extended-range drawable (an
/// scRGB swap chain on Windows).
fn set_window_edr(window: &gpui_kit::Window, on: bool) {
    #[cfg(target_os = "macos")]
    macos::set_window_edr(window, on);
    #[cfg(windows)]
    windows::set_window_edr(window, on);
    #[cfg(not(any(target_os = "macos", windows)))]
    let _ = (window, on);
}

/// The renderer took `window` back to 8-bit after it asked for extended
/// range (its float pipelines did not build), so an HDR picture shows
/// clamped to standard range.
fn renderer_left_edr(window: &gpui_kit::Window) -> bool {
    #[cfg(test)]
    if TEST_LEFT_EDR.with(std::cell::Cell::get) {
        return true;
    }
    #[cfg(target_os = "macos")]
    return macos::window_edr(window) == Some(false);
    #[cfg(windows)]
    return windows::hdr_failed(window);
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = window;
        false
    }
}
