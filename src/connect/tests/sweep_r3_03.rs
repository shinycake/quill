//! Sweep fixes (r3-03): lists a membership change made `Stale` are asked
//! again by the next reader, whatever request was out before the change.
use super::support::toggle_driver;
use crate::connect::ConnectDriver;
use crate::diagnostics::DiagnosticSink;
use crate::ids::ChatId;
use crate::state::{AdminListFetch, MemberListFilter, SupergroupMembersFetch};
use crate::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const GROUP: &str = r#"{"@type":"updateNewChat","chat":{"id":-100,"title":"Group","type":{"@type":"chatTypeSupergroup","supergroup_id":100,"is_channel":false},"unread_count":0}}"#;
const CREATOR: &str = r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":100,"status":{"@type":"chatMemberStatusCreator","is_member":true}}}"#;

fn feed_group<S: crate::connect::JsonSender>(
    driver: &mut ConnectDriver<S>,
    sink: &Arc<dyn DiagnosticSink>,
    seq: &AtomicU64,
) {
    for json in [GROUP, CREATOR] {
        driver
            .ingest(copy_and_parse(json, seq, sink).expect("parse"))
            .expect("ingest");
    }
}

/// 08-002: a page made `Stale` while its request is still out is asked
/// again (the older request does not count as the list being asked), and
/// the older answer does not land on the new stage.
#[test]
fn a_stale_member_list_is_asked_again_while_an_older_request_is_out() {
    let (_dir, mut driver, _sender, sink, seq) = toggle_driver();
    feed_group(&mut driver, &sink, &seq);
    let key = (-100, MemberListFilter::Recent);
    let first = driver
        .fetch_supergroup_members(ChatId(-100), MemberListFilter::Recent, "")
        .unwrap()
        .expect("asked");
    driver
        .session
        .groups
        .supergroup_members
        .get_mut(&key)
        .map(|list| {
            list.fetch = SupergroupMembersFetch::Loaded {
                members: Vec::new(),
                total_count: 0,
            }
        });
    driver.session.groups.members_changed(-100);
    assert!(driver.session.groups.member_list_is_stale(key));
    let second = driver
        .fetch_supergroup_members(ChatId(-100), MemberListFilter::Recent, "")
        .unwrap()
        .expect("a stale list is asked again");
    assert_ne!(first, second);
    assert!(matches!(
        driver.session.groups.member_fetch(key),
        Some(SupergroupMembersFetch::Loading { request, .. }) if *request == second
    ));
}

/// A stale list that cannot be asked (the send is refused) says so with
/// Retry instead of staying stale for the poll to ask again every tick.
#[test]
fn a_stale_member_list_that_cannot_be_sent_becomes_not_sent() {
    let (_dir, mut driver, sender, sink, seq) = toggle_driver();
    feed_group(&mut driver, &sink, &seq);
    let key = (-100, MemberListFilter::Recent);
    driver.session.groups.set_member_fetch(
        key,
        SupergroupMembersFetch::Loaded {
            members: Vec::new(),
            total_count: 0,
        },
    );
    driver.session.groups.members_changed(-100);
    sender.refuse(true);
    assert!(
        driver
            .fetch_supergroup_members(ChatId(-100), MemberListFilter::Recent, "")
            .is_err()
    );
    assert!(matches!(
        driver.session.groups.member_fetch(key),
        Some(SupergroupMembersFetch::NotSent(_))
    ));
    assert!(!driver.session.groups.member_list_is_stale(key));
}

/// The administrators list a membership change made stale is asked again
/// even while the request asked before the change is on its way.
#[test]
fn stale_administrators_are_asked_again() {
    let (_dir, mut driver, _sender, sink, seq) = toggle_driver();
    feed_group(&mut driver, &sink, &seq);
    let first = driver
        .fetch_chat_administrators(ChatId(-100))
        .unwrap()
        .expect("asked");
    driver.session.groups.members_changed(-100);
    assert_eq!(driver.session.groups.stale_admin_chats(), vec![-100]);
    let second = driver
        .fetch_chat_administrators(ChatId(-100))
        .unwrap()
        .expect("asked again");
    assert_ne!(first, second);
    assert_eq!(
        driver.session.groups.admin_lists.get(&-100),
        Some(&AdminListFetch::Loading(second))
    );
    assert!(driver.session.groups.stale_admin_chats().is_empty());
}
