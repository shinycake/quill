//! An unhandled-exception filter: exception code, address and a minidump.
//!
//! The filter runs on the faulting thread, possibly with almost no stack
//! left (a stack overflow), so it does nothing that allocates, opens files
//! or formats text. At install time the report and dump files are opened
//! and a dump thread is started; the filter only hands it the exception
//! pointers (`SetEvent`), waits for it to finish (`WaitForSingleObject`,
//! bounded) and then calls the previous filter, or returns
//! `EXCEPTION_CONTINUE_SEARCH` so Windows Error Reporting still runs. The
//! dump thread writes the text report and the minidump of the blocked
//! faulting thread, which is the documented way to dump a process from
//! inside the exception filter.
//!
//! A stack overflow still leaves the filter only a few KiB: Rust reserves a
//! guaranteed stack region on the main thread for exactly this, and the
//! filter needs far less than that. If the dump thread itself faults, or
//! takes longer than [`DUMP_WAIT_MS`], the process goes on to die without a
//! report.
//!
//! The files stay open until the process ends; Rust opens them with
//! `FILE_SHARE_DELETE`, so a clean quit can still delete them
//! (`discard_active`).

use super::{ACTIVE_DUMP_FILE, ACTIVE_REPORT_FILE};
use std::fs::File;
use std::io::Write;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::ptr;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicUsize, Ordering};
use windows_sys::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Diagnostics::Debug::{
    EXCEPTION_POINTERS, LPTOP_LEVEL_EXCEPTION_FILTER, MINIDUMP_EXCEPTION_INFORMATION,
    MiniDumpNormal, MiniDumpWriteDump, SetUnhandledExceptionFilter,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId, INFINITE, SetEvent,
    WaitForSingleObject,
};

/// `EXCEPTION_CONTINUE_SEARCH`: let the system handle the exception.
const CONTINUE_SEARCH: i32 = 0;
/// How long the filter waits for the dump thread.
const DUMP_WAIT_MS: u32 = 15_000;

static PREVIOUS: OnceLock<LPTOP_LEVEL_EXCEPTION_FILTER> = OnceLock::new();
/// The exception being dumped, and the thread it happened on.
static EXCEPTION: AtomicPtr<EXCEPTION_POINTERS> = AtomicPtr::new(ptr::null_mut());
static FAULT_THREAD: AtomicU32 = AtomicU32::new(0);
/// Event handles (as integers), set by `install` before the filter exists.
static REQUEST_EVENT: AtomicUsize = AtomicUsize::new(0);
static DONE_EVENT: AtomicUsize = AtomicUsize::new(0);
static IN_FILTER: AtomicBool = AtomicBool::new(false);
static INSTALLED: AtomicBool = AtomicBool::new(false);

pub(super) fn install(dir: &Path) {
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let (Ok(report), Ok(dump)) = (
        File::create(dir.join(ACTIVE_REPORT_FILE)),
        File::create(dir.join(ACTIVE_DUMP_FILE)),
    ) else {
        return;
    };
    // SAFETY: null attributes and name are allowed; an auto-reset request
    // event and a manual-reset done event, both initially unsignalled.
    let (request, done) = unsafe {
        (
            CreateEventW(ptr::null(), 0, 0, ptr::null()),
            CreateEventW(ptr::null(), 1, 0, ptr::null()),
        )
    };
    if request.is_null() || done.is_null() {
        return;
    }
    REQUEST_EVENT.store(request as usize, Ordering::SeqCst);
    DONE_EVENT.store(done as usize, Ordering::SeqCst);
    // Resolved here so the dump thread never has to initialise anything.
    let _ = crate::crash_log::session_start();
    let started = std::thread::Builder::new()
        .name("native-crash-dump".to_owned())
        .spawn(move || dump_thread(report, dump));
    if started.is_err() {
        return;
    }
    // SAFETY: `on_exception` has the filter signature and stays valid for
    // the life of the process.
    let previous = unsafe { SetUnhandledExceptionFilter(Some(on_exception)) };
    let _ = PREVIOUS.set(previous);
}

/// Allocation-free: atomics, `SetEvent`, `WaitForSingleObject`.
unsafe extern "system" fn on_exception(info: *const EXCEPTION_POINTERS) -> i32 {
    let request = REQUEST_EVENT.load(Ordering::SeqCst) as HANDLE;
    let done = DONE_EVENT.load(Ordering::SeqCst) as HANDLE;
    if !request.is_null() && !done.is_null() {
        if IN_FILTER.swap(true, Ordering::SeqCst) {
            // Another thread is being dumped; let it finish.
            // SAFETY: `done` is a live event handle for the whole process.
            unsafe { WaitForSingleObject(done, DUMP_WAIT_MS) };
        } else {
            EXCEPTION.store(info.cast_mut(), Ordering::SeqCst);
            // SAFETY: no arguments, no preconditions.
            FAULT_THREAD.store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);
            // SAFETY: both are live event handles for the whole process.
            unsafe {
                SetEvent(request);
                WaitForSingleObject(done, DUMP_WAIT_MS);
            }
        }
    }
    match PREVIOUS.get().copied().flatten() {
        // SAFETY: the previous filter was registered with the system under
        // the same contract, and gets the same pointer.
        Some(previous) => unsafe { previous(info) },
        None => CONTINUE_SEARCH,
    }
}

/// Waits for the filter's request, then writes both files.
fn dump_thread(mut report: File, dump: File) {
    let request = REQUEST_EVENT.load(Ordering::SeqCst) as HANDLE;
    let done = DONE_EVENT.load(Ordering::SeqCst) as HANDLE;
    // SAFETY: `request` is a live event handle for the whole process.
    if unsafe { WaitForSingleObject(request, INFINITE) } != WAIT_OBJECT_0 {
        return;
    }
    let info = EXCEPTION.load(Ordering::SeqCst).cast_const();
    let _ = report.write_all(report_text(info).as_bytes());
    let _ = report.flush();
    write_minidump(&dump, info);
    // SAFETY: `done` is a live event handle for the whole process.
    unsafe { SetEvent(done) };
}

fn report_text(info: *const EXCEPTION_POINTERS) -> String {
    // SAFETY: the faulting thread is blocked inside the filter, so the
    // system's `EXCEPTION_POINTERS` (or null, which `as_ref` handles) stays
    // valid; the record is read, never written.
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
    format!(
        "Quill closed because of a native crash.\n\
         exception code: {code:#010x}\n\
         exception address: {address:#x}\n\
         time: {now} (unix seconds)\n\
         Quill {version}\n\
         os: {os} ({arch})\n\
         pid: {pid}\n\
         started: {started} (unix seconds)\n\
         minidump: the .dmp file with the same name as this report\n\
         \n\
         end of report\n",
        version = crate::version::APP,
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        // SAFETY: no arguments, no preconditions.
        pid = unsafe { GetCurrentProcessId() },
        started = crate::crash_log::session_start(),
    )
}

fn write_minidump(file: &File, info: *const EXCEPTION_POINTERS) {
    let exception = MINIDUMP_EXCEPTION_INFORMATION {
        ThreadId: FAULT_THREAD.load(Ordering::SeqCst),
        ExceptionPointers: info.cast_mut(),
        // The pointers are in this process's own address space.
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
            ptr::null(),
            ptr::null(),
        );
    }
}
