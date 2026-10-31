use crate::paid_reaction::*;

#[test]
fn a_tap_waits_for_the_accumulate_period() {
    let mut paid = PaidReactions::default();
    assert_eq!(paid.add(1, 10, QUICK_AMOUNT, None, 1_000), Some(1));
    assert!(paid.has_pending());
    assert!(paid.take_due(1_000 + ACCUMULATE_MS - 1).is_empty());
    let due = paid.take_due(1_000 + ACCUMULATE_MS);
    assert_eq!(due.len(), 1);
    assert_eq!(
        (due[0].chat_id, due[0].message_id, due[0].stars),
        (1, 10, 1)
    );
    assert!(!paid.has_pending());
}

#[test]
fn taps_add_up_and_restart_the_timer() {
    let mut paid = PaidReactions::default();
    paid.add(1, 10, 1, None, 0);
    assert_eq!(paid.add(1, 10, 1, None, 4_000), Some(2));
    assert_eq!(paid.pending_stars(1, 10), 2);
    // The first deadline (5.5 s) passed but the second tap pushed it.
    assert!(paid.take_due(6_000).is_empty());
    assert_eq!(paid.take_due(9_500).len(), 1);
}

#[test]
fn undo_forgets_the_waiting_stars() {
    let mut paid = PaidReactions::default();
    paid.add(1, 10, 5, Some(PaidSender::Anonymous), 0);
    assert!(paid.undo(1, 10));
    assert!(!paid.undo(1, 10));
    assert_eq!(paid.total_pending(), 0);
    assert!(paid.take_due(u64::MAX).is_empty());
}

#[test]
fn the_toast_follows_the_latest_message() {
    let mut paid = PaidReactions::default();
    paid.add(1, 10, 1, None, 0);
    paid.add(1, 11, 3, None, 500);
    assert_eq!(paid.toast().map(|p| p.message_id), Some(11));
    paid.add(1, 10, 1, None, 900);
    assert_eq!(paid.toast().map(|p| (p.message_id, p.stars)), Some((10, 2)));
    assert_eq!(paid.total_pending(), 5);
    paid.undo(1, 10);
    assert_eq!(paid.toast().map(|p| p.message_id), Some(11));
}

#[test]
fn the_cap_refuses_more_stars() {
    let mut paid = PaidReactions {
        max_stars: Some(10),
        ..PaidReactions::default()
    };
    assert_eq!(paid.add(1, 10, 8, None, 0), Some(8));
    assert_eq!(paid.add(1, 10, 3, None, 0), None);
    assert_eq!(paid.room(1, 10), 2);
    assert_eq!(paid.add(1, 10, 2, None, 0), Some(10));
    assert_eq!(paid.add(1, 10, 0, None, 0), None);
    // Another message has its own room.
    assert_eq!(paid.add(1, 11, 10, None, 0), Some(10));
}

#[test]
fn the_cap_has_a_fallback() {
    assert_eq!(PaidReactions::default().max(), FALLBACK_MAX);
}

#[test]
fn the_latest_sender_choice_wins() {
    let mut paid = PaidReactions::default();
    paid.add(1, 10, 1, None, 0);
    paid.add(1, 10, 50, Some(PaidSender::Anonymous), 10);
    paid.add(1, 10, 1, None, 20);
    let due = paid.take_due(u64::MAX);
    assert_eq!(due[0].sender, Some(PaidSender::Anonymous));
    assert_eq!(due[0].stars, 52);
}

#[test]
fn a_refused_tap_is_taken_back() {
    let mut paid = PaidReactions::default();
    paid.add(1, 10, 1, None, 0);
    paid.add(1, 10, 4, None, 5);
    paid.rollback(1, 10, 4);
    assert_eq!(paid.pending_stars(1, 10), 1);
    paid.rollback(1, 10, 1);
    assert!(!paid.has_pending());
    paid.rollback(1, 10, 1);
}

#[test]
fn seconds_left_counts_down_to_one() {
    let mut paid = PaidReactions::default();
    paid.add(1, 10, 1, None, 0);
    let p = paid.toast().unwrap();
    assert_eq!(p.seconds_left(0), 6);
    assert_eq!(p.seconds_left(1_000), 5);
    assert_eq!(p.seconds_left(5_499), 1);
    assert_eq!(p.seconds_left(9_000), 1);
}

#[test]
fn the_picker_offer_is_per_message() {
    let paid = PaidReactions {
        offered: Some((1, 10)),
        ..PaidReactions::default()
    };
    assert!(paid.offered_for(1, 10));
    assert!(!paid.offered_for(1, 11));
}

