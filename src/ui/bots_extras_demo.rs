//! Screenshot-demo fixtures for the bot extras (`--screenshot-demo
//! ready-bot-extras`, mode from `QUILL_DEMO_BOTEXTRAS`). Everything goes
//! through the real reducers; no live Telegram. English-only text.

use super::app::QuillApp;
use super::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::{InfoPanelTarget, RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

fn apply(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64, json: &str) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

const BOT_TYPE: &str = r#"{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":true,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}"#;

/// Bot "Weather Desk" (id 21): its private chat, with an inline keyboard of
/// five buttons as the last message.
fn apply_bot_chat(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    session.my_user_id = Some(777);
    session.open_chat(ChatId(21));
    for json in [
        format!(
            r#"{{"@type":"updateUser","user":{{"id":21,"first_name":"Weather Desk","usernames":{{"@type":"usernames","active_usernames":["weatherdeskbot"],"disabled_usernames":[],"editable_username":"weatherdeskbot","collectible_usernames":[]}},"type":{BOT_TYPE}}}}}"#
        ),
        r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Weather Desk","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#.to_string(),
        r#"{"@type":"updateChatPosition","chat_id":21,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"40","is_pinned":false}}"#.to_string(),
        r#"{"@type":"updateNewMessage","message":{"id":301,"chat_id":21,"date":1790000000,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"/forecast","entities":[]}}}}"#.to_string(),
        r#"{"@type":"updateNewMessage","message":{"id":302,"chat_id":21,"date":1790000005,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupInlineKeyboard","rows":[[{"@type":"inlineKeyboardButton","text":"Today","type":{"@type":"inlineKeyboardButtonTypeCallback","data":"dG9kYXk="}},{"@type":"inlineKeyboardButton","text":"Tomorrow","type":{"@type":"inlineKeyboardButtonTypeCallback","data":"dG9tb3Jyb3c="}},{"@type":"inlineKeyboardButton","text":"This week","type":{"@type":"inlineKeyboardButtonTypeCallback","data":"d2Vlaw=="}}],[{"@type":"inlineKeyboardButton","text":"Change city","type":{"@type":"inlineKeyboardButtonTypeCallback","data":"Y2l0eQ=="}},{"@type":"inlineKeyboardButton","text":"Open site","type":{"@type":"inlineKeyboardButtonTypeUrl","url":"https://example.com/"}}]]},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Which forecast do you want?","entities":[]}}}}"#.to_string(),
    ] {
        apply(session, sink, seq, &json);
    }
}

/// Chats the viewer manages (a group, a channel, a basic group they only
/// belong to) and the bot's `userFullInfo`, so the picker has rows.
fn apply_add_bot(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    session.my_user_id = Some(777);
    let admin = r#"{"@type":"chatMemberStatusAdministrator","can_be_edited":true,"rights":{"@type":"chatAdministratorRights","can_manage_chat":true,"can_change_info":true,"can_post_messages":true,"can_edit_messages":true,"can_delete_messages":true,"can_invite_users":true,"can_restrict_members":true,"can_pin_messages":true,"can_manage_topics":false,"can_promote_members":true,"can_manage_video_chats":true,"can_post_stories":false,"can_edit_stories":false,"can_delete_stories":false,"can_manage_direct_messages":false,"can_manage_tags":false,"can_send_welcome_messages":false,"is_anonymous":false}}"#;
    let chat = |id: i64, title: &str, sg: i64, channel: bool| {
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{{"@type":"chatTypeSupergroup","supergroup_id":{sg},"is_channel":{channel}}},"unread_count":0}}}}"#
        )
    };
    let group_status = |sg: i64, channel: bool, status: &str| {
        format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{sg},"is_channel":{channel},"status":{status}}}}}"#
        )
    };
    for json in [
        format!(
            r#"{{"@type":"updateUser","user":{{"id":21,"first_name":"Moderator Bot","usernames":{{"@type":"usernames","active_usernames":["modhelperbot"],"disabled_usernames":[],"editable_username":"modhelperbot","collectible_usernames":[]}},"type":{BOT_TYPE}}}}}"#
        ),
        r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Moderator Bot","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#.to_string(),
        chat(-1001000000011, "Rust Programmers", 11, false),
        chat(-1001000000012, "Compiler Weekly", 12, true),
        chat(-1001000000013, "Weekend Hikers", 13, false),
        group_status(11, false, admin),
        group_status(12, true, r#"{"@type":"chatMemberStatusCreator","is_anonymous":false,"is_member":true}"#),
        group_status(13, false, r#"{"@type":"chatMemberStatusMember"}"#),
    ] {
        apply(session, sink, seq, &json);
    }
    // Members of "Weekend Hikers" may invite: the group's default
    // permissions allow it.
    apply(
        session,
        sink,
        seq,
        r#"{"@type":"updateChatPermissions","chat_id":-1001000000013,"permissions":{"@type":"chatPermissions","can_send_basic_messages":true,"can_invite_users":true}}"#,
    );
    let info = session.request_for_user(RequestPurpose::GetUserFullInfo, 21);
    apply(
        session,
        sink,
        seq,
        &format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","block_list":null,"bio":{{"@type":"formattedText","text":"","entities":[]}},"bot_info":{{"@type":"botInfo","short_description":"Keeps groups tidy","description":"Removes spam and welcomes new members.","commands":[],"privacy_policy_url":"","default_group_administrator_rights":{{"@type":"chatAdministratorRights","can_manage_chat":true,"can_change_info":false,"can_post_messages":false,"can_edit_messages":false,"can_delete_messages":true,"can_invite_users":true,"can_restrict_members":true,"can_pin_messages":true,"can_manage_topics":false,"can_promote_members":false,"can_manage_video_chats":false,"can_post_stories":false,"can_edit_stories":false,"can_delete_stories":false,"can_manage_direct_messages":false,"can_manage_tags":false,"can_send_welcome_messages":false,"is_anonymous":false}},"default_channel_administrator_rights":{{"@type":"chatAdministratorRights","can_manage_chat":true,"can_change_info":false,"can_post_messages":true,"can_edit_messages":true,"can_delete_messages":true,"can_invite_users":true,"can_restrict_members":false,"can_pin_messages":false,"can_manage_topics":false,"can_promote_members":false,"can_manage_video_chats":false,"can_post_stories":false,"can_edit_stories":false,"can_delete_stories":false,"can_manage_direct_messages":false,"can_manage_tags":false,"can_send_welcome_messages":false,"is_anonymous":false}}}}}}"#,
            info.0
        ),
    );
}

