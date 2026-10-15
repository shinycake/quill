//! The noise suppression switch: on by default, saved when flipped, in
//! the voice chat window's Settings box (its only place), which Escape
//! closes (needs the `demo-capture` feature for gpui-kit's test
//! support). The demo session has no microphone or connection.
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
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
            app.dismiss_topmost_layer(window, cx),
            "Escape closes the box"
        );
        assert!(!app.group_call.settings_open);
        assert!(app.group_call_settings_overlay(cx).is_none());
    });
}
