use crate::giveaway::{
    ADDITIONAL_CHATS_MAX, GiveawayDraft, GiveawayFormError, GiveawayKind, GiveawayTextContext,
    PERIOD_MAX_SECS, PRIZE_DESCRIPTION_MAX, about_text, premium_options_for, premium_winner_counts,
    settle_premium, settle_stars,
};
use crate::telegram::envelope::{
    GiveawayInfoData, GiveawayParticipantStatus, GiveawayPrize, GiveawaySummary,
    ParsedPrepaidGiveaway, PremiumGiveawayOption, SecretCode, StarGiveawayOption,
};

const NOW: i32 = 1_000_000;

fn premium_options() -> Vec<PremiumGiveawayOption> {
    let option = |winners, months, amount| PremiumGiveawayOption {
        currency: "USD".into(),
        amount,
        winner_count: winners,
        month_count: months,
    };
    vec![
        option(10, 12, 9000),
        option(5, 6, 2500),
        option(5, 3, 1500),
        option(10, 3, 3000),
    ]
}

fn star_options() -> Vec<StarGiveawayOption> {
    let option = |stars, winners: Vec<i32>, default| StarGiveawayOption {
        currency: "USD".into(),
        amount: 100,
        star_count: stars,
        yearly_boost_count: 6,
        winner_counts: winners,
        is_default: default,
        is_additional: false,
    };
    vec![
        option(500, vec![5, 10], false),
        option(1000, vec![10, 20], true),
    ]
}

#[test]
fn a_new_form_starts_three_days_out() {
    let draft = GiveawayDraft::new(-100, None, NOW);
    assert_eq!(draft.selection_date, NOW + 3 * 86_400);
    assert_eq!(draft.days_ahead(NOW), 3);
    assert!(draft.has_public_winners && !draft.only_new_members);
}

#[test]
fn choosing_options_settles_on_offered_values() {
    let mut draft = GiveawayDraft::new(-100, None, NOW);
    let options = premium_options();
    assert_eq!(premium_winner_counts(&options), vec![5, 10]);
    settle_premium(&mut draft, &options);
    assert_eq!((draft.premium_winners, draft.premium_months), (5, 3));
    draft.premium_winners = 10;
    settle_premium(&mut draft, &options);
    assert_eq!((draft.premium_winners, draft.premium_months), (10, 3));
    let months: Vec<i32> = premium_options_for(&options, 10)
        .iter()
        .map(|o| o.month_count)
        .collect();
    assert_eq!(months, vec![3, 12]);
    let stars = star_options();
    draft.kind = GiveawayKind::Stars;
    settle_stars(&mut draft, &stars);
    assert_eq!((draft.star_option, draft.star_winners), (1, 10));
    assert_eq!(draft.boost_count(&stars), 6);
}

#[test]
fn validation_covers_every_field() {
    let mut draft = GiveawayDraft::new(-100, None, NOW);
    assert_eq!(draft.validate(NOW), Err(GiveawayFormError::NoOption));
    settle_premium(&mut draft, &premium_options());
    assert_eq!(draft.validate(NOW), Ok(()));
    draft.prize_description = "x".repeat(PRIZE_DESCRIPTION_MAX + 1);
    assert_eq!(draft.validate(NOW), Err(GiveawayFormError::PrizeTooLong));
    draft.prize_description = "x".repeat(PRIZE_DESCRIPTION_MAX);
    assert_eq!(draft.validate(NOW), Ok(()));
    draft.selection_date = NOW + 30;
    assert_eq!(draft.validate(NOW), Err(GiveawayFormError::DateTooSoon));
    draft.selection_date = NOW + PERIOD_MAX_SECS + 1;
    assert_eq!(draft.validate(NOW), Err(GiveawayFormError::DateTooLate));
    draft.set_days_ahead(NOW, 99);
    assert_eq!(draft.validate(NOW), Ok(()));
    draft.set_days_ahead(NOW, 0);
    assert_eq!(draft.days_ahead(NOW), 1);
}

#[test]
fn extra_chats_toggle_and_stop_at_the_limit() {
    let mut draft = GiveawayDraft::new(-100, None, NOW);
    for id in 0..ADDITIONAL_CHATS_MAX as i64 {
        assert_eq!(draft.toggle_chat(-200 - id), Ok(()));
    }
    assert_eq!(
        draft.toggle_chat(-999),
        Err(GiveawayFormError::TooManyChats)
    );
    assert_eq!(draft.toggle_chat(-200), Ok(()));
    assert_eq!(draft.additional_chat_ids.len(), ADDITIONAL_CHATS_MAX - 1);
    assert_eq!(draft.toggle_chat(-999), Ok(()));
}

