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
    session.groups.set_boost_status(
        13,
        crate::telegram::envelope::ParsedBoostStatus {
            level,
            boost_count: 5,
            ..Default::default()
        },
    );
    (session, sink, seq)
}

#[test]
fn the_same_level_keeps_the_cached_status() {
    let (mut session, sink, seq) = channel_with_status(2);
    apply_json(&mut session, &seq, &sink, &supergroup_update(2));
    assert!(session.outdated_boost_chats().is_empty());
    assert!(session.groups.boost_status(13).is_some());
}

#[test]
fn a_moved_level_outdates_the_cached_status_and_keeps_it_visible() {
    let (mut session, sink, seq) = channel_with_status(2);
    apply_json(&mut session, &seq, &sink, &supergroup_update(3));
    apply_json(&mut session, &seq, &sink, &supergroup_update(3));
    assert_eq!(session.outdated_boost_chats(), vec![ChatId(13)]);
    assert_eq!(
        session.groups.boost_level(13),
        Some(2),
        "kept until replaced"
    );
    assert!(session.groups.boost_status_wanted(13));
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
    assert!(session.outdated_boost_chats().is_empty());
    assert!(session.groups.boost_status(13).is_some());
}

#[test]
fn the_owned_star_count_also_updates_the_subscriptions_snapshot() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.payments.star_subscriptions.set_data(Some(
        crate::telegram::envelope::StarSubscriptionsData {
            star_amount: 10,
            subscriptions: Vec::new(),
            required_star_count: 0,
            next_offset: String::new(),
        },
    ));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateOwnedStarCount","star_amount":{"@type":"starAmount","star_count":42,"nanostar_count":5}}"#,
    );
    assert_eq!(session.payments.hub.balance.map(|b| b.stars), Some(42));
    assert_eq!(
        session
            .payments
            .star_subscriptions
            .data()
            .map(|s| s.star_amount),
        Some(42)
    );
}
