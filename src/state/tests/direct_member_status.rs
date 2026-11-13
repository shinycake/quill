//! The banned-or-not status of a direct-messages conversation's user: a
//! check that fails says so (and can be asked again), and only the check
//! that waits for an answer takes it.
use super::common::*;
use super::*;

const CHANNEL: ChatId = ChatId(9);
const USER: i64 = 5;

fn asked(session: &mut Session, request: RequestId) {
    let status = session
        .threads
        .direct
        .kicked
        .entry((CHANNEL.0, USER))
        .or_default();
    status.begin_with(request);
}

#[test]
fn a_refused_check_is_failed_and_asked_again() {
    let (mut session, _sink) = session();
    asked(&mut session, RequestId(3));
    assert!(!session.direct_member_check_failed(CHANNEL, USER));
    // Another request's refusal changes nothing.
    session.fail_direct_member(CHANNEL, USER, RequestId(2));
    assert!(!session.direct_member_check_failed(CHANNEL, USER));
    session.fail_direct_member(CHANNEL, USER, RequestId(3));
    assert!(session.direct_member_check_failed(CHANNEL, USER));
    assert_eq!(session.direct_member_banned(CHANNEL, USER), None);
    // Asked again, it waits again and the new answer lands.
    asked(&mut session, RequestId(4));
    assert!(!session.direct_member_check_failed(CHANNEL, USER));
    session.answer_direct_member(CHANNEL, USER, Some(RequestId(3)), true);
    assert_eq!(session.direct_member_banned(CHANNEL, USER), None, "old one");
    session.answer_direct_member(CHANNEL, USER, Some(RequestId(4)), true);
    assert_eq!(session.direct_member_banned(CHANNEL, USER), Some(true));
}
