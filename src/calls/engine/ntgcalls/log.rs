//! ntgcalls' own log (its calls code and WebRTC): warnings and errors
//! into the always-on call log, everything else into Quill's debug log
//! while debug mode is on, so a real call can be diagnosed afterwards
//! (`advanced::debug_mode`).
use crate::advanced::debug_mode;
use ntgcalls_sys::{
    Loader, NTG_LOG_DEBUG, NTG_LOG_ERROR, NTG_LOG_INFO, NTG_LOG_SOURCE_WEBRTC, NTG_LOG_WARNING,
    ntg_log_message,
};
use std::ffi::{CStr, c_char, c_void};
use std::sync::Once;

static INSTALLED: Once = Once::new();

/// Register the forwarder once per process; the library keeps one global
/// log callback.
pub(super) fn install(api: &Loader) {
    INSTALLED.call_once(|| {
        // SAFETY: `forward` matches `ntg_log_cb`, never unwinds and reads
        // no user data.
        unsafe { (api.ntg_set_log_callback)(Some(forward), std::ptr::null_mut()) };
    });
}

unsafe extern "C" fn forward(message: ntg_log_message, _: *mut c_void) {
    // Warnings and errors always reach the call log; the rest only the
    // debug log, while it is on.
    let always = matches!(message.level, NTG_LOG_ERROR | NTG_LOG_WARNING);
    if !always && !debug_mode::enabled() {
        return;
    }
    // WebRTC's debug level is a firehose of per-packet lines.
    if message.source == NTG_LOG_SOURCE_WEBRTC && message.level == NTG_LOG_DEBUG {
        return;
    }
    let level = match message.level {
        NTG_LOG_ERROR => "error",
        NTG_LOG_WARNING => "warning",
        NTG_LOG_INFO => "info",
        _ => "debug",
    };
    let source = if message.source == NTG_LOG_SOURCE_WEBRTC {
        "webrtc"
    } else {
        "ntgcalls"
    };
    // SAFETY: ntgcalls passes NUL-terminated strings (or null) that live
    // for the duration of the callback.
    let (file, text) = unsafe { (text(message.file), text(message.message)) };
    let _ = std::panic::catch_unwind(|| {
        let line = format!("[call] {source} {level} {file}:{} {text}", message.line);
        if always {
            debug_mode::call_log(&line);
        } else {
            debug_mode::debug_log(&line);
        }
    });
}

/// # Safety
/// `value` is null or a NUL-terminated string alive for this call.
unsafe fn text(value: *const c_char) -> String {
    if value.is_null() {
        return String::new();
    }
    // SAFETY: per the contract above.
    unsafe { CStr::from_ptr(value) }
        .to_string_lossy()
        .into_owned()
}
