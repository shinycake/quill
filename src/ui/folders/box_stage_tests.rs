#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! The folder and contact boxes wait for the answer to their own request
//! (sweep 03-048, 09-033, 09-034, 09-100, 09-108, 09-X054, 09-X002): a box
//! never closes on the send, never takes another box's answer, and a failed
//! request ends in the error the box draws, not in "Loading".

use crate::ui::app::QuillApp;
use crate::ui::dialogs::{
    AddContactDialog, AddContactStage, ContactSave, CopyMark, EditContactDialog,
    FolderDeleteConfirm, FolderEditorDialog, FolderInviteDialog, FolderLoad, FolderShareDialog,
    FolderShareView, InviteStage, LinkForm, LinkOp,
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

/// The folder's spec fetch waits for `request`, as the driver records it.
fn spec_waits_for(app: &mut QuillApp, folder_id: i32, request: RequestId) {
    if let Some(session) = app.demo_session.as_mut() {
        let fetch = session.chat_list.folder_specs.fetch_mut(folder_id);
        fetch.begin_with(request);
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
        let mut dialog = FolderEditorDialog::new(window, cx, Some(5), FolderLoad::ready());
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
        assert!(dialog.load.is_ready());
        assert!(dialog.load.error().is_some(), "the box says why");
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
        app.folders.editor = Some(FolderEditorDialog::new(
            window,
            cx,
            Some(5),
            FolderLoad::Loading(request),
        ));
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
        let mut dialog = FolderShareDialog::new(5);
        dialog.view = FolderShareView::edit(
            None,
            LinkForm::new(window, cx, String::new(), Default::default()),
        );
        dialog.view.set_op(LinkOp::Saving(first));
        app.folders.share = Some(dialog);
        // Cancel (the form lets go of the request), then a new link is made.
        let second = sent(app, RequestPurpose::CreateChatFolderInviteLink, Some(5));
        if let Some(dialog) = app.folders.share.as_mut() {
            dialog.view = FolderShareView::edit(
                None,
                LinkForm::new(window, cx, String::new(), Default::default()),
            );
            dialog.view.set_op(LinkOp::Saving(second));
        }
        // The first save's answer lands late: not this form's.
        answer(app, first, r#""@type":"chatFolderInviteLink","invite_link":"https://t.me/addlist/a","name":"","chat_ids":[1]"#);
        assert!(!app.settle_folder_share(cx), "not its answer");
        assert_eq!(
            app.folders.share.as_ref().map(|d| d.view.op().clone()),
            Some(LinkOp::Saving(second))
        );
        // Its own refusal ends the wait and shows the error.
        answer(app, second, REFUSED);
        assert!(app.settle_folder_share(cx));
        let dialog = app.folders.share.as_ref().expect("open");
        assert!(matches!(dialog.view.op(), LinkOp::Failed(_)));
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
        dialog.stage = AddContactStage::Saving(request);
        app.dialogs.add_contact_dialog = Some(dialog);
        answer(app, request, REFUSED);
        assert!(app.settle_add_contact(cx));
        assert!(
            matches!(
                app.dialogs.add_contact_dialog.as_ref().map(|d| &d.stage),
                Some(AddContactStage::Failed(_))
            ),
            "the box stays open with the refusal"
        );
        let request = sent(app, RequestPurpose::AddContact, None);
        if let Some(dialog) = app.dialogs.add_contact_dialog.as_mut() {
            dialog.stage = AddContactStage::Saving(request);
        }
        answer(app, request, r#""@type":"ok""#);
        assert!(app.settle_add_contact(cx));
        assert!(app.dialogs.add_contact_dialog.is_none(), "added: closed");
    });
}

/// A request lost with no answer kept (a reset, or more answers than the
/// session keeps) ends the Add button's wait in the refusal.
#[gpui_kit::test]
fn a_lost_add_contact_request_ends_in_the_refusal(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let request = sent(app, RequestPurpose::AddContact, None);
        let mut dialog = AddContactDialog::new(window, cx, 42, "+1555", "Ada", "");
        dialog.stage = AddContactStage::Saving(request);
        app.dialogs.add_contact_dialog = Some(dialog);
        assert!(!app.settle_add_contact(cx), "still out: it waits");
        // The session forgets the request with no answer.
        if let Some(session) = app.session_mut() {
            session.requests.take(request);
        }
        assert!(app.settle_add_contact(cx));
        assert!(
            matches!(
                app.dialogs.add_contact_dialog.as_ref().map(|d| &d.stage),
                Some(AddContactStage::Failed(_))
            ),
            "not a spinner for good"
        );
    });
}

