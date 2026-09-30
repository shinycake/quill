//! Shared helpers for the `tests/replay_*` integration tests.
//! Split from `tests/replay.rs` — pure code motion.
pub use quill::diagnostics::{Diagnostic, DiagnosticSink, MemorySink};
pub use quill::ids::AccountKey;
pub use quill::state::{MemberListFilter, RequestPurpose, Session};
pub use quill::telegram::client::{ReceiveBridge, copy_and_parse};
pub use quill::telegram::envelope::EnvelopePayload;
pub use std::sync::Arc;
pub use std::sync::atomic::AtomicU64;
pub use std::time::Duration;

pub fn apply_all(session: &mut Session, sink: &Arc<MemorySink>, jsons: &[&str]) {
    let seq = AtomicU64::new(0);
    apply_all_seq(session, sink, &seq, jsons);
}

pub fn apply_all_seq(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
    jsons: &[&str],
) {
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    for json in jsons {
        let owned = copy_and_parse(json, seq, &dyn_sink).unwrap();
        session.apply(owned);
    }
}

/// Phase 2.2 channel fixtures. The channel chat is now ungated: sponsored rows
/// fetch through the same pipeline while history opens normally.
pub fn sponsored_test_session(sink: &Arc<MemorySink>, seq: &AtomicU64) -> Session {
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    apply_all_seq(
        &mut session,
        sink,
        seq,
        &[
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
        ],
    );
    // Channels are supported since Phase 2.2; the sponsored pipeline runs for
    // the open channel.
    let chat = session.chats.get(&13).unwrap();
    assert!(chat.supported());
    assert!(chat.kind.gate_reason().is_none());
    assert!(chat.is_channel());
    assert!(!chat.can_post());

    session.open_chat(quill::ids::ChatId(13));
    let extra = session.request(
        RequestPurpose::GetChatSponsoredMessages,
        Some(quill::ids::ChatId(13)),
    );
    let thumb = r#"{"@type":"file","id":61,"size":24,"expected_size":24,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}"#;
    apply_all_seq(
        &mut session,
        sink,
        seq,
        &[&format!(
            r#"{{"@type":"sponsoredMessages","@extra":"{}","messages_between":3,"messages":[{{"@type":"sponsoredMessage","message_id":9001,"is_recommended":false,"can_be_reported":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"summer sale","entities":[]}}}},"sponsor":{{"@type":"advertisementSponsor","url":"https://example.com/promo","photo":{{"@type":"photo","has_stickers":false,"sizes":[]}},"info":"Example Ads"}},"title":"Summer sale","button_text":"Shop now","accent_color_id":0,"background_custom_emoji_id":"0","additional_info":"Ad by Example"}},{{"@type":"sponsoredMessage","message_id":9002,"is_recommended":true,"can_be_reported":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{thumb},"width":240,"height":160,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"pick","entities":[]}},"has_spoiler":false,"is_secret":false}},"sponsor":{{"@type":"advertisementSponsor","url":"https://example.com/pick","photo":{{"@type":"photo","has_stickers":false,"sizes":[]}},"info":"Curated"}},"title":"Editors' pick","button_text":"Learn more","accent_color_id":0,"background_custom_emoji_id":"0","additional_info":""}}]}}"#,
            extra.0
        )],
    );
    session
}

pub const BOT_USER_JSON: &str = r#"{"@type":"updateUser","user":{"id":21,"first_name":"Demo","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#;
