//! Screenshot demo for the forward box (`forward_box`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::*;
use quill::ids::ChatId;

impl QuillApp {
    /// Demo: the box over a chat list with Saved Messages, people (online,
    /// last seen, a bot), a group, a channel and right-to-left names; two
    /// chats chosen. `QUILL_DEMO_FORWARD_BOX=empty` opens it with nothing
    /// chosen.
    fn demo_ready_forward_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use quill::composer::ForwardDraft;
        use quill::ids::MessageId;
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui
            .seq
            .store(session.last_seq, std::sync::atomic::Ordering::SeqCst);
        let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
            self.demo_ui.sink.clone();
        session.my_user_id = Some(99);
        let private = |id: i64| format!(r#"{{"@type":"chatTypePrivate","user_id":{id}}}"#);
        let user = |id: i64, first: &str, status: &str, kind: &str| {
            format!(
                r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"","usernames":null,"phone_number":"","status":{status},"profile_photo":null,"is_contact":true,"is_mutual_contact":false,"is_close_friend":false,"is_verified":false,"is_premium":false,"is_scam":false,"is_fake":false,"type":{{"@type":"{kind}"}}}}}}"#
            )
        };
        let chats: [(i64, &str, String, i64); 8] = [
            (99, "Demo me", private(99), 5),
            (61, "נועה פרץ", private(61), 9),
            (62, "Daniel Katz", private(62), 8),
            (
                63,
                "צוות הפיתוח של קוויל",
                r#"{"@type":"chatTypeBasicGroup","basic_group_id":63}"#.to_string(),
                7,
            ),
            (64, "Weather Bot", private(64), 6),
            (
                65,
                "A very long chat title that keeps going past the edge of the box",
                private(65),
                4,
            ),
            (66, "יעל דגן", private(66), 3),
            (
                67,
                "Book Club",
                r#"{"@type":"chatTypeBasicGroup","basic_group_id":67}"#.to_string(),
                2,
            ),
        ];
        let mut jsons = vec![
            user(61, "נועה", r#"{"@type":"userStatusOnline","expires":4102444800}"#, "userTypeRegular"),
            user(62, "Daniel", r#"{"@type":"userStatusRecently","by_my_privacy_settings":false}"#, "userTypeRegular"),
            user(64, "Weather", r#"{"@type":"userStatusEmpty"}"#, "userTypeBot"),
            user(65, "Long", r#"{"@type":"userStatusLastWeek","by_my_privacy_settings":false}"#, "userTypeRegular"),
            user(66, "יעל", r#"{"@type":"userStatusOffline","was_online":1790600000}"#, "userTypeRegular"),
            r#"{"@type":"updateBasicGroup","basic_group":{"@type":"basicGroup","id":63,"member_count":14,"status":{"@type":"chatMemberStatusMember"},"is_active":true,"upgraded_to_supergroup_id":0}}"#.to_string(),
            r#"{"@type":"updateBasicGroup","basic_group":{"@type":"basicGroup","id":67,"member_count":6,"status":{"@type":"chatMemberStatusMember"},"is_active":true,"upgraded_to_supergroup_id":0}}"#.to_string(),
        ];
        for (id, title, kind, order) in chats {
            jsons.push(format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{kind},"unread_count":0}}}}"#
            ));
            jsons.push(format!(
                r#"{{"@type":"updateChatPosition","chat_id":{id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{order}","is_pinned":false}}}}"#
            ));
        }
        for json in jsons {
            if let Some(owned) =
                quill::telegram::client::copy_and_parse(&json, &self.demo_ui.seq, &sink)
            {
                session.apply(owned);
            }
        }
        if let Some(mut draft) = ForwardDraft::from_message(ChatId(11), MessageId(101), false) {
            draft.toggle(ChatId(11), MessageId(102), false);
            self.share.pending_forward = Some(draft);
        }
        self.open_forward_picker(window, cx);
        if std::env::var("QUILL_DEMO_FORWARD_BOX").as_deref() != Ok("empty") {
            self.share.selection.toggle(ChatId(61));
            self.share.selection.toggle(ChatId(63));
        }
    }
}

crate::ui::screenshot_demo::register_demos![
    crate::ui::screenshot_demo::DemoSpec::chats(
        "ready-forward-box",
        "screenshot demo — forward box over the chat"
    )
    .setup(QuillApp::demo_ready_forward_box),
];