#[gpui_kit::test]
fn a_save_after_the_folder_changed_goes_onto_the_fresh_folder(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        // The form was filled from a folder with chats 1 and 2; the person
        // adds chat 5 and presses Save, which finds the folder changed.
        let base = quill::telegram::envelope::ChatFolderSpec {
            name: "Work".into(),
            included_chat_ids: vec![1, 2],
            ..Default::default()
        };
        let mut dialog = FolderEditorDialog::new(window, cx, Some(5), FolderLoad::ready());
        dialog.prefill_from_spec(&base, window, cx);
        dialog.editor.toggle_included(5);
        let request = sent(app, RequestPurpose::GetChatFolder, Some(5));
        spec_waits_for(app, 5, request);
        dialog.load = FolderLoad::Merging(request);
        app.folders.editor = Some(dialog);
        app.maybe_prefill_folder_editor(window, cx);
        assert!(
            app.folders.editor.is_some(),
            "still waiting for the fresh folder"
        );
        // Elsewhere chat 1 was pinned and chat 7 added.
        answer(
            app,
            request,
            r#""@type":"chatFolder","name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"Work","entities":[]}},"icon":null,"color_id":-1,"is_shareable":false,"pinned_chat_ids":[1],"included_chat_ids":[2,7],"excluded_chat_ids":[],"exclude_muted":false,"exclude_read":false,"exclude_archived":false,"include_contacts":false,"include_non_contacts":false,"include_bots":false,"include_groups":false,"include_channels":false"#,
        );
        app.maybe_prefill_folder_editor(window, cx);
        assert!(app.folders.editor.is_none(), "saved");
        let saved = app
            .demo_session
            .as_ref()
            .and_then(|s| s.chat_list.folder_specs.get(&5))
            .cloned()
            .expect("the saved spec");
        assert_eq!(saved.pinned_chat_ids, vec![1], "the new pin stays");
        assert_eq!(saved.included_chat_ids, vec![2, 5, 7], "7 stays, 5 added");
    });
}

fn edit_contact_save(app: &QuillApp) -> Option<ContactSave> {
    match &app.dialogs.profile_dialog {
        Some(crate::ui::dialogs::ProfileDialog::EditContact(dialog)) => Some(dialog.save.clone()),
        _ => None,
    }
}

#[gpui_kit::test]
fn the_edit_contact_box_closes_on_its_answer_and_shows_a_refusal(cx: &mut TestAppContext) {
    use crate::ui::dialogs::ProfileDialog;
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let request = sent(app, RequestPurpose::AddContact, None);
        let mut dialog = EditContactDialog::new(window, cx, 42, "Ada", "", None);
        dialog.save = ContactSave::Saving(request);
        app.dialogs.profile_dialog = Some(ProfileDialog::EditContact(dialog));
        // Another request's answer does not settle it.
        let other = sent(app, RequestPurpose::AddContact, None);
        answer(app, other, r#""@type":"ok""#);
        assert!(!app.settle_edit_contact(cx));
        assert_eq!(edit_contact_save(app), Some(ContactSave::Saving(request)));
        // The refusal stays in the box, which Saves again.
        answer(app, request, REFUSED);
        assert!(app.settle_edit_contact(cx));
        assert!(matches!(
            edit_contact_save(app),
            Some(ContactSave::Failed(_))
        ));
        let retry = sent(app, RequestPurpose::AddContact, None);
        if let Some(ProfileDialog::EditContact(dialog)) = &mut app.dialogs.profile_dialog {
            dialog.save = ContactSave::Saving(retry);
        }
        answer(app, retry, r#""@type":"ok""#);
        assert!(app.settle_edit_contact(cx));
        assert!(app.dialogs.profile_dialog.is_none(), "saved: closed");
    });
}

