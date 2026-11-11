//! `QUILL_DEMO_DIRECT=admin` for `ready-direct-messages`: the same channel
//! as its owner, whose direct messages hold three people's chats (two
//! unread, one marked unread by hand). Maya Chen's chat is open: her tab,
//! her name and "N messages" in the header, and only her messages.
//! `admin-all` stays in All, where the composer asks to choose a message
//! to reply to.
#![deny(clippy::indexing_slicing)]

use super::direct_messages::{CHANNEL, CHANNEL_GROUP, DIRECT, DIRECT_GROUP};
use crate::ui::app::QuillApp;
use gpui_kit::*;
use quill::diagnostics::DiagnosticSink;
use quill::ids::ChatId;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::Ordering;

/// The people writing to the channel, their topic id = their user id.
pub(super) const MAYA: i64 = 501;
pub(super) const SAM: i64 = 502;
pub(super) const PRIYA: i64 = 503;

#[derive(Clone, Copy)]
struct DemoMessage {
    id: i64,
    topic: i64,
    /// The channel answering.
    outgoing: bool,
    text: &'static str,
}

const fn msg(id: i64, topic: i64, outgoing: bool, text: &'static str) -> DemoMessage {
    DemoMessage {
        id,
        topic,
        outgoing,
        text,
    }
}

const MESSAGES: [DemoMessage; 7] = [
    msg(
        1001,
        MAYA,
        false,
        "Hi! Is the Friday issue still going out?",
    ),
    msg(1002, MAYA, true, "Yes, it goes out at 9 am as usual."),
    msg(1003, MAYA, false, "Great. Could you cover the new release?"),
    msg(1004, MAYA, false, "Happy to send my notes on it."),
    msg(2001, SAM, false, "Loved the piece on Linux packaging."),
    msg(3001, PRIYA, false, "Thanks for letting me write for free!"),
    msg(3002, PRIYA, true, "Any time. Enjoy the issue."),
];

fn message_json(message: DemoMessage) -> String {
    let DemoMessage {
        id,
        topic,
        outgoing,
        text,
    } = message;
    let sender = if outgoing {
        format!(r#"{{"@type":"messageSenderChat","chat_id":{CHANNEL}}}"#)
    } else {
        format!(r#"{{"@type":"messageSenderUser","user_id":{topic}}}"#)
    };
    let date = 1_762_000_000 + id;
    format!(
        r#"{{"id":{id},"chat_id":{DIRECT},"sender_id":{sender},"is_outgoing":{outgoing},"date":{date},"topic_id":{{"@type":"messageTopicDirectMessages","direct_messages_chat_topic_id":{topic}}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}"#
    )
}

fn last_message(topic: i64) -> Option<DemoMessage> {
    MESSAGES.iter().rev().find(|m| m.topic == topic).copied()
}

fn topic_json(topic: i64, order: i64, unread: i32, read: i64, marked: bool, free: bool) -> String {
    let last = last_message(topic).map_or_else(|| "null".to_string(), message_json);
    format!(
        r#"{{"@type":"updateDirectMessagesChatTopic","topic":{{"@type":"directMessagesChatTopic","chat_id":{DIRECT},"id":{topic},"sender_id":{{"@type":"messageSenderUser","user_id":{topic}}},"order":"{order}","can_send_unpaid_messages":{free},"is_marked_as_unread":{marked},"unread_count":{unread},"last_read_inbox_message_id":{read},"last_read_outbox_message_id":0,"unread_reaction_count":0,"last_message":{last},"draft_message":null}}}}"#
    )
}

impl QuillApp {
    pub(super) fn demo_apply_jsons(&mut self, jsons: impl IntoIterator<Item = String>) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        let sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        for json in jsons {
            if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &sink) {
                session.apply(owned);
            }
        }
    }

    /// The channel's owner side; `all` keeps All open.
    pub(super) fn demo_direct_admin(
        &mut self,
        all: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let people = [
            (MAYA, "Maya", "Chen"),
            (SAM, "Sam", "Ortiz"),
            (PRIYA, "Priya", "Nair"),
        ];
        let mut jsons: Vec<String> = people
            .iter()
            .map(|(id, first, last)| {
                format!(
                    r#"{{"@type":"updateUser","user":{{"id":{id},"first_name":"{first}","last_name":"{last}","type":{{"@type":"userTypeRegular"}}}}}}"#
                )
            })
            .collect();
        jsons.extend([
            format!(
                r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{CHANNEL_GROUP},"is_channel":true,"status":{{"@type":"chatMemberStatusCreator","is_anonymous":false,"is_member":true}},"member_count":48210,"has_linked_chat":true,"has_direct_messages_group":true}}}}"#
            ),
            format!(
                r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{DIRECT_GROUP},"is_channel":false,"is_direct_messages_group":true,"is_administered_direct_messages_group":true,"status":{{"@type":"chatMemberStatusAdministrator","can_be_edited":false,"rights":{{"@type":"chatAdministratorRights","can_manage_direct_messages":true}}}},"paid_message_star_count":50}}}}"#
            ),
            format!(
                r#"{{"@type":"updateChatPosition","chat_id":{DIRECT},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"95","is_pinned":false}}}}"#
            ),
            format!(
                r#"{{"@type":"updateSupergroupFullInfo","supergroup_id":{DIRECT_GROUP},"supergroup_full_info":{{"description":"","direct_messages_chat_id":{CHANNEL},"outgoing_paid_message_star_count":0}}}}"#
            ),
            format!(
                r#"{{"@type":"updateChatLastMessage","chat_id":{DIRECT},"last_message":{},"positions":[{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"95","is_pinned":false}}]}}"#,
                last_message(MAYA).map_or_else(|| "null".to_string(), message_json)
            ),
            format!(
                r#"{{"@type":"updateChatReadInbox","chat_id":{DIRECT},"last_read_inbox_message_id":1002,"unread_count":2}}"#
            ),
            topic_json(MAYA, 300, 2, 1002, false, false),
            topic_json(SAM, 200, 0, 2001, true, false),
            topic_json(PRIYA, 100, 0, 3001, false, true),
        ]);
        self.demo_apply_jsons(jsons);
        if !all && let Some(session) = self.demo_session.as_mut() {
            session.select_direct_topic(ChatId(DIRECT), Some(MAYA));
        }
        self.select_listed_chat(ChatId(DIRECT), window, cx);
        self.demo_refill_direct_history(ChatId(DIRECT));
        cx.notify();
    }

    /// Puts the open conversation's messages (or all of them, in All)
    /// back after a switch; the session keeps only the open one's.
    pub(in crate::ui) fn demo_refill_direct_history(&mut self, chat_id: ChatId) {
        if chat_id != ChatId(DIRECT) {
            return;
        }
        self.demo_apply_jsons(MESSAGES.iter().map(|message| {
            format!(
                r#"{{"@type":"updateNewMessage","message":{}}}"#,
                message_json(*message)
            )
        }));
        if let Some(session) = self.demo_session.as_mut() {
            session
                .histories
                .entry(DIRECT)
                .or_default()
                .set_loaded_complete(true);
            if let Some(topic) = session.open_direct_topic(chat_id) {
                let count = MESSAGES.iter().filter(|m| m.topic == topic).count();
                session
                    .threads
                    .direct
                    .message_counts
                    .insert((DIRECT, topic), i32::try_from(count).unwrap_or(0));
            }
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "direct_messages_admin_tests.rs"]
mod tests;
