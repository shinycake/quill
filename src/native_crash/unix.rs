//! SIGSEGV, SIGBUS, SIGILL, SIGFPE and SIGABRT on macOS and Linux.
//!
//! Everything the handler needs is prepared at install time: the open file
//! descriptor, the header text and the alternate stack. The handler itself
//! only uses `write(2)`, `time(2)`, `backtrace(3)` /
//! `backtrace_symbols_fd(3)` (warmed up once at install, so the first call
//! does not load libraries inside the handler) and `raise(3)`.
//!
//! A handler is async-signal-safe only if it does not allocate or take
//! locks, so no `format!`, no `String`, no logging below.

use super::ACTIVE_REPORT_FILE;
use std::ffi::c_int;
use std::os::fd::IntoRawFd;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

const SIGNALS: [c_int; 5] = [
    libc::SIGSEGV,
    libc::SIGBUS,
    libc::SIGILL,
    libc::SIGFPE,
    libc::SIGABRT,
];
const ALT_STACK_BYTES: usize = 256 * 1024;
#[cfg(any(target_os = "macos", all(target_os = "linux", target_env = "gnu")))]
const MAX_FRAMES: usize = 128;

static CRASH_FD: AtomicI32 = AtomicI32::new(-1);
static IN_HANDLER: AtomicBool = AtomicBool::new(false);
static INSTALLED: AtomicBool = AtomicBool::new(false);
/// Text written after the signal line: version, OS, pid.
static HEADER: OnceLock<Box<[u8]>> = OnceLock::new();

pub(super) fn install(dir: &Path) {
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let Ok(file) = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(dir.join(ACTIVE_REPORT_FILE))
    else {
        return;
    };
    let header = format!(
        "Quill {version}\nos: {os} ({arch})\npid: {pid}\nstarted: {started} (unix seconds)\n",
        version = crate::version::APP,
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        pid = std::process::id(),
        started = crate::crash_log::session_start(),
    );
    let _ = HEADER.set(header.into_bytes().into_boxed_slice());
    warm_up_backtrace();
    CRASH_FD.store(file.into_raw_fd(), Ordering::SeqCst);
    install_alt_stack();
    for signal in SIGNALS {
        install_one(signal);
    }
}

#[cfg(any(target_os = "macos", all(target_os = "linux", target_env = "gnu")))]
fn warm_up_backtrace() {
    let mut frames = [std::ptr::null_mut(); 4];
    // SAFETY: `frames` holds 4 pointers and the size argument says 4.
    unsafe { libc::backtrace(frames.as_mut_ptr(), 4) };
}

#[cfg(not(any(target_os = "macos", all(target_os = "linux", target_env = "gnu"))))]
fn warm_up_backtrace() {}

fn install_alt_stack() {
    // Leaked on purpose: the kernel uses it for the life of the process.
    let stack: &'static mut [u8] = Box::leak(vec![0u8; ALT_STACK_BYTES].into_boxed_slice());
    let spec = libc::stack_t {
        ss_sp: stack.as_mut_ptr().cast(),
        ss_flags: 0,
        ss_size: stack.len(),
    };
    // SAFETY: `spec` points at a leaked, writable region of `ss_size`
    // bytes that outlives the process; the old-stack pointer may be null.
    unsafe { libc::sigaltstack(&spec, std::ptr::null_mut()) };
}

type Handler = extern "C" fn(c_int, *mut libc::siginfo_t, *mut libc::c_void);

fn install_one(signal: c_int) {
    // SAFETY: `sigaction` is plain data; all-zero is a valid empty value
    // (no flags, empty mask) that the fields below then fill in.
    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
    action.sa_sigaction = on_signal as Handler as usize;
    // NODEFER lets the re-raise below be delivered from inside the handler;
    // RESETHAND makes that delivery the default action.
    action.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK | libc::SA_RESETHAND | libc::SA_NODEFER;
    // SAFETY: `action.sa_mask` is a valid sigset_t owned by this frame.
    unsafe { libc::sigemptyset(&mut action.sa_mask) };
    // SAFETY: `action` is fully initialised and the handler has the
    // `SA_SIGINFO` signature; the old action is not wanted.
    unsafe { libc::sigaction(signal, &action, std::ptr::null_mut()) };
}

