//! Sweep r3-04: answers settle only the request that asked.
use super::common::*;
use crate::state::fetch::Fetch;
use super::*;

#[test]
fn the_accounts_own_full_info_turns_hidden_ads_back_on() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.my_user_id = Some(5);
    session.accept_sponsored_hidden(ChatId(3));
    assert!(session.messages.sponsored_hidden);
    // Another user's full info says nothing about this account's ads.
    let other = session.request_for_user(RequestPurpose::GetUserFullInfo, 31);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","bio":null,"bot_info":null,"has_sponsored_messages_enabled":true}}"#,
            other.0
        ),
    );
    assert!(session.messages.sponsored_hidden);
    let own = session.request_for_user(RequestPurpose::GetUserFullInfo, 5);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","bio":null,"bot_info":null,"has_sponsored_messages_enabled":true}}"#,
            own.0
        ),
    );
    assert!(!session.messages.sponsored_hidden);
}

#[test]
fn a_device_link_answer_after_its_screen_went_away_lands_nowhere() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let id = session.request(RequestPurpose::ConfirmDeviceLogin, None);
    session.settings.device_login = DeviceLogin::Waiting { request: id };
    session.settings.sessions_op.begin(id);
    session.settings.device_login.discard();
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"session","@extra":"{}","id":11}}"#, id.0),
    );
    assert_eq!(session.settings.device_login, DeviceLogin::Idle);
    assert!(!session.settings.sessions_op.is_mutating());

    let id = session.request(RequestPurpose::ConfirmDeviceLogin, None);
    session.settings.device_login = DeviceLogin::Waiting { request: id };
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"LOGIN_TOKEN_EXPIRED"}}"#,
            id.0
        ),
    );
    assert_eq!(
        session.settings.device_login.take_result_for(id),
        Some(crate::auth::DeviceLoginResult::Failed)
    );
}

#[test]
fn a_late_storage_clear_answer_does_not_settle_another_clear() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let first = session.request(RequestPurpose::OptimizeStorage, None);
    session.settings.storage_clear = StorageClear::Clearing { request: first };
    // The page was reopened and the clear failed; a second clear starts.
    session.settings.storage_clear = StorageClear::Failed("x".into());
    let second = session.request(RequestPurpose::OptimizeStorage, None);
    session.settings.storage_clear = StorageClear::Clearing { request: second };
    session.settings.storage_stats = Fetch::Ready(StorageStats {
        total_size: 1,
        by_file_type: Vec::new(),
        by_chat: Vec::new(),
    });
    let stats = |extra: RequestId| {
        format!(
            r#"{{"@type":"storageStatistics","@extra":"{}","size":"4096","count":1,"by_chat":[]}}"#,
            extra.0
        )
    };
    apply_json(&mut session, &seq, &sink, &stats(first));
    assert!(session.settings.storage_clear.is_clearing());
    assert!(session.settings.storage_stats.data().is_some());
    apply_json(&mut session, &seq, &sink, &stats(second));
    assert_eq!(session.settings.storage_clear.freed(), Some(4096));
    assert!(session.settings.storage_stats.data().is_none());
}

#[test]
fn a_failed_gift_request_closes_only_its_own_sale_confirmation() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let error = |extra: RequestId| {
        format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"NOPE"}}"#,
            extra.0
        )
    };
    let view = |session: &mut Session| {
        session.payments.hub.received.view.select(Some("g1".into()));
        session
            .payments
            .hub
            .received
            .view
            .confirm_convert(Some("g1".into()));
    };
    session.payments.hub.received.open();
    view(&mut session);
    // A failed save toggle, and the failed sale of another gift.
    let toggle = session.request(
        RequestPurpose::Payments(PaymentsPurpose::ToggleGiftSaved { saved: true }),
        None,
    );
    apply_json(&mut session, &seq, &sink, &error(toggle));
    assert_eq!(session.payments.hub.received.view.confirming(), Some("g1"));
    let other = session.request_for_subject(RequestPurpose::SellGift, "g2");
    apply_json(&mut session, &seq, &sink, &error(other));
    assert_eq!(session.payments.hub.received.view.confirming(), Some("g1"));
    let own = session.request_for_subject(RequestPurpose::SellGift, "g1");
    apply_json(&mut session, &seq, &sink, &error(own));
    assert_eq!(session.payments.hub.received.view.confirming(), None);
}

