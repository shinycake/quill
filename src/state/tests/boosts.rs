//! State reducer tests: boost details, level table and the unrestrict
//! boosts setting.
use super::common::*;
use super::*;

const CREATOR: &str = r#"{"@type":"chatMemberStatusCreator","is_member":true}"#;
const MEMBER: &str = r#"{"@type":"chatMemberStatusMember"}"#;

fn supergroup(
    session: &mut Session,
    seq: &AtomicU64,
    sink: &Arc<MemorySink>,
    is_channel: bool,
    status: &str,
) {
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":13,"title":"G","type":{{"@type":"chatTypeSupergroup","supergroup_id":113,"is_channel":{is_channel}}},"unread_count":0}}}}"#
        ),
    );
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":113,"is_channel":{is_channel},"status":{status}}}}}"#
        ),
    );
}

#[test]
fn boost_status_keeps_progress_details() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetChatBoostStatus, Some(ChatId(13)));
    let fetch = session.groups.chat_boost_status.entry(13).or_default();
    fetch.begin_with(extra);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatBoostStatus","@extra":"{}","level":2,"boost_count":15,"current_level_boost_count":10,"next_level_boost_count":20,"premium_member_count":4,"premium_member_percentage":2.0}}"#,
            extra.0
        ),
    );
    let details = session.groups.boost_status(13).expect("answered");
    assert_eq!(details.level_fraction(), 0.5);
    assert_eq!(details.boosts_to_next_level(), Some(5));
    assert_eq!(session.groups.boost_level_and_count(13), Some((2, 15)));
}

fn boost_status_json(extra: RequestId, level: i32, count: i32) -> String {
    format!(
        r#"{{"@type":"chatBoostStatus","@extra":"{}","level":{level},"boost_count":{count}}}"#,
        extra.0
    )
}

#[test]
fn a_boost_status_waits_for_its_own_request_and_keeps_the_old_level_on_a_failed_refetch() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let first = session.request(RequestPurpose::GetChatBoostStatus, Some(ChatId(13)));
    let fetch = session.groups.chat_boost_status.entry(13).or_default();
    fetch.begin_with(first);
    apply_json(&mut session, &seq, &sink, &boost_status_json(first, 1, 3));
    assert_eq!(session.groups.boost_level_and_count(13), Some((1, 3)));

    // A change made the level due again: it stays on screen, and the
    // request in flight asked before the change, so its answer is dropped.
    let stale = session.request(RequestPurpose::GetChatBoostStatus, Some(ChatId(13)));
    if let Some(fetch) = session.groups.chat_boost_status.get_mut(&13) {
        fetch.begin_with(stale);
    }
    session.groups.outdate_boost_status(13);
    assert_eq!(session.groups.boost_level_and_count(13), Some((1, 3)));
    apply_json(&mut session, &seq, &sink, &boost_status_json(stale, 2, 9));
    assert_eq!(
        session.groups.boost_level_and_count(13),
        Some((1, 3)),
        "an answer nobody waits for is dropped"
    );

    // The refetch fails: the old level stays, the fetch says why.
    let refetch = session.request(RequestPurpose::GetChatBoostStatus, Some(ChatId(13)));
    if let Some(fetch) = session.groups.chat_boost_status.get_mut(&13) {
        fetch.begin_with(refetch);
    }
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_ADMIN_REQUIRED"}}"#,
            refetch.0
        ),
    );
    let fetch = session.groups.chat_boost_status.get(&13);
    assert!(fetch.is_some_and(|fetch| fetch.error().is_some()));
    assert_eq!(session.groups.boost_level_and_count(13), Some((1, 3)));
}

#[test]
fn boost_features_cached_per_chat() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetChatBoostFeatures, Some(ChatId(13)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatBoostFeatures","@extra":"{}","features":[{{"level":1,"story_per_day_count":1}}]}}"#,
            extra.0
        ),
    );
    assert_eq!(session.groups.chat_boost_features[&13].levels.len(), 1);
}

#[test]
fn unrestrict_value_applies_on_ok_and_drops_on_error() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    supergroup(&mut session, &seq, &sink, false, CREATOR);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateSupergroupFullInfo","supergroup_id":113,"supergroup_full_info":{"description":"d","unrestrict_boost_count":2}}"#,
    );
    assert!(session.chat_can_edit_unrestrict_boosts(ChatId(13)));
    assert_eq!(session.chat_unrestrict_boost_count(ChatId(13)), 2);

    let extra = session.request(
        RequestPurpose::SetSupergroupUnrestrictBoostCount,
        Some(ChatId(13)),
    );
    session.groups.unrestrict_boost_saves.insert(
        13,
        UnrestrictSave {
            request: extra,
            value: 4,
        },
    );
    assert_eq!(session.chat_unrestrict_boost_count(ChatId(13)), 4);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(session.groups.unrestrict_boost_saves.is_empty());
    assert_eq!(session.chat_unrestrict_boost_count(ChatId(13)), 4);

    let extra = session.request(
        RequestPurpose::SetSupergroupUnrestrictBoostCount,
        Some(ChatId(13)),
    );
    session.groups.unrestrict_boost_saves.insert(
        13,
        UnrestrictSave {
            request: extra,
            value: 0,
        },
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"BAD"}}"#,
            extra.0
        ),
    );
    assert!(session.groups.unrestrict_boost_saves.is_empty());
    assert_eq!(session.chat_unrestrict_boost_count(ChatId(13)), 4);
}

#[test]
fn unrestrict_setting_is_for_supergroup_rights_only() {
    let seq = AtomicU64::new(0);
    let (mut channel, sink) = session();
    supergroup(&mut channel, &seq, &sink, true, CREATOR);
    assert!(!channel.chat_can_edit_unrestrict_boosts(ChatId(13)));
    let (mut member, sink) = session();
    supergroup(&mut member, &seq, &sink, false, MEMBER);
    assert!(!member.chat_can_edit_unrestrict_boosts(ChatId(13)));
}

#[test]
fn a_boost_count_save_in_flight_is_reported_until_it_answers() {
    let (mut session, _sink) = super::common::session();
    assert!(!session.unrestrict_boost_saving(ChatId(13)));
    session.groups.unrestrict_boost_saves.insert(
        13,
        UnrestrictSave {
            request: RequestId(1),
            value: 4,
        },
    );
    assert!(session.unrestrict_boost_saving(ChatId(13)));
    assert!(!session.unrestrict_boost_saving(ChatId(14)));
}

/// An answer of an older save does not settle the save that replaced it.
#[test]
fn an_older_boost_save_answer_does_not_settle_the_newer_save() {
    let (mut session, sink) = super::common::session();
    let seq = AtomicU64::new(0);
    let old = session.request(
        RequestPurpose::SetSupergroupUnrestrictBoostCount,
        Some(ChatId(13)),
    );
    let new = session.request(
        RequestPurpose::SetSupergroupUnrestrictBoostCount,
        Some(ChatId(13)),
    );
    session.groups.unrestrict_boost_saves.insert(
        13,
        UnrestrictSave {
            request: new,
            value: 3,
        },
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, old.0),
    );
    assert!(
        session.unrestrict_boost_saving(ChatId(13)),
        "still the newer"
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, new.0),
    );
    assert!(!session.unrestrict_boost_saving(ChatId(13)));
}
