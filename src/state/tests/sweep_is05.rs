//! Sweep is-05: the stages that became one value each leave through their
//! answer, their error, a cancel or their owner going away.
use super::common::*;
use super::*;
use crate::passkeys::{ListStage, Passkey, PasskeyRemove};
use crate::proxy::PingStatus;
use crate::state::fetch::{Change, Fetch};

fn passkey(id: &str) -> Passkey {
    Passkey {
        id: id.into(),
        name: id.into(),
        addition_date: 0,
        last_usage_date: 0,
        software_icon_custom_emoji_id: 0,
    }
}

#[test]
fn a_removal_is_finished_only_by_its_own_answer() {
    let (mut session, sink) = session();
    let seq = std::sync::atomic::AtomicU64::new(0);
    let passkeys = &mut session.settings.passkeys;
    passkeys.stage = ListStage::Asked {
        at: std::time::Instant::now(),
    };
    passkeys.accept_list(vec![passkey("a"), passkey("b")], 0);
    let remove = session.request(
        RequestPurpose::Settings(SettingsPurpose::Passkeys(
            crate::passkeys::PasskeysPurpose::Remove,
        )),
        None,
    );
    session.settings.passkeys.remove = PasskeyRemove::Removing {
        id: "a".into(),
        request: remove,
    };
    // Another passkey request's `ok` (a different id) does not finish it.
    let other = session.request(
        RequestPurpose::Settings(SettingsPurpose::Passkeys(
            crate::passkeys::PasskeysPurpose::Remove,
        )),
        None,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, other.0),
    );
    assert_eq!(session.settings.passkeys.removing(), Some("a"));
    assert_eq!(session.settings.passkeys.list.len(), 2);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, remove.0),
    );
    assert_eq!(session.settings.passkeys.removing(), None);
    assert_eq!(session.settings.passkeys.list.len(), 1);
    assert_eq!(session.settings.passkeys.stage, ListStage::Stale);
}

#[test]
fn a_late_ping_answer_does_not_fill_a_slot_that_asked_again() {
    let (mut session, sink) = session();
    let seq = std::sync::atomic::AtomicU64::new(0);
    let old = session.request(
        RequestPurpose::Settings(SettingsPurpose::PingProxy { proxy_id: 4 }),
        None,
    );
    let new = session.request(
        RequestPurpose::Settings(SettingsPurpose::PingProxy { proxy_id: 4 }),
        None,
    );
    session
        .settings
        .proxy
        .pings
        .insert(4, PingStatus::Checking(new));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"seconds","seconds":0.5,"@extra":"{}"}}"#,
            old.0
        ),
    );
    assert_eq!(
        session.settings.proxy.pings.get(&4),
        Some(&PingStatus::Checking(new))
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"seconds","seconds":0.5,"@extra":"{}"}}"#,
            new.0
        ),
    );
    assert_eq!(
        session.settings.proxy.pings.get(&4),
        Some(&PingStatus::Available(500))
    );
}

#[test]
fn the_next_channel_preload_leaves_loading_on_its_error() {
    let (mut session, sink) = session();
    let seq = std::sync::atomic::AtomicU64::new(0);
    let chat = ChatId(9);
    let request = session.request(RequestPurpose::PreloadHistory, Some(chat));
    session
        .chat_list
        .history_preloads
        .insert(chat.0, HistoryPreload::Loading(request));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"boom"}}"#,
            request.0
        ),
    );
    assert!(session.chat_list.history_preloads.is_empty());
    assert!(!session.chat_list.history_preloaded(chat.0));
}

#[test]
fn an_outdated_fetch_keeps_its_answer_and_a_change_clears_its_line() {
    let mut list = Fetch::Ready(vec![1, 2]);
    list.outdate();
    assert!(list.is_outdated());
    assert_eq!(list.items(), [1, 2]);
    let mut never: Fetch<Vec<i32>> = Fetch::Idle;
    never.outdate();
    assert!(never.is_outdated() && never.items().is_empty());
    list.begin();
    assert!(list.is_loading() && !list.is_outdated());
    list.succeed(vec![3]);
    assert_eq!(list.items(), [3]);

    let mut change = Change::Idle;
    change.begin();
    assert!(change.busy());
    change.fail("could not");
    assert_eq!(change.error(), Some("could not"));
    assert!(!change.busy());
    change.clear_error();
    assert_eq!(change, Change::Idle);
}