#[test]
fn an_older_star_pack_failure_leaves_the_newer_options() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let old = session.request(RequestPurpose::GetStarPackOptions, None);
    let new = session.request(RequestPurpose::GetStarPackOptions, None);
    session.payments.star_packs.options.begin();
    session.payments.star_packs.options.sent(new);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"x"}}"#,
            old.0
        ),
    );
    assert!(session.payments.star_packs.options.waits_for(new));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"starPaymentOptions","@extra":"{}","options":[]}}"#,
            new.0
        ),
    );
    assert!(session.payments.star_packs.options.data().is_some());
}

#[test]
fn a_gift_text_limit_failure_keeps_the_limit_an_update_set() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let asked = session.request(RequestPurpose::GetGiftTextLimit, None);
    session.payments.gift_text_length_max = Some(255);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"x"}}"#,
            asked.0
        ),
    );
    assert_eq!(session.payments.gift_text_length_max, Some(255));
}

#[test]
fn an_older_sticker_list_answer_does_not_replace_the_newer_one() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let sticker = |id: i64| {
        format!(
            r#"{{"@type":"sticker","id":"{id}","set_id":"77","width":512,"height":512,"emoji":"😀","format":{{"@type":"stickerFormatWebp"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":null,"sticker":null}}"#
        )
    };
    let answer = |extra: RequestId, id: i64| {
        format!(
            r#"{{"@type":"stickers","stickers":[{}],"@extra":"{}"}}"#,
            sticker(id),
            extra.0
        )
    };
    let old = session.request(RequestPurpose::GetFavoriteStickers, None);
    let new = session.request(RequestPurpose::GetFavoriteStickers, None);
    let list = &mut session.stickers.stickers.lists.favorites;
    list.begin();
    list.sent(new);
    apply_json(&mut session, &seq, &sink, &answer(old, 1));
    assert!(session.stickers.stickers.favorites.is_empty());
    assert!(session.stickers.stickers.lists.favorites.is_loading());
    apply_json(&mut session, &seq, &sink, &answer(new, 2));
    assert_eq!(session.stickers.stickers.favorites.len(), 1);
    // The newer answer was applied first; the older one lands late.
    apply_json(&mut session, &seq, &sink, &answer(old, 1));
    assert_eq!(session.stickers.stickers.favorites.len(), 1);
    assert_eq!(session.stickers.stickers.favorites[0].emoji, "😀");
}

#[test]
fn only_the_story_page_in_flight_settles_its_list() {
    use crate::state::ActiveStoriesPaging;
    use std::time::Instant;
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let old = session.request(RequestPurpose::LoadActiveStories, None);
    session.stories.main_paging.begin(old);
    // The list was paged from the start again; the old page's 404 lands late.
    session.stories.reset_active_stories_paging();
    let new = session.request(RequestPurpose::LoadActiveStories, None);
    session.stories.main_paging.begin(new);
    let error = |extra: RequestId, code: i32| {
        format!(
            r#"{{"@type":"error","@extra":"{}","code":{code},"message":"x"}}"#,
            extra.0
        )
    };
    apply_json(&mut session, &seq, &sink, &error(old, 404));
    assert_eq!(
        session.stories.main_paging,
        ActiveStoriesPaging::Loading {
            request: new,
            wait: None
        }
    );
    assert!(!session.stories.main_paging.may_load(Instant::now()));
    apply_json(&mut session, &seq, &sink, &error(new, 404));
    assert_eq!(session.stories.main_paging, ActiveStoriesPaging::Exhausted);
}

