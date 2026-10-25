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

#[test]
fn invite_link_join_over_the_limit_opens_the_channels_box() {
    let (mut session, sink) = session();
    session.chats_state.deep_link = Some(DeepLinkState::ResolvingChat {
        action: DeepLinkAction::JoinInvite { hash: "h".into() },
        generation: 3,
    });
    fail(
        &mut session,
        &sink,
        RequestPurpose::Chats(ChatsPurpose::DeepLinkJoin { generation: 3 }),
        None,
        "CHANNELS_TOO_MUCH",
    );
    assert_eq!(session.chats_state.deep_link, None);
    assert_eq!(
        session.payments.hub.limit_box,
        Some((PremiumLimitKind::Channels, 0))
    );
}

#[test]
fn a_saved_messages_pin_over_the_limit_opens_its_own_box() {
    let (mut session, sink) = session();
    fail(
        &mut session,
        &sink,
        RequestPurpose::Threads(ThreadsPurpose::ToggleSavedMessagesTopicPinned { topic_id: 7 }),
        None,
        "The maximum number of pinned chats exceeded",
    );
    assert_eq!(
        session.payments.hub.limit_box,
        Some((PremiumLimitKind::SavedPins, 0))
    );
    assert_eq!(session.chats_state.chat_action_error, None);
}

#[test]
fn the_saved_pin_check_counts_pinned_sublists_against_the_limit() {
    let (mut session, _sink) = session();
    let topic = |id: i64, is_pinned: bool| crate::telegram::envelope::SavedMessagesTopic {
        id,
        kind: crate::telegram::envelope::SavedTopicKind::MyNotes,
        is_pinned,
        order: id,
        last_message_id: 0,
        last_message_date: 0,
        last_message_preview: String::new(),
        hidden_sender_name: String::new(),
    };
    for id in 1..=5 {
        session.threads.saved.topics.insert(id, topic(id, id != 5));
    }
    assert!(!session.saved_pin_limit_reached());
    session.threads.saved.topics.insert(5, topic(5, true));
    assert!(session.saved_pin_limit_reached());
    session.payments.hub.limits.apply_premium_limit(
        "premiumLimitTypePinnedSavedMessagesTopicCount",
        5,
        100,
    );
    session.payments.premium_option = Some(true);
    assert!(!session.saved_pin_limit_reached());
    session
        .payments
        .hub
        .limits
        .apply_option("pinned_saved_messages_topic_count_max", 5);
    assert!(session.saved_pin_limit_reached());
}

#[test]
fn toast_kinds_queue_a_toast_instead_of_a_box() {
    let (mut session, _sink) = session();
    session.open_premium_limit(PremiumLimitKind::FavoriteStickers, 0);
    assert_eq!(session.payments.hub.limit_box, None);
    assert_eq!(
        session.payments.hub.limit_toast,
        Some(PremiumLimitKind::FavoriteStickers)
    );
}
