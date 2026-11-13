//! A save in flight keeps its editor: opening the form again must not
//! replace the stage its answer lands in.
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use quill::state::{RequestPurpose, Submit};

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
fn a_save_in_flight_keeps_its_topic_editor(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    let chat = ChatId(-1001);
    app.update_in(&mut vcx, |app, window, cx| {
        app.open_forum_manage_dialog(chat, window, cx);
        app.begin_topic_editor(None, window, cx);
        let request = app
            .demo_session
            .as_mut()
            .map(|session| session.request(RequestPurpose::CreateForumTopic, Some(chat)))
            .expect("the demo session");
        if let Some(editor) = app
            .admin
            .forum_manage_dialog
            .as_mut()
            .and_then(|dialog| dialog.editor.as_mut())
        {
            editor.submit = Submit::Sending(vec![request]);
        }
        app.begin_topic_editor(Some(3), window, cx);
        let editor = app
            .admin
            .forum_manage_dialog
            .as_ref()
            .and_then(|dialog| dialog.editor.as_ref())
            .expect("the editor stays");
        assert_eq!(editor.target, None, "still the new-topic form");
        assert_eq!(editor.submit, Submit::Sending(vec![request]));
        // Opening the manage dialog again keeps the sending editor: the same
        // forum's box stays, another forum's is refused.
        assert!(app.open_forum_manage_dialog(chat, window, cx));
        assert!(!app.open_forum_manage_dialog(ChatId(-1002), window, cx));
        let dialog = app.admin.forum_manage_dialog.as_ref().expect("the box");
        assert_eq!(dialog.chat_id, chat, "still this forum's box");
        assert_eq!(
            dialog.editor.as_ref().map(|editor| editor.submit.clone()),
            Some(Submit::Sending(vec![request])),
            "the answer still lands in it"
        );
    });
}

/// Sweep r5 (17-001): Cancel waits for a create in flight, so its answer
/// or refusal still lands in the editor; once it is answered Cancel works.
#[gpui_kit::test]
fn cancel_waits_for_a_create_in_flight(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    let chat = ChatId(-1001);
    app.update_in(&mut vcx, |app, window, cx| {
        app.open_forum_manage_dialog(chat, window, cx);
        app.begin_topic_editor(None, window, cx);
        let request = app
            .demo_session
            .as_mut()
            .map(|session| session.request(RequestPurpose::CreateForumTopic, Some(chat)))
            .expect("the demo session");
        fn editor(app: &mut QuillApp) -> Option<&mut Submit> {
            app.admin
                .forum_manage_dialog
                .as_mut()
                .and_then(|dialog| dialog.editor.as_mut())
                .map(|editor| &mut editor.submit)
        }
        if let Some(submit) = editor(app) {
            *submit = Submit::Sending(vec![request]);
        }
        app.cancel_topic_editor(cx);
        assert_eq!(
            editor(app).cloned(),
            Some(Submit::Sending(vec![request])),
            "the editor waits for its answer"
        );
        if let Some(submit) = editor(app) {
            *submit = Submit::Editing;
        }
        app.cancel_topic_editor(cx);
        assert_eq!(editor(app).cloned(), None, "nothing out: Cancel closes it");
    });
}
