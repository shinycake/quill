//! `ready-direct-messages`: a verified channel and its direct messages,
//! which charge 50 Stars a message, as a subscriber who has not written
//! yet (injected, no live Telegram). The direct messages chat is open:
//! back arrow, "Direct messages", the price card and the paid composer.
//!
//! `QUILL_DEMO_DIRECT=channel` opens the channel instead, for its bar with
//! the direct messages button, Mute and Discuss; `free` makes the direct
//! messages free; `admin` and `admin-all` show them to the channel's owner
//! (`direct_messages_admin.rs`).
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::diagnostics::DiagnosticSink;
use quill::ids::ChatId;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::Ordering;

register_demos![
    DemoSpec::chats(
        "ready-direct-messages",
        "screenshot demo — a channel's paid direct messages, empty"
    )
    .setup(QuillApp::demo_direct_messages),
];

pub(super) const CHANNEL: i64 = -1_000_000_000_081;
pub(super) const CHANNEL_GROUP: i64 = 81;
pub(super) const DIRECT: i64 = -1_000_000_000_082;
pub(super) const DIRECT_GROUP: i64 = 82;
const DISCUSSION: i64 = -1_000_000_000_083;
const DISCUSSION_GROUP: i64 = 83;

impl QuillApp {
    fn demo_direct_messages(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = std::env::var("QUILL_DEMO_DIRECT").unwrap_or_default();
        let stars = if mode == "free" { 0 } else { 50 };
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        let sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        let verified = r#"{"@type":"verificationStatus","is_verified":true,"is_scam":false,"is_fake":false,"bot_verification_icon_custom_emoji_id":"0"}"#;
        let jsons = [
            format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{CHANNEL},"title":"Quill Weekly","type":{{"@type":"chatTypeSupergroup","supergroup_id":{CHANNEL_GROUP},"is_channel":true}},"unread_count":0}}}}"#
            ),
            format!(
                r#"{{"@type":"updateChatPosition","chat_id":{CHANNEL},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"90","is_pinned":false}}}}"#
            ),
            format!(
                r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{CHANNEL_GROUP},"is_channel":true,"status":{{"@type":"chatMemberStatusMember"}},"member_count":48210,"has_linked_chat":true,"has_direct_messages_group":true,"verification_status":{verified}}}}}"#
            ),
            format!(
                r#"{{"@type":"updateSupergroupFullInfo","supergroup_id":{CHANNEL_GROUP},"supergroup_full_info":{{"description":"","member_count":48210,"linked_chat_id":{DISCUSSION},"direct_messages_chat_id":{DIRECT}}}}}"#
            ),
            format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{DISCUSSION},"title":"Quill Weekly Chat","type":{{"@type":"chatTypeSupergroup","supergroup_id":{DISCUSSION_GROUP},"is_channel":false}},"unread_count":0}}}}"#
            ),
            format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{DIRECT},"title":"Quill Weekly","type":{{"@type":"chatTypeSupergroup","supergroup_id":{DIRECT_GROUP},"is_channel":false}},"unread_count":0}}}}"#
            ),
            format!(
                r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{DIRECT_GROUP},"is_channel":false,"is_direct_messages_group":true,"is_administered_direct_messages_group":false,"status":{{"@type":"chatMemberStatusLeft"}},"paid_message_star_count":{stars}}}}}"#
            ),
            format!(
                r#"{{"@type":"updateSupergroupFullInfo","supergroup_id":{DIRECT_GROUP},"supergroup_full_info":{{"description":"","direct_messages_chat_id":{CHANNEL},"outgoing_paid_message_star_count":{stars}}}}}"#
            ),
        ];
        for json in jsons {
            if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &sink) {
                session.apply(owned);
            }
        }
        session
            .histories
            .entry(DIRECT)
            .or_default()
            .set_loaded_complete(true);
        if let Some(view) = mode.strip_prefix("admin") {
            self.demo_direct_admin(view == "-all", window, cx);
            return;
        }
        let open = if mode == "channel" { CHANNEL } else { DIRECT };
        self.select_listed_chat(ChatId(open), window, cx);
        cx.notify();
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "direct_messages_tests.rs"]
mod tests;