#[gpui_kit::test]
fn a_limit_box_leaves_the_folder_editor_without_an_error_line(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let request = sent(app, RequestPurpose::CreateChatFolder, None);
        let mut dialog = FolderEditorDialog::new(window, cx, None, FolderLoad::ready());
        dialog.load = FolderLoad::Saving(request);
        app.folders.editor = Some(dialog);
        answer(
            app,
            request,
            r#""@type":"error","code":400,"message":"DIALOG_FILTERS_TOO_MUCH""#,
        );
        app.maybe_prefill_folder_editor(window, cx);
        let dialog = app.folders.editor.as_ref().expect("still open");
        assert_eq!(
            dialog.load,
            FolderLoad::ready(),
            "Save works again, with no line: the limit box says it alone"
        );
    });
}

#[gpui_kit::test]
fn a_save_waiting_for_the_fresh_folder_never_spins_without_a_request(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let mut dialog = FolderEditorDialog::new(window, cx, Some(5), FolderLoad::ready());
        // The fetch is in flight: the stage waits and asks nothing more.
        let request = sent(app, RequestPurpose::GetChatFolder, Some(5));
        dialog.load = FolderLoad::Merging(request);
        app.folders.editor = Some(dialog);
        app.maybe_prefill_folder_editor(window, cx);
        assert_eq!(
            app.folders.editor.as_ref().map(|d| d.load.clone()),
            Some(FolderLoad::Merging(request))
        );
        // It failed: back to the form with the error, edits kept.
        answer(app, request, REFUSED);
        app.maybe_prefill_folder_editor(window, cx);
        let dialog = app.folders.editor.as_ref().expect("still open");
        assert!(dialog.load.is_ready());
        assert!(dialog.load.error().is_some(), "the form says why");
    });
}

#[gpui_kit::test]
fn a_save_waiting_for_the_fresh_folder_ends_when_its_own_request_is_gone(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let mut dialog = FolderEditorDialog::new(window, cx, Some(5), FolderLoad::ready());
        // Its own request is no longer out (swept), while another box's
        // fetch of the same folder is: that one is not ours to wait for.
        let own = sent(app, RequestPurpose::GetChatFolder, Some(5));
        if let Some(s) = app.demo_session.as_mut() {
            s.requests.take(own);
        }
        let _other = sent(app, RequestPurpose::GetChatFolder, Some(5));
        dialog.load = FolderLoad::Merging(own);
        app.folders.editor = Some(dialog);
        app.maybe_prefill_folder_editor(window, cx);
        let dialog = app.folders.editor.as_ref().expect("still open");
        assert!(dialog.load.is_ready(), "back on the form");
        assert!(dialog.load.error().is_some(), "with the reason");
    });
}

fn shared_delete_box(app: &mut QuillApp) -> RequestId {
    app.folders.delete_confirm = Some(FolderDeleteConfirm {
        folder_id: 5,
        name: "Work".into(),
        has_links: false,
        shared: true,
        keep: Default::default(),
    });
    let request = sent(app, RequestPurpose::GetChatFolderChatsToLeave, Some(5));
    if let Some(session) = app.demo_session.as_mut() {
        let to_leave = session
            .chat_list
            .folder_chats_to_leave
            .entry(5)
            .or_default();
        to_leave.begin_with(request);
    }
    request
}

#[gpui_kit::test]
fn deleting_a_shared_folder_waits_for_the_chats_it_could_leave(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        let request = shared_delete_box(app);
        // An early click does not delete without offering the chats.
        app.confirm_folder_delete(cx);
        assert!(app.folders.delete_confirm.is_some(), "still waiting");
        answer(
            app,
            request,
            r#""@type":"chats","total_count":1,"chat_ids":[7]"#,
        );
        app.confirm_folder_delete(cx);
        assert!(app.folders.delete_confirm.is_none(), "deleted once offered");
    });
}

#[gpui_kit::test]
fn a_failed_list_of_chats_to_leave_ends_in_an_error_the_box_can_delete_past(
    cx: &mut TestAppContext,
) {
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        let request = shared_delete_box(app);
        answer(app, request, REFUSED);
        let failed = app
            .session()
            .and_then(|s| s.chat_list.folder_chats_to_leave.get(&5))
            .and_then(|f| f.error().map(str::to_string));
        assert!(failed.is_some(), "the box draws the error and a Retry");
        app.confirm_folder_delete(cx);
        assert!(app.folders.delete_confirm.is_none());
    });
}

