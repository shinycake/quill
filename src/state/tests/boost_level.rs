//! A channel's boost level moving in `updateSupergroup` drops the cached
//! boost status so the open boost UI asks again.
use super::common::*;
use super::*;

fn supergroup_update(level: i32) -> String {
    format!(
        r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":113,"is_channel":true,"boost_level":{level},"status":{{"@type":"chatMemberStatusMember"}}}}}}"#
    )
}

fn channel_with_status(level: i32) -> (Session, Arc<MemorySink>, AtomicU64) {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":13,"title":"G","type":{"@type":"chatTypeSupergroup","supergroup_id":113,"is_channel":true},"unread_count":0}}"#,
    );
    apply_json(&mut session, &seq, &sink, &supergroup_update(level));
    session.groups.chat_boost_status.insert(13, (level, 5));
    (session, sink, seq)
}

#[test]
fn the_same_level_keeps_the_cached_status() {
    let (mut session, sink, seq) = channel_with_status(2);
    apply_json(&mut session, &seq, &sink, &supergroup_update(2));
    assert!(session.take_moved_boost_chats().is_empty());
    assert!(session.groups.chat_boost_status.contains_key(&13));
}

#[test]
fn a_moved_level_drops_the_cached_status_once() {
    let (mut session, sink, seq) = channel_with_status(2);
    apply_json(&mut session, &seq, &sink, &supergroup_update(3));
    apply_json(&mut session, &seq, &sink, &supergroup_update(3));
    assert_eq!(session.take_moved_boost_chats(), vec![ChatId(13)]);
    assert!(!session.groups.chat_boost_status.contains_key(&13));
    assert!(session.take_moved_boost_chats().is_empty());
}

#[test]
fn another_supergroup_moving_leaves_the_status_alone() {
    let (mut session, sink, seq) = channel_with_status(2);
    for level in [1, 2] {
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":999,"boost_level":{level}}}}}"#
            ),
        );
    }
    assert!(session.take_moved_boost_chats().is_empty());
    assert!(session.groups.chat_boost_status.contains_key(&13));
}

#[test]
fn the_owned_star_count_also_updates_the_subscriptions_snapshot() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.payments.star_subscriptions = Some(crate::telegram::envelope::StarSubscriptionsData {
        star_amount: 10,
        subscriptions: Vec::new(),
        required_star_count: 0,
        next_offset: String::new(),
    });
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateOwnedStarCount","star_amount":{"@type":"starAmount","star_count":42,"nanostar_count":5}}"#,
    );
    assert_eq!(session.payments.hub.balance.map(|b| b.stars), Some(42));
    assert_eq!(
        session.payments.star_subscriptions.map(|s| s.star_amount),
        Some(42)
    );
}
