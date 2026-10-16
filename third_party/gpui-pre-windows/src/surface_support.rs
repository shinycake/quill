// Added by the Quill project (2026). See third_party/gpui-pre-windows/QUILL-CHANGES.md.
//! The upload trace lines (`QUILL_TRACE_VIDEO`) that let a video's upload
//! cost be measured: NV12 surface uploads (`yuv_surfaces`) and large image
//! atlas uploads (`directx_atlas`).
//!
//! Whether a window can draw an NV12 frame is answered per frame, not
//! here: the renderer marks each `SurfaceFrame` drawn or refused.

use std::sync::OnceLock;
use std::time::Instant;

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
