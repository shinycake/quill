//! 1:1 call state + actions.

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::settings::CallPrefs;
use quill::state::{ActiveCall, HistoryMessage, RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{
    CallDiscardReason, CallState, ChatKind, MessageContent, ParsedMessage, call_entry_label,
};
use quill::telegram::requests::PrivacyWho;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;
use std::time::Instant;
/// Scheduled video-chat fixture — the "Design voice" supergroup (id
/// 51) has a scheduled (not yet started) video chat (id 555, starts
/// in ~2h), so the overlay renders the "Scheduled voice chat" card:
/// the "Starts in …" line, the admin "Start now" button, and the
/// "Notify me when it starts" toggle
/// (`toggleVideoChatEnabledStartNotification`, schema 1.8.67,
/// :14282). Injected, no live Telegram, no media.
pub(super) fn apply_ready_group_call_scheduled(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let chat_id = 51i64;
    let call_id = 555i32;
    let start_date = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64 + 7200)
        .unwrap_or(1788003600);
    let jsons = [
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Design voice","type":{{"@type":"chatTypeSupergroup","supergroup_id":{chat_id},"is_channel":false}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatVideoChat","chat_id":{chat_id},"video_chat":{{"@type":"videoChat","group_call_id":{call_id},"has_participants":false,"default_participant_id":null}}}}"#
        ),
        format!(
            r#"{{"@type":"updateGroupCall","group_call":{{"@type":"groupCall","id":{call_id},"unique_id":"999","title":"Design sync planning","invite_link":"","paid_message_star_count":0,"scheduled_start_date":{start_date},"enabled_start_notification":false,"is_active":false,"is_video_chat":true,"is_live_story":false,"is_rtmp_stream":false,"is_joined":false,"need_rejoin":false,"is_owned":false,"can_be_managed":true,"participant_count":0,"has_hidden_listeners":false,"loaded_all_participants":false,"message_sender_id":null,"recent_speakers":[],"is_my_video_enabled":false,"is_my_video_paused":false,"can_enable_video":true,"mute_new_participants":false,"can_toggle_mute_new_participants":true,"can_send_messages":true,"are_messages_allowed":true,"can_toggle_are_messages_allowed":true,"can_delete_messages":false,"record_duration":0,"is_video_recorded":false,"duration":0}}}}"#
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// Phase C2i: synthetic Recent-calls fixture — users + private chats
/// plus a `foundMessages` payload carrying three `messageCall`
/// entries (missed / declined / answered video), applied through the
/// real `SearchCallMessages` reducer. Privacy values and the
/// confirm-before-calling pref are seeded too. Injected demo data.
pub(super) fn apply_ready_calls_settings(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let updates = [
        r#"{"@type":"updateUser","user":{"id":61,"first_name":"Maya","last_name":"Chen","type":{"@type":"userTypeRegular"}}}"#,
        r#"{"@type":"updateUser","user":{"id":62,"first_name":"Leo","last_name":"Park","type":{"@type":"userTypeRegular"}}}"#,
        r#"{"@type":"updateUser","user":{"id":63,"first_name":"Ana","last_name":"Ruiz","type":{"@type":"userTypeRegular"}}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":71,"title":"Maya Chen","type":{"@type":"chatTypePrivate","user_id":61},"unread_count":0}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":72,"title":"Leo Park","type":{"@type":"chatTypePrivate","user_id":62},"unread_count":0}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":73,"title":"Ana Ruiz","type":{"@type":"chatTypePrivate","user_id":63},"unread_count":0}}"#,
    ];
    for json in updates {
        if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i32)
        .unwrap_or(1_700_000_000);
    let call = |id: i64,
                chat_id: i64,
                outgoing: bool,
                date: i32,
                video: bool,
                reason: &str,
                duration: i32| {
        format!(
            r#"{{"id":{id},"chat_id":{chat_id},"is_outgoing":{outgoing},"date":{date},"content":{{"@type":"messageCall","unique_id":{id},"is_video":{video},"discard_reason":{{"@type":"{reason}"}},"duration":{duration}}}}}"#,
        )
    };
    let messages = [
        call(
            901,
            71,
            false,
            now - 320,
            false,
            "callDiscardReasonMissed",
            0,
        ),
        call(
            902,
            72,
            true,
            now - 5400,
            false,
            "callDiscardReasonDeclined",
            0,
        ),
        call(
            903,
            73,
            false,
            now - 86400,
            true,
            "callDiscardReasonHungUp",
            372,
        ),
    ];
    let extra = session.request(RequestPurpose::SearchCallMessages, None);
    let json = format!(
        r#"{{"@type":"foundMessages","@extra":"{}","total_count":3,"messages":[{}],"next_offset":""}}"#,
        extra.0,
        messages.join(",")
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
    session.call_privacy_allow_calls = Some(PrivacyWho::Contacts);
    session.call_privacy_p2p = Some(PrivacyWho::Everybody);
    for (key, who) in [
        (
            quill::telegram::requests_privacy::PrivacySettingKey::AllowCalls,
            PrivacyWho::Contacts,
        ),
        (
            quill::telegram::requests_privacy::PrivacySettingKey::PeerToPeer,
            PrivacyWho::Everybody,
        ),
    ] {
        session.privacy.insert(
            key,
            quill::privacy::PrivacyKeyState::Ready(quill::privacy::PrivacyRuleDetail {
                who: Some(who),
                ..Default::default()
            }),
        );
    }
    session.call_prefs.confirm_before_calling = true;
}

/// Phase C1: incoming-call fixture — a pending incoming voice call
/// from Zed (user 41), so the call overlay renders its incoming-call
/// card. Injected, no live Telegram.
pub(super) fn apply_ready_call(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let user_id = 41i64;
    let jsons = [
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{user_id},"first_name":"Zed","last_name":"Hopper","usernames":{{"@type":"usernames","active_usernames":["zedhopper"],"disabled_usernames":[],"editable_username":"zedhopper","collectible_usernames":[]}},"phone_number":"+15550101041","status":{{"@type":"userStatusOnline","expires":9999999999}},"is_contact":false,"type":{{"@type":"userTypeRegular"}}}}}}"#
        ),
        r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#.to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// Swap-prompt fixture — an active outgoing voice call with Zed
