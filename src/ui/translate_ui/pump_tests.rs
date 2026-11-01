//! A language picked for a translated chat reaches TDLib on the pass the
//! pick wakes (needs the `demo-capture` feature for gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, MessageId};
use quill::wake::UPDATES;

const CHANNEL: ChatId = ChatId(13);
const NEWEST_POST: MessageId = MessageId(208);

fn settle(vcx: &mut VisualTestContext) {
    for _ in 0..3 {
        vcx.update(|window, _| window.refresh());
        vcx.run_until_parked();
    }
}

#[gpui_kit::test]
fn a_picked_language_is_asked_for_by_the_pass_the_pick_wakes(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chat-translate");
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    settle(&mut vcx);
    let Some(app) = slot.borrow().clone() else {
        panic!("the window opened");
    };
    let shown_in = app.update_in(&mut vcx, |app, window, cx| {
        app.toggle_chat_translation(CHANNEL, cx);
        app.open_translate_chooser("en", window, cx);
        let session = app.session().expect("the demo session");
        session.chat_translated_to(CHANNEL).map(str::to_string)
    });
    assert_eq!(shown_in.as_deref(), Some("en"));

    let woke = UPDATES.epoch();
    app.update(&mut vcx, |app, cx| app.choose_translate_language("ar", cx));
    assert_ne!(UPDATES.epoch(), woke, "a resting poll loop runs a pass");

    let asked = app.update(&mut vcx, |app, _| {
        app.pump_translation();
        app.settings.translate.asked.clone()
    });
    assert_eq!(
        asked.first(),
        Some(&(NEWEST_POST, "ar")),
        "asked: {asked:?}"
    );
}