#[gpui_kit::test]
fn joining_new_chats_sends_only_what_the_folder_still_offers(cx: &mut TestAppContext) {
    let (_app, _vcx) = open(cx);
    let dialog = crate::ui::folder_extras::FolderNewChatsDialog {
        folder_id: 5,
        selected: [1, 2, 3].into_iter().collect(),
        stage: crate::ui::folder_extras::JoinStage::Choosing,
    };
    assert_eq!(dialog.still_offered(&[3, 9, 1]), vec![1, 3]);
    assert!(dialog.still_offered(&[]).is_empty());
}

#[gpui_kit::test]
fn the_copied_check_mark_goes_back_to_the_copy_icon(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |app, _window, cx| {
        app.folders.share = Some(FolderShareDialog::new(5));
        app.copy_folder_link("https://t.me/addlist/a", cx);
        assert!(matches!(
            app.folders.share.as_ref().map(|d| d.copy_mark()),
            Some(CopyMark::Copied { .. })
        ));
    });
    vcx.executor()
        .advance_clock(std::time::Duration::from_millis(2100));
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| {
        assert!(matches!(
            app.folders.share.as_ref().map(|d| d.copy_mark()),
            Some(CopyMark::Idle)
        ));
    });
}

#[gpui_kit::test]
fn the_reset_statistics_confirmation_ends_on_the_confirm(cx: &mut TestAppContext) {
    use crate::ui::dialogs::data_extra::NetworkReset;
    let (app, mut vcx) = open(cx);
    app.update(&mut vcx, |app, _| {
        // Without a live driver the confirmed reset is local and done.
        app.privacy.extra.network_reset = NetworkReset::Confirming;
        app.reset_network_usage();
        assert_eq!(app.privacy.extra.network_reset, NetworkReset::Idle);
        // Pressing the confirm twice does not send twice.
        app.reset_network_usage();
        assert_eq!(app.privacy.extra.network_reset, NetworkReset::Idle);
    });
}

#[gpui_kit::test]
fn a_limit_release_settles_on_the_answers_of_its_own_requests(cx: &mut TestAppContext) {
    use crate::ui::index_sliders::StorageDrag;
    let (app, mut vcx) = open(cx);
    app.update(&mut vcx, |app, cx| {
        app.ensure_storage_sliders(cx);
        let account = app.session().map(|s| s.account.clone()).expect("session");
        let first = sent(app, RequestPurpose::SetStorageOption, None);
        let second = sent(app, RequestPurpose::SetStorageOption, None);
        app.index_sliders.storage = StorageDrag::Sent {
            account: account.clone(),
            requests: vec![first, second],
        };
        // One answer of two: still waiting, whichever came first.
        answer(app, second, r#""@type":"ok""#);
        app.settle_storage_sliders(cx);
        assert!(matches!(
            app.index_sliders.storage,
            StorageDrag::Sent { .. }
        ));
        // The last one is refused: the sliders go back on what TDLib holds.
        answer(app, first, REFUSED);
        app.settle_storage_sliders(cx);
        assert!(matches!(app.index_sliders.storage, StorageDrag::Settled));
        let held = app.session().map(|s| {
            (
                s.settings.storage_limits.size_limit(),
                s.settings.storage_limits.keep_for(),
            )
        });
        assert_eq!(held, Some(app.storage_slider_limits(cx)));
        // Answered ok with limits TDLib rounded: the release is settled
        // without waiting for the numbers to match.
        let third = sent(app, RequestPurpose::SetStorageOption, None);
        app.index_sliders.storage = StorageDrag::Sent {
            account,
            requests: vec![third],
        };
        answer(app, third, r#""@type":"ok""#);
        app.settle_storage_sliders(cx);
        assert!(matches!(app.index_sliders.storage, StorageDrag::Settled));
    });
}

#[gpui_kit::test]
fn the_folder_delete_toast_waits_for_tdlibs_answer(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        let refused = sent(app, RequestPurpose::DeleteChatFolder, Some(5));
        let accepted = sent(app, RequestPurpose::DeleteChatFolder, Some(6));
        app.folders.deletes.insert(refused);
        app.folders.deletes.insert(accepted);
        assert!(!app.settle_folder_deletes(cx), "nothing answered yet");
        assert_eq!(app.folders.deletes.len(), 2);
        answer(app, refused, REFUSED);
        assert!(app.settle_folder_deletes(cx));
        assert!(
            !app.folders.deletes.contains(&refused),
            "the answer ends its entry"
        );
        assert!(
            app.folders.deletes.contains(&accepted),
            "another delete still waits"
        );
        let said = app.connection.status_note().to_string();
        assert!(
            !said.contains("deleted"),
            "a refused delete is not reported as deleted: {said}"
        );
        answer(app, accepted, r#""@type":"ok""#);
        assert!(app.settle_folder_deletes(cx));
        assert!(app.folders.deletes.is_empty());
    });
}

#[gpui_kit::test]
fn another_link_does_not_replace_an_add_in_flight(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        let request = sent(app, RequestPurpose::AddChatFolderByInviteLink, None);
        let info = Box::new(quill::telegram::envelope::ChatFolderInviteLinkInfo {
            folder: quill::telegram::envelope::ChatFolderInfo {
                id: 0,
                name: "Work".into(),
                icon_name: String::new(),
                color_id: -1,
                is_shareable: false,
                has_my_invite_links: false,
            },
            missing_chat_ids: vec![],
            added_chat_ids: vec![],
        });
        app.folders.invite = Some(FolderInviteDialog::new(
            "first".into(),
            InviteStage::Adding {
                request,
                info,
                next: None,
            },
        ));
        app.open_folder_invite("second".into(), cx);
        let dialog = app.folders.invite.as_ref().expect("the box stays");
        assert_eq!(dialog.link, "first", "the add in flight keeps its box");
        assert!(matches!(
            &dialog.stage,
            InviteStage::Adding { request: kept, next: Some(link), .. }
                if *kept == request && link == "second"
        ));
    });
}

