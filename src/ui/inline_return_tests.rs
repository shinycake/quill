#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! The inline results button's round trip ends when the bot's chat cannot
//! be opened, so its start never goes to a chat opened later (sweep 10-119).

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;

fn open(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
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
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    app.update(&mut vcx, |app, _| {
        let session = app.demo_session.as_mut().expect("a demo session");
        let first = session.chats.keys().min().copied().expect("a chat");
        session.open_chat = Some(ChatId(first));
    });
    (app, vcx)
}

fn waiting(app: &Entity<QuillApp>, vcx: &VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| app.composer_ui.inline_return.is_some())
}

#[gpui_kit::test]
fn a_failed_bot_lookup_ends_the_round_trip(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update(&mut vcx, |app, _| {
        app.begin_inline_return(900_001, "somebot", "param");
    });
    assert!(waiting(&app, &vcx), "waits for the bot's chat");
    app.update(&mut vcx, |app, _| app.cancel_inline_opening());
    assert!(!waiting(&app, &vcx), "the failed lookup leaves nothing");
}
