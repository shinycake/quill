//! Performance fixture: `QUILL_DEMO_UPDATE_STREAM=<updates per second>`
//! feeds a screenshot demo a steady stream of synthetic TDLib updates,
//! through the real reducer and the poll loop's redraw policy, so the
//! idle cost of a signed-in account (contacts going on and offline,
//! people typing in other chats, unread totals, automatic downloads) can
//! be measured on a fixture.
//!
//! The mix repeats every eight updates: a contact goes offline, someone
//! starts typing in another chat, an update Quill does not parse, the
//! contact comes back online, the typing stops, the unread totals change,
//! an automatic download progresses, another chat is read elsewhere.

use super::app::QuillApp;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::telegram::client::OwnedEnvelope;
use quill::telegram::envelope::{ChatKind, parse_envelope};
use std::time::Duration;

/// File id the stream reports automatic download progress for.
const STREAM_FILE_ID: i32 = 990_001;

pub(super) fn update_stream_rate() -> Option<u32> {
    std::env::var("QUILL_DEMO_UPDATE_STREAM")
        .ok()?
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|rate| *rate > 0)
}

impl QuillApp {
    pub(super) fn spawn_demo_update_stream(&mut self, rate: u32, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let mut step = 0_u64;
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs_f64(1.0 / f64::from(rate)))
                    .await;
                if this
                    .update(cx, |this, cx| this.demo_stream_step(step, cx))
                    .is_err()
                {
                    break;
                }
                step += 1;
            }
        })
        .detach();
    }

    fn demo_stream_step(&mut self, step: u64, cx: &mut Context<Self>) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let open = session.open_chat;
        // A private chat and another chat, neither of them open.
        let mut others: Vec<(ChatId, Option<i64>)> = session
            .chats
            .values()
            .filter(|chat| Some(chat.id) != open)
            .map(|chat| {
                let user = match chat.kind {
                    ChatKind::Private { user_id } => Some(user_id.0),
                    _ => None,
                };
                (chat.id, user)
            })
            .collect();
        others.sort_by_key(|(id, _)| id.0);
        let Some(&(private_chat, Some(user))) = others.iter().find(|(_, user)| user.is_some())
        else {
            return;
        };
        let other = others
            .iter()
            .map(|(id, _)| *id)
            .find(|id| *id != private_chat)
            .unwrap_or(private_chat);
        session.downloading.insert(STREAM_FILE_ID);
        let json = match step % 8 {
            0 => format!(
                r#"{{"@type":"updateUserStatus","user_id":{user},"status":{{"@type":"userStatusOffline","was_online":{step}}}}}"#
            ),
            1 => typing(other.0, user, "chatActionTyping"),
            2 => r#"{"@type":"updateHavePendingNotifications","have_delayed_notifications":false,"have_unreceived_notifications":false}"#.to_string(),
            3 => format!(
                r#"{{"@type":"updateUserStatus","user_id":{user},"status":{{"@type":"userStatusOnline","expires":{step}}}}}"#
            ),
            4 => typing(other.0, user, "chatActionCancel"),
            5 => format!(
                r#"{{"@type":"updateUnreadChatCount","chat_list":{{"@type":"chatListMain"}},"total_count":40,"unread_count":{},"unread_unmuted_count":1,"marked_as_unread_count":0,"marked_as_unread_unmuted_count":0}}"#,
                step % 5
            ),
            6 => format!(
                r#"{{"@type":"updateFile","file":{{"@type":"file","id":{STREAM_FILE_ID},"size":100000,"expected_size":100000,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":true,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":{}}},"remote":{{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":100000}}}}}}"#,
                step % 100_000
            ),
            _ => format!(
                r#"{{"@type":"updateChatReadOutbox","chat_id":{},"last_read_outbox_message_id":{step}}}"#,
                other.0
            ),
        };
        let Ok(envelope) = parse_envelope(&json) else {
            return;
        };
        let need = quill::state::redraw_need(session, &envelope);
        let seq = session.last_seq + 1;
        session.apply(OwnedEnvelope {
            seq,
            client_id: None,
            envelope,
        });
        self.redraw_polled(need, cx);
    }
}

fn typing(chat_id: i64, user_id: i64, action: &str) -> String {
    format!(
        r#"{{"@type":"updateChatAction","chat_id":{chat_id},"topic_id":null,"sender_id":{{"@type":"messageSenderUser","user_id":{user_id}}},"action":{{"@type":"{action}"}}}}"#
    )
}
