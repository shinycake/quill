//! Screenshot fixtures for the composer leftovers: a group that restricts
//! the viewer, and a paused voice recording. Injected through the normal
//! reducer (no live Telegram).

use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{ChannelMemberStatus, ChatPermissions, MemberRestriction};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const GROUP: i64 = -1001700000001;

/// A supergroup whose admins restricted the viewer. `QUILL_DEMO_RESTRICTION`
/// picks the sentence: `until` (default, a restriction that ends in two
/// days), `forever`, `everyone` (the group itself forbids sending) or
/// `media` (text allowed, photos and voice refused).
pub(super) fn apply_ready_restricted_composer(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
    variant: &str,
    now: i64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let jsons = [
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":1700000001,"usernames":null,"status":{"@type":"chatMemberStatusMember"},"member_count":128,"is_channel":false,"is_broadcast_group":false}}"#.to_string(),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{GROUP},"title":"Design Crew","type":{{"@type":"chatTypeSupergroup","supergroup_id":1700000001,"is_channel":false}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":1048576,"chat_id":{GROUP},"sender_id":{{"@type":"messageSenderUser","user_id":11}},"is_outgoing":false,"date":1790632300,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Moodboard is in the shared folder. Feedback by Friday please.","entities":[]}}}}}}}}"#
        ),
    ];
    session.open_chat(ChatId(GROUP));
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    let Some(chat) = session.chats.get_mut(&GROUP) else {
        return;
    };
    let everything = ChatPermissions::all();
    let text_only = ChatPermissions {
        can_send_basic_messages: true,
        ..ChatPermissions::default()
    };
    match variant {
        "everyone" => {
            chat.permissions = Some(ChatPermissions::default());
            chat.can_send_basic_messages = false;
            chat.set_my_restriction(None);
            chat.my_member_status = Some(ChannelMemberStatus::Member);
        }
        "media" => {
            chat.permissions = Some(everything);
            chat.set_my_restriction(Some(MemberRestriction {
                until_date: 0,
                permissions: text_only,
            }));
            chat.my_member_status = Some(ChannelMemberStatus::Restricted);
        }
        other => {
            chat.permissions = Some(everything);
            let until_date = if other == "forever" {
                0
            } else {
                (now + 2 * 24 * 3600) as i32
            };
            chat.set_my_restriction(Some(MemberRestriction {
                until_date,
                permissions: ChatPermissions::default(),
            }));
            chat.my_member_status = Some(ChannelMemberStatus::Restricted);
        }
    }
}
