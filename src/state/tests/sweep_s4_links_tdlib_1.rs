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
fn a_superseded_username_check_failure_leaves_the_current_check_pending() {
    let mut session = ready_session();
    let first = session.request(RequestPurpose::CheckUsername, None);
    let second = session.request(RequestPurpose::CheckUsername, None);
    session.users_state.username_check.begin(second, "newer");

    let lost = session.requests.take(first).expect("registered");
    session.settle_lost_request(lost);
    assert_eq!(
        session.users_state.username_check.pending(),
        Some(second),
        "the current check stays pending"
    );

    let lost = session.requests.take(second).expect("registered");
    session.settle_lost_request(lost);
    assert_eq!(session.users_state.username_check.pending(), None);
}

/// Sweep 02-030 (r2 06-021): a failed count stays unknown, never a zero
/// for good; every open asks for all of them again (`fetch_chat_media_counts`).
#[test]
fn a_failed_media_count_is_asked_again_on_the_next_open() {
    let mut session = ready_session();
    let chat = ChatId(5);
    session
        .media
        .chat_media_counts
        .insert(chat.0, [(1u8, 4)].into_iter().collect());
    let extra = session.request(
        RequestPurpose::Users(UsersPurpose::GetChatMessageCount { filter: 2 }),
        Some(chat),
    );
    let lost = session.requests.take(extra).expect("registered");
    session.settle_lost_request(lost);
    let counts = &session.media.chat_media_counts[&chat.0];
    assert_eq!(counts.get(&1), Some(&4), "an older count stays shown");
    assert_eq!(counts.get(&2), None, "the failed one is unknown, not zero");
}
