//! A second call ringing in during a 1:1 call is answerable in the call
//! window, not only in the main window's box (which can be hidden to the
//! tray or behind the call). Needs the `demo-capture` feature for
//! gpui-kit's test support.
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::state::SecondCall;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_call_window(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
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
                Some(ScreenshotDemo::named("ready-call-add-people")),
            )
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let Some(app) = slot.borrow().clone() else {
        panic!("the app");
    };
    let mut main = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut main);
    app.update(&mut main, |app, cx| app.sync_call_window(cx));
    frames(&mut main);
    let Some(window) = app.read_with(&main, |app, _| app.calls.window) else {
        panic!("the call window opens");
    };
    let call = VisualTestContext::from_window(window, cx);
    (app, call)
}

fn drawn(vcx: &mut VisualTestContext, id: &'static str) -> bool {
    vcx.update(|window, _| window.try_find(id).is_some())
}

#[gpui_kit::test]
fn the_call_window_answers_a_second_call(cx: &mut TestAppContext) {
    let (app, mut call) = open_call_window(cx);
    let call = &mut call;
    frames(call);
    assert!(
        !drawn(call, "call-window-swap-call"),
        "no card without a second call"
    );
    app.update(call, |app, cx| {
        if let Some(session) = app.demo_session.as_deref_mut() {
            session.calls.second_call = SecondCall::Prompt {
                call_id: 6,
                user_id: 4502,
                is_video: false,
            };
        }
        cx.notify();
    });
    frames(call);
    assert!(drawn(call, "call-window-swap-call-answer"));
    call.update(|window, cx| window.click("call-window-swap-call-decline", cx));
    frames(call);
    let prompt = app.read_with(call, |app, _| {
        app.session()
            .and_then(|session| session.calls.second_call.prompt())
            .is_some()
    });
    assert!(!prompt, "declined from the call window");
}

/// Round 17 C finding 7: a call window you closed opens again when a second
/// call rings, so its Decline and End & Answer are reachable with the main
/// window minimized; it closes again once the prompt is answered.
#[gpui_kit::test]
fn a_second_call_reopens_a_call_window_you_closed(cx: &mut TestAppContext) {
    let (app, mut call) = open_call_window(cx);
    let call = &mut call;
    let active = app.read_with(call, |app, _| {
        app.session()
            .and_then(|session| session.calls.active_call.as_ref().map(|call| call.id))
    });
    app.update(call, |app, cx| {
        app.call_screen.window_closed_by_user = active;
        app.sync_call_window(cx);
    });
    call.run_until_parked();
    assert!(app.read_with(call, |app, _| app.calls.window.is_none()));
    app.update(call, |app, cx| {
        if let Some(session) = app.demo_session.as_deref_mut() {
            session.calls.second_call = SecondCall::Prompt {
                call_id: 6,
                user_id: 4502,
                is_video: false,
            };
        }
        app.sync_call_window(cx);
    });
    call.run_until_parked();
    assert!(
        app.read_with(call, |app, _| app.calls.window.is_some()),
        "the window opens for the second call"
    );
    app.update(call, |app, cx| {
        if let Some(session) = app.demo_session.as_deref_mut() {
            session.calls.second_call = SecondCall::None;
        }
        app.sync_call_window(cx);
    });
    call.run_until_parked();
    assert!(
        app.read_with(call, |app, _| app.calls.window.is_none()),
        "and closes again once answered"
    );
}
