//! The noise suppression switch: on by default, saved when flipped, in
//! the voice chat window's Settings box (its only place). That window's
//! boxes (screen privacy, Settings, rename) take Esc there, topmost
//! first, and go when the window closes (needs the `demo-capture`
//! feature for gpui-kit's test support). The demo session has no
//! microphone or connection.
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screen_share_state::ScreenFor;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::state::ActiveGroupCall;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-privacy")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().unwrap();
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

fn saved_switch(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| {
        app.demo_session
            .as_ref()
            .unwrap()
            .calls
            .prefs
            .call_noise_suppression
    })
}

#[gpui_kit::test]
fn the_switch_is_on_by_default_and_saves(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    assert!(saved_switch(&app, vcx));
    app.update_in(vcx, |app, _, cx| {
        assert!(app.call_noise_suppression());
        app.set_call_noise_suppression(false, cx);
    });
    assert!(!saved_switch(&app, vcx));
    app.update_in(vcx, |app, _, cx| app.set_call_noise_suppression(true, cx));
    assert!(saved_switch(&app, vcx));
}

#[gpui_kit::test]
fn the_call_window_settings_box_opens_and_escape_closes_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        let session = app.demo_session.as_mut().unwrap();
        session.calls.active_group_call = Some(ActiveGroupCall {
            is_joined: true,
            ..ActiveGroupCall::fresh(77)
        });
        assert!(app.group_call_settings_overlay(cx).is_none());
        app.open_group_call_settings(cx);
        assert!(app.group_call_settings_overlay(cx).is_some());
        app.set_call_noise_suppression(false, cx);
    });
    vcx.run_until_parked();
    assert!(!saved_switch(&app, vcx));
    app.update_in(vcx, |app, window, cx| {
        assert!(
            !app.dismiss_topmost_layer(window, cx),
            "the main window's Escape leaves the call window's box"
        );
        assert!(app.group_call_window_escape(cx), "Escape closes the box");
        assert!(!app.group_call_screen.settings_open);
        assert!(app.group_call_settings_overlay(cx).is_none());
    });
}

/// A video chat with all three boxes open over its window.
fn open_boxes(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, window, cx| {
        let Some(session) = app.demo_session.as_mut() else {
            panic!("the demo has a session");
        };
        session.calls.active_group_call = Some(ActiveGroupCall {
            is_video_chat: true,
            can_be_managed: true,
            ..ActiveGroupCall::fresh(77)
        });
        app.sync_call_windows(cx);
        app.group_call_screen.title_dialog = Some(
            crate::ui::dialogs::group_call::GroupCallTitleDialog::new(window, cx, "x"),
        );
        app.group_call_screen.settings_open = true;
        app.screen_share.group_call = Some(77);
        app.screen_share.privacy_for = Some(ScreenFor::Group);
        cx.notify();
    });
}

fn boxes(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> (bool, bool, bool) {
    app.read_with(vcx, |app, _| {
        (
            app.screen_share.privacy_for.is_some(),
            app.group_call_screen.settings_open,
            app.group_call_screen.title_dialog.is_some(),
        )
    })
}

#[gpui_kit::test]
fn esc_in_the_video_chat_window_closes_its_boxes_topmost_first(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    open_boxes(&app, &mut vcx);
    vcx.run_until_parked();
    let Some(handle) = app.read_with(&vcx, |app, _| app.group_call.window) else {
        panic!("the video chat window opened");
    };
    let mut call = VisualTestContext::from_window(handle, cx);
    call.update(|window, cx| window.render_frame(cx));
    call.simulate_keystrokes("escape");
    assert_eq!(boxes(&app, &mut vcx), (false, true, true), "privacy first");
    call.simulate_keystrokes("escape");
    assert_eq!(boxes(&app, &mut vcx), (false, false, true), "then Settings");
    call.simulate_keystrokes("escape");
    assert_eq!(boxes(&app, &mut vcx), (false, false, false), "then rename");
    let still = app.read_with(&vcx, |app, _| {
        app.session()
            .is_some_and(|session| session.calls.active_group_call.is_some())
    });
    assert!(still, "the call itself never closes on Esc");
}

#[gpui_kit::test]
fn closing_the_video_chat_window_closes_its_boxes(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    open_boxes(&app, &mut vcx);
    vcx.run_until_parked();
    app.update_in(&mut vcx, |app, _, _| {
        // What the window's close does.
        app.group_call_screen.window_closed_by_user = Some(77);
        app.sync_group_call_boxes();
    });
    assert_eq!(boxes(&app, &mut vcx), (false, false, false));
}
