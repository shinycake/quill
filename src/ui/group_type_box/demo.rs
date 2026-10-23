//! Screenshot demos of the Type box (injected, no live Telegram): a public
//! group the viewer created with three links, the same box after a taken
//! link was typed, and a private channel with its primary link.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::{InviteLinkFetch, InviteLinkList, Session, UsernameVerdict};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{ParsedChatInviteLink, UsernameCheckResult};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

register_demos![
    DemoSpec::chats("ready-group-type-box", "screenshot demo — group Type box")
        .setup(QuillApp::demo_group_type_box),
    DemoSpec::chats(
        "ready-group-type-taken",
        "screenshot demo — group Type box, taken link"
    )
    .setup(QuillApp::demo_group_type_taken),
    DemoSpec::chats(
        "ready-channel-type-private",
        "screenshot demo — private channel Type box"
    )
    .setup(QuillApp::demo_channel_type_private),
];

pub(super) const PUBLIC_GROUP: i64 = 71;
const PRIVATE_CHANNEL: i64 = 72;
/// The link the taken-link demo types.
pub(super) const TAKEN: &str = "quillchat";
const CREATOR: &str = r#"{"@type":"chatMemberStatusCreator","is_member":true}"#;

fn apply(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64, jsons: &[String]) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    for json in jsons {
        if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

fn seed_type_box_chats(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let position = |id: i64| {
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"78","is_pinned":false}}}}"#
        )
    };
    apply(
        session,
        sink,
        seq,
        &[
            format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{PUBLIC_GROUP},"title":"Bay Area Hikers","type":{{"@type":"chatTypeSupergroup","supergroup_id":{PUBLIC_GROUP},"is_channel":false}},"unread_count":0,"can_be_deleted_for_all_users":true}}}}"#
            ),
            position(PUBLIC_GROUP),
            format!(
                r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{PUBLIC_GROUP},"is_channel":false,"status":{CREATOR},"member_count":412,"usernames":{{"@type":"usernames","active_usernames":["bayhikers","hikesf"],"disabled_usernames":["oldtrails"],"editable_username":"bayhikers","collectible_usernames":[]}}}}}}"#
            ),
            format!(
                r#"{{"@type":"updateSupergroupFullInfo","supergroup_id":{PUBLIC_GROUP},"supergroup_full_info":{{"description":"Weekend hikes around the bay.","member_count":412}}}}"#
            ),
            format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{PRIVATE_CHANNEL},"title":"Quill Nightly","type":{{"@type":"chatTypeSupergroup","supergroup_id":{PRIVATE_CHANNEL},"is_channel":true}},"unread_count":0,"can_be_deleted_for_all_users":true}}}}"#
            ),
            position(PRIVATE_CHANNEL),
            format!(
                r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{PRIVATE_CHANNEL},"is_channel":true,"status":{CREATOR},"member_count":96}}}}"#
            ),
            format!(
                r#"{{"@type":"updateSupergroupFullInfo","supergroup_id":{PRIVATE_CHANNEL},"supergroup_full_info":{{"description":"Nightly builds.","member_count":96}}}}"#
            ),
        ],
    );
    session.groups.invite_links.insert(
        PRIVATE_CHANNEL,
        InviteLinkFetch::Loaded(InviteLinkList {
            total_count: 1,
            links: vec![ParsedChatInviteLink {
                invite_link: "https://t.me/+QuIlLnIgHtLy42".to_string(),
                name: String::new(),
                creator_user_id: 0,
                date: 0,
                edit_date: 0,
                expiration_date: 0,
                subscription_pricing: None,
                member_limit: 0,
                member_count: 37,
                expired_member_count: 0,
                pending_join_request_count: 0,
                creates_join_request: false,
                is_primary: true,
                is_revoked: false,
            }],
        }),
    );
}

impl QuillApp {
    fn demo_type_box(
        &mut self,
        chat: i64,
        note: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            seed_type_box_chats(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.open_group_edit_box(ChatId(chat), window, cx);
        self.open_group_type_box(window, cx);
        self.connection.set_note(StatusNote::silent(note));
        cx.notify();
    }

    fn demo_group_type_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_type_box(PUBLIC_GROUP, "screenshot demo — group Type box", window, cx);
    }

    fn demo_group_type_taken(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_type_box(
            PUBLIC_GROUP,
            "screenshot demo — group Type box, taken link",
            window,
            cx,
        );
        if let Some(type_box) = self.group_type_box() {
            type_box
                .link_input
                .update(cx, |input, cx| input.set_value(TAKEN, window, cx));
        }
        self.demo_answer_username_check(ChatId(PUBLIC_GROUP), TAKEN, UsernameCheckResult::Occupied);
        cx.notify();
    }

    fn demo_channel_type_private(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_type_box(
            PRIVATE_CHANNEL,
            "screenshot demo — private channel Type box",
            window,
            cx,
        );
    }

    /// What `checkChatUsername` would answer, put in the demo session.
    pub(in crate::ui) fn demo_answer_username_check(
        &mut self,
        chat_id: ChatId,
        username: &str,
        result: UsernameCheckResult,
    ) {
        if let Some(session) = self.demo_session.as_mut() {
            session.groups.group_type.check = Some(UsernameVerdict {
                chat_id,
                username: username.to_string(),
                result,
            });
        }
    }
}
