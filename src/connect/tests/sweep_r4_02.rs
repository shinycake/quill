//! Sweep fixes (r4-02): stages that hold the request they wait for, and a
//! refused send that leaves nothing waiting.
use super::sweep_s3_payments_1::Flaky;
use crate::ids::{ChatId, MessageId};
use crate::state::fetch::Fetch;
use serde_json::json;

fn link(url: &str) -> serde_json::Value {
    json!({"@type":"chatFolderInviteLink","invite_link":url,"name":"","chat_ids":[1]})
}

/// 03-004: Share Folder reopened while a link delete is still out does not
/// ask for the list (it could still hold the link); the list is asked once
/// the delete answered.
#[test]
fn reopening_share_folder_waits_for_a_delete_in_flight() {
    let mut t = Flaky::new();
    t.driver.fetch_folder_share(3).unwrap();
    t.answer(
        "getChatFolderInviteLinks",
        json!({"@type":"chatFolderInviteLinks","invite_links":[link("https://t.me/addlist/a")]}),
    );
    t.driver
        .delete_folder_invite_link(3, "https://t.me/addlist/a")
        .unwrap();
    assert_eq!(t.sender.count("getChatFolderInviteLinks"), 1);
    // The box is closed and opened again before the delete answers.
    t.driver.fetch_folder_share(3).unwrap();
    assert_eq!(
        t.sender.count("getChatFolderInviteLinks"),
        1,
        "no list is asked while the delete is out"
    );
    assert!(
        t.driver.session.chat_list.folder_shares[&3]
            .links
            .is_outdated()
    );
    t.answer("deleteChatFolderInviteLink", json!({"@type":"ok"}));
    t.driver.refetch_outdated_folder_links(3);
    assert_eq!(t.sender.count("getChatFolderInviteLinks"), 2);
}

/// 03-001: the add-to-folder list takes only its own request's failure.
#[test]
fn the_add_to_folder_list_records_its_request_and_ends_in_an_error() {
    let mut t = Flaky::new();
    t.sender.refuse(&["getChatListsToAddChat"]);
    assert!(t.driver.fetch_chat_lists_to_add_chat(ChatId(5)).is_err());
    assert!(
        t.driver.session.chat_list.chat_lists_for_add[&5]
            .error()
            .is_some()
    );
    t.sender.refuse(&[]);
    let asked = t
        .driver
        .fetch_chat_lists_to_add_chat(ChatId(5))
        .unwrap()
        .unwrap();
    assert!(t.driver.session.chat_list.chat_lists_for_add[&5].waits_for(asked));
    // A second ask while it is out sends nothing.
    assert_eq!(
        t.driver.fetch_chat_lists_to_add_chat(ChatId(5)).unwrap(),
        None
    );
    assert_eq!(
        t.sender.count("getChatListsToAddChat"),
        1,
        "the refused one never left"
    );
}

/// Sign-in submits: the one this driver sent is busy until its request
/// answers or fails, and refuses a second.
#[test]
fn an_auth_submit_is_busy_until_its_own_request_answers() {
    let mut t = Flaky::new();
    t.feed(json!({"@type":"updateAuthorizationState",
        "authorization_state":{"@type":"authorizationStateWaitPhoneNumber"}}));
    t.driver.submit_phone("+10000000000").unwrap();
    assert!(
        t.driver.submit_phone("+10000000000").is_err(),
        "one at a time"
    );
    assert!(t.driver.request_qr_login().is_err(), "not beside another");
    t.fail("setAuthenticationPhoneNumber", "PHONE_NUMBER_INVALID");
    assert!(t.driver.submit_phone("+10000000000").is_ok());
}

/// 03-008: a refused `getPollOptionProperties` ends the option menu in the
/// error, not a request-less Loading.
#[test]
fn a_refused_poll_option_menu_request_ends_in_an_error() {
    let mut t = Flaky::new();
    t.sender.refuse(&["getPollOptionProperties"]);
    assert!(
        t.driver
            .fetch_poll_option_properties(ChatId(5), MessageId(9), "opt")
            .is_err()
    );
    let menu = t.driver.session.messages.poll_option_menu.as_ref().unwrap();
    assert!(menu.properties.error().is_some());
    t.sender.refuse(&[]);
    let asked = t
        .driver
        .fetch_poll_option_properties(ChatId(5), MessageId(9), "opt")
        .unwrap();
    let menu = t.driver.session.messages.poll_option_menu.as_ref().unwrap();
    assert!(menu.properties.waits_for(asked));
    assert!(matches!(menu.properties, Fetch::Loading { .. }));
}

/// The `getCountries` list is matched to its request.
#[test]
fn the_country_list_takes_only_its_own_request() {
    let mut t = Flaky::new();
    let asked = t.driver.fetch_countries().unwrap().unwrap();
    assert!(t.driver.session.auth_state.countries.waits_for(asked));
    t.fail("getCountries", "TIMEOUT");
    assert!(t.driver.session.auth_state.countries.error().is_some());
    assert!(t.driver.retry_countries().unwrap().is_some());
}

/// The self Stars statistics probe records its request.
#[test]
fn the_star_stats_probe_waits_for_its_own_request() {
    let mut t = Flaky::new();
    t.driver.probe_self_star_stats().unwrap();
    let request = t.sender.last("getStarRevenueStatistics").unwrap()["@extra"].clone();
    let stats = &t.driver.session.payments.earn.self_stats;
    assert!(
        matches!(stats, Fetch::Loading { request: Some(held), .. } if held.as_extra() == request.as_str().unwrap_or_default())
    );
    // A second probe while it is out sends nothing.
    t.driver.probe_self_star_stats().unwrap();
    assert_eq!(t.sender.count("getStarRevenueStatistics"), 1);
}