/// Sweep r3-08 (16-002): closing the Add folder box while the add is out
/// hands the request on, and its answer is taken and told.
#[gpui_kit::test]
fn closing_the_add_folder_box_keeps_the_adds_answer(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        let request = sent(app, RequestPurpose::AddChatFolderByInviteLink, None);
        let info = Box::new(quill::telegram::envelope::ChatFolderInviteLinkInfo {
            folder: quill::telegram::envelope::ChatFolderInfo {
                id: 0,
                name: "Work".into(),
                icon_name: String::new(),
                color_id: -1,
                is_shareable: false,
                has_my_invite_links: false,
            },
            missing_chat_ids: vec![],
            added_chat_ids: vec![],
        });
        app.folders.invite = Some(FolderInviteDialog::new(
            "first".into(),
            InviteStage::Adding {
                request,
                info,
                next: None,
            },
        ));
        app.close_folder_invite(cx);
        assert!(app.folders.invite.is_none(), "the box is gone");
        assert!(app.folders.closed_asks.contains_key(&request));
        // Another request's answer is not its own.
        let other = sent(app, RequestPurpose::AddChatFolderByInviteLink, None);
        answer(app, other, REFUSED);
        assert!(!app.settle_folder_share(cx));
        assert!(app.folders.closed_asks.contains_key(&request));
        // Its own answer is taken, and the entry ends.
        answer(app, request, REFUSED);
        assert!(app.settle_folder_share(cx));
        assert!(app.folders.closed_asks.is_empty());
        let demo = app.demo_session.as_mut().expect("demo session");
        assert!(demo.chat_list.dialog_answers.take(request).is_none());
    });
}

/// Sweep r3-08 (16-003): closing the Share folder box during a link save
/// hands the request on, so a refusal is still told.
#[gpui_kit::test]
fn closing_the_share_box_keeps_a_link_saves_answer(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let request = sent(app, RequestPurpose::CreateChatFolderInviteLink, Some(5));
        let mut dialog = FolderShareDialog::new(5);
        dialog.view = FolderShareView::edit(
            None,
            LinkForm::new(window, cx, String::new(), Default::default()),
        );
        dialog.view.set_op(LinkOp::Saving(request));
        app.folders.share = Some(dialog);
        app.close_folder_share(cx);
        assert!(app.folders.share.is_none());
        assert!(app.folders.closed_asks.contains_key(&request));
        answer(app, request, REFUSED);
        assert!(app.settle_folder_share(cx));
        assert!(app.folders.closed_asks.is_empty());
        // A box closed with nothing out leaves nothing behind.
        app.folders.share = Some(FolderShareDialog::new(5));
        app.close_folder_share(cx);
        assert!(app.folders.closed_asks.is_empty());
    });
}

