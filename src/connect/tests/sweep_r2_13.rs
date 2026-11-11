//! Sweep r2-13: refused sends settle their own part (and not the others), a
//! refused Join reaches the caller, and answers act only for the request
//! that asked.
use super::sweep_s3_payments_1::Flaky;
use crate::connect::JsonSender;
use crate::earn::*;
use serde_json::json;

const CHANNEL: i64 = -100_500;

fn error() -> serde_json::Value {
    json!({"@type": "error", "code": 400, "message": "CHAT_ADMIN_REQUIRED"})
}

/// 02-042: the connected-programs send is refused; "All programs" is still
/// asked, and the failed list shows its Retry.
#[test]
fn a_refused_connected_list_does_not_strand_the_found_list() {
    let mut t = Flaky::new();
    t.sender.refuse(&["getConnectedAffiliatePrograms"]);
    assert!(t.driver.open_starref_join(EarnPeer::Chat(CHANNEL)).is_err());
    assert_eq!(t.sender.count("searchAffiliatePrograms"), 1);
    let join = t.driver.session.payments.earn.join.as_ref().unwrap();
    assert!(
        matches!(join.mine.load, PageLoad::Failed { .. }),
        "Retry shows for My Programs"
    );
    assert!(join.found.load.is_loading());
}

/// 02-043: a refused Join send is the caller's failure, and the box stays
/// open to try again.
#[test]
fn a_refused_join_reports_the_failure() {
    let mut t = Flaky::new();
    t.driver.open_starref_join(EarnPeer::Chat(CHANNEL)).unwrap();
    t.answer(
        "searchAffiliatePrograms",
        json!({"@type": "foundAffiliatePrograms", "total_count": 1, "programs": [
            {"@type": "foundAffiliateProgram", "bot_user_id": 900, "info": {
                "@type": "affiliateProgramInfo",
                "parameters": {"@type": "affiliateProgramParameters",
                    "commission_per_mille": 150, "month_count": 6},
                "end_date": 0,
                "daily_revenue_per_user_amount": {"@type": "starAmount", "star_count": 2, "nanostar_count": 0}}}],
            "next_offset": ""}),
    );
    t.answer(
        "getConnectedAffiliatePrograms",
        json!({"@type": "connectedAffiliatePrograms", "total_count": 0, "programs": [], "next_offset": ""}),
    );
    t.driver.open_affiliate_program(900, false).unwrap();
    t.answer(
        "getConnectedAffiliateProgram",
        json!({"@type": "error", "code": 404, "message": "NOT_FOUND"}),
    );
    t.sender.refuse(&["connectAffiliateProgram"]);
    assert!(t.driver.join_affiliate_program().is_err());
    let join = t.driver.session.payments.earn.join_box.as_ref().unwrap();
    assert_eq!(join.stage, JoinStage::Open);
}

/// 02-044: the error of the first open of a page, answered after the page
/// was opened again, neither fails nor toasts the second one.
#[test]
fn a_late_stats_error_of_a_replaced_page_is_ignored() {
    let mut t = Flaky::new();
    let peer = EarnPeer::Chat(CHANNEL);
    t.driver
        .open_earn_page(peer, EarnScreen::Monetization, false)
        .unwrap();
    t.driver
        .open_earn_page(peer, EarnScreen::Monetization, false)
        .unwrap();
    t.answer_nth("getChatRevenueStatistics", 0, error());
    t.answer_nth("getStarRevenueStatistics", 0, error());
    let earn = &t.driver.session.payments.earn;
    let page = earn.page.as_ref().unwrap();
    assert!(page.ton.is_loading(), "the first open's error is not ours");
    assert!(page.stars.is_loading());
    assert!(earn.notes.is_empty());
    t.answer_nth("getChatRevenueStatistics", 1, error());
    let page = t.driver.session.payments.earn.page.as_ref().unwrap();
    assert!(matches!(page.ton, Load::Failed(_)));
}

