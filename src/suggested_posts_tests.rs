use crate::ids::{ChatId, MessageId};
use crate::suggested_posts::{
    BarLine, Changes, DirectMessagesRole, PriceCheck, SuggestBar, SuggestMode, SuggestTerms,
    SuggestedPostInfo, SuggestedPostLimits, SuggestedPostPayload, SuggestedPostState,
    SuggestedPostsState, SuggestedPrice, SuggestionButtons, accept_text, block_labels, block_title,
    decline_text, field_text, parse_gram_cents, parse_payload, price_label, price_submit_label,
};
use serde_json::json;

#[test]
fn parses_post_info_with_star_price() {
    let value = json!({
        "@type": "suggestedPostInfo",
        "price": {"@type": "suggestedPostPriceStar", "star_count": 250},
        "send_date": 1_800_000_000,
        "state": {"@type": "suggestedPostStatePending"},
        "can_be_approved": true,
        "can_be_declined": true,
    });
    let info = SuggestedPostInfo::parse(Some(&value));
    assert_eq!(
        info,
        Some(SuggestedPostInfo {
            price: Some(SuggestedPrice::Stars(250)),
            send_date: 1_800_000_000,
            state: SuggestedPostState::Pending,
            can_be_approved: true,
            can_be_declined: true,
        })
    );
}

#[test]
fn parses_gram_price_and_states() {
    let value = json!({
        "@type": "suggestedPostInfo",
        "price": {"@type": "suggestedPostPriceGram", "gram_cent_count": 1250},
        "send_date": 0,
        "state": {"@type": "suggestedPostStateDeclined"},
    });
    let info = SuggestedPostInfo::parse(Some(&value)).unwrap_or_default();
    assert_eq!(info.price, Some(SuggestedPrice::TonCents(1250)));
    assert_eq!(info.state, SuggestedPostState::Declined);
    assert!(!info.can_be_approved);
    assert_eq!(SuggestedPostInfo::parse(Some(&json!(null))), None);
    assert_eq!(SuggestedPostInfo::parse(None), None);
}

#[test]
fn zero_price_is_free() {
    let value = json!({"@type": "suggestedPostPriceStar", "star_count": 0});
    assert_eq!(SuggestedPrice::parse(Some(&value)), None);
    assert_eq!(price_label(None), "Free");
}

#[test]
fn input_terms_round_trip() {
    let terms = SuggestTerms {
        price: Some(SuggestedPrice::TonCents(5)),
        send_date: 1_900_000_000,
    };
    let json = terms.to_input_json();
    assert_eq!(json["price"]["gram_cent_count"], 5);
    assert_eq!(SuggestTerms::parse_input(Some(&json)), Some(terms));
    let free = SuggestTerms::default().to_input_json();
    assert!(free["price"].is_null());
    assert_eq!(free["send_date"], 0);
}

#[test]
fn labels_match_tdesktop() {
    assert_eq!(SuggestedPrice::Stars(1).label(), "1 Star");
    assert_eq!(SuggestedPrice::Stars(1500).label(), "1,500 Stars");
    assert_eq!(SuggestedPrice::TonCents(100).label(), "1 Gram");
    assert_eq!(SuggestedPrice::TonCents(1250).label(), "12.5 Grams");
    assert_eq!(SuggestedPrice::TonCents(1).label(), "0.01 Grams");
}

#[test]
fn bar_line_follows_terms() {
    let star = SuggestedPrice::Stars(10);
    assert_eq!(SuggestTerms::default().bar_line(), BarLine::Prompt);
    let priced = SuggestTerms {
        price: Some(star),
        send_date: 0,
    };
    assert_eq!(priced.bar_line(), BarLine::Priced(star));
    let dated = SuggestTerms {
        price: None,
        send_date: 7,
    };
    assert_eq!(dated.bar_line(), BarLine::Dated(7));
    let both = SuggestTerms {
        price: Some(star),
        send_date: 7,
    };
    assert_eq!(both.bar_line(), BarLine::PricedDated(star, 7));
}

#[test]
fn buttons_follow_rights() {
    let mut info = SuggestedPostInfo {
        can_be_approved: true,
        can_be_declined: true,
        ..SuggestedPostInfo::default()
    };
    assert_eq!(
        SuggestionButtons::for_info(&info),
        SuggestionButtons {
            accept: true,
            decline: true,
            change: true
        }
    );
    info.can_be_approved = false;
    assert_eq!(
        SuggestionButtons::for_info(&info),
        SuggestionButtons {
            accept: false,
            decline: true,
            change: false
        }
    );
    info.can_be_declined = false;
    assert!(!SuggestionButtons::for_info(&info).any());
    info.can_be_approved = true;
    info.state = SuggestedPostState::Approved;
    assert!(!SuggestionButtons::for_info(&info).any());
}

