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

/// 13-067, 13-066: the delete behind an agent's Undo deletes and nothing
/// else: the forward the user is building in another chat stays, and the
/// answer says whether the request went out.
#[gpui_kit::test]
fn deleting_messages_leaves_the_users_forward_alone(cx: &mut TestAppContext) {
    use crate::ui::forward_stage::ForwardStage;
    use quill::composer::ForwardDraft;
    let (app, mut vcx) = new_app(cx);
    frames(&mut vcx);
    let id = app.read_with(&vcx, |app, _| {
        let session = app.demo_session.as_ref().expect("demo session");
        let history = session.histories.get(&CHAT.0).expect("history");
        history
            .messages
            .keys()
            .copied()
            .next_back()
            .expect("a message")
    });
    app.update(&mut vcx, |app, _| {
        let draft = ForwardDraft::from_message(ChatId(12), Default::default(), MessageId(5), false)
            .expect("a sent message");
        app.share.forward = ForwardStage::Selecting(draft);
    });
    let went_out = app.update(&mut vcx, |app, _| {
        app.delete_messages(CHAT, &[MessageId(id)], true)
    });
    assert!(went_out, "the demo applies the delete");
    app.read_with(&vcx, |app, _| {
        assert!(
            matches!(app.share.forward, ForwardStage::Selecting(_)),
            "an unrelated forward draft survives"
        );
    });
    // The selection box's own delete still ends selection mode.
    app.update(&mut vcx, |app, cx| {
        app.delete_selection(CHAT, &[], true, cx);
    });
    app.read_with(&vcx, |app, _| {
        assert!(matches!(app.share.forward, ForwardStage::None));
    });
}