register_demos![
    // Bot extras demo (injected, no live Telegram): fast buttons mode,
    // adding a bot to a group or channel, verification badges, sharing
    // a game and owned bots; the mode comes from `QUILL_DEMO_BOTEXTRAS`.
    DemoSpec::chats(
        "ready-bot-extras",
        "screenshot demo — bot extras (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_setup_bot_extras),
];

impl QuillApp {
    /// `QUILL_DEMO_BOTEXTRAS=fast|add|add-rights|add-member|share-game` (default
    /// `fast`).
    fn demo_setup_bot_extras(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        quill::fast_buttons::set_persistence(false);
        let mode = std::env::var("QUILL_DEMO_BOTEXTRAS").unwrap_or_else(|_| "fast".into());
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            match mode.as_str() {
                "fast" => {
                    apply_bot_chat(session, &self.demo_ui.sink, &self.demo_ui.seq);
                    quill::fast_buttons::set_enabled(21, true);
                }
                "share-game" => apply_bot_chat(session, &self.demo_ui.sink, &self.demo_ui.seq),
                _ => {
                    apply_add_bot(session, &self.demo_ui.sink, &self.demo_ui.seq);
                    session.open_info_panel = Some(InfoPanelTarget::User(21));
                }
            }
        }
        match mode.as_str() {
            "add" => self.open_add_bot_dialog(21, Default::default(), cx),
            "add-rights" => self.open_add_bot_target_demo(21, -1001000000011, cx),
            "add-member" => self.open_add_bot_target_demo(21, -1001000000013, cx),
            "share-game" => self.open_share_game_dialog(21, "chess".into(), cx),
            _ => {}
        }
    }
}
