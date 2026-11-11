//! Sweep fixes (s4-links-tdlib-1): failures matched to what asked.
use super::common::*;
use super::*;

fn ready_session() -> Session {
    let (mut session, _sink) = session();
    session.connection = ConnectionState::Ready;
    session
}

/// Sweep 05-156: a username check that was superseded and then failed says
/// nothing about the text now being checked.
#[test]
fn a_superseded_username_check_failure_shows_no_profile_error() {
    let mut session = ready_session();
    let first = session.request(RequestPurpose::CheckUsername, None);
    let second = session.request(RequestPurpose::CheckUsername, None);
    session.users_state.username_check_pending = Some((second, "newer".into()));

    let lost = session.requests.take(first).expect("registered");
    session.settle_lost_request(lost);
    assert_eq!(session.users_state.profile_edit_error, None);
    assert_eq!(
        session.users_state.username_check_pending,
        Some((second, "newer".into())),
        "the current check stays pending"
    );

    let lost = session.requests.take(second).expect("registered");
    session.settle_lost_request(lost);
    assert!(session.users_state.profile_edit_error.is_some());
    assert_eq!(session.users_state.username_check_pending, None);
}

/// Sweep 02-030: a failed count drops the chat's entry, so the next open
/// asks for every count again instead of showing a zero for good.
#[test]
fn a_failed_media_count_is_asked_again_on_the_next_open() {
    let mut session = ready_session();
    let chat = ChatId(5);
    session
        .media
        .chat_media_counts
        .insert(chat.0, Default::default());
    let extra = session.request(
        RequestPurpose::Users(UsersPurpose::GetChatMessageCount { filter: 2 }),
        Some(chat),
    );
    let lost = session.requests.take(extra).expect("registered");
    session.settle_lost_request(lost);
    assert!(!session.media.chat_media_counts.contains_key(&chat.0));
}
