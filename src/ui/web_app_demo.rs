//! Screenshot-demo fixtures for mini apps (`--screenshot-demo
//! ready-mini-app`, mode from `QUILL_DEMO_MINIAPP`): the boxes in the main
//! window, with injected English fixtures and no helper process. The
//! helper's own chrome is captured separately (`quill-webview --capture`,
//! see the decision doc).

use super::app::QuillApp;
use super::screenshot_demo::{DemoSpec, register_demos};
use super::web_app_ui::{PendingOpen, WebAppConfirm};
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::AttachmentMenuBot;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

fn apply(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64, json: &str) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

/// Bot "Weather Desk" (id 21) with a main app and a menu button.
fn apply_bot(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    session.my_user_id = Some(777);
    for json in [
        r#"{"@type":"updateUser","user":{"id":21,"first_name":"Weather Desk","usernames":{"@type":"usernames","active_usernames":["weatherdeskbot"],"disabled_usernames":[],"editable_username":"weatherdeskbot","collectible_usernames":[]},"type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":true,"can_read_all_group_messages":false,"has_main_web_app":true,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":true,"active_user_count":0}}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Weather Desk","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
        r#"{"@type":"updateChatPosition","chat_id":21,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"40","is_pinned":false}}"#,
        r#"{"@type":"updateNewMessage","message":{"id":301,"chat_id":21,"date":1790000005,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupInlineKeyboard","rows":[[{"@type":"inlineKeyboardButton","text":"Open forecast","type":{"@type":"inlineKeyboardButtonTypeWebApp","url":"https://example.com/forecast"}}]]},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Tap below for the hourly forecast.","entities":[]}}}}"#,
        r#"{"@type":"updateAttachmentMenuBots","bots":[{"@type":"attachmentMenuBot","bot_user_id":21,"name":"Weather","supports_self_chat":true,"supports_user_chats":true,"supports_bot_chats":true,"supports_group_chats":true,"supports_channel_chats":false,"request_write_access":true,"is_added":true,"show_in_attachment_menu":true,"show_in_side_menu":false}]}"#,
    ] {
        apply(session, sink, seq, json);
    }
    session.open_chat(ChatId(21));
}

register_demos![
    // Mini apps: the first-open and add-to-menu boxes (`QUILL_DEMO_MINIAPP`).
    DemoSpec::chats(
        "ready-mini-app",
        "screenshot demo — mini app boxes (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_setup_mini_app),
];

impl QuillApp {
    fn demo_setup_mini_app(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        quill::web_app::trust::set_persistence(false);
        let mode = std::env::var("QUILL_DEMO_MINIAPP").unwrap_or_else(|_| "terms".into());
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_bot(session, &self.demo_sink, &self.demo_seq);
        }
        let then = PendingOpen::Open {
            chat_id: ChatId(21),
            bot_id: 21,
            url: "https://example.com/forecast".into(),
            source: quill::web_app::LaunchSource::InlineButton,
        };
        match mode.as_str() {
            "add" => self.show_web_app_confirm_demo(
                WebAppConfirm::AddToMenu {
                    bot: AttachmentMenuBot {
                        bot_user_id: 21,
                        name: "Weather".into(),
                        supports_self_chat: true,
                        supports_user_chats: true,
                        supports_bot_chats: true,
                        supports_group_chats: true,
                        supports_channel_chats: false,
                        request_write_access: true,
                        is_added: false,
                        show_in_attachment_menu: true,
                        show_in_side_menu: false,
                    },
                    allow_write: true,
                    then,
                },
                cx,
            ),
            "terms-write" => self.show_web_app_confirm_demo(
                WebAppConfirm::OpenTerms {
                    then,
                    request_write: true,
                    allow_write: true,
                },
                cx,
            ),
            // The attachment menu with the bot in it, no box.
            "attach" => {}
            // The real helper on a local page (no TDLib): the window,
            // the bridge and the popups.
            "window" => {
                let url = std::env::var("QUILL_DEMO_MINIAPP_URL")
                    .unwrap_or_else(|_| "about:blank".into());
                self.open_mini_app_window_demo(&url, cx);
            }
            _ => self.show_web_app_confirm_demo(
                WebAppConfirm::OpenTerms {
                    then,
                    request_write: false,
                    allow_write: false,
                },
                cx,
            ),
        }
    }
}
