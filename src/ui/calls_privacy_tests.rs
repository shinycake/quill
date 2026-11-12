//! The Calls page draws a Retry for each privacy rule that did not load,
//! and only for that rule.
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, TestAppContext, VisualTestContext, px, size};
use quill::privacy::{PrivacyKeyState, PrivacyRuleDetail};
use quill::settings_catalog::Page;
use quill::telegram::requests_privacy::PrivacySettingKey;

/// Sweep r4-10 round 4: AllowCalls failed and PeerToPeer loaded: one Retry,
/// for the failed key.
#[gpui_kit::test]
fn a_failed_calls_rule_draws_its_own_retry_only(cx: &mut TestAppContext) {
    let root = quill::scratch::process_dir("quill-calls-privacy-ui");
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let Some(app) = slot.borrow().clone() else {
        return;
    };
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        if let Some(session) = app.session_mut() {
            let privacy = &mut session.settings.privacy;
            privacy.insert(
                PrivacySettingKey::AllowCalls,
                PrivacyKeyState::Failed { stale: None },
            );
            privacy.insert(
                PrivacySettingKey::PeerToPeer,
                PrivacyKeyState::Ready(PrivacyRuleDetail::default()),
            );
        }
        app.open_settings(Some(Page::Calls), cx);
        cx.notify();
    });
    for _ in 0..3 {
        vcx.run_until_parked();
        vcx.update(|window, cx| window.render_frame(cx));
    }
    assert!(vcx.debug_bounds("call-privacy-allow-retry").is_some());
    assert!(vcx.debug_bounds("call-privacy-p2p-retry").is_none());
}