/// Sweep r3-08 review: a closed box's entry ends when its request is gone
/// and its answer is not kept (the ring of answers dropped it), with no
/// toast.
#[gpui_kit::test]
fn a_closed_ask_ends_when_its_answer_was_evicted(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let request = sent(app, RequestPurpose::CreateChatFolderInviteLink, Some(5));
        let mut dialog = FolderShareDialog::new(5);
        dialog.view = FolderShareView::edit(
            None,
            LinkForm::new(window, cx, String::new(), Default::default()),
        );
        dialog.view.set_op(LinkOp::Saving(request));
        app.folders.share = Some(dialog);
        app.close_folder_share(cx);
        // Still out: the entry waits.
        assert!(!app.settle_folder_share(cx));
        assert!(app.folders.closed_asks.contains_key(&request));
        // Answered, then the answer is evicted before anyone takes it.
        answer(app, request, REFUSED);
        let demo = app.demo_session.as_mut().expect("demo session");
        assert!(demo.chat_list.dialog_answers.take(request).is_some());
        assert!(!app.settle_folder_share(cx), "no toast for a lost answer");
        assert!(app.folders.closed_asks.is_empty());
    });
}

/// The shared folder's join box stays up, drawing "Joining", until the
/// answer to its own request: the offer gone closes it, the offer standing
/// (the request failed) sends it back to choosing with the ticks kept.
#[gpui_kit::test]
fn the_join_box_waits_for_its_answer(cx: &mut TestAppContext) {
    use crate::ui::folder_extras::{FolderNewChatsDialog, JoinStage};
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let Some(session) = app.demo_session.as_mut() else {
            return;
        };
        session.chat_list.folder_new_chats.insert(5, vec![1, 2]);
        app.folders.new_chats_dialog = Some(FolderNewChatsDialog {
            folder_id: 5,
            selected: [1, 2].into_iter().collect(),
            stage: JoinStage::Joining {
                request: RequestId(900),
            },
        });
        // Its request is gone and the offer stands: it failed.
        app.folder_new_chats_upkeep(window, cx);
        let dialog = app.folders.new_chats_dialog.as_ref();
        assert_eq!(dialog.map(|d| d.stage), Some(JoinStage::Choosing));
        assert_eq!(dialog.map(|d| d.selected.len()), Some(2), "ticks kept");
        if let Some(dialog) = app.folders.new_chats_dialog.as_mut() {
            dialog.stage = JoinStage::Joining {
                request: RequestId(901),
            };
        }
        // The offer is gone: it went through.
        if let Some(session) = app.demo_session.as_mut() {
            session.chat_list.folder_new_chats.remove(&5);
        }
        app.folder_new_chats_upkeep(window, cx);
        assert!(app.folders.new_chats_dialog.is_none());
    });
}

#[gpui_kit::test]
fn a_delete_in_flight_keeps_the_list_and_takes_no_second_request(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let delete = sent(app, RequestPurpose::DeleteChatFolderInviteLink, Some(5));
        let mut dialog = FolderShareDialog::new(5);
        dialog.view.set_op(LinkOp::Saving(delete));
        app.folders.share = Some(dialog);
        // Edit and a second delete wait for the first answer.
        app.start_folder_link_form(None, window, cx);
        assert!(matches!(
            app.folders.share.as_ref().map(|d| &d.view),
            Some(FolderShareView::List { .. })
        ));
        app.delete_folder_link("https://t.me/addlist/b", cx);
        assert_eq!(
            app.folders.share.as_ref().map(|d| d.view.op().clone()),
            Some(LinkOp::Saving(delete))
        );
        // The delete's refusal settles the list it came from.
        answer(app, delete, REFUSED);
        assert!(app.settle_folder_share(cx));
        let dialog = app.folders.share.as_ref().expect("open");
        assert!(matches!(dialog.view, FolderShareView::List { .. }));
        assert!(matches!(dialog.view.op(), LinkOp::Failed(_)));
    });
}

#[gpui_kit::test]
fn two_deletes_of_one_folder_each_settle_on_their_own_answer(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        let first = sent(app, RequestPurpose::DeleteChatFolder, Some(5));
        let second = sent(app, RequestPurpose::DeleteChatFolder, Some(5));
        app.folders.deletes.insert(first);
        app.folders.deletes.insert(second);
        answer(app, second, r#""@type":"ok""#);
        assert!(app.settle_folder_deletes(cx));
        assert!(
            app.folders.deletes.contains(&first),
            "the older delete still waits for its own answer"
        );
        answer(app, first, REFUSED);
        assert!(app.settle_folder_deletes(cx));
        assert!(app.folders.deletes.is_empty());
    });
}

