//! WebView2's browser process outlives the helper for a moment and holds
//! files in the carrier's data folder, so Quill could not remove the
//! folder (cookies the page set included) when the carrier closed. The
//! helper waits for that process, a bounded time, before it exits.

use quill_webview_protocol::carrier::BROWSER_EXIT_WAIT;
use std::time::Instant;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject};
use wry::{WebView, WebViewExtWindows};

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

    /// Wait until the process is gone, at most [`BROWSER_EXIT_WAIT`]
    /// (Quill kills the helper later than that). Call it once the web
    /// view and its environment are dropped.
    pub(super) fn wait(self) {
        let started = Instant::now();
        let millis = u32::try_from(BROWSER_EXIT_WAIT.as_millis()).unwrap_or(u32::MAX);
        // SAFETY: the handle is open until `Drop`.
        let result = unsafe { WaitForSingleObject(self.0, millis) };
        crate::debug(|| {
            let outcome = if result == WAIT_OBJECT_0 {
                "exited"
            } else {
                "still running"
            };
            format!(
                "browser process {outcome} after {:?} (wait result {:#x})",
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
