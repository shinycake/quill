//! The lock closes the call boxes it hides, so none comes back at the unlock.
use crate::ui::app::QuillApp;
use crate::ui::group_call_state::CallBox;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};

fn open_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let demo = ScreenshotDemo::named("ready-chats");
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the app");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    (app, vcx)
}

#[gpui_kit::test]
fn the_lock_closes_the_invite_and_share_boxes(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    app.update_in(&mut vcx, |app, _, _| {
        let Some(session) = app.demo_session.as_mut() else {
            panic!("the demo has a session");
        };
        session.calls.active_group_call = Some(quill::state::ActiveGroupCall::fresh(1));
    });
    for open in [
        CallBox::Invite,
        CallBox::Share(crate::ui::group_call_state::ShareStage::Choosing(
            crate::ui::group_call_state::ShareLink::Listener,
        )),
    ] {
        app.update_in(&mut vcx, |app, _, cx| {
            assert!(app.open_call_box(open), "the call has a window to open it");
            app.close_call_layers_for_lock(cx);
            assert!(matches!(app.call_box(), Some(CallBox::None)));
            assert!(!app.share_open());
        });
    }
}