#[gpui_kit::test]
fn a_replaced_link_check_leaves_no_request_and_no_answer_behind(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        let old = sent(app, RequestPurpose::CheckChatFolderInviteLink, None);
        app.folders.invite = Some(FolderInviteDialog::new(
            "first".into(),
            InviteStage::Checking(old),
        ));
        app.open_folder_invite("second".into(), cx);
        let demo = app.demo_session.as_mut().expect("demo session");
        assert!(demo.requests.get(old).is_none(), "the old check is dropped");
        // Its answer comes after the new link opened the box: nobody waits.
        answer(app, old, REFUSED);
        let demo = app.demo_session.as_mut().expect("demo session");
        assert!(
            demo.chat_list.dialog_answers.take(old).is_none(),
            "a late answer for a replaced check is not kept"
        );
        // Closing a box that still checks drops its request the same way.
        let checking = sent(app, RequestPurpose::CheckChatFolderInviteLink, None);
        app.folders.invite = Some(FolderInviteDialog::new(
            "third".into(),
            InviteStage::Checking(checking),
        ));
        app.close_folder_invite(cx);
        let demo = app.demo_session.as_mut().expect("demo session");
        assert!(demo.requests.get(checking).is_none());
    });
}

/// The form's Cancel goes back to the list with the save it sent: edit and
/// delete wait for that answer. A failed save is not carried over.
#[gpui_kit::test]
fn cancelling_the_form_keeps_the_save_it_sent(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |_, window, cx| {
        let mut form = FolderShareView::edit(
            Some("https://t.me/addlist/a".into()),
            LinkForm::new(window, cx, String::new(), Default::default()),
        );
        form.set_op(LinkOp::Saving(RequestId(4)));
        let list = form.cancelled_form();
        assert!(matches!(list, FolderShareView::List { .. }));
        assert_eq!(list.op(), &LinkOp::Saving(RequestId(4)));
        assert!(list.is_busy(), "edit and delete wait for the answer");
        form.set_op(LinkOp::Failed("no".into()));
        assert_eq!(form.cancelled_form().op(), &LinkOp::Idle);
    });
}

/// Sweep r4-08 fix round (16-004): an `addlist` link that came while an add
/// was out opens once the add settles, whichever way it ended, and an add
/// refusal the next box replaces is told in a toast. The link being added
/// queues nothing, and a close does not open it from inside the close.
mod queued_invite {
    use super::*;

    const ADDED: &str = r#""@type":"ok""#;
    const LIMIT: &str = r#""@type":"error","code":400,"message":"CHATLISTS_TOO_MUCH""#;

    fn start_add(app: &mut QuillApp) -> RequestId {
        let request = sent(app, RequestPurpose::AddChatFolderByInviteLink, None);
        let info = Box::new(quill::telegram::envelope::ChatFolderInviteLinkInfo {
            folder: quill::telegram::envelope::ChatFolderInfo {
                id: 0,
                name: "Work".into(),
                icon_name: String::new(),
                color_id: -1,
                is_shareable: false,
                has_my_invite_links: false,
            },
            missing_chat_ids: vec![],
            added_chat_ids: vec![],
        });
        app.folders.invite = Some(FolderInviteDialog::new(
            "first".into(),
            InviteStage::Adding {
                request,
                info,
                next: None,
            },
        ));
        request
    }

    fn opened_link(app: &QuillApp) -> Option<&str> {
        app.folders.invite.as_ref().map(|d| d.link.as_str())
    }

    fn notes_error(app: &QuillApp) -> bool {
        !app.screen.connection.status_note().is_empty()
    }

    #[gpui_kit::test]
    fn the_queued_link_opens_after_each_way_the_add_ends(cx: &mut TestAppContext) {
        for body in [ADDED, REFUSED, LIMIT] {
            let (app, mut vcx) = open(cx);
            app.update_in(&mut vcx, |app, _, cx| {
                let request = start_add(app);
                app.open_folder_invite("second".into(), cx);
                assert_eq!(opened_link(app), Some("first"));
                answer(app, request, body);
                assert!(app.drive_folder_invite(cx));
                assert_eq!(opened_link(app), Some("second"), "{body}");
                if body == REFUSED {
                    assert!(notes_error(app), "the refusal is told before the swap");
                }
            });
        }
    }

