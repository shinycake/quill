//! The transfer box's password field (needs the `demo-capture` feature for
//! gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::dialogs::OwnershipStage;
use crate::ui::screenshot_demo::ScreenshotDemo;
use gpui_kit::{AppContext, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;

/// 11-040: a transfer that was not handed to TDLib keeps what was typed in
/// the password field.
#[gpui_kit::test]
fn a_transfer_that_was_not_sent_keeps_the_typed_password(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| crate::ui::shell::QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        gpui_kit::component::Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.open_ownership_dialog(
            ChatId(-100),
            OwnershipStage::Confirm {
                user_id: 5,
                leave_after: false,
            },
            window,
            cx,
        );
        let input = app
            .admin
            .ownership_dialog
            .as_ref()
            .map(|d| d.password_input.clone())
            .expect("the dialog");
        input.update(cx, |input, cx| input.set_value("hunter2", window, cx));
        app.submit_ownership_transfer(5, window, cx);
        assert_eq!(input.read(cx).value().as_ref(), "hunter2", "not sent: kept");
    });
}
