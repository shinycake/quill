//! Sweep is-05: the stages that became one value each leave through their
//! answer, their error, a cancel or their owner going away.
use super::common::*;
use super::*;
use crate::state::fetch::Fetch;

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
    let mut list: Fetch<Vec<i32>> = Fetch::Loading {
        stale: None,
        request: None,
    };
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

#[test]
fn closing_a_search_drops_the_query_waiting_out_its_quiet_window() {
    let (mut session, _sink) = session();
    let search = &mut session.search.search;
    search.open_field();
    search.begin_query("dun");
    let token = search.typed.arm("dun".into());
    search.close();
    assert!(search.typed.is_idle());
    assert_eq!(search.typed.take_if(token), None);

    let chat = &mut session.search.chat_search;
    chat.open_for(ChatId(4));
    chat.begin_query("dun");
    let token = chat.typed.arm("dun".into());
    chat.close();
    assert_eq!(chat.typed.take_if(token), None);
}

#[test]
fn a_new_query_makes_the_earlier_timer_stale() {
    let (mut session, _sink) = session();
    let search = &mut session.search.search;
    search.open_field();
    search.begin_query("du");
    let first = search.typed.arm("du".into());
    search.begin_query("dun");
    let second = search.typed.arm("dun".into());
    assert_eq!(search.typed.take_if(first), None);
    assert_eq!(search.typed.take_if(second), Some("dun".to_string()));
}
