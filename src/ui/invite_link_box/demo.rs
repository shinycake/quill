//! Screenshot demos of the link box (injected, no live Telegram): a new
//! link of a private channel, the same box with a monthly fee, an existing
//! group link with a custom date and limit, and the Custom date picker.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::local_time::now_unix;
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::ParsedChatInviteLink;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

register_demos![
    DemoSpec::chats("ready-invite-link-new", "screenshot demo — New Link box")
        .setup(QuillApp::demo_invite_link_new),
    DemoSpec::chats(
        "ready-invite-link-fee",
        "screenshot demo — New Link box, monthly fee"
    )
    .setup(QuillApp::demo_invite_link_fee),
    DemoSpec::chats("ready-invite-link-edit", "screenshot demo — Edit Link box")
        .setup(QuillApp::demo_invite_link_edit),
    DemoSpec::chats(
        "ready-invite-link-custom-date",
        "screenshot demo — link expiry date picker"
    )
    .setup(QuillApp::demo_invite_link_custom_date),
];

const CHANNEL: i64 = 81;
const GROUP: i64 = 82;
const CREATOR: &str = r#"{"@type":"chatMemberStatusCreator","is_member":true}"#;

fn apply(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64, jsons: &[String]) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    for json in jsons {
        if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

fn seed_chats(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let chat = |id: i64, title: &str, channel: bool, members: i32| {
        [
            format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{{"@type":"chatTypeSupergroup","supergroup_id":{id},"is_channel":{channel}}},"unread_count":0,"can_be_deleted_for_all_users":true}}}}"#
            ),
            format!(
                r#"{{"@type":"updateChatPosition","chat_id":{id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"77","is_pinned":false}}}}"#
            ),
            format!(
                r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{id},"is_channel":{channel},"status":{CREATOR},"member_count":{members}}}}}"#
            ),
        ]
    };
    let mut jsons = Vec::new();
    jsons.extend(chat(CHANNEL, "Quill Insiders", true, 640));
    jsons.extend(chat(GROUP, "Bay Area Hikers", false, 412));
    apply(session, sink, seq, &jsons);
}

/// A group link that ends in five days and lets 25 people in.
pub(super) fn demo_link() -> ParsedChatInviteLink {
    let expires = now_unix().saturating_add(5 * 86_400);
    ParsedChatInviteLink {
        invite_link: "https://t.me/+HiKeRs5dAyS".to_string(),
        name: "Spring hikes".to_string(),
        creator_user_id: 0,
        date: 0,
        edit_date: 0,
        expiration_date: i32::try_from(expires).unwrap_or(i32::MAX),
        subscription_pricing: None,
        member_limit: 25,
        member_count: 9,
        expired_member_count: 0,
        pending_join_request_count: 0,
        creates_join_request: false,
        is_primary: false,
        is_revoked: false,
    }
}

impl QuillApp {
    fn demo_invite_link(
        &mut self,
        chat: i64,
        link: Option<&ParsedChatInviteLink>,
        note: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            seed_chats(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.open_invite_link_box(ChatId(chat), link, window, cx);
        self.connection.set_note(StatusNote::silent(note));
        cx.notify();
    }

    fn demo_invite_link_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_invite_link(CHANNEL, None, "screenshot demo — New Link box", window, cx);
    }

    fn demo_invite_link_fee(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_invite_link(
            CHANNEL,
            None,
            "screenshot demo — New Link box, monthly fee",
            window,
            cx,
        );
        self.edit_invite_link_form(cx, |form| {
            form.toggle_subscription();
        });
        if let Some(link_box) = self.invite_link_box() {
            link_box
                .stars_input
                .update(cx, |input, cx| input.set_value("250", window, cx));
        }
    }

    fn demo_invite_link_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let link = demo_link();
        self.demo_invite_link(
            GROUP,
            Some(&link),
            "screenshot demo — Edit Link box",
            window,
            cx,
        );
    }

    fn demo_invite_link_custom_date(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_invite_link(
            GROUP,
            None,
            "screenshot demo — link expiry date picker",
            window,
            cx,
        );
        self.open_invite_link_custom_date(window, cx);
    }
}
