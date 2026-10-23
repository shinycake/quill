//! Screenshot demos of New Group / New Channel (injected, no live
//! Telegram): each step with typed fields; the steps after creation run
//! over a channel the session reports as just made.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::diagnostics::DiagnosticSink;
use quill::ids::{ChatId, RequestId};
use quill::new_chat::{CreationOutcome, NewChatBack, NewChatKind, NewChatStage};
use quill::state::NewChatCreation;
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::UsernameCheckResult;
use std::sync::Arc;
use std::sync::atomic::Ordering;

register_demos![
    DemoSpec::chats("ready-new-group", "screenshot demo — New Group")
        .setup(QuillApp::demo_new_group),
    DemoSpec::chats(
        "ready-new-group-members",
        "screenshot demo — New Group members"
    )
    .setup(QuillApp::demo_new_group_members),
    DemoSpec::chats(
        "ready-new-group-auto-delete",
        "screenshot demo — New Group auto-delete"
    )
    .setup(QuillApp::demo_new_group_auto_delete),
    DemoSpec::chats("ready-new-channel", "screenshot demo — New Channel")
        .setup(QuillApp::demo_new_channel),
    DemoSpec::chats(
        "ready-new-channel-type",
        "screenshot demo — New Channel type"
    )
    .setup(QuillApp::demo_new_channel_type),
    DemoSpec::chats(
        "ready-new-channel-private",
        "screenshot demo — New Channel private"
    )
    .setup(QuillApp::demo_new_channel_private),
    DemoSpec::chats(
        "ready-new-channel-subscribers",
        "screenshot demo — New Channel subscribers"
    )
    .setup(QuillApp::demo_new_channel_subscribers),
];

/// The channel the demos pretend was just created.
pub(super) const NEW_CHANNEL: i64 = 91;
/// The create request the demos pretend was answered.
pub(super) const CREATE_REQUEST: RequestId = RequestId(9_100);
const CREATOR: &str = r#"{"@type":"chatMemberStatusCreator","is_member":true}"#;
const CONTACTS: [(i64, &str, &str); 5] = [
    (9_201, "Maya", "Levin"),
    (9_202, "Daniel", "Ortiz"),
    (9_203, "Priya", "Raman"),
    (9_204, "Tom", "Becker"),
    (9_205, "Lena", "Novak"),
];
/// The link the type demo types.
pub(super) const LINK: &str = "quillnotes";

