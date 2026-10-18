//! Screenshot demo of promoted chats in the chat list: an announcement
//! channel with its text, a proxy sponsor channel (both channels the account
//! has not joined), and a bot with an empty history, as TDLib sends them.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::diagnostics::DiagnosticSink;
use quill::lang::PackValue;
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::ChannelMemberStatus;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

register_demos![
    DemoSpec::chats(
        "ready-promoted-chats",
        "screenshot demo — chat list: promoted chats"
    )
    .setup(QuillApp::demo_promoted_chats),
];

const BOT: &str = r#"{"@type":"updateUser","user":{"id":93,"first_name":"Helper","type":{"@type":"userTypeBot"}}}"#;

fn new_chat(id: i64, title: &str, kind: &str) -> String {
    format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{kind},"last_read_inbox_message_id":0,"last_read_outbox_message_id":0}}}}"#
    )
}

fn position(id: i64, order: i64, source: &str) -> String {
    format!(
        r#"{{"@type":"updateChatPosition","chat_id":{id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{order}","is_pinned":false,"source":{source}}}}}"#
    )
}

impl QuillApp {
    fn demo_promoted_chats(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let seq = AtomicU64::new(session.last_seq);
        let sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        let channel = |id: i64| {
            format!(r#"{{"@type":"chatTypeSupergroup","supergroup_id":{id},"is_channel":true}}"#)
        };
        let psa = r#"{"@type":"chatSourcePublicServiceAnnouncement","type":"covid","text":"Wash your hands and keep your distance."}"#;
        let bot_psa = r#"{"@type":"chatSourcePublicServiceAnnouncement","type":"covid","text":""}"#;
        let proxy = r#"{"@type":"chatSourceMtprotoProxy"}"#;
        let updates = [
            BOT.to_string(),
            new_chat(91, "Health Updates", &channel(91)),
            new_chat(92, "Sponsored Channel", &channel(92)),
            new_chat(93, "Helper", r#"{"@type":"chatTypePrivate","user_id":93}"#),
            position(91, 9_000_000_000_000_000_003, psa),
            position(92, 9_000_000_000_000_000_002, proxy),
            position(93, 9_000_000_000_000_000_001, bot_psa),
        ];
        for json in updates {
            if let Some(owned) = copy_and_parse(&json, &seq, &sink) {
                session.apply(owned);
            }
        }
        self.demo_ui
            .seq
            .store(seq.load(Ordering::SeqCst), Ordering::SeqCst);
        for id in [91, 92] {
            if let Some(chat) = session.chats.get_mut(&id) {
                chat.my_member_status = Some(ChannelMemberStatus::Left);
            }
        }
        session.chat_list.psa_labels.take_wanted("en", ["covid"]);
        let (epoch, _) = session.chat_list.psa_labels.take_wanted("en", []);
        session.chat_list.psa_labels.store(
            epoch,
            vec![(
                "cloud_lng_badge_psa_covid".into(),
                PackValue::Ordinary("COVID-19 Info".into()),
            )],
        );
        self.connection
            .set_note(StatusNote::silent("screenshot demo — promoted chats"));
    }
}
