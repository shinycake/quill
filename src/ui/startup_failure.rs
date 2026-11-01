//! What happens when the main window cannot open (for example no usable GPU
//! or a remote session without one): say so in a native alert where the
//! platform has one, write the error to stderr, and exit with a failure code
//! instead of panicking. On Linux, when nothing can draw the window, the
//! alert says which Vulkan piece is missing and what to install.
//!
//! A GPU that stops working mid-session on Linux ends the app through a
//! normal quit instead, and the alert follows once the quit has finished.
//! On Windows Quill first tries once to draw again ([`device_loss`]).
#![deny(clippy::indexing_slicing)]

#[cfg(any(test, target_os = "windows"))]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
mod device_loss;
#[cfg(any(test, target_os = "linux"))]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod vulkan;

#[cfg(target_os = "windows")]
pub(crate) use device_loss::recover_after_gpu_loss;

#[cfg(target_os = "linux")]
use gpui_kit::ForegroundExecutor;
use gpui_kit::Platform;
use quill::lang::{keys, tr};
use std::cell::RefCell;
use std::rc::Rc;
#[cfg(target_os = "linux")]
use std::rc::Weak;

/// Report `error` from opening the main window and end the process.
#[expect(
    clippy::exit,
    reason = "no window could open, so there is no UI to keep running; exit with a failure code"
)]
pub(crate) fn window_failed(error: &dyn std::fmt::Display) -> ! {
    let detail = error.to_string();
    eprintln!("quill: could not open a window: {detail}");
    let (message, summary) = alert_detail(&detail);
    #[cfg(target_os = "linux")]
    eprintln!("\n{message}");
    // A missing driver is not a crash: the next launch shows no
    // "closed unexpectedly" notice for this run.
    if let Some(root) = quill::settings::safe_app_root() {
        quill::crash_log::mark_clean_exit(&root);
    }
    log::logger().flush();
    show_alert(tr(keys::Q_MISC_QUILL_COULD_NOT_OPEN_A), &message, &summary);
    quill::telegram::hub::set_exit_status(1);
    std::process::exit(1)
}

/// Says why a launch that opens no window is exiting: a bundled app has no
/// stderr.
pub(crate) fn tell(title: &str, body: &str) {
    show_alert(title, body, body);
}

/// A GPU lost mid-session that the renderer could not recreate (Linux):
/// quit the normal way, so the quit observers close the TDLib clients and
/// save state, and say why once they have run ([`report_gpu_gone`]).
pub(crate) fn watch_gpu_loss(platform: &Rc<dyn Platform>) {
    #[cfg(target_os = "linux")]
    {
        let quit = (platform.foreground_executor(), Rc::downgrade(platform));
        QUIT.with(|slot| *slot.borrow_mut() = Some(quit));
        gpui_wgpu::on_device_gone(gpu_gone);
    }
    #[cfg(not(target_os = "linux"))]
    let _ = platform;
}

#[cfg(target_os = "linux")]
thread_local! {
    static QUIT: RefCell<Option<(ForegroundExecutor, Weak<dyn Platform>)>> =
        const { RefCell::new(None) };
}

