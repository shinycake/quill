//! SIGSEGV, SIGBUS, SIGILL, SIGFPE and SIGABRT on macOS and Linux.
//!
//! Everything the handler needs is prepared at install time: the open file
//! descriptor, the header text and the alternate stack. The handler itself
//! only uses `write(2)`, `time(2)`, `nanosleep(2)`, `backtrace(3)` /
//! `backtrace_symbols_fd(3)` (warmed up once at install, so the first call
//! does not load libraries inside the handler) and `raise(3)`.
//!
//! A handler is async-signal-safe only if it does not allocate or take
//! locks, so no `format!`, no `String`, no logging below.
//!
//! Order of events in a fault: the first thread in writes the report; any
//! other thread that faults meanwhile waits (bounded) so the report is not
//! cut off by the process dying. Then the handler that was installed before
//! ours (Rust's own stack-overflow handler, which prints "has overflowed its
//! stack") is called, the default action is restored, and for a hardware
//! fault (SIGSEGV, SIGBUS, SIGILL, SIGFPE) the handler returns, so the
//! faulting instruction runs again under the default action and the OS crash
//! report keeps the real faulting frame, `si_code` and `si_addr`. A signal
//! sent by a process (`raise`, `kill`) and SIGABRT are re-raised instead.

use super::ACTIVE_REPORT_FILE;
use std::ffi::c_int;
use std::os::fd::IntoRawFd;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering};

const SIGNALS: [c_int; 5] = [
    libc::SIGSEGV,
    libc::SIGBUS,
    libc::SIGILL,
    libc::SIGFPE,
    libc::SIGABRT,
];
/// Only used for a thread that has no alternate stack of its own.
const ALT_STACK_BYTES: usize = 256 * 1024;
/// Small on purpose: the handler may run on a worker thread's small
/// alternate stack.
#[cfg(any(target_os = "macos", all(target_os = "linux", target_env = "gnu")))]
const MAX_FRAMES: usize = 64;
/// How long a second faulting thread waits for the first one's report.
const WAIT_STEP_NANOS: libc::c_long = 50_000_000;
const WAIT_STEPS: u32 = 40;