/// (user 41) plus an incoming pending video call from Ada (user 42),
/// so the state machine raises `call_swap_pending` and the kit dialog
/// renders the swap prompt. Injected, no live Telegram.
pub(super) fn apply_ready_call_swap(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let jsons = [
        r#"{"@type":"updateUser","user":{"id":41,"first_name":"Zed","last_name":"Hopper","usernames":{"@type":"usernames","active_usernames":["zedhopper"],"disabled_usernames":[],"editable_username":"zedhopper","collectible_usernames":[]},"phone_number":"+15550101041","status":{"@type":"userStatusOnline","expires":9999999999},"is_contact":false,"type":{"@type":"userTypeRegular"}}}"#.to_string(),
        r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"98","user_id":41,"is_outgoing":true,"is_video":false,"state":{"@type":"callStateExchangingKeys"}}}"#.to_string(),
        r#"{"@type":"updateUser","user":{"id":42,"first_name":"Ada","last_name":"Lovelace","usernames":{"@type":"usernames","active_usernames":["adalovelace"],"disabled_usernames":[],"editable_username":"adalovelace","collectible_usernames":[]},"phone_number":"+15550101042","status":{"@type":"userStatusOnline","expires":9999999999},"is_contact":false,"type":{"@type":"userTypeRegular"}}}"#.to_string(),
        r#"{"@type":"updateCall","call":{"@type":"call","id":78,"unique_id":"97","user_id":42,"is_outgoing":false,"is_video":true,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#.to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

pub(super) fn apply_ready_call_video(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let user_id = 41i64;
    let jsons = [
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{user_id},"first_name":"Zed","last_name":"Hopper","usernames":{{"@type":"usernames","active_usernames":["zedhopper"],"disabled_usernames":[],"editable_username":"zedhopper","collectible_usernames":[]}},"phone_number":"+15550101041","status":{{"@type":"userStatusOnline","expires":9999999999}},"is_contact":false,"type":{{"@type":"userTypeRegular"}}}}}}"#
        ),
        r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":true,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#.to_string(),
        r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":true,"state":{"@type":"callStateExchangingKeys"}}}"#.to_string(),
        r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":false,"is_video":true,"state":{"@type":"callStateReady","protocol":{"@type":"callProtocol","udp_p2p":false,"udp_reflector":false,"min_layer":65,"max_layer":92,"library_versions":[]},"servers":[],"config":"{}","encryption_key":"","emojis":["🍎","🍌","🍒","🍇"],"allow_p2p":false,"is_group_call_supported":false,"custom_parameters":"{}"}}}"#.to_string(),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// Phase C3a: joined group voice-chat fixture — the "Design voice"
/// supergroup (id 51) has a live voice chat (id 555), joined as the
/// demo user (777), with Zed speaking, Mia's hand raised, Raj muted
/// with a paused camera (`is_paused` badge), plus E2E verification
/// emojis. The overlay
/// renders its participant grid, controls, and the honest no-audio
/// note. The demo user OWNS the chat (`is_owned: true`) so the
/// owner-gated Ban buttons render. Injected, no live Telegram, no
/// media.
pub(super) fn apply_ready_group_call(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let chat_id = 51i64;
    let call_id = 555i32;
    let user = |id: i64, first: &str, last: &str| {
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{id},"first_name":"{first}","last_name":"{last}","type":{{"@type":"userTypeRegular"}}}}}}"#
        )
    };
    let video_info = |endpoint: &str, source_id: u32, paused: bool| {
        format!(
            r#"{{"@type":"groupCallParticipantVideoInfo","source_groups":[{{"@type":"groupCallVideoSourceGroup","semantics":"SIM","source_ids":[{source_id}]}}],"endpoint_id":"{endpoint}","is_paused":{paused}}}"#
        )
    };
    let participant = |id: i64, flags: &str, order: &str, video: &str, screen: &str| {
        format!(
            r#"{{"@type":"updateGroupCallParticipant","group_call_id":{call_id},"participant":{{"@type":"groupCallParticipant","participant_id":{{"@type":"messageSenderUser","user_id":{id}}},"audio_source_id":0,"screen_sharing_audio_source_id":0,"video_info":{video},"screen_sharing_video_info":{screen},"bio":"","is_current_user":false,"is_speaking":false,"is_hand_raised":false,"can_be_muted_for_all_users":true,"can_be_unmuted_for_all_users":true,"can_be_muted_for_current_user":true,"can_be_unmuted_for_current_user":true,"is_muted_for_all_users":false,"is_muted_for_current_user":false,"can_unmute_self":false,"volume_level":10000,"order":"{order}"{flags}}}}}"#
        )
    };
    let jsons = [
        user(777, "Demo", "Viewer"),
        user(41, "Zed", "Hopper"),
        user(42, "Mia", "Chen"),
        user(43, "Raj", "Patel"),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Design voice","type":{{"@type":"chatTypeSupergroup","supergroup_id":{chat_id},"is_channel":false}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatVideoChat","chat_id":{chat_id},"video_chat":{{"@type":"videoChat","group_call_id":{call_id},"has_participants":true,"default_participant_id":null}}}}"#
        ),
        format!(
            r#"{{"@type":"updateGroupCall","group_call":{{"@type":"groupCall","id":{call_id},"unique_id":"999","title":"Weekly design sync","invite_link":"","paid_message_star_count":0,"scheduled_start_date":0,"enabled_start_notification":false,"is_active":true,"is_video_chat":true,"is_live_story":false,"is_rtmp_stream":false,"is_joined":true,"need_rejoin":false,"is_owned":true,"can_be_managed":true,"participant_count":4,"has_hidden_listeners":false,"loaded_all_participants":true,"message_sender_id":null,"recent_speakers":[{{"@type":"groupCallRecentSpeaker","participant_id":{{"@type":"messageSenderUser","user_id":41}},"is_speaking":true}}],"is_my_video_enabled":true,"is_my_video_paused":false,"can_enable_video":true,"mute_new_participants":false,"can_toggle_mute_new_participants":true,"can_send_messages":true,"are_messages_allowed":true,"can_toggle_are_messages_allowed":false,"can_delete_messages":false,"record_duration":0,"is_video_recorded":false,"duration":0}}}}"#
        ),
        participant(777, r#","is_current_user":true"#, "a4", "null", "null"),
        participant(
            41,
            r#","is_speaking":true"#,
            "a3",
            &video_info("ep-41", 111, false),
            "null",
        ),
        participant(
            42,
            r#","is_hand_raised":true"#,
            "a2",
            "null",
            &video_info("ep-42-screen", 222, false),
        ),
        participant(
            43,
            r#","is_muted_for_all_users":true"#,
            "a1",
            &video_info("ep-43", 333, true),
            "null",
        ),
        format!(
            r#"{{"@type":"updateGroupCallVerificationState","group_call_id":{call_id},"generation":7,"emojis":["🍎","🍌"]}}"#
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(chat_id));
}

