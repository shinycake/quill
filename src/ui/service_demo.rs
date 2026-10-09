//! `ready-service-messages` screenshot demo: a group whose history is a
//! tour of service messages (members, pins with excerpts, a new group
//! photo, calls, gifts, giveaways, topics, boosts, timers…), injected
//! through the normal reducer — no live Telegram.

use super::demo::{demo_file_json, demo_thumb_png_path};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const ME: i64 = 9;
const DANA: i64 = 1;
const OMAR: i64 = 2;
const LEA: i64 = 3;
const CHAT: i64 = 61;

/// `(sender user, content json)` per message, oldest first.
fn script() -> Vec<(i64, String)> {
    let photo_file = demo_file_json(90, &demo_thumb_png_path(), true);
    let photo = format!(
        r#"{{"@type":"chatPhoto","id":"5","added_date":1,"minithumbnail":null,"sizes":[{{"@type":"photoSize","type":"a","photo":{photo_file},"width":160,"height":160,"progressive_sizes":[]}}]}}"#
    );
    let chat_photo = format!(r#"{{"@type":"messageChatChangePhoto","photo":{photo}}}"#);
    let suggested = format!(r#"{{"@type":"messageSuggestProfilePhoto","photo":{photo}}}"#);
    let text = |t: &str| {
        format!(
            r#"{{"@type":"messageText","text":{{"@type":"formattedText","text":{},"entities":[]}}}}"#,
            serde_json::to_string(t).unwrap_or_default()
        )
    };
    let fixed = |json: &str| json.to_string();
    vec![
        (
            DANA,
            fixed(
                r#"{"@type":"messageBasicGroupChatCreate","title":"Design Club","member_user_ids":[2,3]}"#,
            ),
        ),
        (
            DANA,
            fixed(r#"{"@type":"messageChatAddMembers","member_user_ids":[2,3]}"#),
        ),
        (
            OMAR,
            text("Lunch at noon, bring snacks! We have a lot to cover today."),
        ),
        (
            DANA,
            fixed(r#"{"@type":"messagePinMessage","message_id":2003}"#),
        ),
        (LEA, fixed(r#"{"@type":"messageChatJoinByLink"}"#)),
        (
            DANA,
            fixed(r#"{"@type":"messageChatChangeTitle","title":"Design Club ✨"}"#),
        ),
        (DANA, chat_photo),
        (
            OMAR,
            fixed(r#"{"@type":"messageChatDeleteMember","user_id":2}"#),
        ),
        (
            DANA,
            fixed(r#"{"@type":"messageChatDeleteMember","user_id":3}"#),
        ),
        (
            DANA,
            fixed(r#"{"@type":"messageVideoChatStarted","group_call_id":5}"#),
        ),
        (
            DANA,
            fixed(
                r#"{"@type":"messageInviteVideoChatParticipants","group_call_id":5,"user_ids":[2,3]}"#,
            ),
        ),
        (
            DANA,
            fixed(r#"{"@type":"messageVideoChatEnded","duration":3725}"#),
        ),
        (
            DANA,
            fixed(
                r#"{"@type":"messageForumTopicCreated","name":"Ideas","is_name_implicit":false,"icon":{"@type":"forumTopicIcon","color":7322096,"custom_emoji_id":"0"}}"#,
            ),
        ),
        (
            DANA,
            fixed(r#"{"@type":"messageForumTopicIsClosedToggled","is_closed":true}"#),
        ),
        (
            DANA,
            fixed(r#"{"@type":"messageChatBoost","boost_count":3}"#),
        ),
        (OMAR, fixed(r#"{"@type":"messageScreenshotTaken"}"#)),
        (
            DANA,
            fixed(
                r#"{"@type":"messageChatSetMessageAutoDeleteTime","message_auto_delete_time":86400,"from_user_id":1}"#,
            ),
        ),
        (
            DANA,
            fixed(
                r#"{"@type":"messageChatSetTheme","theme":{"@type":"chatThemeEmoji","name":"☃"}}"#,
            ),
        ),
        (LEA, fixed(r#"{"@type":"messageContactRegistered"}"#)),
        (
            DANA,
            fixed(
                r#"{"@type":"messageGameScore","game_message_id":2999,"game_id":"5","score":120}"#,
            ),
        ),
        (
            OMAR,
            fixed(
                r#"{"@type":"messageProximityAlertTriggered","traveler_id":{"@type":"messageSenderUser","user_id":2},"watcher_id":{"@type":"messageSenderUser","user_id":9},"distance":1500}"#,
            ),
        ),
        (
            DANA,
            fixed(
                r#"{"@type":"messageGiftedPremium","gifter_user_id":1,"receiver_user_id":9,"currency":"USD","amount":999,"month_count":3,"day_count":0}"#,
            ),
        ),
        (
            DANA,
            fixed(
                r#"{"@type":"messageGift","gift":{"@type":"gift","id":"1","star_count":100},"sender_id":{"@type":"messageSenderUser","user_id":1},"receiver_id":{"@type":"messageSenderUser","user_id":9},"prepaid_upgrade_star_count":0}"#,
            ),
        ),
        (
            DANA,
            fixed(r#"{"@type":"messageGiveawayCreated","star_count":500}"#),
        ),
        (
            DANA,
            fixed(
                r#"{"@type":"messageGiveawayCompleted","giveaway_message_id":5,"winner_count":4,"is_star_giveaway":false,"unclaimed_prize_count":0}"#,
            ),
        ),
        (
            DANA,
            fixed(
                r#"{"@type":"messagePaymentRefunded","owner_id":{"@type":"messageSenderChat","chat_id":-1002002},"currency":"USD","total_amount":1999}"#,
            ),
        ),
        (DANA, suggested),
        (
            DANA,
            fixed(
                r#"{"@type":"messageChatSetBackground","old_background_message_id":0,"background":{"@type":"chatBackground","dark_theme_dimming":0},"only_for_self":false}"#,
            ),
        ),
        (
            DANA,
            fixed(r#"{"@type":"messageChatUpgradeTo","supergroup_id":42}"#),
        ),
        (
            ME,
            fixed(
                r#"{"@type":"messageBotWriteAccessAllowed","reason":{"@type":"botWriteAccessAllowReasonConnectedWebsite","domain_name":"example.org"}}"#,
            ),
        ),
        (
            DANA,
            fixed(r#"{"@type":"messagePinMessage","message_id":2003}"#),
        ),
        (
            OMAR,
            fixed(
                r#"{"@type":"messageCustomServiceAction","text":"Welcome to the club! Please read the rules."}"#,
            ),
        ),
    ]
}

pub(super) fn apply_ready_service_messages(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let now = quill::local_time::now_unix() - 4000;
    let mut jsons: Vec<String> = vec![
        format!(
            r#"{{"@type":"updateOption","name":"my_id","value":{{"@type":"optionValueInteger","value":"{ME}"}}}}"#
        ),
        r#"{"@type":"updateNewChat","chat":{"id":-1002002,"title":"Launch Channel","type":{"@type":"chatTypeSupergroup","supergroup_id":2002,"is_channel":true},"unread_count":0}}"#.to_string(),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{CHAT},"title":"Design Club","type":{{"@type":"chatTypeBasicGroup","basic_group_id":{CHAT}}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{CHAT},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"4999","is_pinned":false}}}}"#
        ),
    ];
    for (id, first, last) in [
        (ME, "Idan", "Birman"),
        (DANA, "Dana", "Cole"),
        (OMAR, "Omar", "Haddad"),
        (LEA, "Lea", "Stern"),
    ] {
        jsons.push(format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"{last}","usernames":null,"phone_number":"","status":{{"@type":"userStatusRecently"}},"profile_photo":null,"is_contact":true,"type":{{"@type":"userTypeRegular"}}}}}}"#
        ));
    }
    for (index, (sender, content)) in script().into_iter().enumerate() {
        // Ids from 2001; the pinned text message is the third (2003).
        let id = 2001 + index as i64;
        let date = now + index as i64 * 90;
        let outgoing = sender == ME;
        jsons.push(format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{CHAT},"sender_id":{{"@type":"messageSenderUser","user_id":{sender}}},"is_outgoing":{outgoing},"date":{date},"content":{content}}}}}"#
        ));
    }
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(CHAT));
}
