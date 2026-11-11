//! New Group / New Channel: the group's Next leads to the members step and
//! back, an empty name blocks the first step, and a created channel walks
//! the type and subscribers steps into the new chat. A picked photo goes
//! through the crop shown over the dialog and lands on it.

use super::demo::NEW_CHANNEL;
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::profile_photo_sources::PhotoTarget;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use quill::new_chat::NewChatStep;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_demo(
    cx: &mut TestAppContext,
    name: &'static str,
) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named(name);
    let handle = cx.open_window(size(px(1200.), px(1000.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, shell, vcx)
}

fn step(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<NewChatStep> {
    app.read_with(vcx, |app, _| app.new_chat_view().map(|view| view.step))
}

fn click(vcx: &mut VisualTestContext, id: &'static str) {
    vcx.update(|window, cx| window.click(id, cx));
    frames(vcx);
}

#[gpui_kit::test]
fn a_group_goes_to_members_and_back(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx, "ready-new-group");
    let vcx = &mut vcx;
    assert!(shell.read_with(vcx, |s, _| s.open_dialog == Some(DialogKind::CreateChat)));
    assert_eq!(step(&app, vcx), Some(NewChatStep::Info));
    click(vcx, "new-chat-confirm");
    assert_eq!(step(&app, vcx), Some(NewChatStep::Members));
    click(vcx, "new-chat-members-cancel");
    assert_eq!(step(&app, vcx), Some(NewChatStep::Info));
    assert_eq!(
        app.read_with(vcx, |app, cx| app.new_chat_dialog().map(|d| d
            .title_input
            .read(cx)
            .value()
            .to_string())),
        Some("Weekend Hikers".to_string())
    );
}

#[gpui_kit::test]
fn an_empty_name_keeps_the_first_step(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = open_demo(cx, "ready-new-channel");
    let vcx = &mut vcx;
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            if let Some(dialog) = app.new_chat_dialog() {
                dialog
                    .title_input
                    .update(cx, |input, cx| input.set_value("  ", window, cx));
            }
        });
    });
    frames(vcx);
    click(vcx, "new-chat-confirm");
    assert_eq!(step(&app, vcx), Some(NewChatStep::Info));
    assert_eq!(
        app.read_with(vcx, |app, _| app.new_chat_dialog().map(|d| d.title_error)),
        Some(true)
    );
}

#[gpui_kit::test]
fn a_created_channel_walks_type_and_subscribers(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx, "ready-new-channel-type");
    let vcx = &mut vcx;
    let chat = ChatId(NEW_CHANNEL);
    assert_eq!(step(&app, vcx), Some(NewChatStep::Type(chat)));
    click(vcx, "new-chat-type-skip");
    assert_eq!(step(&app, vcx), Some(NewChatStep::Subscribers(chat)));
    click(vcx, "new-chat-add-skip");
    assert_eq!(step(&app, vcx), None);
    assert!(shell.read_with(vcx, |s, _| s.open_dialog != Some(DialogKind::CreateChat)));
    assert_eq!(
        app.read_with(vcx, |app, _| app.session().and_then(|s| s.open_chat)),
        Some(chat)
    );
}

#[gpui_kit::test]
fn a_picked_photo_opens_the_crop_and_lands_on_the_new_group(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = open_demo(cx, "ready-new-group");
    let vcx = &mut vcx;
    let picture = image::RgbaImage::from_pixel(900, 600, image::Rgba([40, 120, 200, 255]));
    app.update(vcx, |app, cx| {
        app.open_profile_photo_crop(PhotoTarget::NewChat, picture, cx)
    });
    frames(vcx);
    assert!(shell.read_with(vcx, |s, _| s.open_dialog
        == Some(DialogKind::ProfilePhotoCrop)));
    click(vcx, "photo-editor-done");
    assert!(shell.read_with(vcx, |s, _| s.open_dialog == Some(DialogKind::CreateChat)));
    assert_eq!(step(&app, vcx), Some(NewChatStep::Info));
    assert!(app.read_with(vcx, |app, _| {
        app.new_chat_dialog()
            .is_some_and(|dialog| dialog.photo.is_some())
    }));
}

#[gpui_kit::test]
fn cancel_closes_the_box_while_the_create_request_is_out(cx: &mut TestAppContext) {
    use quill::new_chat::{NewChatBack, NewChatStage};
    use quill::state::RequestPurpose;
    let (app, shell, mut vcx) = open_demo(cx, "ready-new-channel");
    let vcx = &mut vcx;
    app.update(vcx, |app, _| {
        let request = app
            .demo_session
            .as_mut()
            .map(|session| session.request(RequestPurpose::SetChatTitle, None))
            .expect("demo session");
        if let Some(dialog) = app.admin.create_chat_dialog.as_mut() {
            dialog.stage = NewChatStage::Creating {
                request,
                back: NewChatBack::Info,
            };
        }
        assert!(app.new_chat_view().is_some_and(|view| view.busy));
    });
    frames(vcx);
    // Offline, the request never answers: Cancel must still work.
    click(vcx, "new-chat-cancel");
    assert_eq!(step(&app, vcx), None);
    assert!(shell.read_with(vcx, |s, _| s.open_dialog != Some(DialogKind::CreateChat)));
}

/// A plain group whose create was answered ends its dialog (and with it
/// the creation row), instead of leaving a hidden dialog behind.
#[gpui_kit::test]
fn a_made_group_takes_its_dialog_away(cx: &mut TestAppContext) {
    use quill::new_chat::{CreationOutcome, NewChatBack, NewChatStage};
    use quill::state::{NewChatCreation, RequestPurpose};
    let (app, shell, mut vcx) = open_demo(cx, "ready-new-group");
    let vcx = &mut vcx;
    app.update(vcx, |app, _| {
        let Some(session) = app.demo_session.as_mut() else {
            return;
        };
        let request = session.request(RequestPurpose::SetChatTitle, None);
        session.groups.new_chats.creations.insert(
            request,
            NewChatCreation {
                outcome: Some(CreationOutcome::Created(ChatId(77))),
                ..NewChatCreation::default()
            },
        );
        if let Some(dialog) = app.admin.create_chat_dialog.as_mut() {
            dialog.stage = NewChatStage::Creating {
                request,
                back: NewChatBack::Info,
            };
        }
    });
    assert_eq!(step(&app, vcx), Some(NewChatStep::Done(ChatId(77))));
    assert!(app.read_with(vcx, |app, _| app.admin.create_chat_dialog.is_some()));
    app.update(vcx, |app, cx| {
        assert!(app.drain_new_chat_done(cx));
        assert!(!app.drain_new_chat_done(cx));
    });
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.admin.create_chat_dialog.is_none()));
    assert!(shell.read_with(vcx, |s, _| s.open_dialog != Some(DialogKind::CreateChat)));
}