impl QuillApp {
    fn demo_ingest(&mut self, updates: &[String]) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        let sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        for json in updates {
            if let Some(owned) = copy_and_parse(json, &self.demo_ui.seq, &sink) {
                session.apply(owned);
            }
        }
    }

    /// The viewer's contacts for the pickers; returns the first `picked`.
    fn demo_contacts(&mut self, picked: usize) -> Vec<i64> {
        let updates: Vec<String> = CONTACTS
            .iter()
            .map(|(id, first, last)| {
                format!(
                    r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"{last}","is_contact":true,"status":{{"@type":"userStatusRecently"}},"type":{{"@type":"userTypeRegular"}}}}}}"#
                )
            })
            .collect();
        self.demo_ingest(&updates);
        let ids: Vec<i64> = CONTACTS.iter().map(|(id, ..)| *id).collect();
        if let Some(session) = self.demo_session.as_mut() {
            session.users_state.contacts = Some(ids.clone());
        }
        ids.into_iter().take(picked).collect()
    }

    fn demo_new_chat(
        &mut self,
        kind: NewChatKind,
        title: &str,
        description: &str,
        note: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_new_chat(kind, window, cx);
        if let Some(dialog) = self.admin.create_chat_dialog.as_ref() {
            dialog
                .title_input
                .update(cx, |input, cx| input.set_value(title, window, cx));
            dialog
                .description_input
                .update(cx, |input, cx| input.set_value(description, window, cx));
        }
        self.connection.set_note(StatusNote::silent(note));
        cx.notify();
    }

    fn demo_new_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_new_chat(
            NewChatKind::Group,
            "Weekend Hikers",
            "",
            "screenshot demo — New Group",
            window,
            cx,
        );
    }

    fn demo_new_group_members(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_new_group(window, cx);
        let picks = self.demo_contacts(2);
        if let Some(dialog) = self.admin.create_chat_dialog.as_mut() {
            dialog.stage = NewChatStage::Members;
            dialog.selected_users = picks;
        }
        if let Some(session) = self.demo_session.as_mut() {
            session.groups.supergroup_size_max = 200_000;
        }
        cx.notify();
    }

    fn demo_new_group_auto_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_new_group(window, cx);
        if let Some(dialog) = self.admin.create_chat_dialog.as_mut() {
            dialog.auto_delete = Some(604_800);
        }
        self.open_new_chat_auto_delete(window, cx);
    }

    fn demo_new_channel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_new_chat(
            NewChatKind::Channel,
            "Quill Notes",
            "Release notes and tips for every Quill build.",
            "screenshot demo — New Channel",
            window,
            cx,
        );
    }

    /// The channel as TDLib reports a fresh one, and the create answered.
    fn demo_channel_created(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_new_channel(window, cx);
        self.demo_ingest(&[
            format!(
                r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{NEW_CHANNEL},"is_channel":true,"status":{CREATOR},"member_count":1}}}}"#
            ),
            format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{NEW_CHANNEL},"title":"Quill Notes","type":{{"@type":"chatTypeSupergroup","supergroup_id":{NEW_CHANNEL},"is_channel":true}},"unread_count":0,"can_be_deleted_for_all_users":true}}}}"#
            ),
        ]);
        if let Some(session) = self.demo_session.as_mut() {
            session.groups.new_chats.creations.insert(
                CREATE_REQUEST,
                NewChatCreation {
                    outcome: Some(CreationOutcome::Created(ChatId(NEW_CHANNEL))),
                    ..NewChatCreation::default()
                },
            );
        }
        if let Some(dialog) = self.admin.create_chat_dialog.as_mut() {
            dialog.stage = NewChatStage::Creating {
                request: CREATE_REQUEST,
                back: NewChatBack::Info,
            };
        }
        cx.notify();
    }

    fn demo_new_channel_type(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_channel_created(window, cx);
        if let Some(type_box) = self.group_type_box() {
            type_box
                .link_input
                .update(cx, |input, cx| input.set_value(LINK, window, cx));
        }
        self.demo_answer_username_check(ChatId(NEW_CHANNEL), LINK, UsernameCheckResult::Available);
        cx.notify();
    }

    fn demo_new_channel_private(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_channel_created(window, cx);
        self.choose_group_public(false, cx);
        if let Some(session) = self.demo_session.as_mut() {
            session.groups.invite_links.insert(
                NEW_CHANNEL,
                quill::state::InviteLinkFetch::Loaded(quill::state::InviteLinkList {
                    total_count: 1,
                    links: vec![quill::telegram::envelope::ParsedChatInviteLink {
                        invite_link: "https://t.me/+QnOtEs7xY2a".to_string(),
                        name: String::new(),
                        creator_user_id: 0,
                        date: 0,
                        edit_date: 0,
                        expiration_date: 0,
                        subscription_pricing: None,
                        member_limit: 0,
                        member_count: 0,
                        expired_member_count: 0,
                        pending_join_request_count: 0,
                        creates_join_request: false,
                        is_primary: true,
                        is_revoked: false,
                    }],
                }),
            );
        }
        cx.notify();
    }

    fn demo_new_channel_subscribers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_channel_created(window, cx);
        let picks = self.demo_contacts(1);
        if let Some(dialog) = self.admin.create_chat_dialog.as_mut() {
            dialog.stage = NewChatStage::Subscribers {
                chat: ChatId(NEW_CHANNEL),
            };
            dialog.selected_users = picks;
        }
        cx.notify();
    }
}
