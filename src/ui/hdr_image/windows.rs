//! Windows HDR stills: the CPU decoders (`cpu`) give half-float pixels,
//! which the vendored DirectX renderer uploads and paints with its
//! `hdr_surface` shader. While one shows, the window asks for an scRGB
//! swap chain (`gpui_windows::hdr_output`), which the renderer switches to
//! only when the display is in Windows HDR mode.

use super::{HdrPixels, Headroom, cpu};
use crate::ui::image_decode::DecodeError;
use gpui_kit::{SurfaceFormat, SurfaceFrame};
use std::path::Path;
use std::sync::Arc;

/// A decoded HDR still, ready for the renderer.
pub(crate) struct Frame {
    frame: Arc<SurfaceFrame>,
}

impl Frame {
    pub(crate) fn surface(&self) -> Arc<SurfaceFrame> {
        self.frame.clone()
    }

    pub(crate) fn set_headroom(&self, headroom: f32) {
        self.frame.set_headroom(headroom);
    }
}

pub(crate) fn probe(path: &Path) -> Option<bool> {
    cpu::probe(path)
}

pub(crate) fn decode(path: &Path, long_edge: u32) -> Result<Option<HdrPixels>, DecodeError> {
    let Some(decoded) = cpu::decode(path, long_edge)? else {
        return Ok(None);
    };
    let bytes = decoded.bytes;
    let frame = SurfaceFrame::new(
        decoded.width,
        decoded.height,
        SurfaceFormat::Rgba16Float,
        bytes,
    )
    .map_err(|err| DecodeError::Failed(format!("HDR frame: {err}")))?;
    Ok(Some(HdrPixels {
        frame: Frame {
            frame: Arc::new(frame),
        },
        size: (decoded.width, decoded.height),
        peak: decoded.peak,
    }))
}

/// The headroom of the display the window is on: none unless Windows HDR
/// is on for it.
pub(crate) fn headroom(window: &gpui_kit::Window) -> Headroom {
    let Some(hwnd) = hwnd(window) else {
        return Headroom::SDR;
    };
    let info = gpui_windows::hdr_output::display_info(hwnd);
    let headroom = info.headroom();
    Headroom {
        current: headroom,
        potential: headroom,
    }
}

/// Ask the renderer for (or release) the scRGB swap chain.
pub(crate) fn set_window_edr(window: &gpui_kit::Window, on: bool) {
    if let Some(hwnd) = hwnd(window) {
        gpui_windows::hdr_output::set_hdr_output(hwnd, on);
    }
}

/// Whether the renderer could not switch the window to the scRGB swap
/// chain it asked for, so it draws in SDR.
pub(crate) fn hdr_failed(window: &gpui_kit::Window) -> bool {
    hwnd(window).is_some_and(gpui_windows::hdr_output::hdr_failed)
}

fn hwnd(window: &gpui_kit::Window) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
        _ => None,
    }
}
