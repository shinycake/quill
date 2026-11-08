//! The faulting thread's frames for the text report, as module and offset
//! (`quill.exe+0x1a2b3c`), so a report symbolicates against that release's
//! build even when the minidump is not sent along.
//!
//! Runs on the dump thread while the faulting thread waits in the filter,
//! so it may allocate and call DbgHelp (which is not thread-safe: nothing
//! else in Quill calls it).

use std::fmt::Write as _;
use std::ptr;
use windows_sys::Win32::Foundation::{CloseHandle, HMODULE};
use windows_sys::Win32::System::Diagnostics::Debug::{
    AddrModeFlat, CONTEXT, EXCEPTION_POINTERS, STACKFRAME64, StackWalk64, SymFunctionTableAccess64,
    SymGetModuleBase64, SymInitialize,
};
use windows_sys::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    GetModuleFileNameW, GetModuleHandleExW,
};
use windows_sys::Win32::System::SystemInformation::IMAGE_FILE_MACHINE_AMD64;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenThread, THREAD_GET_CONTEXT, THREAD_QUERY_INFORMATION,
};

const MAX_FRAMES: usize = 64;

/// One line per frame, starting at the faulting instruction; empty when
/// the context is missing.
pub(super) fn frames(info: *const EXCEPTION_POINTERS, thread_id: u32) -> String {
    // SAFETY: the faulting thread is blocked in the filter, so its
    // `EXCEPTION_POINTERS` and context record stay valid; the context is
    // copied, since the walk rewrites it.
    let Some(mut context) = (unsafe {
        info.as_ref()
            .and_then(|info| info.ContextRecord.as_ref())
            .copied()
    }) else {
        return String::new();
    };
    let mut frame = first_frame(&context);
    // SAFETY: plain calls on this process; the thread handle is closed below.
    let (process, thread) = unsafe {
        let process = GetCurrentProcess();
        // Loads the modules' unwind tables lazily; no symbol path needed.
        SymInitialize(process, ptr::null(), 1);
        let thread = OpenThread(THREAD_GET_CONTEXT | THREAD_QUERY_INFORMATION, 0, thread_id);
        (process, thread)
    };
    let mut text = String::new();
    for index in 0..MAX_FRAMES {
        // SAFETY: `frame` and `context` are this thread's locals, set up
        // for a flat x64 walk; the routines are DbgHelp's own.
        let walked = unsafe {
            StackWalk64(
                u32::from(IMAGE_FILE_MACHINE_AMD64),
                process,
                thread,
                &mut frame,
                (&raw mut context).cast(),
                None,
                Some(SymFunctionTableAccess64),
                Some(SymGetModuleBase64),
                None,
            )
        };
        if walked == 0 || frame.AddrPC.Offset == 0 {
            break;
        }
        let address = frame.AddrPC.Offset;
        let _ = writeln!(text, "{index:<3} {}", describe(address));
    }
    if !thread.is_null() {
        // SAFETY: opened above, closed once.
        unsafe { CloseHandle(thread) };
    }
    text
}

fn first_frame(context: &CONTEXT) -> STACKFRAME64 {
    // SAFETY: `STACKFRAME64` is plain data; all-zero is its documented
    // starting state.
    let mut frame: STACKFRAME64 = unsafe { std::mem::zeroed() };
    frame.AddrPC.Offset = context.Rip;
    frame.AddrPC.Mode = AddrModeFlat;
    frame.AddrFrame.Offset = context.Rbp;
    frame.AddrFrame.Mode = AddrModeFlat;
    frame.AddrStack.Offset = context.Rsp;
    frame.AddrStack.Mode = AddrModeFlat;
    frame
}

/// `module+0xoffset`, or the bare address outside any module.
fn describe(address: u64) -> String {
    let mut module: HMODULE = ptr::null_mut();
    // SAFETY: with FROM_ADDRESS the name argument is an address inside the
    // module; UNCHANGED_REFCOUNT means nothing needs freeing.
    let found = unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            address as usize as *const u16,
            &mut module,
        )
    } != 0;
    if !found || module.is_null() {
        return format!("{address:#x}");
    }
    let mut name = [0u16; 260];
    // SAFETY: the buffer and its length match.
    let length = unsafe { GetModuleFileNameW(module, name.as_mut_ptr(), name.len() as u32) };
    let path = String::from_utf16_lossy(name.get(..length as usize).unwrap_or(&[]));
    let file = path.rsplit(['\\', '/']).next().unwrap_or(&path);
    let base = module as usize as u64;
    format!(
        "{file}+{offset:#x} ({address:#x})",
        offset = address.wrapping_sub(base)
    )
}