#[test]
fn parameters_json_matches_the_schema() {
    let mut draft = GiveawayDraft::new(-1001, None, NOW);
    draft.additional_chat_ids = vec![-1002, -1003];
    draft.only_new_members = true;
    draft.has_public_winners = false;
    draft.prize_description = "  A mug ".into();
    let json = draft.parameters_json();
    assert_eq!(json["@type"], "giveawayParameters");
    assert_eq!(json["boosted_chat_id"], -1001);
    assert_eq!(
        json["additional_chat_ids"],
        serde_json::json!([-1002, -1003])
    );
    assert_eq!(json["winners_selection_date"], NOW + 3 * 86_400);
    assert_eq!(json["only_new_members"], true);
    assert_eq!(json["has_public_winners"], false);
    assert_eq!(json["country_codes"], serde_json::json!([]));
    assert_eq!(json["prize_description"], "A mug");
}

#[test]
fn a_prepaid_giveaway_fixes_prize_and_winners() {
    let prepaid = ParsedPrepaidGiveaway {
        id: 77,
        winner_count: 7,
        prize: GiveawayPrize::Premium { month_count: 3 },
        boost_count: 0,
        payment_date: 1,
    };
    let draft = GiveawayDraft::new(-1, Some(prepaid.clone()), NOW);
    assert_eq!(draft.validate(NOW), Ok(()));
    assert_eq!(draft.prepaid_launch(), Some((77, 7, 0)));
    assert_eq!(draft.boost_count(&[]), 28);
    let stars = ParsedPrepaidGiveaway {
        prize: GiveawayPrize::Stars { star_count: 500 },
        boost_count: 12,
        ..prepaid
    };
    let draft = GiveawayDraft::new(-1, Some(stars), NOW);
    assert_eq!(draft.kind, GiveawayKind::Stars);
    assert_eq!(draft.prepaid_launch(), Some((77, 7, 500)));
    assert_eq!(draft.boost_count(&[]), 12);
    assert_eq!(GiveawayDraft::new(-1, None, NOW).prepaid_launch(), None);
}

fn summary(prize: GiveawayPrize) -> GiveawaySummary {
    GiveawaySummary {
        boosted_chat_id: -1,
        info_message_id: 0,
        additional_chat_count: 0,
        selection_date: 2,
        only_new_members: false,
        has_public_winners: true,
        prize_description: String::new(),
        winner_count: 5,
        unclaimed_count: 0,
        prize,
        finished: false,
        was_refunded: false,
    }
}

fn ctx<'a>(
    title: &'a dyn Fn(i64) -> String,
    day: &'a dyn Fn(i32) -> String,
    date_time: &'a dyn Fn(i32) -> String,
) -> GiveawayTextContext<'a> {
    GiveawayTextContext {
        first_channel: "News".into(),
        group: false,
        chat_title: title,
        day,
        date_time,
    }
}

#[test]
fn ongoing_giveaway_says_how_to_take_part() {
    let title = |_: i64| "Other".to_string();
    let day = |_: i32| "October 6".to_string();
    let date_time = |_: i32| "October 1 at 10:00".to_string();
    let ctx = ctx(&title, &day, &date_time);
    let eligible = GiveawayInfoData::Ongoing {
        creation_date: 1,
        status: GiveawayParticipantStatus::Eligible,
        is_ended: false,
    };
    let text = about_text(
        &summary(GiveawayPrize::Premium { month_count: 3 }),
        &eligible,
        &ctx,
    );
    assert_eq!(text.title, "About this giveaway");
    assert!(text.result.is_none());
    assert_eq!(
        text.paragraphs[0],
        "This giveaway is sponsored by the admins of News, who acquired 5 Telegram Premium \
         subscriptions for 3 months for its followers.\n\nOn October 6, Telegram will \
         automatically select 5 random subscribers of News."
    );
    assert_eq!(
        text.paragraphs[1],
        "To take part in this giveaway please join the channel News before October 6."
    );
}