#[test]
fn changes_resolve_like_tdesktop() {
    let was = SuggestTerms {
        price: Some(SuggestedPrice::Stars(10)),
        send_date: 100,
    };
    assert_eq!(Changes::resolve(None, was, true), None);
    let same = Changes::resolve(Some(was), was, true).unwrap_or_default();
    assert!(same.message && !same.price && !same.date);
    let priced = SuggestTerms {
        price: Some(SuggestedPrice::Stars(20)),
        ..was
    };
    let price_only = Changes::resolve(Some(was), priced, true).unwrap_or_default();
    assert!(price_only.price && !price_only.date && !price_only.message);
    let both = SuggestTerms {
        price: None,
        send_date: 200,
    };
    let all = Changes::resolve(Some(was), both, false).unwrap_or_default();
    assert!(all.price && all.date && all.message);
}

#[test]
fn block_titles() {
    assert_eq!(
        block_title(true, "Ann", None),
        "You suggest to post this message."
    );
    assert_eq!(
        block_title(false, "Ann", None),
        "Ann suggests to post this message."
    );
    let price = Changes {
        price: true,
        ..Changes::default()
    };
    assert_eq!(
        block_title(false, "Ann", Some(price)),
        "Ann suggests a new price for the message."
    );
    let time = Changes {
        date: true,
        ..Changes::default()
    };
    assert_eq!(
        block_title(false, "Ann", Some(time)),
        "Ann suggests a new time for the message."
    );
    let both = Changes {
        price: true,
        date: true,
        message: false,
    };
    assert_eq!(
        block_title(false, "Ann", Some(both)),
        "Ann suggests a new price and time for the message."
    );
    let message = Changes {
        message: true,
        ..both
    };
    assert_eq!(
        block_title(false, "Ann", Some(message)),
        "Ann suggests changes for the message."
    );
    assert_eq!(block_labels(Some(both)), ("New Price", "New Time"));
    assert_eq!(block_labels(None), ("Price", "Time"));
}

#[test]
fn limits_take_options() {
    let mut limits = SuggestedPostLimits::default();
    limits.apply_option("suggested_post_star_count_min", 10);
    limits.apply_option("suggested_post_gram_cent_count_max", 500);
    limits.apply_option("unrelated", 1);
    assert_eq!(limits.star_min, 10);
    assert_eq!(limits.ton_cent_max, 500);
    assert_eq!(limits.star_max, 100_000);
}

#[test]
fn price_check() {
    let limits = SuggestedPostLimits::default();
    assert_eq!(limits.check_price("", false), PriceCheck::Ok(None));
    assert_eq!(limits.check_price("0", false), PriceCheck::Ok(None));
    assert_eq!(limits.check_price("4", false), PriceCheck::TooSmall);
    assert_eq!(
        limits.check_price("5", false),
        PriceCheck::Ok(Some(SuggestedPrice::Stars(5)))
    );
    assert_eq!(limits.check_price("100001", false), PriceCheck::TooLarge);
    assert_eq!(limits.check_price("abc", false), PriceCheck::Invalid);
    assert_eq!(
        limits.check_price("1.5", true),
        PriceCheck::Ok(Some(SuggestedPrice::TonCents(150)))
    );
    assert_eq!(limits.check_price("1.555", true), PriceCheck::Invalid);
    assert_eq!(limits.min_price_toast(false), "Minimum price is 5 Stars.");
    assert_eq!(limits.min_price_toast(true), "Minimum price is 0.01 Grams.");
}

#[test]
fn gram_cents_parse() {
    assert_eq!(parse_gram_cents("12"), Some(1200));
    assert_eq!(parse_gram_cents("12,5"), Some(1250));
    assert_eq!(parse_gram_cents(".5"), Some(50));
    assert_eq!(parse_gram_cents("."), None);
    assert_eq!(parse_gram_cents("-1"), None);
    assert_eq!(parse_gram_cents("99999999999999999999"), None);
    assert_eq!(field_text(Some(SuggestedPrice::TonCents(1250))), "12.5");
    assert_eq!(field_text(Some(SuggestedPrice::Stars(7))), "7");
}

#[test]
fn commission_and_time_range() {
    let limits = SuggestedPostLimits::default();
    assert_eq!(
        limits.after_commission_text(SuggestedPrice::Stars(100)),
        "85"
    );
    assert_eq!(
        limits.after_commission_text(SuggestedPrice::TonCents(100)),
        "0.85"
    );
    assert_eq!(limits.percent_label(SuggestedPrice::Stars(1)), "85%");
    assert_eq!(
        limits.you_get(SuggestedPrice::Stars(100)),
        "You will receive 85 Stars (85%) for publishing this post."
    );
    let (min, max) = limits.time_range(1000);
    assert_eq!(min, 1360);
    assert_eq!(max, 1000 + 2_678_400);
    assert_eq!(limits.initial_time(1000, 0), 87_400);
    assert_eq!(limits.initial_time(1000, 5), 1360);
    assert!(limits.delete_loses_payment(1000, 900));
    assert!(!limits.delete_loses_payment(100_000, 900));
}

