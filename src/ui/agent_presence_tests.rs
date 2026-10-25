//! The connected strip in a real window: it follows who is connected and
//! Revoke turns Agent access off.
use crate::ui::agent_state::{TEST_LOCK, reset_for_test};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, TestAppContext, VisualTestContext, px, size};
use quill::agent::{access, activity};
use quill::ids::AccountKey;

fn new_app(cx: &mut TestAppContext) -> VisualTestContext {
    let root = quill::scratch::process_dir("quill-agent-presence-ui");
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        Root::new(shell, window, cx)
    });
    VisualTestContext::from_window(handle.into(), cx)
}

fn settle(vcx: &mut VisualTestContext) {
    for _ in 0..3 {
        vcx.run_until_parked();
        vcx.executor()
            .advance_clock(std::time::Duration::from_millis(300));
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

#[gpui_kit::test]
fn the_strip_follows_who_is_connected(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let mut vcx = new_app(cx);
    let vcx = &mut vcx;
    reset_for_test(&AccountKey::primary());
    settle(vcx);
    assert!(vcx.debug_bounds("agent-connected").is_none(), "nobody yet");

    let first = activity::connect("Claude Code");
    settle(vcx);
    assert!(vcx.debug_bounds("agent-connected").is_some());
    let second = activity::connect("Cursor");
    settle(vcx);
    assert!(vcx.debug_bounds("agent-connected").is_some());
    drop(first);
    drop(second);
    settle(vcx);
    assert!(
        vcx.debug_bounds("agent-connected").is_none(),
        "it goes when the last agent leaves"
    );
}

#[gpui_kit::test]
fn revoke_turns_agent_access_off(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let mut vcx = new_app(cx);
    let vcx = &mut vcx;
    reset_for_test(&AccountKey::primary());
    let _agent = activity::connect("Claude Code");
    settle(vcx);
    assert!(access::enabled());

    vcx.update(|window, cx| window.click("agent-revoke", cx));
    settle(vcx);
    assert!(!access::enabled());
}
