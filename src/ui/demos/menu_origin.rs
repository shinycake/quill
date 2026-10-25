//! Screenshot demo: the message menu's origin rows. `QUILL_DEMO_ORIGIN` picks
//! the open chat: `forward` (a forwarded message), `gift` (a gift service
//! message you sent), `replies` (the Replies chat) or `times` (messages from
//! 2024, from earlier this year and from today, the last one edited: day
//! separators, bubble times and the Edited row). `times-menu` opens the menu
//! of the last message with its read time; `read-hidden` does the same with
//! your read time hidden ("show when").

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::ids::MessageId;
use quill::state::Session;

use crate::ui::message_menu_ui::origin_rows::{FORWARD_CHAT, GIFT_CHAT, REPLIES_CHAT, TIMES_CHAT};

register_demos![
    // The original date of a forward, "Send Another Gift" and the Replies
    // chat's "Block user" (injected, no live Telegram).
    DemoSpec::chats(
        "ready-origin-menus",
        "screenshot demo — message menu over a forward, a gift and a reply in the Replies chat (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_origin_menus),
];

impl QuillApp {
    fn demo_origin_menus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use std::sync::atomic::Ordering;
        let mode = std::env::var("QUILL_DEMO_ORIGIN").unwrap_or_else(|_| "forward".into());
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_origin_menus(session, &self.demo_ui.sink, &self.demo_ui.seq);
            session.open_chat(match mode.as_str() {
                "gift" => GIFT_CHAT,
                "replies" => REPLIES_CHAT,
                "times" | "times-menu" | "read-hidden" => TIMES_CHAT,
                _ => FORWARD_CHAT,
            });
        }
        let read = match mode.as_str() {
            "times-menu" => Some(quill::telegram::envelope::MessageReadDate::Read(
                1_773_504_420,
            )),
            "read-hidden" => Some(quill::telegram::envelope::MessageReadDate::MyPrivacyRestricted),
            _ => None,
        };
        if let Some(read) = read {
            // The menu of a sent message in a private chat with a read time.
            // window-scope: demo setup in the one demo window.
            cx.defer_in(window, move |this, window, cx| {
                this.open_message_menu(
                    crate::ui::menu_states::MessageMenuState {
                        chat_id: TIMES_CHAT,
                        message_id: MessageId(71),
                        position: point(px(700.), px(300.)),
                    },
                    window,
                    cx,
                );
                if let Some(session) = this.demo_session.as_mut() {
                    let mut audience =
                        quill::state::MessageAudience::new(TIMES_CHAT, MessageId(71));
                    audience.read_date = quill::state::Audience::Ready(read);
                    session.messages.message_audience = Some(audience);
                }
                cx.notify();
            });
        }
        cx.notify();
    }
}

fn apply_origin_menus(
    session: &mut Session,
    sink: &std::sync::Arc<quill::diagnostics::MemorySink>,
    seq: &std::sync::atomic::AtomicU64,
) {
    use quill::diagnostics::DiagnosticSink;
    use quill::telegram::client::copy_and_parse;
    let dyn_sink: std::sync::Arc<dyn DiagnosticSink> = sink.clone();
    let text = |value: &str| {
        format!(
            r#"{{"@type":"messageText","text":{{"@type":"formattedText","text":"{value}","entities":[]}}}}"#
        )
    };
    let forwarded = |id: i64, chat: i64, value: &str| {
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{chat},"sender_id":{{"@type":"messageSenderUser","user_id":{chat}}},"is_outgoing":false,"date":1700000{id},"content":{},"forward_info":{{"@type":"messageForwardInfo","origin":{{"@type":"messageOriginUser","sender_user_id":32}},"date":1650000000,"source":null,"public_service_announcement_type":""}}}}}}"#,
            text(value)
        )
    };
    let chat = |id: i64, title: &str, order: &str| {
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{{"@type":"chatTypePrivate","user_id":{id}}},"unread_count":0}}}}"#
        ) + &format!(
            r#"|{{"@type":"updateChatPosition","chat_id":{id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{order}","is_pinned":false}}}}"#
        )
    };
    let user = |id: i64, first: &str| {
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{id},"first_name":"{first}","last_name":"","type":{{"@type":"userTypeRegular"}},"status":{{"@type":"userStatusOffline","was_online":1}}}}}}"#
        )
    };
    let gift = r#"{"@type":"updateNewMessage","message":{"id":50,"chat_id":42,"sender_id":{"@type":"messageSenderUser","user_id":1},"is_outgoing":true,"date":1700000050,"content":{"@type":"messageGiftedStars","gifter_user_id":1,"receiver_user_id":42,"currency":"USD","amount":499,"star_count":500}}}"#.to_string();
    let mut jsons: Vec<String> = vec![
        format!(
            r#"{{"@type":"updateOption","name":"replies_bot_chat_id","value":{{"@type":"optionValueInteger","value":"{}"}}}}"#,
            REPLIES_CHAT.0
        ),
        user(32, "Nora"),
        user(41, "Ada"),
        user(42, "Mira"),
        user(REPLIES_CHAT.0, "Replies"),
    ];
    for (id, title, order) in [
        (41, "Ada", "60"),
        (42, "Mira", "59"),
        (REPLIES_CHAT.0, "Replies", "58"),
    ] {
        jsons.extend(chat(id, title, order).split('|').map(str::to_string));
    }
    jsons.push(forwarded(40, 41, "Sharing this from another chat"));
    jsons.push(gift);
    jsons.push(forwarded(
        60,
        REPLIES_CHAT.0,
        "Great post, thanks for sharing",
    ));
    jsons.push(user(TIMES_CHAT.0, "Ada"));
    jsons.extend(
        chat(TIMES_CHAT.0, "Ada", "57")
            .split('|')
            .map(str::to_string),
    );
    let now = quill::local_time::now_unix();
    let sample = |id: i64, date: i64, edit_date: i64, value: &str| {
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{chat},"sender_id":{{"@type":"messageSenderUser","user_id":{chat}}},"is_outgoing":false,"date":{date},"edit_date":{edit_date},"content":{}}}}}"#,
            text(value),
            chat = TIMES_CHAT.0
        )
    };
    jsons.push(sample(70, 1_730_809_200, 0, "A message from November 2024"));
    jsons.push(sample(
        71,
        1_773_500_820,
        0,
        "A message from March this year",
    ));
    jsons.push(sample(
        72,
        now - 2 * 3600,
        now - 3600,
        "Sent two hours ago and edited an hour later",
    ));
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}