#[test]
fn a_lost_story_page_waits_and_pages_again() {
    use crate::state::ActiveStoriesPaging;
    let (mut session, _sink) = session();
    let page = session.request(RequestPurpose::LoadHiddenStories, None);
    session.stories.hidden_paging.begin(page);
    let lost = session.requests.take(page).expect("pending");
    session.settle_lost_request(lost);
    assert!(matches!(
        session.stories.hidden_paging,
        ActiveStoriesPaging::Waiting { .. }
    ));
}

#[test]
fn another_windows_answers_never_push_out_the_waiting_one() {
    let (mut session, _sink) = session();
    assert!(session.open_window_chat(ChatId(5)));
    let waiting = AskingView::Window {
        chat: ChatId(5),
        opening: session.window_chats.opening_of(ChatId(5)),
    };
    session.topic_action_answered(RequestId(1), waiting, true);
    // Far more answers of the main window than any cap, none taken.
    for id in 100..140 {
        session.topic_action_answered(RequestId(id), AskingView::Main, true);
    }
    assert_eq!(session.take_topic_action_answer(RequestId(1)), Some(true));
    // The main window keeps its newest answers, not all of them.
    assert_eq!(session.take_topic_action_answer(RequestId(139)), Some(true));
    assert_eq!(session.take_topic_action_answer(RequestId(100)), None);
}

#[test]
fn the_answers_of_a_closed_window_go_with_it() {
    let (mut session, _sink) = session();
    assert!(session.open_window_chat(ChatId(5)));
    let window = AskingView::Window {
        chat: ChatId(5),
        opening: session.window_chats.opening_of(ChatId(5)),
    };
    session.bot_thread_answered(RequestId(1), window, Ok(12));
    assert!(session.close_window_chat(ChatId(5)));
    session.bot_thread_answered(RequestId(2), AskingView::Main, Ok(13));
    assert_eq!(session.take_bot_thread_start(RequestId(1)), None);
    assert_eq!(session.take_bot_thread_start(RequestId(2)), Some(Ok(13)));
    assert!(session.threads.bot_thread_starts.is_empty());
}

#[test]
fn a_late_forum_topics_error_does_not_fail_a_loaded_list() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let chat = ChatId(16);
    let old = session.request(RequestPurpose::GetForumTopics, Some(chat));
    let new = session.ask_forum_topics(chat);
    let answer = format!(
        r#"{{"@type":"forumTopics","@extra":"{}","total_count":0,"topics":[],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}}"#,
        new.0
    );
    apply_json(&mut session, &seq, &sink, &answer);
    assert!(session.threads.forum_topics.contains_key(&16));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"x"}}"#,
            old.0
        ),
    );
    assert!(!session.threads.forum_topics.failed(16));
    assert!(session.threads.forum_topics.contains_key(&16));
}

#[test]
fn a_forum_topics_answer_nobody_waits_for_is_dropped() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let chat = ChatId(16);
    // The list was forgotten (a topic was created) while the ask was out.
    let ask = session.ask_forum_topics(chat);
    session.threads.forum_topics.remove(&16);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"forumTopics","@extra":"{}","total_count":0,"topics":[],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}}"#,
            ask.0
        ),
    );
    assert!(!session.threads.forum_topics.asked(16));
}

#[test]
fn only_the_direct_messages_page_in_flight_ends_the_list() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let chat = ChatId(9);
    let old = session.request(RequestPurpose::LoadDirectMessagesChatTopics, Some(chat));
    session.threads.direct.page_asked(9, old);
    let ok = |extra: RequestId| format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0);
    apply_json(&mut session, &seq, &sink, &ok(old));
    assert!(!session.threads.direct.loading(9));
    let new = session.request(RequestPurpose::LoadDirectMessagesChatTopics, Some(chat));
    session.threads.direct.page_asked(9, new);
    // The older page's 404 arrives after the newer page went out.
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":404,"message":"x"}}"#,
            old.0
        ),
    );
    assert!(!session.threads.direct.exhausted(9));
    assert!(session.threads.direct.loading(9));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":404,"message":"x"}}"#,
            new.0
        ),
    );
    assert!(session.threads.direct.exhausted(9));
}
