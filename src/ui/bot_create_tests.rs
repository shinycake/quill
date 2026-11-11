//! A second managed-bot press while a create is out (needs the
//! `demo-capture` feature for gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::state::{CreateStage, CreateSubmission};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    (app, VisualTestContext::from_window(handle.into(), cx))
}

/// 08-018: a press while an earlier createBot is out neither resets the
/// stage (its answer would never match) nor adopts it (this press's share
/// would never be answered): it opens no box and says so.
#[gpui_kit::test]
fn a_second_press_during_creating_is_refused_and_keeps_the_first(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let session = app.session_mut().expect("the demo session");
        let first = CreateSubmission {
            generation: 3,
            name: "First".into(),
            username: "first_bot".into(),
            manager_id: 7,
            share: None,
        };
        session.bots.owned.create.generation = 3;
        session.bots.owned.create.stage = CreateStage::Creating(first.clone());
        app.open_bot_create(
            7,
            "Second".into(),
            "second_bot".into(),
            false,
            None,
            window,
            cx,
        );
        assert!(app.dialogs.bot_create_dialog.is_none(), "no box adopts it");
        assert_eq!(
            app.screen.connection.status_note(),
            quill::lang::tr(quill::lang::keys::Q_MEDIA_STILL_CREATING_A_BOT)
        );
        let create = app.session().map(|s| s.bots.owned.create.stage.clone());
        assert_eq!(create, Some(CreateStage::Creating(first)), "first kept");
    });
}
