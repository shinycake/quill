//! A child process installs the native crash handler and then crashes; the
//! parent reads the report it left. The test binary is its own child: the
//! `QUILL_NATIVE_CRASH_DIR` variable tells it to crash instead of test, and
//! `QUILL_NATIVE_CRASH_MODE` says how.
#![allow(
    clippy::unwrap_used,
    reason = "integration test code is exempt (docs/contributing/panic-safety.md)"
)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const CHILD_DIR_VAR: &str = "QUILL_NATIVE_CRASH_DIR";
const CHILD_MODE_VAR: &str = "QUILL_NATIVE_CRASH_MODE";
/// A crashing child that has not died by now is hung: kill it.
const CHILD_DEADLINE: Duration = Duration::from_secs(30);

/// Runs in the child when the variables are set; returns normally otherwise.
fn run_as_child(mode: &str) {
    let Some(dir) = std::env::var_os(CHILD_DIR_VAR) else {
        return;
    };
    if std::env::var(CHILD_MODE_VAR).as_deref() != Ok(mode) {
        return;
    }
    silence_the_os_crash_reporter();
    child_before_install(mode);
    quill::native_crash::install(&PathBuf::from(dir));
    crash(mode);
    // Not reached when the crash works; abort so a missing crash still
    // makes the parent's assertions fail.
    std::process::abort();
}