#[test]
fn confirm_texts() {
    let limits = SuggestedPostLimits::default();
    let admin = accept_text(
        true,
        "Ann",
        "News",
        Some(SuggestedPrice::Stars(100)),
        None,
        &limits,
    );
    assert!(admin.starts_with("Do you want to publish this post from Ann?"));
    assert!(admin.contains("News will receive 85 Stars (85%) for publishing right now."));
    assert!(admin.contains("21 days"));
    let member = accept_text(
        false,
        "Ann",
        "News",
        Some(SuggestedPrice::TonCents(100)),
        Some("Mar 5 at 10:00"),
        &limits,
    );
    assert!(member.ends_with("You will pay 1 Gram for publishing Mar 5 at 10:00."));
    assert_eq!(
        accept_text(false, "Ann", "News", None, None, &limits),
        "Do you want to publish this post to News?"
    );
    assert_eq!(
        decline_text(true, "Ann", "News"),
        "Do you want to decline publishing this post from Ann?"
    );
}

#[test]
fn submit_labels() {
    assert_eq!(price_submit_label(SuggestMode::New, None), "Offer for Free");
    assert_eq!(
        price_submit_label(SuggestMode::New, Some(SuggestedPrice::Stars(5))),
        "Offer"
    );
    assert_eq!(
        price_submit_label(SuggestMode::Change, None),
        "Update Terms"
    );
    assert_eq!(SuggestMode::Publish.time_none(), "Publish Now");
    assert_eq!(SuggestMode::Change.time_submit(), "Update Time");
}

#[test]
fn role_from_flags() {
    assert_eq!(DirectMessagesRole::from_flags(false, true), None);
    assert_eq!(
        DirectMessagesRole::from_flags(true, true),
        Some(DirectMessagesRole::Admin)
    );
    assert_eq!(
        DirectMessagesRole::from_flags(true, false),
        Some(DirectMessagesRole::Member)
    );
}

#[test]
fn draft_sync_keeps_edits() {
    let mut state = SuggestedPostsState::default();
    let chat = ChatId(5);
    let terms = SuggestTerms {
        price: Some(SuggestedPrice::Stars(9)),
        send_date: 0,
    };
    state.sync_draft(chat, Some(terms));
    assert_eq!(state.draft_terms(chat), Some(terms));
    state.sync_draft(chat, None);
    assert_eq!(state.bar(chat), None);
    state.bars.insert(
        chat.0,
        SuggestBar {
            terms,
            mode: SuggestMode::Change,
            edit_of: Some(MessageId(3)),
        },
    );
    state.sync_draft(chat, None);
    assert!(state.bar(chat).is_some());
    assert_eq!(state.draft_terms(chat), None);
}

#[test]
fn parses_updates() {
    let update = json!({
        "@type": "updateMessageSuggestedPostInfo",
        "chat_id": -100,
        "message_id": 42,
        "suggested_post_info": null,
    });
    assert_eq!(
        parse_payload("updateMessageSuggestedPostInfo", &update),
        Some(SuggestedPostPayload::InfoUpdated {
            chat_id: ChatId(-100),
            message_id: MessageId(42),
            info: None,
        })
    );
    let grams = json!({"@type": "updateOwnedGramCount", "gram_amount": 5_000_000_000_i64});
    assert_eq!(
        parse_payload("updateOwnedGramCount", &grams),
        Some(SuggestedPostPayload::GramBalance(5_000_000_000))
    );
    assert_eq!(parse_payload("updateOther", &grams), None);
}

#[test]
fn date_labels() {
    use crate::local_time::CivilTime;
    use crate::suggested_posts::{block_time_label, date_time_label, time_value};
    let at = |year, month, day| CivilTime {
        year,
        month,
        day,
        hour: 9,
        minute: 5,
        weekday: 0,
    };
    let now = at(2026, 10, 14);
    assert_eq!(date_time_label(&at(2026, 3, 5), &now), "Mar 5 at 09:05");
    assert_eq!(
        date_time_label(&at(2027, 1, 2), &now),
        "Jan 2, 2027 at 09:05"
    );
    assert_eq!(block_time_label(&now, &now), "today at 09:05");
    assert_eq!(
        block_time_label(&at(2026, 10, 13), &now),
        "yesterday at 09:05"
    );
    assert_eq!(block_time_label(&at(2026, 10, 20), &now), "Oct 20 at 09:05");
    assert_eq!(time_value(0, 0, |_| now), "Anytime");
}

#[test]
fn balance_shortfall() {
    use crate::suggested_posts::shortfall;
    let stars = SuggestedPrice::Stars(10);
    assert_eq!(shortfall(stars, Some(9), None), Some(stars));
    assert_eq!(shortfall(stars, Some(10), None), None);
    assert_eq!(shortfall(stars, None, None), None);
    let grams = SuggestedPrice::TonCents(150);
    // Grams give what is missing, in cents, rounded up.
    assert_eq!(
        shortfall(grams, None, Some(1_400_000_000)),
        Some(SuggestedPrice::TonCents(10))
    );
    assert_eq!(
        shortfall(grams, None, Some(1_399_999_999)),
        Some(SuggestedPrice::TonCents(11))
    );
    assert_eq!(
        shortfall(grams, None, Some(0)),
        Some(SuggestedPrice::TonCents(150))
    );
    assert_eq!(shortfall(grams, None, Some(1_500_000_000)), None);
}
