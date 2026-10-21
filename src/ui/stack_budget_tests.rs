//! Windows gives the main thread 1 MB of stack, and a debug build keeps
//! several copies of `QuillApp` on it while the first window opens. A
//! `Session` held by value in `QuillApp` (36 KB) once took the launch
//! past 1 MB, so the demo session is boxed. These bounds catch the next
//! large value held by value in either type: box it instead of raising
//! the stack size.
use super::app::QuillApp;
use quill::state::Session;

/// About 73 KB today.
const QUILL_APP_MAX: usize = 80 * 1024;
/// About 36 KB today.
const SESSION_MAX: usize = 40 * 1024;

#[test]
fn quill_app_stays_small_enough_for_the_main_thread() {
    let size = std::mem::size_of::<QuillApp>();
    assert!(
        size <= QUILL_APP_MAX,
        "QuillApp is {size} bytes (limit {QUILL_APP_MAX}): box the large new field"
    );
}

#[test]
fn session_stays_small_enough_for_the_main_thread() {
    let size = std::mem::size_of::<Session>();
    assert!(
        size <= SESSION_MAX,
        "Session is {size} bytes (limit {SESSION_MAX}): box the large new field"
    );
}
