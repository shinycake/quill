//! An incoming call on the account on screen rings from the poll, while
//! no window draws (Quill hidden or closed to the Dock).
use crate::ui::app::QuillApp;
use crate::ui::call_sounds::CallSound;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::telegram::envelope::CallState;

#[gpui_kit::test]
fn an_incoming_call_rings_before_any_window_draws(cx: &mut TestAppContext) {
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
                Some(ScreenshotDemo::named("ready-video-call")),
            )
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.run_until_parked();
    app.update(&mut vcx, |app, cx| {
        let call = app
            .demo_session
            .as_mut()
            .and_then(|session| session.calls.active_call.as_mut())
            .expect("the demo's call");
        call.is_outgoing = false;
        call.state = CallState::Pending {
            is_created: true,
            is_received: true,
        };
        // What a poll does after it applied the update, inside one update:
        // no window has drawn since.
        app.deliver_notifications(None, cx);
        assert_eq!(app.calls.sounds.looping(), Some(CallSound::Incoming));
    });
}