static CRASH_FD: AtomicI32 = AtomicI32::new(-1);
static IN_HANDLER: AtomicBool = AtomicBool::new(false);
static REPORT_DONE: AtomicBool = AtomicBool::new(false);
static INSTALLED: AtomicBool = AtomicBool::new(false);
/// The `sa_sigaction` value and `sa_flags` each signal had before ours, in
/// `SIGNALS` order; zero is `SIG_DFL`.
static OLD_HANDLER: [AtomicUsize; SIGNALS.len()] = [const { AtomicUsize::new(0) }; SIGNALS.len()];
static OLD_FLAGS: [AtomicI32; SIGNALS.len()] = [const { AtomicI32::new(0) }; SIGNALS.len()];
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
    ensure_alt_stack();
    for (index, signal) in SIGNALS.into_iter().enumerate() {
        install_one(index, signal);
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

/// Keep the alternate stack the thread already has: Rust's runtime sets one
/// up on the main thread, with a guard page below it, for its own
/// stack-overflow handler. Ours (leaked, no guard page) is only for a thread
/// that has none.
fn ensure_alt_stack() {
    // SAFETY: `stack_t` is plain data; all-zero is a valid value.
    let mut current: libc::stack_t = unsafe { std::mem::zeroed() };
    // SAFETY: a null new stack only queries; `current` is writable.
    let queried = unsafe { libc::sigaltstack(std::ptr::null(), &mut current) } == 0;
    if queried && current.ss_flags & libc::SS_DISABLE == 0 {
        return;
    }
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

fn install_one(index: usize, signal: c_int) {
    // SAFETY: `sigaction` is plain data; all-zero is a valid empty value
    // (no flags, empty mask) that the fields below then fill in.
    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
    action.sa_sigaction = on_signal as Handler as usize;
    // No RESETHAND and no NODEFER: the handler restores the default action
    // itself once the report is written, so a second thread that faults
    // meanwhile still reaches it (and waits), and a fault inside the handler,
    // with its signal blocked, ends the process.
    action.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
    // SAFETY: `action.sa_mask` is a valid sigset_t owned by this frame.
    unsafe { libc::sigemptyset(&mut action.sa_mask) };
    // SAFETY: `sigaction` is plain data; all-zero is a valid value.
    let mut old: libc::sigaction = unsafe { std::mem::zeroed() };
    // SAFETY: `action` is fully initialised and the handler has the
    // `SA_SIGINFO` signature; `old` is writable.
    let installed = unsafe { libc::sigaction(signal, &action, &mut old) } == 0;
    if installed
        && let (Some(handler), Some(flags)) = (OLD_HANDLER.get(index), OLD_FLAGS.get(index))
    {
        handler.store(old.sa_sigaction, Ordering::SeqCst);
        flags.store(old.sa_flags as i32, Ordering::SeqCst);
    }
}

// Cannot unwind: it has no panicking operations (no indexing, no unwrap, no
// allocation), and an unwind out of `extern "C"` aborts.
extern "C" fn on_signal(signal: c_int, info: *mut libc::siginfo_t, context: *mut libc::c_void) {
    if IN_HANDLER.swap(true, Ordering::SeqCst) {
        // Another thread is writing the report; dying now would cut it off.
        wait_for_report();
    } else {
        let fd = CRASH_FD.load(Ordering::SeqCst);
        if fd >= 0 {
            write_report(fd, signal, fault_address(info));
        }
        REPORT_DONE.store(true, Ordering::SeqCst);
    }
    chain_to_previous(signal, info, context);
    // SAFETY: restoring the default action is a plain libc call.
    unsafe { libc::signal(signal, libc::SIG_DFL) };
    if signal == libc::SIGABRT || sent_by_a_process(info) {
        // SAFETY: re-raising is a plain libc call; it stays pending while
        // this handler runs and is delivered, now with the default action,
        // when it returns.
        unsafe { libc::raise(signal) };
    }
    // A hardware fault returns here: the instruction faults again and the
    // default action (and the OS crash reporter) sees the real fault.
}

fn wait_for_report() {
    let pause = libc::timespec {
        tv_sec: 0,
        tv_nsec: WAIT_STEP_NANOS,
    };
    for _ in 0..WAIT_STEPS {
        if REPORT_DONE.load(Ordering::SeqCst) {
            return;
        }
        // SAFETY: `pause` is a valid timespec; the remainder is not wanted.
        unsafe { libc::nanosleep(&pause, std::ptr::null_mut()) };
    }
}

/// Call the handler this signal had before ours, if it was a function: for
/// SIGSEGV and SIGBUS that is Rust's, which prints the stack-overflow
/// message when the fault hit a guard page and restores the default action.
fn chain_to_previous(signal: c_int, info: *mut libc::siginfo_t, context: *mut libc::c_void) {
    let Some(index) = SIGNALS.iter().position(|known| *known == signal) else {
        return;
    };
    let handler = OLD_HANDLER
        .get(index)
        .map_or(0, |slot| slot.load(Ordering::SeqCst));
    let flags = OLD_FLAGS
        .get(index)
        .map_or(0, |slot| slot.load(Ordering::SeqCst));
    // SIG_DFL, SIG_IGN and SIG_ERR are 0, 1 and -1.
    if handler == 0 || handler == 1 || handler == usize::MAX {
        return;
    }
    if flags & libc::SA_SIGINFO != 0 {
        // SAFETY: the previous action was registered with SA_SIGINFO, so
        // its address is a function with the three-argument signature.
        let previous = unsafe { std::mem::transmute::<usize, Handler>(handler) };
        previous(signal, info, context);
    } else {
        // SAFETY: without SA_SIGINFO the address is a one-argument handler.
        let previous = unsafe { std::mem::transmute::<usize, extern "C" fn(c_int)>(handler) };
        previous(signal);
    }
}

/// True for a signal sent with `raise`, `kill` or `sigqueue`, which does not
/// come back by itself when the handler returns.
fn sent_by_a_process(info: *mut libc::siginfo_t) -> bool {
    // SAFETY: the kernel passes a valid `siginfo_t` to an `SA_SIGINFO`
    // handler; null is checked by `as_ref`.
    let code = unsafe { info.as_ref().map(|info| info.si_code) };
    match code {
        None => true,
        // Linux: SI_USER is 0, SI_QUEUE -1, SI_TKILL -6: all <= 0.
        #[cfg(any(target_os = "linux", target_os = "android"))]
        Some(code) => code <= 0,
        // macOS: SI_USER and SI_QUEUE are 0x10001 and 0x10002, but `raise`
        // (pthread_kill) reports a hardware-looking code (SEGV_ACCERR) with a
        // zero `si_addr`. A real fault at address 0 is also taken for a sent
        // signal: it is re-raised, the process still dies of the signal.
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        Some(code) => code <= 0 || code == 0x10001 || code == 0x10002 || fault_address(info) == 0,
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
    write_all(fd, b"\nend of report\n");
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
