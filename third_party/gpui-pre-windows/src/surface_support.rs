// Added by the Quill project (2026). See third_party/gpui-pre-windows/QUILL-CHANGES.md.
//! Which `SurfaceFrame` formats the DirectX renderer can draw right now,
//! for producers that choose a picture layout (Quill's video player asks
//! before decoding into NV12), and the upload trace lines
//! (`QUILL_TRACE_VIDEO`) that let a video's upload cost be measured.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

/// Cleared when the NV12 pipeline or one of its textures could not be
/// made; set again when a lost device is recovered with a working one.
static NV12: AtomicBool = AtomicBool::new(true);

/// Whether NV12 frames are drawn. When this turns false, frames already
/// handed over are skipped (not drawn), so a producer that sees it switch
/// should hand over BGRA images from then on, including the picture on
/// screen.
pub fn nv12_available() -> bool {
    NV12.load(Ordering::Relaxed)
}

pub(crate) fn set_nv12_available(available: bool) {
    NV12.store(available, Ordering::Relaxed);
}

/// `QUILL_TRACE_VIDEO` is set (read once): texture uploads of video
/// pictures print a line each.
pub(crate) fn trace_uploads() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("QUILL_TRACE_VIDEO").is_some())
}

/// One `VU,<path>,<bytes>,<upload_us>` line on stderr: an upload of
/// `bytes` through `path` (`nv12` for the YUV surfaces, `atlas` for an
/// image going into the sprite atlas) that took `upload_us` on the CPU.
pub(crate) fn trace_upload(path: &str, bytes: usize, started: Instant) {
    eprintln!("VU,{path},{bytes},{}", started.elapsed().as_micros());
}
