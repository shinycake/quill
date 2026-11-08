#![allow(clippy::unwrap_used, reason = "test code")]

use super::{ACTIVE_DUMP_FILE, ACTIVE_REPORT_FILE, discard_active, promote_previous};
use crate::crash_log::{self, KEEP_CRASH_FILES};
use std::fs;
use std::path::PathBuf;

fn temp_root(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("quill-native-crash-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

#[test]
fn a_previous_native_report_becomes_a_crash_file_with_its_dump() {
    let root = temp_root("promote");
    let dir = crash_log::log_dir(&root);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(ACTIVE_REPORT_FILE), "signal: SIGSEGV (11)\n").unwrap();
    fs::write(dir.join(ACTIVE_DUMP_FILE), b"MDMP").unwrap();

    let path = promote_previous(&dir, 1_700, KEEP_CRASH_FILES)
        .unwrap()
        .unwrap();
    assert_eq!(path, dir.join("crash-1700.txt"));
    assert!(fs::read_to_string(&path).unwrap().contains("SIGSEGV"));
    assert_eq!(fs::read(dir.join("crash-1700.dmp")).unwrap(), b"MDMP");
    assert!(!dir.join(ACTIVE_REPORT_FILE).exists());
    assert!(!dir.join(ACTIVE_DUMP_FILE).exists());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn an_empty_active_report_is_not_a_crash() {
    let root = temp_root("empty");
    let dir = crash_log::log_dir(&root);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(ACTIVE_REPORT_FILE), "").unwrap();
    assert_eq!(
        promote_previous(&dir, 1_700, KEEP_CRASH_FILES).unwrap(),
        None
    );
    assert!(!dir.join(ACTIVE_REPORT_FILE).exists());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn only_the_newest_reports_and_their_dumps_stay() {
    let root = temp_root("rotate");
    let dir = crash_log::log_dir(&root);
    fs::create_dir_all(&dir).unwrap();
    for secs in 1_000..1_005u64 {
        fs::write(dir.join(ACTIVE_REPORT_FILE), "signal: SIGABRT (6)\n").unwrap();
        fs::write(dir.join(ACTIVE_DUMP_FILE), b"MDMP").unwrap();
        promote_previous(&dir, secs, 2).unwrap().unwrap();
    }
    let mut names: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "crash-1003.dmp",
            "crash-1003.txt",
            "crash-1004.dmp",
            "crash-1004.txt"
        ]
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_clean_quit_discards_the_active_files() {
    let root = temp_root("discard");
    let dir = crash_log::log_dir(&root);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(ACTIVE_REPORT_FILE), "x").unwrap();
    fs::write(dir.join(ACTIVE_DUMP_FILE), "x").unwrap();
    discard_active(&dir);
    assert!(!dir.join(ACTIVE_REPORT_FILE).exists());
    assert!(!dir.join(ACTIVE_DUMP_FILE).exists());
    let _ = fs::remove_dir_all(&root);
}

/// The wiring behind the "closed unexpectedly" notice: the next start finds
/// the native report as the pending crash, and writes no second, generic one.
#[test]
fn the_next_start_offers_the_native_report_to_the_notice() {
    let root = temp_root("notice");
    let dir = crash_log::log_dir(&root);
    fs::create_dir_all(&dir).unwrap();
    // The previous run: started at 2_000, left `running` and a native report.
    fs::write(dir.join("running"), "4242\n1.0\n2000\n").unwrap();
    fs::write(dir.join(ACTIVE_REPORT_FILE), "signal: SIGBUS (10)\n").unwrap();

    crash_log::begin_run_for_session(&root, 3_000, 4_343);
    let pending = crash_log::pending_crash_before(&root, 3_000).unwrap();
    let text = fs::read_to_string(&pending.path).unwrap();
    assert!(
        text.contains("SIGBUS"),
        "the notice reveals the native report: {text}"
    );
    let crash_files = fs::read_dir(&dir)
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with("crash-"))
        })
        .count();
    assert_eq!(crash_files, 1, "no extra generic report");
    let _ = fs::remove_dir_all(&root);
}

/// Set in the child run of `a_native_report_lists_the_faulting_frames`.
#[cfg(any(
    target_os = "macos",
    all(target_os = "linux", target_env = "gnu"),
    all(windows, target_arch = "x86_64")
))]
mod fault {
    use super::super::ACTIVE_REPORT_FILE;
    use super::temp_root;
    use crate::crash_log;
    use std::fs;

    const CHILD_DIR: &str = "QUILL_TEST_NATIVE_FAULT_DIR";

    #[inline(never)]
    fn fault_here_for_the_report() -> u32 {
        // SAFETY: none; the read faults on purpose (null page, offset 0x20,
        // the shape of issue #1389's crash) and the process dies of it.
        unsafe { std::ptr::read_volatile(std::hint::black_box(0x20usize) as *const u32) }
    }

    /// Only does something in the child process the test below starts.
    #[test]
    fn native_fault_child() {
        let Some(dir) = std::env::var_os(CHILD_DIR) else {
            return;
        };
        super::super::install(std::path::Path::new(&dir));
        #[cfg(windows)]
        // SAFETY: a plain call; no error dialog may wait for a click on CI.
        unsafe {
            windows_sys::Win32::System::Diagnostics::Debug::SetErrorMode(
                windows_sys::Win32::System::Diagnostics::Debug::SEM_NOGPFAULTERRORBOX,
            );
        }
        // A worker thread, like most native faults; on Unix it gets the
        // handler's own alternate stack.
        // The fault ends the process; the parent sees a live exit as a failure.
        let _ = std::thread::spawn(fault_here_for_the_report).join();
    }

    /// The report lists the faulting thread's frames. On macOS `backtrace`
    /// run on the alternate signal stack can return nothing (the empty
    /// report in issue #1389), so the report walks the interrupted registers
    /// there; Windows walks the exception's context.
    #[test]
    fn a_native_report_lists_the_faulting_frames() {
        let root = temp_root("fault");
        let dir = crash_log::log_dir(&root);
        fs::create_dir_all(&dir).unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "native_crash::tests::fault::native_fault_child",
                "--nocapture",
            ])
            .env(CHILD_DIR, &dir)
            .status()
            .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(status.signal(), Some(libc::SIGSEGV), "{status:?}");
        }
        #[cfg(windows)]
        assert_eq!(
            status.code().map(|code| code as u32),
            Some(0xc000_0005),
            "{status:?}"
        );
        let report = fs::read_to_string(dir.join(ACTIVE_REPORT_FILE)).unwrap();
        let frames = report
            .split("backtrace:\n")
            .nth(1)
            .and_then(|rest| rest.split("\nend of report").next())
            .unwrap_or("");
        assert!(frames.lines().count() >= 3, "frames are listed:\n{report}");
        #[cfg(unix)]
        assert!(report.contains("fault address: 0x20"), "{report}");
        // Linux names only exported symbols; its frames are `quill(+offset)`.
        #[cfg(target_os = "macos")]
        {
            assert!(report.contains("load address: 0x"), "{report}");
            assert!(
                report.contains("fault_here_for_the_report"),
                "the faulting function is in the backtrace:\n{report}"
            );
        }
        // The first frame is the faulting instruction, in the test binary.
        #[cfg(windows)]
        assert!(
            frames
                .lines()
                .next()
                .is_some_and(|line| line.contains(".exe+0x")),
            "{report}"
        );
        let _ = fs::remove_dir_all(&root);
    }
}
