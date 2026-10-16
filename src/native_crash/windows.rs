//! An unhandled-exception filter: exception code, address and a minidump.
//!
//! Unlike a Unix signal handler this runs on the faulting thread in a normal
//! context, so `std::fs` is used; failures are ignored and the previous
//! filter (or the default, which lets Windows Error Reporting run) is
//! always called afterwards. The filter cannot panic out: its body is under
//! `catch_unwind`.

use super::{ACTIVE_DUMP_FILE, ACTIVE_REPORT_FILE};
use std::io::Write;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use windows_sys::Win32::System::Diagnostics::Debug::{
    EXCEPTION_POINTERS, LPTOP_LEVEL_EXCEPTION_FILTER, MINIDUMP_EXCEPTION_INFORMATION,
    MiniDumpNormal, MiniDumpWriteDump, SetUnhandledExceptionFilter,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId,
};

/// `EXCEPTION_CONTINUE_SEARCH`: let the system handle the exception.
const CONTINUE_SEARCH: i32 = 0;

static PATHS: OnceLock<(PathBuf, PathBuf)> = OnceLock::new();
static PREVIOUS: OnceLock<LPTOP_LEVEL_EXCEPTION_FILTER> = OnceLock::new();

pub(super) fn install(dir: &Path) {
    if PATHS
        .set((dir.join(ACTIVE_REPORT_FILE), dir.join(ACTIVE_DUMP_FILE)))
        .is_err()
    {
        return;
    }
    // SAFETY: `on_exception` has the filter signature and stays valid for
    // the life of the process.
    let previous = unsafe { SetUnhandledExceptionFilter(Some(on_exception)) };
    let _ = PREVIOUS.set(previous);
}

unsafe extern "system" fn on_exception(info: *const EXCEPTION_POINTERS) -> i32 {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| write_crash(info)));
    match PREVIOUS.get().copied().flatten() {
        // SAFETY: the previous filter was registered with the system under
        // the same contract, and gets the same pointer.
        Some(previous) => unsafe { previous(info) },
        None => CONTINUE_SEARCH,
    }
}

fn write_crash(info: *const EXCEPTION_POINTERS) {
    let Some((text_path, dump_path)) = PATHS.get() else {
        return;
    };
    // SAFETY: the system passes a valid `EXCEPTION_POINTERS` (or null,
    // which `as_ref` handles); the record is read, never written.
    let record = unsafe { info.as_ref().and_then(|info| info.ExceptionRecord.as_ref()) };
    let (code, address) = record
        .map(|record| {
            (
                record.ExceptionCode as u32,
                record.ExceptionAddress as usize,
            )
        })
        .unwrap_or((0, 0));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let report = format!(
        "Quill closed because of a native crash.\n\
         exception code: {code:#010x}\n\
         exception address: {address:#x}\n\
         time: {now} (unix seconds)\n\
         Quill {version}\n\
         os: {os} ({arch})\n\
         pid: {pid}\n\
         started: {started} (unix seconds)\n\
         minidump: the .dmp file with the same name as this report\n",
        version = crate::version::APP,
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        // SAFETY: no arguments, no preconditions.
        pid = unsafe { GetCurrentProcessId() },
        started = crate::crash_log::session_start(),
    );
    if let Ok(mut file) = std::fs::File::create(text_path) {
        let _ = file.write_all(report.as_bytes());
    }
    write_minidump(dump_path, info);
}

fn write_minidump(path: &Path, info: *const EXCEPTION_POINTERS) {
    let Ok(file) = std::fs::File::create(path) else {
        return;
    };
    let exception = MINIDUMP_EXCEPTION_INFORMATION {
        // SAFETY: no arguments, no preconditions.
        ThreadId: unsafe { GetCurrentThreadId() },
        ExceptionPointers: info.cast_mut(),
        ClientPointers: 0,
    };
    // SAFETY: the process pseudo-handle and the open file handle are valid
    // for the call; `exception` lives across it and the user-stream and
    // callback parameters are allowed to be null.
    unsafe {
        MiniDumpWriteDump(
            GetCurrentProcess(),
            GetCurrentProcessId(),
            file.as_raw_handle(),
            MiniDumpNormal,
            &exception,
            std::ptr::null(),
            std::ptr::null(),
        );
    }
}