#[test]
fn paid_reactors_parse_sorted_with_anonymous_hidden() {
    use crate::telegram::envelope::{MessageSender, parse_message_reactions};
    let reactions = parse_message_reactions(Some(&serde_json::json!({
        "@type": "messageReactions",
        "reactions": [],
        "are_tags": false,
        "can_get_added_reactions": false,
        "paid_reactors": [
            {"@type": "paidReactor", "sender_id": {"@type": "messageSenderUser", "user_id": 7},
             "star_count": 10, "is_top": false, "is_me": true, "is_anonymous": false},
            {"@type": "paidReactor", "sender_id": {"@type": "messageSenderUser", "user_id": 8},
             "star_count": 500, "is_top": true, "is_me": false, "is_anonymous": true}
        ]
    })))
    .unwrap();
    let list = &reactions.paid_reactors;
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].star_count, 500);
    assert!(list[0].is_anonymous && list[0].sender.is_none() && list[0].is_top);
    assert_eq!(list[1].sender, Some(MessageSender::User { user_id: 7 }));
    assert!(list[1].is_me);
}

#[test]
fn sender_types_serialize() {
    assert_eq!(
        PaidSender::Regular.to_tdlib_json()["@type"],
        "paidReactionTypeRegular"
    );
    assert_eq!(
        PaidSender::Anonymous.to_tdlib_json()["@type"],
        "paidReactionTypeAnonymous"
    );
}

#[test]
fn the_balance_covers_waiting_stars_too() {
    let mut paid = PaidReactions::default();
    assert_eq!(paid.refusal(1, 10, 5, Some(10)), None);
    assert_eq!(paid.refusal(1, 10, 5, None), None);
    paid.add(1, 10, 8, None, 0);
    assert_eq!(paid.refusal(1, 11, 5, Some(10)), Some(Refusal::NotEnough));
    assert_eq!(paid.refusal(1, 11, 2, Some(10)), None);
    assert_eq!(paid.refusal(1, 11, 0, Some(10)), Some(Refusal::TooMany));
}

#[test]
fn the_slider_maps_to_amounts_and_back() {
    assert_eq!(amount_for_step(0, 10_000), 1);
    assert_eq!(amount_for_step(SLIDER_STEPS, 10_000), 10_000);
    let mut last = 0;
    for step in 0..=SLIDER_STEPS {
        let amount = amount_for_step(step, 10_000);
        assert!(amount >= last);
        last = amount;
    }
    for amount in [1, 2, 10, DEFAULT_AMOUNT, 500, 10_000] {
        let step = step_for_amount(amount, 10_000);
        let back = amount_for_step(step, 10_000);
        // Cubic steps are coarse at the top: within one step of the target.
        let next = amount_for_step((step + 1).min(SLIDER_STEPS), 10_000);
        let prev = amount_for_step((step - 1).max(0), 10_000);
        assert!(prev <= amount && amount <= next, "{amount} -> {back}");
    }
    assert_eq!(amount_for_step(50, 2), 1);
    assert_eq!(amount_for_step(SLIDER_STEPS, 2), 2);
}

#[test]
fn the_next_change_lands_where_the_countdown_shows_another_second() {
    let mut paid = PaidReactions::default();
    assert_eq!(paid.next_change_ms(0), None);
    paid.add(1, 10, 1, None, 0);
    let shown = |now_ms: u64| paid.toast().map(|p| p.seconds_left(now_ms));
    let mut now_ms = 0;
    let mut seconds = vec![shown(now_ms)];
    while let Some(wait) = paid.next_change_ms(now_ms).filter(|wait| *wait > 0) {
        assert_eq!(shown(now_ms + wait - 1), shown(now_ms), "no change before it");
        now_ms += wait;
        seconds.push(shown(now_ms));
    }
    assert_eq!(now_ms, ACCUMULATE_MS, "the last one is the commit");
    assert_eq!(
        seconds,
        [6, 5, 4, 3, 2, 1, 1].map(Some),
        "the commit itself shows no new second"
    );
}

#[test]
fn the_next_change_is_the_soonest_of_every_waiting_reaction() {
    let mut paid = PaidReactions::default();
    paid.add(1, 10, 1, None, 0);
    paid.add(1, 11, 1, None, 300);
    assert_eq!(paid.next_change_ms(300), Some(200));
    assert_eq!(paid.next_change_ms(ACCUMULATE_MS), Some(0));
}
