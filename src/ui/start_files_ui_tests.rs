//! Files handed to Quill from outside open the "Forward to…" chooser; a
//! chat that takes no files refuses them, any other opens with the files in
//! its composer (needs the `demo-capture` feature for gpui-kit's test
//! support). Nothing is sent: the app has no connection.
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use quill::telegram::envelope::{ChatKind, ChatPermissions};
use std::path::PathBuf;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-privacy")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().unwrap();
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, shell, vcx)
}

fn a_file() -> PathBuf {
    let path = std::env::temp_dir().join(format!("quill-start-ui-{}.txt", std::process::id()));
    std::fs::write(&path, b"hello").unwrap();
    path
}

fn first_chat(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> ChatId {
    app.update_in(vcx, |app, _, _| {
        let session = app.demo_session.as_ref().expect("demo session");
        ChatId(*session.chats.keys().min().expect("a chat"))
    })
}

#[gpui_kit::test]
fn files_from_outside_open_the_chooser(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let file = a_file();
    app.update_in(vcx, |app, _, cx| {
        app.begin_start_files(vec![file.clone(), file.clone()], cx);
        assert_eq!(app.start_files.choosing, vec![file.clone()]);
    });
    vcx.run_until_parked();
    shell.read_with(vcx, |shell, _| {
        assert_eq!(shell.open_dialog, Some(DialogKind::StartFiles));
    });
}

#[gpui_kit::test]
fn a_chat_that_takes_no_files_keeps_the_chooser_open(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let chat = first_chat(&app, vcx);
    app.update_in(vcx, |app, window, cx| {
        let session = app.demo_session.as_mut().unwrap();
        let summary = session.chats.get_mut(&chat.0).unwrap();
        summary.kind = ChatKind::Supergroup {
            supergroup_id: 4242,
            is_channel: false,
        };
        summary.permissions = Some(ChatPermissions {
            can_send_basic_messages: true,
            ..ChatPermissions::default()
        });
        app.begin_start_files(vec![a_file()], cx);
        app.choose_start_files_chat(chat, window, cx);
        assert_eq!(app.start_files.choosing.len(), 1);
        assert_eq!(
            app.connection.status_note,
            "Sorry, no way to send media here :("
        );
        assert!(app.composer_ui.pending_attachments.is_empty());
    });
}

#[gpui_kit::test]
fn a_chat_that_takes_files_opens_with_them_attached(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let chat = first_chat(&app, vcx);
    app.update_in(vcx, |app, window, cx| {
        app.begin_start_files(vec![a_file()], cx);
        app.choose_start_files_chat(chat, window, cx);
        assert!(app.start_files.choosing.is_empty());
        assert_eq!(app.open_chat_id(), Some(chat));
        assert_eq!(app.composer_ui.pending_attachments.len(), 1);
    });
}
