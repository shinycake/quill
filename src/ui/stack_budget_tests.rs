//! Windows gives the main thread 1 MB of stack, and a debug build keeps
//! several copies of `QuillApp` on it while the first window opens. A
//! `Session` held by value in `QuillApp` (36 KB) once took the launch
//! past 1 MB, so the demo session is boxed, and so are `Session`'s large
//! domains. These bounds catch the next large value held by value in
//! either type: box it instead of raising the stack size. The first frame
//! of the main window is bounded too: a render builder that grows past it
//! goes in smaller functions (docs/decisions/main-thread-stack.md).
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

/// Demos whose first frame must draw within `FIRST_FRAME_STACK_BUDGET`, each
/// with an element its main window must have laid out: the chat list with
/// an open chat, the busier chats (bot card, media, group info panel) and
/// the settings page.
#[cfg(feature = "demo-capture")]
const FIRST_FRAME_DEMOS: [(&str, &str); 6] = [
    ("ready-chats", "conversation-slot"),
    ("ready-showcase", "conversation-slot"),
    ("ready-bot-chat", "conversation-slot"),
    ("ready-media", "conversation-slot"),
    ("ready-info-group", "conversation-slot"),
    ("ready-settings", "settings-window"),
];

/// Smallest debug test-thread stack each demo's first frame drew with on
/// macOS arm64, in 16 KB steps (2026-10-21):
///
/// | Demo               | Stack  |
/// |--------------------|--------|
/// | `ready-chats`      | 688 KB |
/// | `ready-showcase`   | 688 KB |
/// | `ready-bot-chat`   | 688 KB |
/// | `ready-media`      | 688 KB |
/// | `ready-info-group` | 704 KB |
/// | `ready-settings`   | 608 KB |
///
/// `ready-chats` needed 864 KB before the render builders were split, most
/// of it in `folder_tabs` (315 KB) and `sidebar` (187 KB). What is left is
/// GPUI's layout recursion and the kit's `Button::render`
/// (docs/decisions/main-thread-stack.md).
///
/// A stack overflow here aborts the whole test binary with no test name
/// and no assertion message. If `cargo test --bin quill` dies with "has
/// overflowed its stack" on a `first-frame-<demo>` thread, a render builder
/// on that demo's first frame grew: split it rather than raise this budget.
#[cfg(feature = "demo-capture")]
const FIRST_FRAME_STACK_BUDGET: usize = 768 * 1024;

/// Opens the main window on `kind`, draws one frame and checks that
/// `selector` was laid out.
#[cfg(feature = "demo-capture")]
fn first_frame(kind: &'static str, selector: &'static str) {
    use super::screenshot_demo::ScreenshotDemo;
    use super::shell::QuillShell;
    use gpui_kit::component::Root;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{AppContext, TestAppContext, VisualTestContext, px, size};

    let root = quill::scratch::process_dir("quill-stack-budget");
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    let mut cx = TestAppContext::single();
    cx.update(gpui_kit::init);
    let demo = ScreenshotDemo::named(kind);
    let handle = cx.open_window(size(px(1200.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.run_until_parked();
    vcx.update(|window, cx| window.render_frame(cx));
    assert!(
        vcx.debug_bounds(selector).is_some(),
        "{kind} drew {selector} in the main window"
    );
    drop(vcx);
    cx.quit();
}

#[test]
#[cfg(feature = "demo-capture")]
fn the_first_frame_draws_within_the_stack_budget() {
    for (kind, selector) in FIRST_FRAME_DEMOS {
        let run = std::thread::Builder::new()
            .name(format!("first-frame-{kind}"))
            .stack_size(FIRST_FRAME_STACK_BUDGET)
            .spawn(move || first_frame(kind, selector))
            .expect("spawn the first-frame thread");
        assert!(
            run.join().is_ok(),
            "{kind}'s first frame drew on a {FIRST_FRAME_STACK_BUDGET} byte stack"
        );
    }
}
