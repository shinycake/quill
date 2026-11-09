//! Bot "Verify accounts": each pick's description starts from the bot's
//! default, so text typed for one chat is never sent for the next (needs
//! the `demo-capture` feature for gpui-kit's test support).
use super::Step;
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let Some(app) = slot.borrow().clone() else {
        panic!("the app");
    };
    (app, VisualTestContext::from_window(handle.into(), cx))
}

fn description(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> String {
    app.read_with(vcx, |app, cx| {
        app.dialogs
            .bot_verify_dialog
            .as_ref()
            .map(|d| d.input.read(cx).value().to_string())
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
fn a_description_typed_for_one_chat_does_not_carry_to_the_next(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.open_bot_verify(500, window, cx);
        app.set_bot_verify_step(Step::Add(1), window, cx);
        if let Some(dialog) = app.dialogs.bot_verify_dialog.as_ref() {
            dialog
                .input
                .update(cx, |input, cx| input.set_value("for chat one", window, cx));
        }
    });
    assert_eq!(description(&app, vcx), "for chat one");
    app.update_in(vcx, |app, window, cx| {
        app.set_bot_verify_step(Step::Pick, window, cx);
        app.set_bot_verify_step(Step::Add(2), window, cx);
    });
    assert_eq!(description(&app, vcx), "", "the next pick starts afresh");
}
