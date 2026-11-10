//! Round 17 (area B, finding 3): the text typed into a notification is
//! taken by whichever path runs its Reply click. A click on a toast of an
//! account that was not on screen switches accounts first and then runs
//! `run_notification_action`; the text used to stay parked there, so a
//! later Reply click for the chat sent it unasked.
use crate::ui::app::QuillApp;
use crate::ui::notifications::native_extras::{take_reply_text, test_texts};
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use quill::notify::NotificationAction;

const CHAT: ChatId = ChatId(11);

#[gpui_kit::test]
fn a_reply_run_after_the_switch_takes_the_typed_text(cx: &mut TestAppContext) {
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
    let app = slot.borrow().clone();
    let Some(app): Option<Entity<QuillApp>> = app else {
        panic!("the window builder did not run");
    };
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.run_until_parked();
    app.update_in(&mut vcx, |app, window, cx| {
        let account = app.current_account().expect("the demo account");
        test_texts::park(&account, CHAT, "on my way");
        // What `open_pending_account_chat` runs once the switch landed.
        app.run_notification_action(CHAT, NotificationAction::Reply, window, cx);
        let tag = quill::notify::notification_tag(&account.0, CHAT);
        assert_eq!(
            take_reply_text(&tag, &account, CHAT),
            None,
            "the text was taken, not left for a later click"
        );
        // With nothing sent (the demo has no client), the chat opens.
        assert_eq!(app.open_chat_id(), Some(CHAT));
        // The typed words are not lost with the send: they are in the
        // composer of the chat that opened.
        assert_eq!(app.composer.read(cx).value().to_string(), "on my way");
        assert!(app.delivery.unsent_replies.is_empty());
    });
}