/// Calls-polish fixture: the Ready group voice chat with Mia's screen
/// share paused (`screen_sharing_video_info.is_paused`), Raj's camera
/// still paused, and Zed's camera pinned by the caller.
pub(super) fn apply_ready_group_call_polish(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    apply_ready_group_call(session, sink, seq);
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let json = r#"{"@type":"updateGroupCallParticipant","group_call_id":555,"participant":{"@type":"groupCallParticipant","participant_id":{"@type":"messageSenderUser","user_id":42},"audio_source_id":0,"screen_sharing_audio_source_id":0,"video_info":null,"screen_sharing_video_info":{"@type":"groupCallParticipantVideoInfo","source_groups":[{"@type":"groupCallVideoSourceGroup","semantics":"SIM","source_ids":[222]}],"endpoint_id":"ep-42-screen","is_paused":true},"bio":"","is_current_user":false,"is_speaking":false,"is_hand_raised":true,"can_be_muted_for_all_users":true,"can_be_unmuted_for_all_users":true,"can_be_muted_for_current_user":true,"can_be_unmuted_for_current_user":true,"is_muted_for_all_users":false,"is_muted_for_current_user":false,"can_unmute_self":false,"volume_level":10000,"order":"a2"}}"#;
    if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

/// Calls-polish fixture: an unjoined voice chat in a group where the user
/// can also join as the "Design Team" channel (join-as picker).
pub(super) fn apply_ready_group_call_join_as(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    apply_ready_group_call(session, sink, seq);
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let json = r#"{"@type":"updateNewChat","chat":{"id":-1001001,"title":"Design Team","type":{"@type":"chatTypeSupergroup","supergroup_id":1001001,"is_channel":true},"unread_count":0}}"#;
    if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
        session.apply(owned);
    }
    if let Some(call) = session.active_group_call.as_mut() {
        call.is_joined = false;
        call.join_as_requested = true;
        call.join_as_options = vec![
            quill::telegram::envelope::MessageSender::User { user_id: 777 },
            quill::telegram::envelope::MessageSender::Chat { chat_id: -1001001 },
        ];
        call.join_as = Some(quill::telegram::envelope::MessageSender::Chat { chat_id: -1001001 });
    }
}

/// Phase C2f: invite-picker fixture — the Ready group voice chat plus
/// two extra contacts (Lena, Omar) not in the call; `session.contacts`
/// is seeded so the invite picker lists them (Zed is already in the
/// call, so the picker excludes him).
pub(super) fn apply_ready_group_call_invite(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    apply_ready_group_call(session, sink, seq);
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let user = |id: i64, first: &str, last: &str| {
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{id},"first_name":"{first}","last_name":"{last}","type":{{"@type":"userTypeRegular"}}}}}}"#
        )
    };
    for json in [user(44, "Lena", "Katz"), user(45, "Omar", "Reyes")] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.contacts = Some(vec![44, 45, 41]);
}

