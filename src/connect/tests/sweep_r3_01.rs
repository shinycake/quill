//! Sweep fixes (r3-01): stages that record the request they wait for, and
//! refused sends that leave nothing behind.
use super::sweep_s3_payments_1::Flaky;
use crate::ids::{ChatId, MessageId};
use crate::state::{StyleError, StyleReply};
use serde_json::json;

fn style(name: &str) -> serde_json::Value {
    json!({"@type":"textCompositionStyle","name":name,"custom_emoji_id":"0","title":"Pirate",
        "is_custom":true,"is_creator":false,"install_count":0,"prompt":"","creator_user_id":0})
}

#[test]
fn a_late_links_answer_does_not_replace_a_list_a_failed_delete_dropped() {
    let mut t = Flaky::new();
    t.driver.fetch_folder_share(3).unwrap();
    let asked = t.driver.session.chat_list.folder_shares[&3].links.clone();
    assert!(matches!(
        asked,
        crate::state::fetch::Fetch::Loading {
            request: Some(_),
            ..
        }
    ));
    t.driver
        .delete_folder_invite_link(3, "https://t.me/addlist/a")
        .unwrap();
    t.fail("deleteChatFolderInviteLink", "INVITE_LINK_INVALID");
    assert!(
        t.driver.session.chat_list.folder_shares[&3]
            .links
            .error()
            .is_some()
    );
    // The older list answer finds a list that no longer waits for it.
    t.answer_nth(
        "getChatFolderInviteLinks",
        0,
        json!({"@type":"chatFolderInviteLinks","invite_links":[]}),
    );
    assert!(
        t.driver.session.chat_list.folder_shares[&3]
            .links
            .error()
            .is_some()
    );
}

#[test]
fn star_packs_and_wearable_gifts_wait_for_their_own_request() {
    let mut t = Flaky::new();
    let packs = t.driver.maybe_fetch_star_packs().unwrap().unwrap();
    assert!(
        t.driver
            .session
            .payments
            .star_packs
            .options
            .waits_for(packs)
    );
    let gifts = t.driver.load_wearable_gifts().unwrap().unwrap();
    assert!(t.driver.session.users_state.wearable_gifts.waits_for(gifts));
    // An answer under another request id lands nowhere.
    t.feed(json!({"@type":"starPaymentOptions","@extra":"9999","options":[]}));
    assert!(
        t.driver
            .session
            .payments
            .star_packs
            .options
            .waits_for(packs)
    );
    // Its own answer settles it.
    t.answer(
        "getStarPaymentOptions",
        json!({"@type":"starPaymentOptions","options":[]}),
    );
    assert!(!t.driver.session.payments.star_packs.options.is_loading());
    // A failure of the gifts list ends the stage that waits for it.
    t.fail("getReceivedGifts", "BAD");
    assert!(
        t.driver
            .session
            .users_state
            .wearable_gifts
            .error()
            .is_some()
    );
}

#[test]
fn a_style_lookup_is_settled_by_its_own_request_and_a_lost_one_fails_it() {
    let mut t = Flaky::new();
    t.driver.search_text_composition_style("pirate", 5).unwrap();
    let request = t.sender.last("searchTextCompositionStyle").unwrap()["@extra"].clone();
    let lookup = t
        .driver
        .session
        .messages
        .ai_compose
        .style_lookup
        .clone()
        .unwrap();
    let StyleReply::Running { request: waiting } = lookup.outcome else {
        panic!("the lookup runs");
    };
    assert_eq!(json!(waiting.0.to_string()), request);
    // An answer for another request does not settle it.
    let mut other = style("other");
    other["@extra"] = json!("9999");
    t.feed(other);
    assert!(matches!(
        t.driver
            .session
            .messages
            .ai_compose
            .style_lookup
            .as_ref()
            .map(|l| &l.outcome),
        Some(StyleReply::Running { .. })
    ));
    // The request is lost (swept after its TTL): the lookup fails.
    t.driver.session.connection = crate::telegram::ConnectionState::Ready;
    let later = std::time::Instant::now() + crate::state::PENDING_REQUEST_TTL * 2;
    assert!(t.driver.session.sweep_stale_requests(
        later,
        crate::state::PENDING_REQUEST_TTL,
        |_| false
    ));
    assert!(matches!(
        t.driver
            .session
            .messages
            .ai_compose
            .style_lookup
            .as_ref()
            .map(|l| &l.outcome),
        Some(StyleReply::Answered(Err(StyleError::Other(_))))
    ));
}

#[test]
fn a_refused_public_forwards_send_ends_in_an_error_with_retry() {
    let mut t = Flaky::new();
    t.sender.refuse(&["getMessagePublicForwards"]);
    t.driver
        .fetch_message_statistics(ChatId(-100_500), MessageId(5), false)
        .unwrap();
    let state = t
        .driver
        .session
        .groups
        .stats
        .message_for(-100_500, 5)
        .unwrap();
    assert!(state.forwards.error().is_some());
    assert!(!state.forwards.loading());
    // Retry asks again once the sender takes it.
    t.sender.refuse(&[]);
    assert!(
        t.driver
            .fetch_message_public_forwards(ChatId(-100_500), MessageId(5), "")
            .unwrap()
            .is_some()
    );
}

#[test]
fn a_refused_mini_app_custom_request_leaves_no_entry() {
    let mut t = Flaky::new();
    t.sender.refuse(&["sendWebAppCustomRequest"]);
    assert!(
        t.driver
            .send_web_app_custom_request(900, 1, "r1", "method", "{}")
            .is_err()
    );
    assert!(t.driver.session.bots.web_apps.custom_requests.is_empty());
    t.sender.refuse(&[]);
    let request = t
        .driver
        .send_web_app_custom_request(900, 1, "r2", "method", "{}")
        .unwrap();
    assert_eq!(
        t.driver
            .session
            .bots
            .web_apps
            .custom_requests
            .get(&request.0),
        Some(&(1, "r2".to_string()))
    );
}
