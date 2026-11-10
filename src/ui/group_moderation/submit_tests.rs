//! The moderation boxes close when TDLib answers the request they sent,
//! and stay, with the reason, when it refuses.

use crate::ui::app::QuillApp;
use crate::ui::dialogs::AdminDialog;
use crate::ui::screenshot_demo::ScreenshotDemo;
use gpui_kit::{TestAppContext, WindowHandle};
use quill::ids::ChatId;
use quill::state::{RequestPurpose, Submit};

const CHAT: ChatId = ChatId(11);

fn window(cx: &mut TestAppContext) -> WindowHandle<QuillApp> {
    cx.update(gpui_kit::init);
    cx.add_window(|window, cx| {
        QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
    })
}

#[gpui_kit::test]
fn a_demote_stays_until_the_answer_then_closes(cx: &mut TestAppContext) {
    let window = window(cx);
    window
        .update(cx, |app, _, cx| {
            let id = app
                .demo_session
                .as_mut()
                .map(|session| session.request(RequestPurpose::SetChatTitle, Some(CHAT)))
                .expect("demo session");
            let mut dialog = AdminDialog::demote_confirm(CHAT, 5);
            dialog.submit = Submit::sent(vec![id]);
            app.admin.admin_dialog = Some(dialog);
            app.settle_moderation_submits(cx);
            assert!(app.admin.admin_dialog.is_some(), "still out");
            if let Some(session) = app.demo_session.as_mut() {
                session.requests.take(id);
            }
            app.settle_moderation_submits(cx);
            assert!(app.admin.admin_dialog.is_none(), "answered");
        })
        .unwrap();
}

#[gpui_kit::test]
fn a_refused_demote_stays_with_the_reason(cx: &mut TestAppContext) {
    let window = window(cx);
    window
        .update(cx, |app, _, cx| {
            let id = app
                .demo_session
                .as_mut()
                .map(|session| session.request(RequestPurpose::SetChatTitle, Some(CHAT)))
                .expect("demo session");
            let mut dialog = AdminDialog::demote_confirm(CHAT, 5);
            dialog.submit = Submit::sent(vec![id]);
            app.admin.admin_dialog = Some(dialog);
            if let Some(session) = app.demo_session.as_mut() {
                session.requests.take(id);
                session
                    .requests
                    .record_failure(id, "CHAT_ADMIN_REQUIRED".into());
            }
            app.settle_moderation_submits(cx);
            let submit = app.admin.admin_dialog.as_ref().map(|d| d.submit.clone());
            assert_eq!(submit, Some(Submit::Failed("CHAT_ADMIN_REQUIRED".into())));
            // A second settle keeps it (the registry may forget the id).
            app.settle_moderation_submits(cx);
            assert!(app.admin.admin_dialog.is_some());
        })
        .unwrap();
}