// Cannot unwind: it has no panicking operations (no indexing, no unwrap, no
// allocation), and an unwind out of `extern "C"` aborts.
extern "C" fn on_signal(signal: c_int, info: *mut libc::siginfo_t, _context: *mut libc::c_void) {
    if !IN_HANDLER.swap(true, Ordering::SeqCst) {
        let fd = CRASH_FD.load(Ordering::SeqCst);
        if fd >= 0 {
            write_report(fd, signal, fault_address(info));
        }
    }
    // SAFETY: restoring the default action and re-raising are plain libc
    // calls; the OS crash reporter then sees the original signal.
    unsafe {
        libc::signal(signal, libc::SIG_DFL);
        libc::raise(signal);
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn fault_address(info: *mut libc::siginfo_t) -> u64 {
    // SAFETY: the kernel passes a valid `siginfo_t` to an `SA_SIGINFO`
    // handler; null is checked by `as_ref`.
    unsafe { info.as_ref().map(|info| info.si_addr() as u64) }.unwrap_or(0)
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn fault_address(info: *mut libc::siginfo_t) -> u64 {
    // SAFETY: the kernel passes a valid `siginfo_t` to an `SA_SIGINFO`
    // handler; null is checked by `as_ref`.
    unsafe { info.as_ref().map(|info| info.si_addr as u64) }.unwrap_or(0)
}

fn signal_name(signal: c_int) -> &'static [u8] {
    match signal {
        libc::SIGSEGV => b"SIGSEGV",
        libc::SIGBUS => b"SIGBUS",
        libc::SIGILL => b"SIGILL",
        libc::SIGFPE => b"SIGFPE",
        libc::SIGABRT => b"SIGABRT",
        _ => b"(unknown)",
    }
}

fn write_report(fd: c_int, signal: c_int, address: u64) {
    write_all(fd, b"Quill closed because of a native crash.\nsignal: ");
    write_all(fd, signal_name(signal));
    write_all(fd, b" (");
    write_decimal(fd, u64::from(signal.unsigned_abs()));
    write_all(fd, b")\nfault address: 0x");
    write_hex(fd, address);
    write_all(fd, b"\ntime: ");
    // SAFETY: `time` accepts a null pointer and is async-signal-safe.
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    write_decimal(fd, now.unsigned_abs() as u64);
    write_all(fd, b" (unix seconds)\n");
    if let Some(header) = HEADER.get() {
        write_all(fd, header);
    }
    write_all(fd, b"\nbacktrace:\n");
    write_backtrace(fd);
}

#[cfg(any(target_os = "macos", all(target_os = "linux", target_env = "gnu")))]
fn write_backtrace(fd: c_int) {
    let mut frames = [std::ptr::null_mut(); MAX_FRAMES];
    // SAFETY: `frames` holds MAX_FRAMES pointers and that is the size passed.
    let count = unsafe { libc::backtrace(frames.as_mut_ptr(), MAX_FRAMES as c_int) };
    // SAFETY: the first `count` entries were just filled in by `backtrace`;
    // the symbol writer only reads them and writes to `fd`.
    unsafe { libc::backtrace_symbols_fd(frames.as_ptr(), count, fd) };
}

#[cfg(not(any(target_os = "macos", all(target_os = "linux", target_env = "gnu"))))]
fn write_backtrace(fd: c_int) {
    write_all(fd, b"(no backtrace on this libc)\n");
}

/// `write(2)` until everything is written or the call keeps failing.
fn write_all(fd: c_int, mut bytes: &[u8]) {
    let mut failures = 0u8;
    while !bytes.is_empty() {
        // SAFETY: the pointer and length come from a live slice.
        let written = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
        match usize::try_from(written) {
            Ok(count) if count > 0 => bytes = bytes.get(count..).unwrap_or(&[]),
            _ => {
                // Interrupted or failed: a few retries, then give up.
                failures = failures.saturating_add(1);
                if failures > 3 {
                    return;
                }
            }
        }
    }
}

fn write_decimal(fd: c_int, mut value: u64) {
    let mut digits = [b'0'; 20];
    let mut used = 0usize;
    for slot in digits.iter_mut().rev() {
        *slot = b'0' + (value % 10) as u8;
        value /= 10;
        used += 1;
        if value == 0 {
            break;
        }
    }
    let start = digits.len().saturating_sub(used);
    write_all(fd, digits.get(start..).unwrap_or(&[]));
}

fn write_hex(fd: c_int, mut value: u64) {
    let mut digits = [b'0'; 16];
    let mut used = 0usize;
    for slot in digits.iter_mut().rev() {
        *slot = b"0123456789abcdef"
            .get((value & 0xf) as usize)
            .copied()
            .unwrap_or(b'0');
        value >>= 4;
        used += 1;
        if value == 0 {
            break;
        }
    }
    let start = digits.len().saturating_sub(used);
    write_all(fd, digits.get(start..).unwrap_or(&[]));
}