    #[gpui_kit::test]
    fn the_link_being_added_queues_nothing(cx: &mut TestAppContext) {
        let (app, mut vcx) = open(cx);
        app.update_in(&mut vcx, |app, _, cx| {
            let request = start_add(app);
            app.open_folder_invite("first".into(), cx);
            answer(app, request, ADDED);
            assert!(app.drive_folder_invite(cx));
            assert!(
                app.folders.invite.is_none(),
                "no second box for the folder just added"
            );
        });
    }

    #[gpui_kit::test]
    fn closing_keeps_the_queued_link_for_the_adds_answer(cx: &mut TestAppContext) {
        let (app, mut vcx) = open(cx);
        app.update_in(&mut vcx, |app, _, cx| {
            let request = start_add(app);
            app.open_folder_invite("second".into(), cx);
            // A caller that closes until the box is gone opens nothing.
            app.close_folder_invite(cx);
            app.close_folder_invite(cx);
            assert!(app.folders.invite.is_none());
            answer(app, request, ADDED);
            assert!(app.settle_folder_share(cx));
            assert_eq!(opened_link(app), Some("second"));
        });
    }
}

/// A dialog whose add is out, with `next` queued behind it.
fn adding(link: &str, request: RequestId, next: Option<&str>) -> FolderInviteDialog {
    let info = Box::new(quill::telegram::envelope::ChatFolderInviteLinkInfo {
        folder: quill::telegram::envelope::ChatFolderInfo {
            id: 0,
            name: "Work".into(),
            icon_name: String::new(),
            color_id: -1,
            is_shareable: false,
            has_my_invite_links: false,
        },
        missing_chat_ids: vec![],
        added_chat_ids: vec![],
    });
    FolderInviteDialog::new(
        link.into(),
        InviteStage::Adding {
            request,
            info,
            next: next.map(String::from),
        },
    )
}

/// Sweep r4-08 review: a link queued behind a closed add gives way to a
/// box the user opened since, and with two closed adds the newest link wins.
#[gpui_kit::test]
fn a_link_queued_behind_a_closed_add_does_not_replace_a_newer_box(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        let request = sent(app, RequestPurpose::AddChatFolderByInviteLink, None);
        app.folders.invite = Some(adding("one", request, Some("two")));
        app.close_folder_invite(cx);
        // Link three opens its own box meanwhile.
        app.open_folder_invite("three".into(), cx);
        answer(app, request, REFUSED);
        assert!(app.settle_folder_share(cx));
        let dialog = app.folders.invite.as_ref().expect("link three's box");
        assert_eq!(dialog.link, "three", "the older queued link is dropped");
    });
}

#[gpui_kit::test]
fn the_newest_queued_link_opens_when_closed_adds_settle_together(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        let first = sent(app, RequestPurpose::AddChatFolderByInviteLink, None);
        app.folders.invite = Some(adding("a", first, Some("older")));
        app.close_folder_invite(cx);
        let second = sent(app, RequestPurpose::AddChatFolderByInviteLink, None);
        app.folders.invite = Some(adding("b", second, Some("newer")));
        app.close_folder_invite(cx);
        answer(app, first, REFUSED);
        answer(app, second, REFUSED);
        assert!(app.settle_folder_share(cx));
        let dialog = app.folders.invite.as_ref().expect("a box opens");
        assert_eq!(dialog.link, "newer");
    });
}

/// A closed add whose answer the ring evicted still opens its queued link.
#[gpui_kit::test]
fn an_evicted_closed_add_still_opens_its_queued_link(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        let request = sent(app, RequestPurpose::AddChatFolderByInviteLink, None);
        app.folders.invite = Some(adding("one", request, Some("two")));
        app.close_folder_invite(cx);
        answer(app, request, REFUSED);
        let demo = app.demo_session.as_mut().expect("demo session");
        assert!(demo.chat_list.dialog_answers.take(request).is_some());
        app.settle_folder_share(cx);
        let dialog = app.folders.invite.as_ref().expect("a box opens");
        assert_eq!(dialog.link, "two");
    });
}
