//! State reducer tests: failed requests that name a Premium limit open the
//! limit box instead of an error line.
use super::common::*;
use super::*;
use crate::premium_limits::PremiumLimitKind;

fn fail(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    purpose: RequestPurpose,
    chat: Option<ChatId>,
    message: &str,
) {
    static SEQ: AtomicU64 = AtomicU64::new(1);
    let seq = AtomicU64::new(SEQ.fetch_add(100, std::sync::atomic::Ordering::Relaxed));
    let extra = session.request(purpose, chat);
    apply_json(
        session,
        &seq,
        sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"{message}"}}"#,
            extra.0
        ),
    );
}

#[test]
fn pin_over_the_limit_opens_the_pins_box() {
    let (mut session, sink) = session();
    fail(
        &mut session,
        &sink,
        RequestPurpose::ToggleChatIsPinned,
        Some(ChatId(11)),
        "Maximum number of pinned chats exceeded",
    );
    assert_eq!(
        session.payments.hub.limit_box,
        Some((PremiumLimitKind::Pins, 0))
    );
    assert_eq!(session.chats_state.chat_action_error, None);
}

#[test]
fn other_pin_errors_stay_error_lines() {
    let (mut session, sink) = session();
    fail(
        &mut session,
        &sink,
        RequestPurpose::ToggleChatIsPinned,
        Some(ChatId(11)),
        "Chat not found",
    );
    assert_eq!(session.payments.hub.limit_box, None);
    assert!(session.chats_state.chat_action_error.is_some());
}

#[test]
fn joining_too_many_communities_opens_the_channels_box() {
    let (mut session, sink) = session();
    fail(
        &mut session,
        &sink,
        RequestPurpose::JoinChat,
        Some(ChatId(-100)),
        "CHANNELS_TOO_MUCH",
    );
    assert_eq!(
        session.payments.hub.limit_box,
        Some((PremiumLimitKind::Channels, 0))
    );
}

#[test]
fn public_link_and_caption_errors_route_to_their_boxes() {
    let (mut session, sink) = session();
    fail(
        &mut session,
        &sink,
        RequestPurpose::SetSupergroupUsername,
        Some(ChatId(-100)),
        "CHANNELS_ADMIN_PUBLIC_TOO_MUCH",
    );
    assert_eq!(
        session.payments.hub.limit_box,
        Some((PremiumLimitKind::PublicLinks, 0))
    );
    session.payments.hub.limit_box = None;
    fail(
        &mut session,
        &sink,
        RequestPurpose::SendMessage,
        Some(ChatId(11)),
        "MEDIA_CAPTION_TOO_LONG",
    );
    assert_eq!(
        session.payments.hub.limit_box,
        Some((PremiumLimitKind::Caption, 0))
    );
}

#[test]
fn premium_availability_follows_the_option() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    assert!(session.premium_possible());
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateOption","name":"is_premium_available","value":{"@type":"optionValueBoolean","value":false}}"#,
    );
    assert!(!session.premium_possible());
}
