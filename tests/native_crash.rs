//! A child process installs the native crash handler and then crashes; the
//! parent reads the report it left. The test binary is its own child: the
//! `QUILL_NATIVE_CRASH_DIR` variable tells it to crash instead of test.
#![allow(
    clippy::unwrap_used,
    reason = "integration test code is exempt (docs/contributing/panic-safety.md)"
)]

use std::path::PathBuf;
use std::process::Command;

const CHILD_DIR_VAR: &str = "QUILL_NATIVE_CRASH_DIR";

#[test]
fn a_native_fault_leaves_a_report() {
    if let Some(dir) = std::env::var_os(CHILD_DIR_VAR) {
        quill::native_crash::install(&PathBuf::from(dir));
        crash_now();
        return;
    }
    let dir = std::env::temp_dir().join(format!("quill-native-crash-it-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "a_native_fault_leaves_a_report",
            "--test-threads=1",
        ])
        .env(CHILD_DIR_VAR, &dir)
        .status()
        .unwrap();
    assert!(!status.success(), "the child must die");
    assert_died_of_the_signal(&status);

    let report =
        std::fs::read_to_string(dir.join(quill::native_crash::ACTIVE_REPORT_FILE)).unwrap();
    assert_report(&report);
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

#[cfg(unix)]
fn crash_now() {
    // SAFETY: raising a signal has no memory-safety preconditions; the
    // point is to hit the installed handler.
    unsafe { libc::raise(libc::SIGSEGV) };
}

#[cfg(unix)]
fn assert_died_of_the_signal(status: &std::process::ExitStatus) {
    use std::os::unix::process::ExitStatusExt;
    // The handler re-raises with the default action: the OS still sees it.
    assert_eq!(status.signal(), Some(libc::SIGSEGV));
}

#[cfg(unix)]
fn assert_report(report: &str) {
    assert!(report.contains("SIGSEGV"), "{report}");
    assert!(report.contains("backtrace:"), "{report}");
    assert!(report.contains("Quill "), "{report}");
}

#[cfg(windows)]
fn crash_now() {
    // SAFETY: deliberately an access violation, to reach the exception
    // filter in the child process.
    unsafe { std::ptr::write_volatile(std::ptr::null_mut::<u32>(), 1) };
}

#[cfg(windows)]
fn assert_died_of_the_signal(_status: &std::process::ExitStatus) {}

#[cfg(windows)]
fn assert_report(report: &str) {
    assert!(report.contains("0xc0000005"), "{report}");
    assert!(report.contains("exception address"), "{report}");
}
