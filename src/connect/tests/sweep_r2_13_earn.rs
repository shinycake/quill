//! Sweep r2-13 (earn): a refused send of one list does not strand the
//! other, a refused Join reaches the caller, and the revenue page takes
//! only the answer of the open that asked.
use super::sweep_s3_payments_1::Flaky;
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
    assert!(matches!(join.mine.load, PageLoad::Failed { .. }), "Retry shows for My Programs");
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
    assert_eq!(t.driver.session.stickers.emoji.trending.next_page(), Some(2));
    assert!(t.driver.session.stickers.emoji.trending.error().is_some());
    t.sender.refuse(&[]);
    t.driver.more_trending_emoji_packs().unwrap();
    assert_eq!(
        t.sender.last("getTrendingStickerSets").unwrap()["offset"],
        2,
        "the failed page, not the first"
    );
}