#[test]
fn a_sticker_batch_is_done_once_every_set_answered() {
    let mut batch = StickerBatch::begin(vec![1, 2]);
    batch.finish(9, true);
    assert_eq!(batch.pending(), [1, 2], "a set that is not pending");
    batch.finish(1, true);
    assert_eq!(batch.completed(), 1);
    batch.finish(2, false);
    assert_eq!(
        batch,
        StickerBatch::Done {
            total: 2,
            completed: 1,
            failed: 1,
        }
    );
    assert!(batch.pending().is_empty());
}

#[test]
fn a_wearable_gifts_failure_leaves_loading_and_a_retry_asks_again() {
    let (mut session, _sink) = session();
    let gifts = &mut session.users_state.wearable_gifts;
    gifts.begin();
    gifts.fail("boom");
    assert!(gifts.can_ask() && gifts.error().is_some());
    gifts.begin();
    assert!(gifts.is_loading());
}

#[test]
fn a_change_during_a_first_ask_outdates_it_and_drops_the_old_answer() {
    let mut list: Fetch<Vec<i32>> = Fetch::Loading { stale: None };
    list.outdate();
    assert!(list.is_outdated() && list.shows_loading());
    // The answer that was asked before the change does not land.
    assert!(!list.answer(vec![1]));
    assert!(list.is_outdated());
    assert!(!list.answer_failed("late"));
    assert!(list.is_outdated());
    list.begin();
    assert!(list.answer(vec![2]));
    assert_eq!(list.items(), [2]);
}

#[test]
fn a_change_during_the_first_proxy_fetch_drops_it_and_refetches() {
    let (mut session, sink) = session();
    let seq = std::sync::atomic::AtomicU64::new(0);
    let fetch = session.request(RequestPurpose::GetProxies, None);
    session.settings.proxy.list.begin(fetch, false);
    // A link-box add succeeds while the first `getProxies` is in flight.
    let add = session.request(RequestPurpose::MutateProxy, None);
    session.settings.proxy.mutation = crate::proxy::ProxyMutation::InFlight(add);
    let pending = session.requests.get(add).cloned();
    session.apply_added_proxy(pending.as_ref());
    assert!(session.settings.proxy.list.is_outdated());
    assert!(session.settings.proxy.list.shows_loading());
    assert_eq!(
        session.settings.proxy.mutation,
        crate::proxy::ProxyMutation::Idle
    );
    // The pre-change answer lands on nothing.
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"proxies","proxies":[],"@extra":"{}"}}"#,
            fetch.0
        ),
    );
    assert!(session.settings.proxy.list.is_outdated());
}

#[test]
fn an_active_sessions_answer_asked_before_a_terminate_is_dropped() {
    let (mut session, sink) = session();
    let seq = std::sync::atomic::AtomicU64::new(0);
    let fetch = session.request(RequestPurpose::GetActiveSessions, None);
    session.settings.sessions.begin();
    session.settings.sessions.outdate();
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"sessions","sessions":[],"inactive_session_ttl_days":180,"@extra":"{}"}}"#,
            fetch.0
        ),
    );
    assert!(session.settings.sessions.is_outdated());
}

#[test]
fn hiding_ads_keeps_a_report_in_flight() {
    use crate::state::{SponsoredReportFlow, SponsoredReportTarget};
    let (mut session, _sink) = session();
    session.messages.sponsored_report = SponsoredReportFlow::Asking(SponsoredReportTarget {
        chat_id: ChatId(3),
        message_id: 7,
        request: Some(RequestId(11)),
    });
    session.accept_sponsored_hidden(ChatId(3));
    assert!(matches!(
        session.messages.sponsored_report,
        SponsoredReportFlow::Asking(_)
    ));
    assert!(session.messages.sponsored_hidden);
}