/// 02-054: a trending page that fails keeps the offset it asked at, so
/// "load more" asks that page again; a stale answer is not applied.
#[test]
fn a_failed_trending_page_is_asked_again_at_its_own_offset() {
    use crate::emoji::{EmojiSetTab, TrendingPaging};
    let mut t = Flaky::new();
    t.driver.open_emoji_sets().unwrap();
    t.driver
        .select_emoji_set_tab(EmojiSetTab::Trending)
        .unwrap();
    let set = |id: i64| {
        json!({"@type": "stickerSetInfo", "id": id.to_string(), "title": "S", "name": "s",
            "is_installed": false, "is_archived": false, "is_official": false,
            "sticker_type": {"@type": "stickerTypeCustomEmoji"}, "size": 1, "covers": []})
    };
    t.answer(
        "getTrendingStickerSets",
        json!({"@type": "trendingStickerSets", "total_count": 4,
            "sets": [set(1), set(2)], "is_premium": false}),
    );
    assert_eq!(
        t.driver.session.stickers.emoji.trending,
        TrendingPaging::More { next: 2 }
    );
    t.sender.refuse(&["getTrendingStickerSets"]);
    assert!(t.driver.more_trending_emoji_packs().is_err());
    assert_eq!(
        t.driver.session.stickers.emoji.trending.next_page(),
        Some(2)
    );
    assert!(t.driver.session.stickers.emoji.trending.error().is_some());
    t.sender.refuse(&[]);
    t.driver.more_trending_emoji_packs().unwrap();
    assert_eq!(
        t.sender.last("getTrendingStickerSets").unwrap()["offset"],
        2,
        "the failed page, not the first"
    );
}

/// 02-052: the invite-links send is refused; the chats half is still asked,
/// and the links half shows its error.
#[test]
fn a_refused_folder_links_send_does_not_strand_the_chats_half() {
    let mut t = Flaky::new();
    t.sender.refuse(&["getChatFolderInviteLinks"]);
    assert!(t.driver.fetch_folder_share(5).is_err());
    assert_eq!(t.sender.count("getChatsForChatFolderInviteLink"), 1);
    let share = t.driver.session.chat_list.folder_shares.get(&5).unwrap();
    assert!(share.links.error().is_some());
    assert!(share.chats.is_loading());
}

/// 02-051 is not a bug: a refused `getForumTopic` after "mark all mentions
/// as read" costs nothing, because the read's own `ok` clears the badge
/// (`clear_topic_marks`), so the refetch only double-checks.
#[test]
fn a_refused_topic_refetch_leaves_the_mention_badge_cleared_by_the_read_ok() {
    let mut t = Flaky::new();
    use crate::telegram::envelope::{ChatNotificationSettings, ForumTopic};
    let topic = ForumTopic {
        forum_topic_id: 4,
        name: "Topic".into(),
        is_general: false,
        is_closed: false,
        is_pinned: false,
        is_hidden: false,
        unread_count: 3,
        order: 5,
        last_message_preview: String::new(),
        icon_color: 0,
        icon_custom_emoji_id: 0,
        last_message_id: 0,
        last_read_inbox_message_id: 0,
        notification_settings: ChatNotificationSettings::default(),
        unread_mention_count: 3,
        unread_reaction_count: 0,
        creation_date: 0,
        creator: None,
        is_outgoing: false,
        draft: None,
    };
    t.driver
        .session
        .threads
        .forum_topics
        .insert(-100_777, vec![topic]);
    let count = |t: &Flaky| {
        t.driver
            .session
            .threads
            .forum_topics
            .get(&-100_777)
            .and_then(|topics| topics.iter().find(|topic| topic.forum_topic_id == 4))
            .map(|topic| topic.unread_mention_count)
    };
    assert_eq!(count(&t), Some(3));
    t.sender.refuse(&["getForumTopic"]);
    t.driver
        .read_all_forum_topic_mentions(crate::ids::ChatId(-100_777), 4)
        .unwrap();
    t.answer("readAllForumTopicMentions", json!({"@type": "ok"}));
    assert_eq!(count(&t), Some(0));
}

