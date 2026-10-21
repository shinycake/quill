//! Keeps a carrier from outliving Quill.
//!
//! The command reader runs on its own thread from before the web view is
//! built. When Quill's end of the pipe closes (Quill quit, crashed or was
//! killed) or sends something unusable, it asks the loop to exit and, if
//! the loop has not managed to within [`HANGUP_GRACE`] (its thread stuck in
//! the web view), ends the process itself. On Linux the kernel also kills
//! the helper when the thread that started it goes.

use quill_webview_protocol::carrier::{CarrierCommand, MAX_DELIVERY_BYTES, base64_len};
use quill_webview_protocol::decode;
use quill_webview_protocol::lines::{Line, read_bounded_line};
use std::time::Duration;

/// How long the event loop gets to exit on its own after Quill hangs up.
const HANGUP_GRACE: Duration = Duration::from_millis(300);

/// Read the rest of stdin, one command per line, handing each to `deliver`
/// until it returns `false` (the loop is gone). Never returns after Quill
/// hangs up: the process ends.
pub(super) fn read_commands(
    mut input: impl std::io::BufRead,
    mut deliver: impl FnMut(Option<CarrierCommand>) -> bool,
) {
    let cap = base64_len(MAX_DELIVERY_BYTES) + 1024;
    loop {
        let command = match read_bounded_line(&mut input, cap) {
            Ok(Line::Text(line)) => std::str::from_utf8(&line)
                .ok()
                .and_then(|line| decode::<CarrierCommand>(line).ok()),
            Ok(Line::TooLong) | Ok(Line::End) | Err(_) => None,
        };
        let hangup = command.is_none();
        if !deliver(command) || hangup {
            break;
        }
    }
    std::thread::sleep(HANGUP_GRACE);
    end_process();
}

/// Exit now, without running exit handlers a stuck thread may hold the
/// locks of.
#[cfg(unix)]
pub(super) fn end_process() -> ! {
    // SAFETY: `_exit` takes no pointers and never returns.
    unsafe { libc::_exit(0) }
}

/// Exit now, without running exit handlers a stuck thread may hold the
/// locks of.
#[cfg(not(unix))]
pub(super) fn end_process() -> ! {
    #[cfg(windows)]
    {
        use windows::Win32::System::Threading::{GetCurrentProcess, TerminateProcess};
        // SAFETY: the pseudo handle of the current process is always valid.
        let _ = unsafe { TerminateProcess(GetCurrentProcess(), 0) };
    }
    // Only reached if the system refused to end the process.
    std::process::abort()
}

/// Linux: die with the thread that started us, and leave at once if that
/// already happened.
pub(super) fn follow_parent() {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: `prctl(PR_SET_PDEATHSIG)` takes a signal number and no
        // pointers; `getppid` has no arguments.
        let orphaned = unsafe {
            libc::prctl(
                libc::PR_SET_PDEATHSIG,
                libc::SIGKILL as libc::c_ulong,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
            );
            libc::getppid() == 1
        };
        if orphaned {
            end_process();
        }
    }
}
