//! The delete box warns before a paid suggested post loses its payment.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::composer::DeleteConfirm;
use quill::ids::{ChatId, MessageId};
use quill::local_time::now_unix;
use quill::telegram::envelope::PaidSuggestedPost;

const CHAT: ChatId = ChatId(11);

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    (app, VisualTestContext::from_window(handle.into(), cx))
}

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

/// Mark the chat's newest message as a paid post published `age` seconds
/// ago and open the delete box for it.
fn delete_paid_post(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, age: i64) {
    app.update_in(vcx, |app, window, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        let history = session.histories.get_mut(&CHAT.0).expect("history");
        let (&id, message) = history.messages.iter_mut().next_back().expect("a message");
        message.extras.paid_suggested_post = Some(PaidSuggestedPost::Stars);
        message.date = i32::try_from(now_unix() - age).unwrap_or(i32::MAX);
        let confirm = DeleteConfirm {
            chat_id: CHAT,
            message_id: MessageId(id),
            revoke: true,
            can_revoke: true,
        };
        app.open_delete_dialog(confirm, window, cx);
    });
    frames(vcx);
}

#[gpui_kit::test]
fn a_young_paid_post_warns_before_delete(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    frames(&mut vcx);
    delete_paid_post(&app, &mut vcx, 60);
    assert!(vcx.debug_bounds("delete-paid-warning").is_some());
}

#[gpui_kit::test]
fn an_old_paid_post_deletes_without_the_warning(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    frames(&mut vcx);
    delete_paid_post(&app, &mut vcx, 30 * 86_400);
    assert!(vcx.debug_bounds("delete-paid-warning").is_none());
}
