//! WebView2's browser process outlives the helper for a moment and holds
//! files in the carrier's data folder, so Quill could not remove the
//! folder (cookies the page set included) when the carrier closed. The
//! helper waits for that process, a bounded time, before it exits.

use std::time::{Duration, Instant};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject};
use wry::{WebView, WebViewExtWindows};

/// How long the helper waits; Quill kills it later than this
/// (`CLOSE_GRACE` in `src/proxy_carrier/helper.rs`).
pub(super) const WAIT: Duration = Duration::from_secs(2);

pub(super) struct BrowserProcess(HANDLE);

impl BrowserProcess {
    /// The browser process behind `webview`, when WebView2 names one.
    pub(super) fn of(webview: &WebView) -> Option<Self> {
        let mut pid = 0u32;
        // SAFETY: plain COM and Win32 calls on the live web view, on its
        // own thread; the handle is closed in `Drop`.
        unsafe {
            webview.webview().BrowserProcessId(&mut pid).ok()?;
            if pid == 0 {
                return None;
            }
            OpenProcess(PROCESS_SYNCHRONIZE, false, pid).ok().map(Self)
        }
    }

    /// Wait until the process is gone, at most `WAIT`. Call it once the
    /// web view is dropped.
    pub(super) fn wait(self) {
        let started = Instant::now();
        let millis = u32::try_from(WAIT.as_millis()).unwrap_or(u32::MAX);
        // SAFETY: the handle is open until `Drop`.
        let result = unsafe { WaitForSingleObject(self.0, millis) };
        crate::debug(|| {
            format!(
                "browser process wait ended after {:?} ({:#x})",
                started.elapsed(),
                result.0
            )
        });
    }
}

impl Drop for BrowserProcess {
    fn drop(&mut self) {
        // SAFETY: opened in `of`, closed only here.
        let _ = unsafe { CloseHandle(self.0) };
    }
}
