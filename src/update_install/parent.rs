//! Waiting for the app that handed an update over to exit.
#![deny(clippy::indexing_slicing)]

use super::claim;
use std::time::{Duration, Instant};

/// `ERROR_INVALID_PARAMETER`: OpenProcess says no process has that id, so
/// the parent is gone. Any other failure (access denied, an elevated or
/// other-session parent) leaves the parent running.
#[cfg(any(windows, test))]
pub(super) fn open_failure_means_gone(code: u32) -> bool {
    code == 87
}

/// Whether process `pid` still exists. `EPERM` means it exists but belongs
/// to someone else.
#[cfg(unix)]
pub(super) fn unix_process_alive(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    // SAFETY: signal 0 only checks that the process exists and can be
    // signalled; no signal is delivered.
    let result = unsafe { libc::kill(pid, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Blocks until the launch that handed `claim` over has exited, by its lock
/// on the claim. `false` where the file system has no locks: the caller
/// waits on the process id instead ([`wait_for_parent`]).
pub(super) fn wait_for_launch(claim: &mut claim::Claim) -> bool {
    loop {
        match claim.poll_launch() {
            claim::Launch::Gone => return true,
            claim::Launch::Unlockable => return false,
            claim::Launch::Holds => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

/// Blocks until process `pid` has exited (30 s at most).
pub(super) fn wait_for_parent(pid: u32) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let deadline = Instant::now() + Duration::from_secs(30);
        while unix_process_alive(pid) {
            if Instant::now() >= deadline {
                return Err(std::io::Error::other(
                    "App did not exit; update was not installed",
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, WAIT_OBJECT_0};
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
        };
        let deadline = Instant::now() + Duration::from_secs(30);
        let handle = loop {
            // SAFETY: OpenProcess takes plain integers and returns an owned
            // handle, or null with the failure in the thread's last error.
            let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
            if !handle.is_null() {
                break handle;
            }
            // SAFETY: reads the calling thread's last-error value.
            let code = unsafe { GetLastError() };
            if open_failure_means_gone(code) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(std::io::Error::other(
                    "App did not exit; update was not installed",
                ));
            }
            std::thread::sleep(Duration::from_millis(200));
        };
        let remaining = deadline.saturating_duration_since(Instant::now());
        let millis = u32::try_from(remaining.as_millis()).unwrap_or(30_000);
        // SAFETY: `handle` is a valid process handle with SYNCHRONIZE access.
        let status = unsafe { WaitForSingleObject(handle, millis) };
        // SAFETY: closes the handle opened above, once.
        unsafe { CloseHandle(handle) };
        if status == WAIT_OBJECT_0 {
            Ok(())
        } else {
            Err(std::io::Error::other(
                "App did not exit; update was not installed",
            ))
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        Err(std::io::Error::other(
            "Updates are unavailable on this platform",
        ))
    }
}
