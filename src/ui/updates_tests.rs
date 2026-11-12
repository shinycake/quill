//! The update timer and "Updated to Quill X".
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::updater::{ReleaseInfo, UpdateState};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    (app, VisualTestContext::from_window(handle.into(), cx))
}

fn release() -> ReleaseInfo {
    ReleaseInfo {
        version: "999.0.0".into(),
        notes: "Notes".into(),
        url: String::new(),
        asset: None,
    }
}

/// Neither the 4-hour timer nor the Check button may replace "Updated to
/// Quill X": the changelog is acknowledged by the user, not by a check.
#[gpui_kit::test]
fn a_check_leaves_the_updated_notice_for_the_user(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update(&mut vcx, |app, cx| {
        app.preferences.update_state = UpdateState::Installed(release(), Default::default());
        app.check_for_updates(cx);
        assert!(matches!(
            app.preferences.update_state,
            UpdateState::Installed(..)
        ));
    });
}

/// The 4-hour timer must not replace a failed install: the button owns it.
#[gpui_kit::test]
fn the_timer_leaves_a_failed_install_for_the_button(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update(&mut vcx, |app, cx| {
        app.preferences.update_state = UpdateState::InstallFailed(release(), "no");
        app.recheck_for_updates(cx);
        assert!(matches!(
            app.preferences.update_state,
            UpdateState::InstallFailed(..)
        ));
    });
}

/// A known update, held or not, is not replaced by the timer's check: it
/// would lose the offer when the check fails offline.
#[gpui_kit::test]
fn the_timer_leaves_a_known_update_alone(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update(&mut vcx, |app, cx| {
        app.preferences.update_state = UpdateState::Available(release());
        app.recheck_for_updates(cx);
        assert!(matches!(
            app.preferences.update_state,
            UpdateState::Available(..)
        ));
        app.preferences.update_state = UpdateState::Staged(release(), Default::default());
        app.recheck_for_updates(cx);
        assert!(matches!(
            app.preferences.update_state,
            UpdateState::Staged(..)
        ));
    });
}

/// A quit that never ends the process must not leave "Installing" for good.
#[gpui_kit::test]
fn a_quit_that_never_lands_ends_in_install_failed(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update(&mut vcx, |app, cx| {
        app.preferences.update_state = UpdateState::Installing(release(), Default::default());
        app.restart_did_not_land(cx);
        assert!(matches!(
            app.preferences.update_state,
            UpdateState::InstallFailed(..)
        ));
    });
}
