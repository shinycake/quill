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
    assert!(matches!(asked, crate::state::fetch::Fetch::Loading { .. }));
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

fn link(url: &str) -> serde_json::Value {
    json!({"@type":"chatFolderInviteLink","invite_link":url,"name":"","chat_ids":[1]})
}

fn link_urls(t: &Flaky) -> Vec<String> {
    t.driver.session.chat_list.folder_shares[&3]
        .links
        .data()
        .map(|links| links.iter().map(|l| l.invite_link.clone()).collect())
        .unwrap_or_default()
}

#[test]
fn a_late_links_answer_does_not_bring_back_a_link_deleted_meanwhile() {
    let mut t = Flaky::new();
    t.driver.fetch_folder_share(3).unwrap();
    t.driver
        .delete_folder_invite_link(3, "https://t.me/addlist/a")
        .unwrap();
    // The list waits for the delete's answer: a parallel query could be
    // read before the delete lands.
    t.driver.refetch_outdated_folder_links(3);
    assert_eq!(t.sender.count("getChatFolderInviteLinks"), 1);
    t.answer_nth(
        "getChatFolderInviteLinks",
        0,
        json!({"@type":"chatFolderInviteLinks",
            "invite_links":[link("https://t.me/addlist/a"), link("https://t.me/addlist/b")]}),
    );
    assert!(
        !t.driver.session.chat_list.folder_shares[&3]
            .links
            .is_loading()
    );
    t.answer("deleteChatFolderInviteLink", json!({"@type":"ok"}));
    t.driver.refetch_outdated_folder_links(3);
    assert_eq!(t.sender.count("getChatFolderInviteLinks"), 2);
    t.answer_nth(
        "getChatFolderInviteLinks",
        1,
        json!({"@type":"chatFolderInviteLinks","invite_links":[link("https://t.me/addlist/b")]}),
    );
    assert_eq!(link_urls(&t), ["https://t.me/addlist/b"]);
}

#[test]
fn a_late_links_answer_does_not_drop_a_link_created_meanwhile() {
    let mut t = Flaky::new();
    t.driver.fetch_folder_share(3).unwrap();
    t.driver.create_folder_invite_link(3, "", &[1]).unwrap();
    t.answer("createChatFolderInviteLink", link("https://t.me/addlist/n"));
    t.driver.refetch_outdated_folder_links(3);
    assert_eq!(t.sender.count("getChatFolderInviteLinks"), 2);
    t.answer_nth(
        "getChatFolderInviteLinks",
        0,
        json!({"@type":"chatFolderInviteLinks","invite_links":[]}),
    );
    assert_eq!(link_urls(&t), ["https://t.me/addlist/n"]);
    t.answer_nth(
        "getChatFolderInviteLinks",
        1,
        json!({"@type":"chatFolderInviteLinks","invite_links":[link("https://t.me/addlist/n")]}),
    );
    assert_eq!(link_urls(&t), ["https://t.me/addlist/n"]);
    assert!(
        !t.driver.session.chat_list.folder_shares[&3]
            .links
            .is_loading()
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
    t.driver.search_text_composition_style("pirate").unwrap();
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
        Some(StyleReply::Answered {
            outcome: Err(StyleError::Other(_)),
            ..
        })
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

#[test]
fn an_older_installed_sets_answer_does_not_replace_a_newer_load() {
    let mut t = Flaky::new();
    let sets = |id: &str| {
        json!({"@type":"stickerSets","sets":[{"@type":"stickerSetInfo","id":id,"title":"T",
            "name":"T","size":0,"is_installed":true}]})
    };
    t.driver.open_sticker_panel().unwrap();
    // A change drops the load that was out and asks again.
    let first = t.sender.nth("getInstalledStickerSets", 0).unwrap()["@extra"].clone();
    drop(
        t.driver
            .session
            .requests
            .take_purpose(crate::state::RequestPurpose::GetInstalledStickerSets),
    );
    t.driver.session.stickers.stickers.lists.installed.outdate();
    t.driver.refresh_installed_sticker_sets().unwrap();
    let asked = t.sender.count("getInstalledStickerSets");
    assert!(asked >= 2);
    // The older answer finds a load that no longer waits for it.
    let mut older = sets("11");
    older["@extra"] = first;
    t.feed(older);
    assert!(t.driver.session.stickers.stickers.sets.is_empty());
    t.answer("getInstalledStickerSets", sets("22"));
    let ids: Vec<_> = t
        .driver
        .session
        .stickers
        .stickers
        .sets
        .iter()
        .map(|set| set.id)
        .collect();
    assert_eq!(ids, [22]);
}

#[test]
fn a_failed_paid_reaction_senders_ask_may_be_asked_again() {
    let mut t = Flaky::new();
    t.driver
        .maybe_fetch_paid_reaction_senders(ChatId(5))
        .unwrap();
    assert!(!t.driver.session.payments.paid.can_ask_senders(5));
    t.fail("getChatAvailablePaidMessageReactionSenders", "BAD");
    assert!(t.driver.session.payments.paid.can_ask_senders(5));
}