/// 02-064: the "your gifts" send is refused; the "on sale" list is still
/// asked, and the refused list shows its error.
#[test]
fn a_refused_craft_candidates_send_does_not_strand_the_market_list() {
    use super::gift_market::received;
    let mut t = Flaky::new();
    t.sender.refuse(&["getGiftsForCrafting"]);
    let primary = crate::gift_market::CraftGift::parse_received(&received(1)).unwrap();
    t.driver.open_gift_craft(primary).unwrap();
    assert_eq!(t.sender.count("searchGiftsForResale"), 1);
    let craft = t
        .driver
        .session
        .payments
        .gift_market
        .craft
        .as_ref()
        .unwrap();
    assert!(craft.candidates_load.error().is_some());
    assert!(craft.market.load.error().is_none());
}

/// 02-065: a close that could not be sent keeps the follow, so the next
/// call closes it before the next auction opens.
#[test]
fn a_refused_auction_close_keeps_following_until_it_goes_out() {
    use crate::gift_market::AuctionFollow;
    let mut t = Flaky::new();
    t.driver.follow_gift_auction(Some(5)).unwrap();
    t.sender.refuse(&["closeGiftAuction"]);
    assert!(t.driver.follow_gift_auction(Some(6)).is_err());
    let follow = t.driver.session.payments.gift_market.follow;
    assert!(follow.is_following(5), "{follow:?}");
    assert_eq!(t.sender.count("openGiftAuction"), 1, "6 is not opened yet");
    t.sender.refuse(&[]);
    t.driver.follow_gift_auction(Some(6)).unwrap();
    assert_eq!(t.sender.count("closeGiftAuction"), 1);
    assert!(matches!(
        t.driver.session.payments.gift_market.follow,
        AuctionFollow::Following { gift_id: 6, .. }
    ));
}

/// 02-062: a refused prices send is a failure the form draws (and retries),
/// not an empty section.
#[test]
fn a_refused_giveaway_prices_send_fails_the_list() {
    let mut t = Flaky::new();
    t.sender.refuse(&[
        "getPremiumGiveawayPaymentOptions",
        "getStarGiveawayPaymentOptions",
    ]);
    assert!(t.driver.request_giveaway_options(-100).is_err());
    let options = &t.driver.session.payments.giveaway.options;
    assert!(options.premium.error().is_some());
    assert!(options.stars.error().is_some());
    t.sender.refuse(&[]);
    t.driver.request_giveaway_options(-100).unwrap();
    assert!(t.driver.session.payments.giveaway.options.loading());
}

