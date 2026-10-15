//! External capture sources: Quill captures the microphone (and, in a
//! later slice, the screen) itself and hands ntgcalls ready frames
//! through `ntg_send_external_frame` (`ntgcalls.h`), instead of letting
//! ntgcalls open the devices. Design: `docs/decisions/call-capture.md`.
//!
//! The engine lives on the connect-driver thread, but frames arrive on
//! capture threads every 10 ms, so the engine hands out a `Send` sender
//! bound to one call ([`ExternalFrameSender`]) that a feeder thread
//! drives without touching the driver.
use super::EngineError;

/// Microphone frames: PCM s16le, 48 kHz mono, 10 ms. ntgcalls reads one
/// frame as `frame_size / (2 * channel_count)` samples
/// (`audio_streamer.cpp`), so every frame is exactly this long.
pub const EXTERNAL_AUDIO_RATE: u32 = 48_000;
/// Samples per external microphone frame (10 ms at 48 kHz).
pub const EXTERNAL_AUDIO_FRAME_SAMPLES: usize = 480;
/// Bytes per external microphone frame (`i16` samples).
pub const EXTERNAL_AUDIO_FRAME_BYTES: usize = EXTERNAL_AUDIO_FRAME_SAMPLES * 2;

/// Which call an external source feeds: a 1:1 call by TDLib call id or a
/// group call by TDLib group call id. The engine maps it to its native
/// chat id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CaptureTarget {
    Private(i32),
    Group(i32),
}

/// The stream an external frame belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalDevice {
    Microphone,
    /// Screen sharing (I420 frames); used from the screen-capture slice on.
    Screen,
}

/// Per-frame data ntgcalls takes beside the bytes (`ntg_frame_data`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExternalFrameMeta {
    /// Unix milliseconds of the capture moment.
    pub timestamp_ms: i64,
    /// Video only; 0 for audio.
    pub width: u16,
    pub height: u16,
}

/// Pushes frames into one call's external sources. Implementations are
/// called from a feeder thread, never from the driver.
pub trait ExternalFrameSender: Send + Sync {
    fn send(
        &self,
        device: ExternalDevice,
        data: &[u8],
        meta: ExternalFrameMeta,
    ) -> Result<(), EngineError>;
}

/// Whether frame bytes have the length ntgcalls expects: exactly one
/// 10 ms microphone frame, or one I420 picture (`w*h*3/2`, even sizes).
pub fn external_frame_valid(device: ExternalDevice, len: usize, meta: ExternalFrameMeta) -> bool {
    match device {
        ExternalDevice::Microphone => len == EXTERNAL_AUDIO_FRAME_BYTES,
        ExternalDevice::Screen => {
            let (w, h) = (usize::from(meta.width), usize::from(meta.height));
            w > 0
                && h > 0
                && w % 2 == 0
                && h % 2 == 0
                && w.checked_mul(h)
                    .and_then(|pixels| pixels.checked_mul(3))
                    .is_some_and(|bytes| bytes / 2 == len)
        }
    }
}

/// Whether calls capture the microphone in Quill (external source) or
/// let ntgcalls open the device itself. `QUILL_CALL_CAPTURE=native` is
/// the debugging fallback; builds without the UI feature have no capture
/// backend and always use ntgcalls' device source.
pub fn external_capture_enabled() -> bool {
    external_capture_from(
        cfg!(feature = "ui"),
        std::env::var("QUILL_CALL_CAPTURE").ok().as_deref(),
    )
}

/// [`external_capture_enabled`] for a given build and `QUILL_CALL_CAPTURE`.
pub fn external_capture_from(has_backend: bool, env: Option<&str>) -> bool {
    has_backend && !env.is_some_and(|value| value.trim().eq_ignore_ascii_case("native"))
}

/// Whether screen sharing sends Quill's own capture (an external screen
/// source) instead of ntgcalls' desktop capture: only where Quill has a
/// screen backend (macOS so far, `calls::capture::screen`), and not
/// under `QUILL_CALL_CAPTURE=native`.
pub fn external_screen_capture_enabled() -> bool {
    external_capture_enabled() && cfg!(all(feature = "ui", target_os = "macos"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn microphone_frames_are_exactly_ten_milliseconds() {
        let meta = ExternalFrameMeta::default();
        assert!(external_frame_valid(ExternalDevice::Microphone, 960, meta));
        assert!(!external_frame_valid(ExternalDevice::Microphone, 958, meta));
        assert!(!external_frame_valid(ExternalDevice::Microphone, 0, meta));
    }

    #[test]
    fn native_fallback_only_on_request_or_without_a_backend() {
        assert!(external_capture_from(true, None));
        assert!(external_capture_from(true, Some("")));
        assert!(external_capture_from(true, Some("external")));
        assert!(!external_capture_from(true, Some("native")));
        assert!(!external_capture_from(true, Some(" Native ")));
        assert!(!external_capture_from(false, None));
    }

    #[test]
    fn screen_capture_is_external_only_with_a_screen_backend() {
        if !cfg!(all(feature = "ui", target_os = "macos")) {
            assert!(!external_screen_capture_enabled());
        }
        if !external_capture_enabled() {
            assert!(!external_screen_capture_enabled());
        }
    }

    #[test]
    fn screen_frames_are_one_i420_picture() {
        let meta = ExternalFrameMeta {
            timestamp_ms: 0,
            width: 320,
            height: 240,
        };
        assert!(external_frame_valid(
            ExternalDevice::Screen,
            320 * 240 * 3 / 2,
            meta
        ));
        assert!(!external_frame_valid(
            ExternalDevice::Screen,
            320 * 240,
            meta
        ));
        let odd = ExternalFrameMeta { width: 321, ..meta };
        assert!(!external_frame_valid(
            ExternalDevice::Screen,
            321 * 240 * 3 / 2,
            odd
        ));
        let empty = ExternalFrameMeta { width: 0, ..meta };
        assert!(!external_frame_valid(ExternalDevice::Screen, 0, empty));
    }
}
