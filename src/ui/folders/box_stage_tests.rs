#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! The folder and contact boxes wait for the answer to their own request
//! (sweep 03-048, 09-033, 09-034, 09-100, 09-108, 09-X054, 09-X002): a box
//! never closes on the send, never takes another box's answer, and a failed
//! request ends in the error the box draws, not in "Loading".

use crate::ui::app::QuillApp;
use crate::ui::dialogs::{
    AddContactDialog, ContactSave, FolderEditorDialog, FolderInviteDialog, FolderLoad,
    FolderShareDialog, InviteStage, LinkOp,
};
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::RequestId;
use quill::state::RequestPurpose;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;

fn open(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1000.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("app");
    (app, VisualTestContext::from_window(handle.into(), cx))
}

/// A request the demo session registered, as a box would have sent it.
fn sent(app: &mut QuillApp, purpose: RequestPurpose, folder: Option<i32>) -> RequestId {
    let session = app.demo_session.as_mut().expect("demo session");
    match folder {
        Some(id) => session.request_for_folder(purpose, id),
        None => session.request(purpose, None),
    }
}

/// What TDLib answers: `json` with the request's `@extra` filled in.
fn answer(app: &mut QuillApp, request: RequestId, body: &str) {
    let sink: Arc<dyn quill::diagnostics::DiagnosticSink> = app.demo_ui.sink.clone();
    let json = format!(r#"{{{body},"@extra":"{}"}}"#, request.0);
    let mut owned = copy_and_parse(&json, &app.demo_ui.seq, &sink).expect("parses");
    // A local answer: it does not depend on the demo session's counter.
    owned.seq = quill::telegram::client::LOCAL_ANSWER_SEQ;
    app.demo_session.as_mut().expect("demo").apply(owned);
}

const REFUSED: &str = r#""@type":"error","code":400,"message":"CHAT_ADMIN_REQUIRED""#;
const FOLDER_INFO: &str = r#""@type":"chatFolderInfo","id":5,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"Work","entities":[]}},"icon":null,"color_id":-1,"is_shareable":false,"has_my_invite_links":false"#;

#[gpui_kit::test]
fn the_folder_editor_closes_on_its_answer_and_shows_a_refusal(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let request = sent(app, RequestPurpose::EditChatFolder, Some(5));
        let mut dialog = FolderEditorDialog::new(window, cx, Some(5));
        dialog.load = FolderLoad::Saving(request);
        app.folders.editor = Some(dialog);
        // Another box's answer does not settle it.
        let other = sent(app, RequestPurpose::CreateChatFolder, None);
        answer(app, other, FOLDER_INFO);
        app.maybe_prefill_folder_editor(window, cx);
        assert_eq!(
            app.folders.editor.as_ref().map(|d| d.load.clone()),
            Some(FolderLoad::Saving(request)),
            "still waiting for its own answer"
        );
        // The refusal ends in the box, edits kept.
        answer(app, request, REFUSED);
        app.maybe_prefill_folder_editor(window, cx);
        let dialog = app.folders.editor.as_ref().expect("still open");
        assert_eq!(dialog.load, FolderLoad::Ready);
        assert!(dialog.error.is_some(), "the box says why");
    });
    app.update_in(&mut vcx, |app, window, cx| {
        let request = sent(app, RequestPurpose::EditChatFolder, Some(5));
        if let Some(dialog) = app.folders.editor.as_mut() {
            dialog.load = FolderLoad::Saving(request);
        }
        answer(app, request, FOLDER_INFO);
        app.maybe_prefill_folder_editor(window, cx);
        assert!(app.folders.editor.is_none(), "saved: the box closes");
    });
}

#[gpui_kit::test]
fn a_failed_folder_fetch_ends_the_editor_in_the_error(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let request = sent(app, RequestPurpose::GetChatFolder, Some(5));
        app.folders.editor = Some(FolderEditorDialog::new(window, cx, Some(5)));
        assert_eq!(
            app.folders.editor.as_ref().map(|d| d.load.clone()),
            Some(FolderLoad::Loading)
        );
        answer(app, request, REFUSED);
        app.maybe_prefill_folder_editor(window, cx);
        assert!(
            matches!(
                app.folders.editor.as_ref().map(|d| &d.load),
                Some(FolderLoad::Failed(_))
            ),
            "Loading folder ends in the error with Retry"
        );
    });
}

#[gpui_kit::test]
fn a_share_save_answer_lands_only_in_the_form_that_sent_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let first = sent(app, RequestPurpose::EditChatFolderInviteLink, Some(5));
        let mut dialog = FolderShareDialog::new(window, cx, 5);
        dialog.op = LinkOp::Saving(first);
        app.folders.share = Some(dialog);
        // Cancel (the form lets go of the request), then a new link is made.
        let second = sent(app, RequestPurpose::CreateChatFolderInviteLink, Some(5));
        if let Some(dialog) = app.folders.share.as_mut() {
            dialog.op = LinkOp::Saving(second);
        }
        // The first save's answer lands late: not this form's.
        answer(app, first, r#""@type":"chatFolderInviteLink","invite_link":"https://t.me/addlist/a","name":"","chat_ids":[1]"#);
        assert!(!app.settle_folder_share(cx), "not its answer");
        assert_eq!(
            app.folders.share.as_ref().map(|d| d.op),
            Some(LinkOp::Saving(second))
        );
        // Its own refusal ends the wait and shows the error.
        answer(app, second, REFUSED);
        assert!(app.settle_folder_share(cx));
        let dialog = app.folders.share.as_ref().expect("open");
        assert_eq!(dialog.op, LinkOp::Idle);
        assert!(dialog.error.is_some());
    });
}

#[gpui_kit::test]
fn a_failed_check_of_an_addlist_ends_the_box_in_the_error(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |app, _window, cx| {
        let request = sent(app, RequestPurpose::CheckChatFolderInviteLink, None);
        app.folders.invite = Some(FolderInviteDialog::new(
            "https://t.me/addlist/x".into(),
            InviteStage::Checking(request),
        ));
        answer(app, request, REFUSED);
        assert!(app.drive_folder_invite(cx));
        assert!(
            matches!(
                app.folders.invite.as_ref().map(|d| &d.stage),
                Some(InviteStage::Failed(_))
            ),
            "not 'Checking link...' for good"
        );
    });
}

#[gpui_kit::test]
fn the_add_contact_box_closes_on_the_answer_and_shows_a_refusal(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let request = sent(app, RequestPurpose::AddContact, None);
        let mut dialog = AddContactDialog::new(window, cx, 42, "+1555", "Ada", "");
        dialog.save = ContactSave::Saving(request);
        app.dialogs.add_contact_dialog = Some(dialog);
        answer(app, request, REFUSED);
        assert!(app.settle_add_contact(cx));
        assert!(
            matches!(
                app.dialogs.add_contact_dialog.as_ref().map(|d| &d.save),
                Some(ContactSave::Failed(_))
            ),
            "the box stays open with the refusal"
        );
        let request = sent(app, RequestPurpose::AddContact, None);
        if let Some(dialog) = app.dialogs.add_contact_dialog.as_mut() {
            dialog.save = ContactSave::Saving(request);
        }
        answer(app, request, r#""@type":"ok""#);
        assert!(app.settle_add_contact(cx));
        assert!(app.dialogs.add_contact_dialog.is_none(), "added: closed");
    });
}