#[test]
fn ongoing_statuses_each_have_their_line() {
    let title = |_: i64| "Other".to_string();
    let day = |_: i32| "October 6".to_string();
    let date_time = |_: i32| "October 1 at 10:00".to_string();
    let ctx = ctx(&title, &day, &date_time);
    let line = |status| {
        let info = GiveawayInfoData::Ongoing {
            creation_date: 1,
            status,
            is_ended: false,
        };
        about_text(
            &summary(GiveawayPrize::Stars { star_count: 100 }),
            &info,
            &ctx,
        )
        .paragraphs[1]
            .clone()
    };
    assert!(line(GiveawayParticipantStatus::Participating).starts_with("You are participating"));
    assert!(
        line(GiveawayParticipantStatus::Administrator { chat_id: -5 })
            .contains("admin of a participating channel (Other)")
    );
    assert!(
        line(GiveawayParticipantStatus::AlreadyWasMember {
            joined_chat_date: 1
        })
        .contains("joined this channel on October 1 at 10:00")
    );
    assert!(
        line(GiveawayParticipantStatus::DisallowedCountry {
            country_code: "DE".into()
        })
        .contains("your country is not included")
    );
}

#[test]
fn ended_giveaways_report_the_users_result() {
    let title = |_: i64| String::new();
    let day = |_: i32| "October 6".to_string();
    let date_time = |_: i32| "October 1 at 10:00".to_string();
    let ctx = ctx(&title, &day, &date_time);
    let completed = |code: &str, stars, winner, refunded| GiveawayInfoData::Completed {
        creation_date: 1,
        actual_winners_selection_date: 2,
        was_refunded: refunded,
        is_winner: winner,
        winner_count: 4,
        activation_count: 2,
        gift_code: SecretCode(code.into()),
        won_star_count: stars,
    };
    let s = summary(GiveawayPrize::Premium { month_count: 12 });
    let won = about_text(&s, &completed("slug", 0, true, false), &ctx);
    assert_eq!(won.title, "Giveaway ended");
    assert!(won.won);
    assert_eq!(
        won.result.as_deref(),
        Some("You won a prize in this giveaway \u{1F3C6}")
    );
    assert!(won.paragraphs[0].contains("Telegram automatically selected 4 random subscribers"));
    assert!(won.paragraphs[0].contains("2 of the winners already used their gift links."));
    assert_eq!(won.paragraphs.len(), 1);
    let stars = about_text(&s, &completed("", 250, true, false), &ctx);
    assert_eq!(
        stars.result.as_deref(),
        Some("You won 250 Stars in this giveaway \u{1F3C6}")
    );
    let lost = about_text(&s, &completed("", 0, false, false), &ctx);
    assert_eq!(
        lost.result.as_deref(),
        Some("You didn't win a prize in this giveaway.")
    );
    // tdesktop keys the result on the code and Stars alone, so a winner
    // flag without either reads as a loss.
    let flag_only = about_text(&s, &completed("", 0, true, false), &ctx);
    assert_eq!(
        flag_only.result.as_deref(),
        Some("You didn't win a prize in this giveaway.")
    );
    assert!(!flag_only.won);
    let refunded = about_text(&s, &completed("", 0, false, true), &ctx);
    assert!(refunded.result.is_none());
    assert!(refunded.refund_notice.unwrap().contains("refunding"));
    let preparing = GiveawayInfoData::Ongoing {
        creation_date: 1,
        status: GiveawayParticipantStatus::Eligible,
        is_ended: true,
    };
    let preparing = about_text(&s, &preparing, &ctx);
    assert_eq!(preparing.paragraphs.len(), 1);
    assert_eq!(preparing.title, "Giveaway ended");
}

#[test]
fn new_members_prizes_and_groups_change_the_wording() {
    let title = |_: i64| String::new();
    let day = |_: i32| "October 6".to_string();
    let date_time = |_: i32| "October 1 at 10:00".to_string();
    let mut ctx = ctx(&title, &day, &date_time);
    ctx.group = true;
    let mut s = summary(GiveawayPrize::Premium { month_count: 1 });
    s.only_new_members = true;
    s.additional_chat_count = 2;
    s.prize_description = "a mug".into();
    s.winner_count = 1;
    let info = GiveawayInfoData::Ongoing {
        creation_date: 1,
        status: GiveawayParticipantStatus::Eligible,
        is_ended: false,
    };
    let text = about_text(&s, &info, &ctx);
    let body = &text.paragraphs[0];
    assert!(body.contains("1 Telegram Premium subscription for 1 month for its members"));
    assert!(body.contains(
        "News also included 1 a mug in the prize. Admins of the group are responsible for delivering this prize."
    ));
    assert!(body.contains(
        "1 random user that joined News and other listed groups and channels after October 1 at 10:00"
    ));
    assert!(text.paragraphs[1].contains("join channel News and other listed groups"));
}