thread_local! {
    static GPU_GONE: RefCell<Option<String>> = const { RefCell::new(None) };
    /// Quill asked to quit or restart: a window closing from now on is part
    /// of that, not a window the renderer gave up on.
    static QUIT_ASKED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Every quit and restart goes through `interface_zoom::ZoomPlatform`, which
/// notes it here.
pub(crate) fn note_quit_asked() {
    QUIT_ASKED.with(|asked| asked.set(true));
}

#[cfg(any(test, target_os = "windows"))]
fn quit_asked() -> bool {
    QUIT_ASKED.with(std::cell::Cell::get)
}

/// Keep why the GPU stopped working for [`report_gpu_gone`]. Whether this
/// is the first report.
#[cfg(any(test, target_os = "linux", target_os = "windows"))]
fn remember_gpu_gone(detail: &str) -> bool {
    GPU_GONE.with(|gone| gone.borrow_mut().replace(detail.to_owned()).is_none())
}

/// Called by the renderer inside a window's draw, so the quit is queued
/// on the main thread's executor and runs once the draw has returned.
#[cfg(target_os = "linux")]
fn gpu_gone(detail: &str) {
    let first = remember_gpu_gone(detail);
    let quit = QUIT.with(|slot| slot.borrow().clone());
    let (true, Some((executor, platform))) = (first, quit) else {
        return;
    };
    executor
        .spawn(async move {
            if let Some(platform) = platform.upgrade() {
                platform.quit();
            }
        })
        .detach();
}

/// Called after the app's `run` returns: the alert for a GPU that stopped
/// working mid-session, if one did.
pub(crate) fn report_gpu_gone() {
    if let Some(detail) = GPU_GONE.with(|gone| gone.borrow_mut().take()) {
        eprintln!("quill: the GPU stopped working: {detail}");
        show_alert(
            tr(keys::Q_MISC_GPU_STOPPED_WORKING),
            &quill::lang::tr_with(
                keys::Q_MISC_GPU_STOPPED_WORKING_DETAIL,
                &[("detail", &detail)],
            ),
            tr(keys::Q_MISC_GPU_STOPPED_WORKING_SUMMARY),
        );
    }
}

/// The alert's body, and a short version for a desktop notification (which
/// shows only a few lines). On Linux a missing Vulkan loader, driver or
/// usable device gets the package to install; other graphics errors get the
/// general driver steps.
fn alert_detail(detail: &str) -> (String, String) {
    #[cfg(target_os = "linux")]
    {
        let setup = vulkan::probe();
        if let Some(missing) = vulkan::diagnose(detail, &setup) {
            return (
                vulkan::message(&missing, setup.virtio_gpu, detail),
                vulkan::summary(&missing).to_owned(),
            );
        }
    }
    #[cfg(target_os = "linux")]
    if is_vulkan_failure(detail) {
        return (
            quill::lang::tr_with(
                keys::Q_MISC_QUILL_COULD_NOT_OPEN_A_GPU,
                &[("detail", detail)],
            ),
            tr(keys::Q_MISC_VULKAN_SUMMARY_GPU).to_owned(),
        );
    }
    (detail.to_string(), detail.to_string())
}

/// Whether the window error comes from the graphics stack (GPUI on Linux
/// draws through wgpu on Vulkan only).
#[cfg(any(test, target_os = "linux"))]
fn is_vulkan_failure(detail: &str) -> bool {
    let lower = detail.to_lowercase();
    ["vulkan", "adapter", "wgpu", "no gpu", "no_gpu", "gpu"]
        .iter()
        .any(|needle| lower.contains(needle))
}

#[cfg(target_os = "macos")]
fn show_alert(title: &str, detail: &str, _summary: &str) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSAlert, NSApplication};
    use objc2_foundation::NSString;
    // AppKit alerts need the main thread; GPUI runs foreground tasks there.
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    // A launch that exits again never started GPUI, which would have made
    // the application object and brought it forward.
    NSApplication::sharedApplication(mtm).activate();
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(title));
    alert.setInformativeText(&NSString::from_str(detail));
    alert.runModal();
}

#[cfg(target_os = "windows")]
fn show_alert(title: &str, detail: &str, _summary: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MessageBoxW,
    };
    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain(std::iter::once(0)).collect() };
    let text = wide(&format!("{title}.\n\n{detail}"));
    let caption = wide(tr(keys::Q_MISC_QUILL));
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
/// helper when one is installed, otherwise a desktop notification (which
/// needs no GPU). stderr already has the message too.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn show_alert(title: &str, detail: &str, summary: &str) {
    use std::process::{Command, Stdio};
    let text = format!("{title}.\n\n{detail}");
    let zenity = Command::new("zenity")
        .args([
            "--error",
            "--no-markup",
            tr(keys::Q_MISC_TITLE_QUILL),
            "--text",
        ])
        .arg(&text)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    // A dialog that could not reach the display exits non-zero (as does one
    // closed from its title bar): fall through to the notification.
    if zenity.is_ok_and(|status| status.success()) {
        return;
    }
    let kdialog = Command::new("kdialog")
        .args(["--title", tr(keys::Q_MISC_QUILL), "--error"])
        .arg(&text)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if kdialog.is_ok_and(|status| status.success()) {
        return;
    }
    #[cfg(target_os = "linux")]
    if let Err(error) = notify(title, summary) {
        eprintln!("quill: could not show a notification either: {error}");
    }
}

/// A critical desktop notification that stays until dismissed.
#[cfg(target_os = "linux")]
fn notify(title: &str, detail: &str) -> zbus::Result<()> {
    use quill::notify::dbus::{APP_NAME, BUS_NAME, DESKTOP_ENTRY, INTERFACE, OBJECT_PATH};
    use std::collections::HashMap;
    use zbus::zvariant::Value;
    const CRITICAL: u8 = 2;
    let connection = zbus::blocking::Connection::session()?;
    let proxy = zbus::blocking::Proxy::new(&connection, BUS_NAME, OBJECT_PATH, INTERFACE)?;
    let hints: HashMap<&str, Value<'_>> = HashMap::from([
        ("urgency", Value::from(CRITICAL)),
        ("desktop-entry", Value::from(DESKTOP_ENTRY)),
    ]);
    // Daemons with body markup read `&`, `<` and `>` as markup.
    let body = detail
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let _id: u32 = proxy.call(
        // The D-Bus method name, never translated.
        "Notify",
        &(
            APP_NAME,
            0_u32,
            "dialog-error",
            title,
            body,
            Vec::<&str>::new(),
            hints,
            0_i32,
        ),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::is_vulkan_failure;

    #[test]
    fn graphics_errors_get_the_vulkan_hint() {
        assert!(is_vulkan_failure(
            "Unable to find a suitable Vulkan adapter"
        ));
        assert!(is_vulkan_failure("NoSupportedDeviceFound: wgpu"));
        assert!(is_vulkan_failure("failed to create GPU context"));
        assert!(!is_vulkan_failure("display connection refused"));
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "quit_flush_tests.rs"]
mod quit_flush_tests;
