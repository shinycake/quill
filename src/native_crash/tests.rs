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
