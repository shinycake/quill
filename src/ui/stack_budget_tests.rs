//! Windows gives the main thread 1 MB of stack, and a debug build keeps
//! several copies of `QuillApp` on it while the first window opens. A
//! `Session` held by value in `QuillApp` (36 KB) once took the launch
//! past 1 MB, so the demo session is boxed, and so are `Session`'s large
//! domains. These bounds catch the next large value held by value in
//! either type: box it instead of raising the stack size.
use super::app::QuillApp;
use quill::state::Session;

/// About 40 KB today.
const QUILL_APP_MAX: usize = 48 * 1024;
/// About 3.5 KB today.
const SESSION_MAX: usize = 8 * 1024;

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

/// Building the app takes 384 KB to 448 KB of a debug test thread's stack.
/// It took 640 KB to 768 KB with `Session`'s domains inline, and 900 KB to
/// 1 MB with the demo session inline too. An overflow aborts the whole
/// test binary.
#[cfg(feature = "demo-capture")]
const BUILD_STACK_BUDGET: usize = 576 * 1024;

#[cfg(feature = "demo-capture")]
fn build_app() {
    use super::screenshot_demo::ScreenshotDemo;
    use gpui_kit::{AppContext, TestAppContext};

    let root = quill::scratch::process_dir("quill-stack-budget");
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    let mut cx = TestAppContext::single();
    cx.update(gpui_kit::init);
    let demo = ScreenshotDemo::named("ready-chats");
    // An empty window, so this measures the build and not the first frame.
    let vcx = cx.add_empty_window();
    let app =
        vcx.update(|window, cx| cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo))));
    drop(app);
    cx.quit();
}

#[test]
#[cfg(feature = "demo-capture")]
fn app_builds_within_the_stack_budget() {
    let run = std::thread::Builder::new()
        .name("stack-budget".into())
        .stack_size(BUILD_STACK_BUDGET)
        .spawn(build_app)
        .expect("spawn the stack-budget thread");
    assert!(
        run.join().is_ok(),
        "the app built on a {BUILD_STACK_BUDGET} byte stack"
    );
}