/// Start the test binary again as the crashing child, wait for it (killing
/// it after [`CHILD_DEADLINE`]) and return what it printed.
fn spawn_child(test: &str, mode: &str, dir: &Path) -> Output {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--test-threads=1", "--nocapture"])
        .env(CHILD_DIR_VAR, dir)
        .env(CHILD_MODE_VAR, mode)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let started = Instant::now();
    while child.try_wait().unwrap().is_none() {
        if started.elapsed() > CHILD_DEADLINE {
            let _ = child.kill();
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        started.elapsed() <= CHILD_DEADLINE + Duration::from_secs(5),
        "the crashing child hung"
    );
    output
}

fn scratch_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("quill-native-crash-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn read_report(dir: &Path) -> String {
    std::fs::read_to_string(dir.join(quill::native_crash::ACTIVE_REPORT_FILE)).unwrap()
}

#[test]
fn a_native_fault_leaves_a_report() {
    run_as_child("raise");
    let dir = scratch_dir("raise");
    let output = spawn_child("a_native_fault_leaves_a_report", "raise", &dir);
    assert!(!output.status.success(), "the child must die");
    assert_died_of_the_crash(&output.status);
    let report = read_report(&dir);
    assert_report(&report);
    assert!(report.contains("end of report"), "{report}");
    #[cfg(windows)]
    assert!(
        dir.join(quill::native_crash::ACTIVE_DUMP_FILE)
            .metadata()
            .unwrap()
            .len()
            > 0
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A real invalid access (not a `raise`): the handler returns, the access
/// repeats under the default action and the process dies of SIGSEGV with
/// the address kept in the report.
#[cfg(unix)]
#[test]
fn a_real_fault_dies_of_the_original_signal() {
    run_as_child("fault");
    let dir = scratch_dir("fault");
    let output = spawn_child("a_real_fault_dies_of_the_original_signal", "fault", &dir);
    assert_died_of_the_crash(&output.status);
    let report = read_report(&dir);
    assert!(report.contains("SIGSEGV"), "{report}");
    assert!(report.contains("fault address: 0x10\n"), "{report}");
    assert!(report.contains("end of report"), "{report}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Two threads fault at once: the second must not kill the process before
/// the first one's report is complete.
#[cfg(unix)]
#[test]
fn a_second_faulting_thread_does_not_cut_the_report_off() {
    run_as_child("two-threads");
    let dir = scratch_dir("two-threads");
    let output = spawn_child(
        "a_second_faulting_thread_does_not_cut_the_report_off",
        "two-threads",
        &dir,
    );
    assert_died_of_the_crash(&output.status);
    let report = read_report(&dir);
    assert!(report.contains("backtrace:"), "{report}");
    assert!(report.contains("end of report"), "{report}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The handler that was there before ours still runs (Rust's own is one: it
/// prints the stack-overflow message).
#[cfg(unix)]
#[test]
fn the_previous_handler_still_runs() {
    run_as_child("chain");
    let dir = scratch_dir("chain");
    let output = spawn_child("the_previous_handler_still_runs", "chain", &dir);
    assert_died_of_the_crash(&output.status);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("previous handler ran"), "{stderr}");
    assert!(read_report(&dir).contains("SIGSEGV"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
fn child_before_install(mode: &str) {
    // No core file from the deliberate crashes.
    let none = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: `none` is a valid rlimit; lowering the core limit is harmless.
    unsafe { libc::setrlimit(libc::RLIMIT_CORE, &none) };
    if mode == "chain" {
        extern "C" fn previous(_signal: libc::c_int) {
            let text = b"previous handler ran\n";
            // SAFETY: write(2) of a static buffer to stderr.
            unsafe { libc::write(2, text.as_ptr().cast(), text.len()) };
        }
        // SAFETY: installs a plain one-argument handler for SIGSEGV; ours
        // goes in after it and must call it.
        unsafe {
            libc::signal(
                libc::SIGSEGV,
                previous as extern "C" fn(libc::c_int) as usize,
            )
        };
    }
}

#[cfg(unix)]
fn silence_the_os_crash_reporter() {}

#[cfg(unix)]
fn crash(mode: &str) {
    match mode {
        "fault" => touch_bad_address(),
        "two-threads" => {
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
            let other = {
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    touch_bad_address();
                })
            };
            barrier.wait();
            touch_bad_address();
            let _ = other.join();
        }
        // SAFETY: raising a signal has no memory-safety preconditions; the
        // point is to hit the installed handler.
        _ => unsafe {
            libc::raise(libc::SIGSEGV);
        },
    }
}

#[cfg(unix)]
fn touch_bad_address() {
    // SAFETY: deliberately an invalid write, to fault in the child process.
    unsafe { std::ptr::write_volatile(0x10 as *mut u32, 1) };
}

#[cfg(unix)]
fn assert_died_of_the_crash(status: &std::process::ExitStatus) {
    use std::os::unix::process::ExitStatusExt;
    // The handler restores the default action: the OS still sees SIGSEGV.
    assert_eq!(status.signal(), Some(libc::SIGSEGV));
}

#[cfg(unix)]
fn assert_report(report: &str) {
    assert!(report.contains("SIGSEGV"), "{report}");
    assert!(report.contains("backtrace:"), "{report}");
    assert!(report.contains("Quill "), "{report}");
}

#[cfg(windows)]
fn child_before_install(_mode: &str) {}

#[cfg(windows)]
fn silence_the_os_crash_reporter() {
    use windows_sys::Win32::System::Diagnostics::Debug::{
        SEM_FAILCRITICALERRORS, SEM_NOGPFAULTERRORBOX, SetErrorMode,
    };
    // No Windows Error Reporting dialog from the deliberate crash.
    // SAFETY: process-wide error mode flags; no pointers.
    unsafe { SetErrorMode(SEM_NOGPFAULTERRORBOX | SEM_FAILCRITICALERRORS) };
}

#[cfg(windows)]
fn crash(_mode: &str) {
    // SAFETY: deliberately an access violation, to reach the exception
    // filter in the child process.
    unsafe { std::ptr::write_volatile(std::ptr::null_mut::<u32>(), 1) };
}

#[cfg(windows)]
fn assert_died_of_the_crash(_status: &std::process::ExitStatus) {}

#[cfg(windows)]
fn assert_report(report: &str) {
    assert!(report.contains("0xc0000005"), "{report}");
    assert!(report.contains("exception address"), "{report}");
}
