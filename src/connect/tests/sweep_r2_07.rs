//! Sweep fixes (r2-07): requests that cannot be sent land in a `Failed`
//! stage the box draws with Retry, not in an idle or loading one.
use super::support::toggle_driver;
use crate::connect::ConnectDriver;
use crate::diagnostics::DiagnosticSink;
use crate::ids::ChatId;
use crate::state::fetch::Fetch;
use crate::state::{MemberListFilter, OwnerLookup, SupergroupMembersFetch};
use crate::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const OWNED_GROUP: &str = r#"{"@type":"updateNewChat","chat":{"id":-100,"title":"Group","type":{"@type":"chatTypeSupergroup","supergroup_id":100,"is_channel":false},"unread_count":0}}"#;
const CREATOR: &str = r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":100,"status":{"@type":"chatMemberStatusCreator","is_member":true}}}"#;

fn feed_owned_group<S: crate::connect::JsonSender>(
    driver: &mut ConnectDriver<S>,
    sink: &Arc<dyn DiagnosticSink>,
    seq: &AtomicU64,
) {
    for json in [OWNED_GROUP, CREATOR] {
        driver
            .ingest(copy_and_parse(json, seq, sink).expect("parse"))
            .expect("ingest");
    }
}

/// 11-038 / 11-039: the box opens while sends are refused: the gate, the
/// heir lookup and the member lists are `Failed`, so the box draws Retry
/// instead of "Checking..." for good.
#[test]
fn the_ownership_box_opened_while_sends_fail_shows_failures_to_retry() {
    let (_dir, mut driver, sender, sink, seq) = toggle_driver();
    feed_owned_group(&mut driver, &sink, &seq);
    sender.refuse(true);
    driver.open_ownership(ChatId(-100));
    let ownership = &driver.session.groups.ownership;
    assert!(ownership.check.error().is_some(), "the gate failed to send");
    assert!(matches!(
        ownership.owner_after_leaving.get(&-100),
        Some(OwnerLookup::Failed(_))
    ));
    for filter in [MemberListFilter::Administrators, MemberListFilter::Recent] {
        assert!(matches!(
            driver.session.groups.member_fetch((-100, filter)),
            Some(SupergroupMembersFetch::Failed(_))
        ));
    }
    // Retry once the connection is back: asked again, `Loading` this time.
    sender.refuse(false);
    driver.request_ownership_candidates(ChatId(-100));
    assert!(driver.check_can_transfer_ownership().is_ok());
    assert!(driver.fetch_chat_owner_after_leaving(ChatId(-100)).is_ok());
    assert!(driver.session.groups.ownership.check.is_loading());
    assert!(matches!(
        driver
            .session
            .groups
            .ownership
            .owner_after_leaving
            .get(&-100),
        Some(OwnerLookup::Loading(_))
    ));
    assert!(matches!(
        driver
            .session
            .groups
            .member_fetch((-100, MemberListFilter::Recent)),
        Some(SupergroupMembersFetch::Loading { .. })
    ));
}

/// 10-070: a welcome-message load that cannot be sent leaves the pack
/// `Failed` (Retry), not absent (drawn as "No welcome messages yet").
#[test]
fn a_welcome_load_that_cannot_be_sent_is_failed_not_empty() {
    let (_dir, mut driver, sender, sink, seq) = toggle_driver();
    feed_owned_group(&mut driver, &sink, &seq);
    sender.refuse(true);
    assert!(driver.load_chat_welcome_messages(ChatId(-100)).is_err());
    assert!(matches!(
        driver.session.groups.welcome_messages.get(&-100),
        Some(Fetch::Failed { .. })
    ));
    sender.refuse(false);
    assert!(driver.load_chat_welcome_messages(ChatId(-100)).is_ok());
    assert!(
        driver
            .session
            .groups
            .welcome_messages
            .get(&-100)
            .is_some_and(Fetch::is_loading)
    );
}

/// A chat that has no welcome messages (not a supergroup) gets no `Failed`
/// pack when the load cannot be sent: nothing would ever clear it.
#[test]
fn a_refused_welcome_load_for_a_plain_chat_stores_no_pack() {
    let (_dir, mut driver, sender, _sink, _seq) = toggle_driver();
    sender.refuse(true);
    assert!(driver.load_chat_welcome_messages(ChatId(77)).is_ok());
    assert!(!driver.session.groups.welcome_messages.contains_key(&77));
}

/// 10-022: the giveaway price lists whose send fails are `Failed` (the form
/// says why and offers Retry), not `Idle` ("not available").
#[test]
fn giveaway_options_that_cannot_be_sent_are_failed_with_a_retry() {
    let (_dir, mut driver, sender, _sink, _seq) = toggle_driver();
    sender.refuse(true);
    assert!(driver.request_giveaway_options(-100).is_err());
    let options = &driver.session.payments.giveaway.options;
    assert!(options.premium.error().is_some());
    assert!(options.stars.error().is_some());
    sender.refuse(false);
    assert!(driver.request_giveaway_options(-100).is_ok());
    let options = &driver.session.payments.giveaway.options;
    assert!(options.premium.is_loading() && options.stars.is_loading());
}

/// 05-034: a Premium price answer to an earlier request (same chat, before
/// a Retry) is not taken by the list waiting for the newer one.
#[test]
fn a_late_price_answer_to_an_older_request_is_not_taken() {
    let (_dir, mut driver, sender, sink, seq) = toggle_driver();
    assert!(driver.request_giveaway_options(-100).is_ok());
    let first = sender
        .snapshot()
        .into_iter()
        .rev()
        .find(|json| json.contains("getPremiumGiveawayPaymentOptions"))
        .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok())
        .and_then(|value| value.get("@extra").cloned())
        .expect("the first request");
    // The form asked again (Retry) for the same chat: only the newer request's
    // answer counts.
    assert!(driver.request_giveaway_options(-100).is_ok());
    let late = serde_json::json!({
        "@type": "premiumGiveawayPaymentOptions", "@extra": first, "options": []
    })
    .to_string();
    driver
        .ingest(copy_and_parse(&late, &seq, &sink).expect("parse"))
        .expect("ingest");
    let options = &driver.session.payments.giveaway.options;
    assert!(
        options.premium.is_loading(),
        "the older request's answer is dropped"
    );
}
