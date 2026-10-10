//! `ReadyChatHeader` fixture (`QUILL_DEMO_HEADER=<variant>`): one open chat
//! showing a header badge or one of the bars that replace the composer,
//! injected through the normal reducer (no live Telegram).

use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const USER: i64 = 7101;
const BOT: i64 = 7102;
const GROUP: i64 = -1001700000001;
const CHANNEL: i64 = -1001700000002;
const DISCUSSION: i64 = -1001700000003;
const SCAM_CHANNEL: i64 = -1001700000004;

const MESSAGE_DATE: i64 = 1790632300;

fn user_json(id: i64, first: &str, last: &str, extra: &str) -> String {
    format!(
        r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"{last}","usernames":null,"phone_number":"","status":{{"@type":"userStatusRecently"}},"profile_photo":null,"is_contact":true,"type":{{"@type":"userTypeRegular"}}{extra}}}}}"#
    )
}

fn message_json(chat: i64, sender: &str, text: &str, outgoing: bool) -> String {
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":1048576,"chat_id":{chat},"sender_id":{sender},"is_outgoing":{outgoing},"date":{MESSAGE_DATE},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}}}"#
    )
}

fn user_sender(id: i64) -> String {
    format!(r#"{{"@type":"messageSenderUser","user_id":{id}}}"#)
}

fn chat_sender(id: i64) -> String {
    format!(r#"{{"@type":"messageSenderChat","chat_id":{id}}}"#)
}

/// The variants a capture script can ask for.
pub(super) fn apply_ready_chat_header(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
    variant: &str,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let mut jsons: Vec<String> = Vec::new();
    let open = match variant {
        // Verified check beside a Premium status star.
        "verified" => {
            jsons.push(user_json(
                USER,
                "Dana",
                "Levi",
                r#","is_premium":true,"verification_status":{"@type":"verificationStatus","is_verified":true,"is_scam":false,"is_fake":false}"#,
            ));
            jsons.push(private_chat(USER, "Dana Levi"));
            jsons.push(message_json(
                USER,
                &user_sender(USER),
                "Welcome to the verified account.",
                false,
            ));
            USER
        }
        // A long history for the middle-click autoscroll capture.
        "autoscroll" => {
            jsons.push(user_json(USER, "Dana", "Levi", ""));
            jsons.push(private_chat(USER, "Dana Levi"));
            for n in 1..=80i64 {
                jsons.push(
                    message_json(
                        USER,
                        &user_sender(USER),
                        &format!("Message number {n} in a long conversation."),
                        n % 3 == 0,
                    )
                    .replace(r#""id":1048576"#, &format!(r#""id":{}"#, 1048576 * n)),
                );
            }
            USER
        }
        "fake" => {
            jsons.push(user_json(
                USER,
                "Omer",
                "Katz",
                r#","verification_status":{"@type":"verificationStatus","is_verified":false,"is_scam":false,"is_fake":true}"#,
            ));
            jsons.push(private_chat(USER, "Omer Katz"));
            jsons.push(message_json(
                USER,
                &user_sender(USER),
                "I am definitely your bank.",
                false,
            ));
            USER
        }
        "scam" => {
            jsons.push(r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":1700000004,"usernames":null,"status":{"@type":"chatMemberStatusMember"},"member_count":9000,"is_channel":true,"is_broadcast_group":false,"verification_status":{"@type":"verificationStatus","is_verified":false,"is_scam":true,"is_fake":false}}}"#.to_string());
            jsons.push(supergroup_chat(
                SCAM_CHANNEL,
                1700000004,
                "Free crypto giveaway",
                true,
            ));
            jsons.push(message_json(
                SCAM_CHANNEL,
                &chat_sender(SCAM_CHANNEL),
                "Send one coin, get two back.",
                false,
            ));
            SCAM_CHANNEL
        }
        // A blocked person: Unblock.
        "unblock" => {
            jsons.push(user_json(USER, "Maya", "Orlov", ""));
            jsons.push(private_chat(USER, "Maya Orlov").replace(
                r#""unread_count":0"#,
                r#""unread_count":0,"block_list":{"@type":"blockListMain"}"#,
            ));
            jsons.push(message_json(
                USER,
                &user_sender(USER),
                "Is this still the right number?",
                false,
            ));
            USER
        }
        // A blocked bot: Restart.
        "restart" => {
            jsons.push(bot_json());
            jsons.push(private_chat(BOT, "Weather Bot").replace(
                r#""unread_count":0"#,
                r#""unread_count":0,"block_list":{"@type":"blockListMain"}"#,
            ));
            jsons.push(message_json(
                BOT,
                &user_sender(BOT),
                "Tomorrow: 21 degrees and clear.",
                false,
            ));
            BOT
        }
        // A bot that was never started: Start.
        "start" => {
            jsons.push(bot_json());
            jsons.push(private_chat(BOT, "Weather Bot"));
            BOT
        }
        "join-group" | "apply" => {
            let by_request = variant == "apply";
            jsons.push(format!(
                r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":1700000001,"usernames":null,"status":{{"@type":"chatMemberStatusLeft"}},"member_count":128,"is_channel":false,"is_broadcast_group":false,"join_by_request":{by_request}}}}}"#
            ));
            jsons.push(supergroup_chat(
                GROUP,
                1700000001,
                "Cats of Telegram",
                false,
            ));
            jsons.push(message_json(
                GROUP,
                &user_sender(USER),
                "Anyone know a good vet near the centre?",
                false,
            ));
            jsons.push(user_json(USER, "Dana", "Levi", ""));
            GROUP
        }
        // A subscribed channel with a discussion group: Mute and Discuss.
        _ => {
            jsons.push(r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":1700000002,"usernames":null,"status":{"@type":"chatMemberStatusMember"},"member_count":5400,"is_channel":true,"is_broadcast_group":false}}"#.to_string());
            jsons.push(supergroup_chat(CHANNEL, 1700000002, "Field Notes", true));
            jsons.push(r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":1700000003,"usernames":null,"status":{"@type":"chatMemberStatusMember"},"member_count":812,"is_channel":false,"is_broadcast_group":false}}"#.to_string());
            jsons.push(supergroup_chat(
                DISCUSSION,
                1700000003,
                "Field Notes Discussion",
                false,
            ));
            jsons.push(
                r#"{"@type":"updateSupergroupFullInfo","supergroup_id":1700000002,"supergroup_full_info":{"description":"Short notes from the field.","member_count":5400,"linked_chat_id":-1001700000003}}"#
                    .to_string(),
            );
            jsons.push(message_json(
                CHANNEL,
                &chat_sender(CHANNEL),
                "New issue is out: the quiet month.",
                false,
            ));
            CHANNEL
        }
    };
    session.open_chat(ChatId(open));
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    // The bot chat above has no messages; mark its history as loaded so the
    // bar can tell "empty" from "not fetched yet".
    if variant == "start" {
        session.histories.entry(open).or_default().loaded_complete = true;
    }
}

fn bot_json() -> String {
    format!(
        r#"{{"@type":"updateUser","user":{{"id":{BOT},"first_name":"Weather","last_name":"Bot","type":{{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}}}}"#
    )
}

fn private_chat(id: i64, title: &str) -> String {
    format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{{"@type":"chatTypePrivate","user_id":{id}}},"unread_count":0}}}}"#
    )
}

fn supergroup_chat(id: i64, supergroup: i64, title: &str, channel: bool) -> String {
    format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{{"@type":"chatTypeSupergroup","supergroup_id":{supergroup},"is_channel":{channel}}},"unread_count":0}}}}"#
    )
}