/// 02-080: the pack option went out and a later send of the switch is
/// refused: TDLib is pointed back at the pack the interface shows.
#[test]
fn a_refused_language_switch_points_tdlib_back_at_the_shown_pack() {
    let mut t = Flaky::new();
    t.sender.refuse(&["getLanguagePackStrings"]);
    let info = crate::telegram::envelope::LanguagePackInfoData {
        id: "de".into(),
        base_language_pack_id: String::new(),
        name: "German".into(),
        native_name: "Deutsch".into(),
        plural_code: "de".into(),
        is_official: true,
        is_rtl: false,
        is_beta: false,
        is_installed: true,
        total_string_count: 0,
        translated_string_count: 0,
    };
    assert!(t.driver.switch_language(info).is_err());
    let options: Vec<_> = t
        .sender
        .all("setOption")
        .into_iter()
        .filter(|v| v["name"] == "language_pack_id")
        .map(|v| v["value"]["value"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(
        options,
        ["de", "en"],
        "the new pack, then back to the shown one"
    );
}

/// 02-073: a gift code check that could not be sent keeps the box showing
/// the code it had.
#[test]
fn a_refused_gift_code_check_keeps_the_box() {
    let mut t = Flaky::new();
    t.driver.request_gift_code_info("A").unwrap();
    t.sender.refuse(&["checkPremiumGiftCode"]);
    assert!(t.driver.request_gift_code_info("B").is_err());
    let lookup = t.driver.session.payments.gift_code.as_ref().unwrap();
    assert_eq!(lookup.code, "A");
}

fn sticker_set_json(extra: &serde_json::Value, ids: &[i32]) -> serde_json::Value {
    let stickers: Vec<serde_json::Value> = ids
        .iter()
        .map(|id| {
            json!({"@type":"sticker","id":id,"set_id":"77","emoji":"x","width":512,"height":512,
                "format":{"@type":"stickerFormatWebp"},
                "full_type":{"@type":"stickerFullTypeRegular"},
                "sticker":{"@type":"file","id":id,"local":{"@type":"localFile","can_be_downloaded":true},
                    "remote":{"@type":"remoteFile"}}})
        })
        .collect();
    json!({"@type":"stickerSet","@extra":extra,"id":"77","title":"Mine","name":"mine",
        "is_installed":true,"is_owned":false,"sticker_type":{"@type":"stickerTypeRegular"},
        "stickers":stickers,"emojis":[]})
}

/// 02-083 / 13-028: the set view's previews are marked asked only once the
/// downloads left; a refused send is asked again.
#[test]
fn a_refused_preview_download_is_asked_again_for_the_set_view() {
    use crate::state::{SetFiles, StickerSetViewStage};
    let mut t = Flaky::new();
    let extra = t.driver.view_sticker_set(77).unwrap();
    t.feed(sticker_set_json(&extra.as_extra().into(), &[1, 2]));
    let files = |t: &Flaky| match &t
        .driver
        .session
        .stickers
        .sticker_set_view
        .as_ref()
        .unwrap()
        .stage
    {
        StickerSetViewStage::Ready { files, .. } => *files,
        other => panic!("not ready: {other:?}"),
    };
    t.sender.refuse(&["downloadFile"]);
    assert!(t.driver.ensure_sticker_set_view_files().is_err());
    assert_eq!(files(&t), SetFiles::Unasked);
    t.sender.refuse(&[]);
    t.driver.ensure_sticker_set_view_files().unwrap();
    assert_eq!(files(&t), SetFiles::Asked);
    assert!(t.sender.count("downloadFile") >= 2);
}

/// 02-084: opening the menu twice asks the emoji pack's title once.
#[test]
fn the_emoji_pack_title_is_asked_once_while_it_is_pending() {
    let mut t = Flaky::new();
    t.driver.fetch_emoji_pack_title(88).unwrap();
    t.driver.fetch_emoji_pack_title(88).unwrap();
    assert_eq!(t.sender.count("getStickerSet"), 1);
}

/// 02-086: a refused viewers send fails that list and the author list is
/// still asked.
#[test]
fn a_refused_audience_list_does_not_strand_the_others() {
    use crate::ids::{ChatId, MessageId};
    use crate::state::Audience;
    use crate::telegram::envelope::MessageActions;
    let mut t = Flaky::new();
    t.sender.refuse(&["getMessageViewers"]);
    let actions = MessageActions {
        can_get_viewers: true,
        can_get_author: true,
        ..MessageActions::default()
    };
    assert!(
        t.driver
            .fetch_message_audience(ChatId(-1_009), MessageId(5), actions)
            .is_err()
    );
    let audience = t.driver.session.messages.message_audience.as_ref().unwrap();
    assert_eq!(audience.viewers, Audience::Failed);
    assert_eq!(audience.author, Audience::Loading);
    assert_eq!(t.sender.count("getMessageAuthor"), 1);
}

/// 02-089: "Don't ask again" is remembered only once the paid send left.
#[test]
fn a_refused_paid_send_does_not_trust_the_price() {
    use std::collections::HashMap;
    let mut t = Flaky::new();
    t.sender
        .guard
        .set_state(HashMap::from([(600, 25)]), HashMap::new());
    let sticker = r#"{"@type":"sendMessage","@extra":"3","chat_id":600,"options":{"@type":"messageSendOptions","paid_message_star_count":0},"input_message_content":{"@type":"inputMessageSticker"}}"#;
    t.driver.sender.send_json(sticker).unwrap();
    t.sender.refuse(&["sendMessage"]);
    assert!(t.driver.release_paid_send(true).is_err());
    let trust = &t.driver.session.payments.paid_messages.trust;
    assert!(!trust.is_trusted(600, 25), "the price was never paid");
    t.sender.refuse(&[]);
    t.driver.release_paid_send(true).unwrap();
    assert!(
        t.driver
            .session
            .payments
            .paid_messages
            .trust
            .is_trusted(600, 25)
    );
}
