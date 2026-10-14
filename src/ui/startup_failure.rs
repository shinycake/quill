//! What happens when the main window cannot open (for example no usable GPU
//! or a remote session without one): say so in a native alert where the
//! platform has one, write the error to stderr, and exit with a failure code
//! instead of panicking.

/// Report `error` from opening the main window and end the process.
#[expect(
    clippy::exit,
    reason = "no window could open, so there is no UI to keep running; exit with a failure code"
)]
pub(crate) fn window_failed(error: &dyn std::fmt::Display) -> ! {
    let detail = error.to_string();
    eprintln!("quill: could not open a window: {detail}");
    show_alert("Quill could not open a window", &detail);
    std::process::exit(1)
}

#[cfg(target_os = "macos")]
fn show_alert(title: &str, detail: &str) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSAlert;
    use objc2_foundation::NSString;
    // AppKit alerts need the main thread; GPUI runs foreground tasks there.
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(title));
    alert.setInformativeText(&NSString::from_str(detail));
    alert.runModal();
}

#[cfg(target_os = "windows")]
fn show_alert(title: &str, detail: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MessageBoxW,
    };
    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain(std::iter::once(0)).collect() };
    let text = wide(&format!("{title}.\n\n{detail}"));
    let caption = wide("Quill");
    // SAFETY: both buffers are NUL-terminated UTF-16 that outlive the call,
    // and a null owner window is allowed; MessageBoxW only reads them.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_OK | MB_ICONERROR | MB_SETFOREGROUND,
        );
    }
}

/// Linux has no system alert API without a window; use the desktop's dialog
/// helper when one is installed. stderr already has the message otherwise.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn show_alert(title: &str, detail: &str) {
    use std::process::{Command, Stdio};
    let text = format!("{title}.\n\n{detail}");
    let zenity = Command::new("zenity")
        .args(["--error", "--no-markup", "--title=Quill", "--text"])
        .arg(&text)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if zenity.is_ok() {
        return;
    }
    let _ = Command::new("kdialog")
        .args(["--title", "Quill", "--error"])
        .arg(&text)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}