/// Phase C2h: management-surface fixture — the Ready group voice
/// chat with invite link, an active recording, RTMP credentials, and
/// two in-call chat messages (all injected, no live Telegram).
pub(super) fn apply_ready_group_call_manage(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    apply_ready_group_call(session, sink, seq);
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let call_id = 555i32;
    session.set_group_call_invite_link(call_id, "https://t.me/+DemoVoiceChat42".to_string());
    let jsons = [
        // Recording live + chat toggle permitted.
        format!(
            r#"{{"@type":"updateGroupCall","group_call":{{"@type":"groupCall","id":{call_id},"unique_id":"999","title":"Weekly design sync","invite_link":"","paid_message_star_count":0,"scheduled_start_date":0,"enabled_start_notification":false,"is_active":true,"is_video_chat":true,"is_live_story":false,"is_rtmp_stream":false,"is_joined":true,"need_rejoin":false,"is_owned":true,"can_be_managed":true,"participant_count":4,"has_hidden_listeners":false,"loaded_all_participants":true,"message_sender_id":null,"recent_speakers":[],"is_my_video_enabled":false,"is_my_video_paused":false,"can_enable_video":true,"mute_new_participants":false,"can_toggle_mute_new_participants":true,"can_send_messages":true,"are_messages_allowed":true,"can_toggle_are_messages_allowed":true,"can_delete_messages":false,"record_duration":125,"is_video_recorded":true,"duration":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewGroupCallMessage","group_call_id":{call_id},"message":{{"@type":"groupCallMessage","message_id":1,"sender_id":{{"@type":"messageSenderUser","user_id":41}},"date":1788000000,"text":{{"@type":"formattedText","text":"Can everyone hear me?","entities":[]}},"paid_message_star_count":0,"is_from_owner":false,"can_be_deleted":false}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewGroupCallMessage","group_call_id":{call_id},"message":{{"@type":"groupCallMessage","message_id":2,"sender_id":{{"@type":"messageSenderUser","user_id":42}},"date":1788000060,"text":{{"@type":"formattedText","text":"Loud and clear — sharing my screen next","entities":[]}},"paid_message_star_count":0,"is_from_owner":false,"can_be_deleted":true}}}}"#
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    if let Some(call) = session.active_group_call.as_mut() {
        call.rtmp_url = Some("rtmp://dc1-rtmp.telegram.org:443/live".to_string());
        call.rtmp_stream_key = Some("demo-stream-key-9f3a2b1c".to_string());
    }
    session.open_chat(ChatId(51));
}

/// Phase C2f: incoming `messageGroupCall` invitation fixture — a Ready
/// group chat ("Design voice") opened on an incoming, pending
/// voice-chat invitation from Priya; the history row renders
/// Accept / Decline.
pub(super) fn apply_ready_group_call_invitation(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let chat_id = 52i64;
    let user_id = 46i64;
    let jsons = [
        format!(
            r#"{{"@type":"updateUser","user":{{"id":{user_id},"first_name":"Priya","last_name":"Nair","type":{{"@type":"userTypeRegular"}}}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Design voice","type":{{"@type":"chatTypeSupergroup","supergroup_id":{chat_id},"is_channel":false}},"unread_count":1}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":701,"chat_id":{chat_id},"sender_id":{{"@type":"messageSenderUser","user_id":{user_id}}},"is_outgoing":false,"date":1700000100,"content":{{"@type":"messageGroupCall","unique_id":"123456789","is_active":false,"was_missed":false,"is_video":false,"duration":0,"other_participant_ids":[]}}}}}}"#
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(chat_id));
}

/// Phase C2i: the peer (user id + display name) of a call entry,
/// when the call message is in a 1:1 chat. Group calls have no single
/// peer — `None` hides "Call again".
pub(super) fn call_message_peer(
    session: Option<&Session>,
    chat_id: ChatId,
) -> Option<(i64, String)> {
    let session = session?;
    let chat = session.chats.get(&chat_id.0)?;
    let user_id = match chat.kind {
        ChatKind::Private { user_id } | ChatKind::Secret { user_id, .. } => user_id.0,
        _ => return None,
    };
    let name = session
        .user(user_id)
        .map(|user| user.display_name())
        .unwrap_or_else(|| format!("User {user_id}"));
    Some((user_id, name))
}

impl QuillApp {
    /// Phase C1b: `createCall` from a user profile. `is_video: true`
    /// starts video-call *signaling* — media transport is still Phase
    /// C2, so the call carries no audio or video and the UI says so.
    /// The outgoing call is tracked once the `callId` answer arrives
    /// (with `is_video` derived from the request args); its states
    /// arrive as `updateCall`.
    /// Phase C2i: preference-aware entry point for starting a call
    /// from a profile / user panel / history row. When the
    /// confirm-before-calling pref is on, the call waits for the user
    /// to confirm in the dialog — the actual `startCall` goes through
    /// `dial_user`.
    pub(super) fn start_call_for_user(
        &mut self,
        user_id: i64,
        is_video: bool,
        cx: &mut Context<Self>,
    ) {
        if self
            .live
            .as_ref()
            .is_some_and(|live| !live.driver.has_call_engine())
        {
            self.status_note = "Calls are unavailable. The audio component could not start.".into();
            cx.notify();
            return;
        }
        // Refuse offline before the confirm dialog — a call can't be
        // queued, so confirming then failing would be dishonest.
        if self
            .live
            .as_ref()
            .is_some_and(|live| live.driver.session.is_offline())
        {
            self.status_note = "You're offline — can't start a call".into();
            cx.notify();
            return;
        }
        if self
            .session()
            .is_some_and(|session| session.call_prefs.confirm_before_calling)
        {
            self.call_confirm = Some((user_id, is_video));
            cx.notify();
            return;
        }
        self.dial_user(user_id, is_video, cx);
    }

    /// Phase C2i: the actual `startCall` send, after any confirmation.
    /// Slice parity:platform-offline-errors — a call can't be queued
    /// like a message, so refuse while offline with an honest note.
    pub(super) fn dial_user(&mut self, user_id: i64, is_video: bool, cx: &mut Context<Self>) {
        if self.live.is_some() {
            if self
                .live
                .as_ref()
                .expect("live")
                .driver
                .session
                .is_offline()
            {
                self.status_note = "You're offline — can't start a call".into();
                cx.notify();
                return;
            }
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .start_call(user_id, is_video);
            self.status_note = match result {
                Ok(_) => {
                    if is_video {
                        "starting video call…".into()
                    } else {
                        "calling…".into()
                    }
                }
                Err(_) => "could not start the call".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note = "demo: call start (no live Telegram)".into();
        }
        cx.notify();
    }

    /// Phase C2i: the user confirmed the pending call in the
    /// confirm-before-calling dialog.
    pub(super) fn confirm_pending_call(&mut self, cx: &mut Context<Self>) {
        if let Some((user_id, is_video)) = self.call_confirm.take() {
            self.dial_user(user_id, is_video, cx);
        } else {
            cx.notify();
        }
    }

    /// Phase C2i: the user cancelled the pending call in the
    /// confirm-before-calling dialog.
    pub(super) fn cancel_pending_call(&mut self, cx: &mut Context<Self>) {
        self.call_confirm = None;
        cx.notify();
    }

    /// Phase C2i: update one call pref in the session and persist it
    /// to the account dir (via `CallDriver::save_call_prefs`).
    pub(super) fn set_call_pref(
        &mut self,
        update: impl FnOnce(&mut CallPrefs),
        cx: &mut Context<Self>,
    ) {
        let mut prefs = self
            .session()
            .map(|session| session.call_prefs.clone())
            .unwrap_or_default();
        update(&mut prefs);
        if let Some(live) = self.live.as_mut() {
            live.driver.session.call_prefs = prefs;
            if let Err(err) = live.driver.save_call_prefs() {
                self.status_note = format!("couldn’t save call settings: {err}");
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.call_prefs = prefs;
            self.status_note = "demo: call settings are not saved".into();
        }
        self.sync_ptt_with_call(cx);
        cx.notify();
    }

    pub(super) fn toggle_call_mute(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let muted = live
                .driver
                .session
                .active_call
                .as_ref()
                .is_some_and(|call| !call.muted);
            self.status_note = match live.driver.set_call_muted(muted) {
                Ok(()) => {
                    if muted {
                        "microphone muted".into()
                    } else {
                        "microphone unmuted".into()
                    }
                }
                Err(err) => format!("could not change mute state: {err}"),
            };
        } else if let Some(call) = self
            .demo_session
            .as_mut()
            .and_then(|session| session.active_call.as_mut())
        {
            call.muted = !call.muted;
        }
        cx.notify();
    }

    /// Phase C2e: camera on/off toggle for a video call. Live: flips the
    /// camera intent and pushes it to the engine; an engine error
    /// surfaces in the status note without flipping the flag (driver
    /// contract). Demo: flips the flag only, no live Telegram.
    pub(super) fn toggle_call_camera(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let Some(call) = live.driver.session.active_call.as_ref() else {
                return;
            };
            let (call_id, camera_on) = (call.id, !call.camera_on);
            let ready = live.driver.call_video_ready();
            self.status_note = match live.driver.set_call_camera(call_id, camera_on && ready) {
                Ok(()) => {
                    if camera_on {
                        "camera on".into()
                    } else {
                        "camera off".into()
                    }
                }
                Err(err) => format!("could not change camera state: {err}"),
            };
        } else if let Some(call) = self
            .demo_session
            .as_mut()
            .and_then(|session| session.active_call.as_mut())
        {
            call.camera_on = !call.camera_on;
            self.status_note = "demo: camera toggle (no live Telegram)".into();
        }
        cx.notify();
    }

    /// Phase C2i: 1:1 screen-share send toggle for a video call.
    /// Live: flips the intent and pushes it to the engine; an engine
    /// error surfaces in the status note without flipping the flag
    /// (driver contract). Demo: flips the flag only, no live
    /// Telegram.
    pub(super) fn toggle_call_screen_share(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let Some(call) = live.driver.session.active_call.as_ref() else {
                return;
            };
            let (call_id, sharing) = (call.id, !call.screen_sharing);
            self.status_note = match live.driver.set_call_screen_share(call_id, sharing) {
                Ok(()) => {
                    if sharing {
                        "screen share on".into()
                    } else {
                        "screen share off".into()
                    }
                }
                Err(err) => format!("could not change screen share state: {err}"),
            };
        } else if let Some(call) = self
            .demo_session
            .as_mut()
            .and_then(|session| session.active_call.as_mut())
        {
            call.screen_sharing = !call.screen_sharing;
            self.status_note = "demo: screen share toggle (no live Telegram)".into();
        }
        cx.notify();
    }

    pub(super) fn select_call_device(
        &mut self,
        kind: quill::calls::engine::MediaDeviceKind,
        device_id: &str,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            // Phase C2e: camera selection goes to the driver, which
            // stores it and re-applies the camera on the active call.
            if kind == quill::calls::engine::MediaDeviceKind::Camera {
                self.status_note = match live.driver.select_call_camera(Some(device_id.to_string()))
                {
                    Ok(()) => "camera selected".into(),
                    Err(err) => format!("could not select camera: {err}"),
                };
                cx.notify();
                return;
            }
            let (microphone, speaker) = live.driver.selected_call_devices();
            let (microphone, speaker) = match kind {
                quill::calls::engine::MediaDeviceKind::Microphone => {
                    (Some(device_id.to_string()), speaker.map(str::to_owned))
                }
                quill::calls::engine::MediaDeviceKind::Speaker => {
                    (microphone.map(str::to_owned), Some(device_id.to_string()))
                }
                _ => return,
            };
            self.status_note = match live.driver.select_call_devices(microphone, speaker) {
                Ok(()) => "audio device selected".into(),
                Err(err) => format!("could not select audio device: {err}"),
            };
        } else {
            match kind {
                quill::calls::engine::MediaDeviceKind::Microphone => {
                    self.demo_selected_devices.0 = Some(device_id.into())
                }
                quill::calls::engine::MediaDeviceKind::Speaker => {
                    self.demo_selected_devices.1 = Some(device_id.into())
                }
                quill::calls::engine::MediaDeviceKind::Camera => {
                    self.demo_selected_camera = Some(device_id.into())
                }
                _ => return,
            }
        }
        cx.notify();
    }

    /// Phase C1: `acceptCall` for the ringing incoming call.
    pub(super) fn accept_incoming_call(&mut self, cx: &mut Context<Self>) {
        if self.live.is_some() {
            let result = self.live.as_mut().expect("live").driver.accept_call();
            self.status_note = match result {
                Ok(_) => "answering…".into(),
                Err(_) => "could not answer the call".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note = "demo: call accept (no live Telegram)".into();
        }
        cx.notify();
    }

    /// Phase C1: `discardCall` for the tracked call (decline an
    /// incoming call, or hang up an active one).
    pub(super) fn hang_up_call(&mut self, cx: &mut Context<Self>) {
        if self.live.is_some() {
            let result = self.live.as_mut().expect("live").driver.discard_call();
            self.status_note = match result {
                Ok(_) => "hanging up…".into(),
                Err(_) => "could not hang up the call".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note = "demo: call hang up (no live Telegram)".into();
        }
        cx.notify();
    }

    /// Phase C2i: star tap opens the rating-detail editor instead of
    /// sending immediately — the user picks problems + an optional
    /// comment, then submits via `submit_call_rating`.
    pub(super) fn open_rating_detail(
        &mut self,
        rating: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.rating_detail = Some(RatingDetail {
            stars: rating,
            problems: [false; 9],
        });
        self.rating_comment_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
        cx.notify();
    }

    pub(super) fn toggle_rating_problem(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(detail) = self.rating_detail.as_mut() {
            detail.problems[index] = !detail.problems[index];
        }
        cx.notify();
    }

    /// Phase C2i: `sendCallRating` with problems + comment (schema
    /// 1.8.67 :14234). Sends only from the detail editor's Submit
    /// button — `open_rating_detail` never sends on its own.
    pub(super) fn submit_call_rating(&mut self, cx: &mut Context<Self>) {
        let detail = match self.rating_detail.take() {
            Some(detail) => detail,
            None => {
                cx.notify();
                return;
            }
        };
        let comment = self.rating_comment_input.read(cx).value().to_string();
        let problems: Vec<&str> = CALL_PROBLEMS
            .iter()
            .enumerate()
            .filter(|(index, _)| detail.problems[*index])
            .map(|(_, (constructor, _))| *constructor)
            .collect();
        let stars = detail.stars;
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .send_call_rating(stars, &comment, &problems);
            self.status_note = match result {
                Ok(_) => "thanks for your feedback".into(),
                Err(_) => "could not send the rating".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note = "demo: call rating (no live Telegram)".into();
            if let Some(session) = self.demo_session.as_mut()
                && let Some(summary) = session.call_summary.as_mut()
            {
                summary.rating_sent = true;
            }
        }
        cx.notify();
    }

    pub(super) fn upload_call_diagnostics(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.send_call_debug_information() {
                Ok(_) => "diagnostics upload sent".into(),
                Err(_) => "could not upload diagnostics".into(),
            };
        } else if let Some(summary) = self
            .demo_session
            .as_mut()
            .and_then(|session| session.call_summary.as_mut())
        {
            summary.debug_information_sent = true;
            summary.debug_information_error = None;
            self.status_note = "demo: diagnostics upload (no live Telegram)".into();
        }
        cx.notify();
    }

    /// Phase C2i: `sendCallLog` from the call-end card — uploads the
    /// ended call's log file (schema 1.8.67 :14240).
    pub(super) fn upload_call_log(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.send_call_log() {
                Ok(_) => "call log upload sent".into(),
                Err(_) => "could not upload the call log".into(),
            };
        } else if let Some(summary) = self
            .demo_session
            .as_mut()
            .and_then(|session| session.call_summary.as_mut())
        {
            summary.log_sent = true;
            summary.log_error = None;
            self.status_note = "demo: call log upload (no live Telegram)".into();
        }
        cx.notify();
    }

    /// Phase C1: dismiss the call-end screen (rating skipped or
    /// acknowledged).
    pub(super) fn dismiss_call_summary(&mut self, cx: &mut Context<Self>) {
        for session in self
            .live
            .as_mut()
            .map(|live| &mut live.driver.session)
            .into_iter()
            .chain(self.demo_session.as_mut())
        {
            session.call_summary = None;
        }
        cx.notify();
    }

    /// Phase C1: dismiss a shown call-request error.
    pub(super) fn dismiss_call_error(&mut self, cx: &mut Context<Self>) {
        for session in self
            .live
            .as_mut()
            .map(|live| &mut live.driver.session)
            .into_iter()
            .chain(self.demo_session.as_mut())
        {
            session.call_error = None;
        }
        cx.notify();
    }

    /// Phase C1: 1s tick while a call is tracked, keeping the overlay's
    /// ringing / connected clock fresh. Mirrors the Phase A1 slow-mode
    /// tick (at most one task; exits when no call is active).
    pub(super) fn ensure_call_tick(&mut self, cx: &mut Context<Self>) {
        let call_active = self.session().is_some_and(|s| s.active_call.is_some());
        if !call_active || self.call_tick_active {
            return;
        }
        self.call_tick_active = true;
        cx.spawn(async move |this, cx| {
            loop {
                // Phase C2e: video calls tick at 100ms so incoming
                // frames reach the tiles; everything else stays at 1s.
                let interval = this
                    .update(cx, |this, _| this.call_tick_interval())
                    .unwrap_or(Duration::from_secs(1));
                cx.background_executor().timer(interval).await;
                let cont = this
                    .update(cx, |this, cx| {
                        let still_active = this.session().is_some_and(|s| s.active_call.is_some());
                        if still_active {
                            cx.notify();
                            true
                        } else {
                            this.call_tick_active = false;
                            false
                        }
                    })
                    .unwrap_or(false);
                if !cont {
                    break;
                }
            }
            let _ = this.update(cx, |this, _| {
                this.call_tick_active = false;
            });
        })
        .detach();
    }

    /// Phase C2e: tick interval for the call overlay — 100ms while a
    /// video call is Ready and a video feed is live (peer streaming or
    /// local camera on), 1s otherwise.
    pub(super) fn call_tick_interval(&self) -> Duration {
        let fast = self
            .session()
            .and_then(|s| s.active_call.as_ref())
            .is_some_and(|call| {
                call.is_video
                    && matches!(call.state, CallState::Ready)
                    && (call.remote_video != quill::calls::engine::RemoteVideoState::Inactive
                        || self.call_camera_effective(call))
            });
        if fast {
            Duration::from_millis(100)
        } else {
            Duration::from_secs(1)
        }
    }

    /// Phase C2e: the local camera actually contributes a feed — the
    /// intent is on and video can run (live: driver ready; demo:
    /// fixtures carry the frames).
    pub(super) fn call_camera_effective(&self, call: &ActiveCall) -> bool {
        call.camera_on
            && (self.live.is_none()
                || self
                    .live
                    .as_ref()
                    .is_some_and(|live| live.driver.call_video_ready()))
    }

    /// kit Phase 2 (redo): call confirm hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(super) fn build_call_confirm_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::CallConfirm, |this, _, cx| {
                this.cancel_pending_call(cx);
            });
        app.update(cx, |this, cx| {
            let Some((user_id, is_video)) = this.call_confirm else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title("Confirm call"))
                    .on_close(on_close.clone());
            };
            let name = this
                .session()
                .and_then(|session| session.user(user_id))
                .map(|user| user.display_name())
                .unwrap_or_else(|| format!("User {user_id}"));
            let kind = if is_video { "video call" } else { "call" };
            let body = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("You asked to confirm before calling."),
                )
                .into_any_element();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("call-confirm-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.cancel_pending_call(cx);
                            this.close_kit_dialog_if_done(DialogKind::CallConfirm, window, cx);
                        })),
                )
                .child(
                    Button::new("call-confirm-ok")
                        .label(if is_video { "Video call" } else { "Call" })
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.confirm_pending_call(cx);
                            this.close_kit_dialog_if_done(DialogKind::CallConfirm, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(format!(
                    "Start {kind} with {name}?"
                )))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// Swap prompt: answer the pending incoming call — the driver
    /// ends the current call first (TDLib allows a single active call)
    /// and accepts the incoming one once the discard lands.
    pub(super) fn answer_swap_call(&mut self, cx: &mut Context<Self>) {
        if self.live.is_some() {
            let result = self.live.as_mut().expect("live").driver.accept_swap_call();
            self.status_note = match result {
                Ok(_) => "ending current call, answering…".into(),
                Err(_) => "could not answer the call".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some((call_id, user_id, is_video)) = session.call_swap_pending.take() {
                session.active_call = None;
                session.call_summary = None;
                session.active_call = Some(ActiveCall {
                    id: call_id,
                    user_id,
                    is_outgoing: false,
                    is_video,
                    state: CallState::Pending {
                        is_created: false,
                        is_received: true,
                    },
                    started_at: Instant::now(),
                    ready_at: None,
                    ready: None,
                    transport: None,
                    transport_error: None,
                    signaling_queue: Vec::new(),
                    signaling_dropped: 0,
                    muted: false,
                    camera_on: is_video,
                    screen_sharing: false,
                    remote_video: quill::calls::engine::RemoteVideoState::Inactive,
                    remote_screen: quill::calls::engine::RemoteVideoState::Inactive,
                });
            }
            self.status_note = "demo: swap accepted (no live Telegram)".into();
        }
        cx.notify();
    }

    /// Swap prompt: decline the pending incoming call as busy.
    pub(super) fn decline_swap_call(&mut self, cx: &mut Context<Self>) {
        if self.live.is_some() {
            let result = self.live.as_mut().expect("live").driver.decline_swap_call();
            self.status_note = match result {
                Ok(_) => "incoming call declined".into(),
                Err(_) => "could not decline the call".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            session.call_swap_pending = None;
            self.status_note = "demo: swap declined (no live Telegram)".into();
        }
        cx.notify();
    }

    /// Swap prompt hosted in a kit `Dialog` via `window.open_dialog`
    /// (the kit Phase 2 redo pattern). Esc / backdrop / ✕ declines the
    /// incoming call as busy — the same honest outcome the old
    /// auto-decline produced.
    pub(super) fn build_call_swap_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::CallSwap, |this, _, cx| {
                this.decline_swap_call(cx);
            });
        app.update(cx, |this, cx| {
            let Some((_call_id, user_id, is_video)) =
                this.session().and_then(|s| s.call_swap_pending)
            else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title("Incoming call"))
                    .on_close(on_close.clone());
            };
            let name = this
                .session()
                .and_then(|session| session.user(user_id))
                .map(|user| user.display_name())
                .unwrap_or_else(|| format!("User {user_id}"));
            let kind = if is_video { "video call" } else { "call" };
            let body = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("You're already in a call. Telegram doesn't support putting a call on hold — answering ends the current one."),
                )
                .into_any_element();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("call-swap-decline")
                        .label("Decline")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.decline_swap_call(cx);
                            this.close_kit_dialog_if_done(DialogKind::CallSwap, window, cx);
                        })),
                )
                .child(
                    Button::new("call-swap-answer")
                        .label("End & answer")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.answer_swap_call(cx);
                            this.close_kit_dialog_if_done(DialogKind::CallSwap, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(format!("{name} is calling ({kind})")))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// Phase C2i: the peer (user id + display name) of a call entry,
    /// when the call message is in a 1:1 chat. Group calls have no
    /// single peer — `None` hides "Call again".
    pub(super) fn recent_call_peer(&self, chat_id: ChatId) -> Option<(i64, String)> {
        call_message_peer(self.session(), chat_id)
    }

    /// Phase C2i: `messageCall` service row — the reason-aware label
    /// (Telegram X `TD.getCallName` style) with duration and a "Call
    /// again" button for 1:1 chats.
    pub(super) fn call_message_row(
        message: &HistoryMessage,
        is_video: bool,
        discard_reason: &CallDiscardReason,
        duration: i32,
        session: Option<&Session>,
        cx: &mut Context<QuillApp>,
    ) -> impl IntoElement {
        let label = call_entry_label(is_video, discard_reason, duration, message.is_outgoing);
        let peer = call_message_peer(session, message.chat_id);
        let missed = discard_reason == &CallDiscardReason::Missed;
        div()
            .id(("call-message-row", message.id.0 as u64))
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_1()
            .child(div().text_sm().child(if is_video { "📹" } else { "📞" }))
            .child(
                div().flex().flex_col().min_w_0().flex_1().child(
                    div()
                        .text_sm()
                        .when(missed, |this| this.text_color(danger_soft()))
                        .child(label),
                ),
            )
            .child(if let Some((user_id, _)) = peer {
                Button::new(("call-message-again", message.id.0 as u64))
                    .label("Call again")
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.call_again(user_id, is_video, cx);
                    }))
                    .into_any_element()
            } else {
                div().into_any_element()
            })
    }

    /// Phase C2i: busy-decline banner for `call_busy_declined` — the
    /// incoming calls declined while another call was active.
    pub(super) fn call_busy_banner(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let declined = self.session()?.call_busy_declined.clone();
        if declined.is_empty() {
            return None;
        }
        let names: Vec<String> = declined
            .iter()
            .map(|(user_id, is_video)| {
                let name = self
                    .session()
                    .and_then(|session| session.user(*user_id))
                    .map(|user| user.display_name())
                    .unwrap_or_else(|| format!("User {user_id}"));
                format!("{name} ({})", if *is_video { "video" } else { "voice" })
            })
            .collect();
        Some(
            div()
                .id("call-busy-banner")
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_2()
                .border_b_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().accent.opacity(0.12))
                .child(div().text_sm().flex_1().child(format!(
                    "Missed call{} from {} — declined because another call was active.",
                    if declined.len() == 1 { "" } else { "s" },
                    names.join(", ")
                )))
                .child(
                    Button::new("call-busy-dismiss")
                        .label("Dismiss")
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(live) = this.live.as_mut() {
                                live.driver.session.call_busy_declined.clear();
                            } else if let Some(session) = this.demo_session.as_mut() {
                                session.call_busy_declined.clear();
                            }
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }

    /// Phase C2i: "Call again" from a history row — honours the
    /// confirm-before-calling preference before dialling.
    pub(super) fn call_again(&mut self, user_id: i64, is_video: bool, cx: &mut Context<Self>) {
        if self
            .live
            .as_ref()
            .is_some_and(|live| live.driver.session.is_offline())
        {
            self.status_note = "You're offline — can't start a call".into();
            cx.notify();
            return;
        }
        let confirm = self
            .session()
            .is_some_and(|session| session.call_prefs.confirm_before_calling);
        if confirm {
            self.call_confirm = Some((user_id, is_video));
            cx.notify();
            return;
        }
        self.dial_user(user_id, is_video, cx);
    }

    /// Recent calls; preferences live in Settings.
    pub(super) fn calls_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut list = div().id("calls-list").flex().flex_col().gap_1().px_1();
        list = list.child(div().text_sm().font_semibold().px_1().child("Recent calls"));
        let (entries, loading, failed, has_more) = self
            .session()
            .map(|session| {
                (
                    session.recent_calls.clone(),
                    session.recent_calls_loading,
                    session.recent_calls_error,
                    !session.recent_calls_offset.is_empty(),
                )
            })
            .unwrap_or_default();
        if failed {
            list = list
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Couldn’t load call history."),
                )
                .child(
                    Button::new("calls-retry")
                        .label("Retry")
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(live) = this.live.as_mut()
                                && let Err(err) = live.driver.fetch_call_history()
                            {
                                this.status_note = format!("call history request failed: {err:?}");
                            }
                            cx.notify();
                        })),
                );
        } else if loading && entries.is_empty() {
            list = list.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Loading calls…"),
            );
        } else if entries.is_empty() {
            list = list.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("No recent calls."),
            );
        } else {
            for entry in &entries {
                list = list.child(self.recent_call_row(entry, cx));
            }
            if has_more {
                list = list.child(
                    Button::new("calls-load-more")
                        .label("Load more")
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(live) = this.live.as_mut()
                                && let Err(err) = live.driver.fetch_more_call_history()
                            {
                                this.status_note = format!("call history request failed: {err:?}");
                            }
                            cx.notify();
                        })),
                );
            }
        }
        list
    }

    /// Phase C2i: one row of the server-side recent-calls list, with a
    /// reason-aware label (Telegram X `TD.getCallName` style) and
    /// "Call again" for 1:1 calls.
    pub(super) fn recent_call_row(
        &self,
        entry: &ParsedMessage,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (label, is_video) = match &entry.content {
            MessageContent::Call {
                is_video,
                discard_reason,
                duration,
            } => (
                call_entry_label(*is_video, discard_reason, *duration, entry.is_outgoing),
                *is_video,
            ),
            MessageContent::GroupCallInvitation { .. } => ("Group call".to_owned(), false),
            _ => ("Call".to_owned(), false),
        };
        let peer = self.recent_call_peer(entry.chat_id);
        let name = peer
            .as_ref()
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| "Call".to_owned());
        let missed = !entry.is_outgoing
            && matches!(
                entry.content,
                MessageContent::Call {
                    discard_reason: CallDiscardReason::Missed,
                    ..
                }
            );
        div()
            .id(("call-row", entry.id.0 as u64))
            .px_2()
            .py_2()
            .rounded_md()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .flex_1()
                    .child(div().font_medium().text_sm().child(name))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(label),
                    ),
            )
            .child(if missed {
                div()
                    .text_xs()
                    .font_medium()
                    .text_color(danger_soft())
                    .child("missed")
                    .into_any_element()
            } else {
                div().into_any_element()
            })
            .child(if let Some((user_id, _)) = peer {
                Button::new(("call-again", entry.id.0 as u64))
                    .label("Call again")
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.call_again(user_id, is_video, cx);
                    }))
                    .into_any_element()
            } else {
                div().into_any_element()
            })
    }
}
