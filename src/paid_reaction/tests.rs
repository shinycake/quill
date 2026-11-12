use crate::ids::RequestId;
use crate::paid_reaction::*;

#[test]
fn a_tap_waits_for_the_accumulate_period() {
    let mut paid = PaidReactions::default();
    assert_eq!(paid.add(1, 10, QUICK_AMOUNT, None, 1_000, None), Some(1));
    assert!(paid.has_pending());
    assert!(paid.due(1_000 + ACCUMULATE_MS - 1).is_empty());
    let due = paid.due(1_000 + ACCUMULATE_MS);
    assert_eq!(due.len(), 1);
    assert_eq!(
        paid.total_pending(),
        1,
        "still reserved against the balance"
    );
    // Due, but still waiting until its commit request goes out.
    assert!(paid.has_pending());
    paid.begin_commit(1, 10, RequestId(1));
    assert!(!paid.has_pending());
}

#[test]
fn taps_add_up_and_restart_the_timer() {
    let mut paid = PaidReactions::default();
    paid.add(1, 10, 1, None, 0, None);
    assert_eq!(paid.add(1, 10, 1, None, 4_000, None), Some(2));
    assert_eq!(paid.pending_stars(1, 10), 2);
    // The first deadline (5.5 s) passed but the second tap pushed it.
    assert!(paid.due(6_000).is_empty());
    assert_eq!(paid.due(9_500).len(), 1);
}

#[test]
fn undo_forgets_the_waiting_stars() {
    let mut paid = PaidReactions::default();
    paid.add(1, 10, 5, Some(PaidSender::Anonymous), 0, None);
    assert!(paid.undo(1, 10));
    assert!(!paid.undo(1, 10));
    assert_eq!(paid.total_pending(), 0);
    assert!(paid.due(u64::MAX).is_empty());
}

#[test]
fn the_toast_follows_the_latest_message() {
    let mut paid = PaidReactions::default();
    paid.add(1, 10, 1, None, 0, None);
    paid.add(1, 11, 3, None, 500, None);
    assert_eq!(paid.toast().map(|p| p.message_id), Some(11));
    paid.add(1, 10, 1, None, 900, None);
    assert_eq!(
        paid.toast().map(|p| (p.message_id, p.stars())),
        Some((10, 2))
    );
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
    assert_eq!(paid.add(1, 10, 8, None, 0, None), Some(8));
    assert_eq!(paid.add(1, 10, 3, None, 0, None), None);
    assert_eq!(paid.room(1, 10), 2);
    assert_eq!(paid.add(1, 10, 2, None, 0, None), Some(10));
    assert_eq!(paid.add(1, 10, 0, None, 0, None), None);
    // Another message has its own room.
    assert_eq!(paid.add(1, 11, 10, None, 0, None), Some(10));
}

#[test]
fn star_totals_saturate() {
    let mut paid = PaidReactions {
        max_stars: Some(i64::MAX),
        ..PaidReactions::default()
    };
    assert_eq!(
        paid.add(1, 10, i64::MAX, None, u64::MAX, None),
        Some(i64::MAX)
    );
    assert_eq!(paid.add(1, 11, i64::MAX, None, 0, None), Some(i64::MAX));
    assert_eq!(paid.total_pending(), i64::MAX);
}

#[test]
fn the_cap_has_a_fallback() {
    assert_eq!(PaidReactions::default().max(), FALLBACK_MAX);
}

#[test]
fn the_latest_sender_choice_wins() {
    let mut paid = PaidReactions::default();
    paid.add(1, 10, 1, None, 0, None);
    paid.add(1, 10, 50, Some(PaidSender::Anonymous), 10, None);
    paid.add(1, 10, 1, None, 20, None);
    let due = paid.due(u64::MAX);
    assert_eq!(due[0].sender, Some(PaidSender::Anonymous));
    assert_eq!(due[0].stars(), 52);
}

#[test]
fn a_refused_tap_takes_back_only_its_own_stars() {
    let mut paid = PaidReactions::default();
    paid.add(1, 10, 1, None, 0, Some(RequestId(1)));
    paid.add(1, 10, 4, None, 5, Some(RequestId(2)));
    paid.rollback(RequestId(2));
    assert_eq!(paid.pending_stars(1, 10), 1);
    paid.rollback(RequestId(1));
    assert!(!paid.has_pending());
    paid.rollback(RequestId(1));
}

#[test]
fn a_late_refusal_does_not_touch_a_newer_entry_of_the_same_message() {
    let mut paid = PaidReactions::default();
    paid.add(1, 10, 3, None, 0, Some(RequestId(1)));
    paid.begin_commit(1, 10, RequestId(9));
    // The tap after the commit went out waits in its own entry.
    paid.add(1, 10, 2, None, 6_000, Some(RequestId(2)));
    assert_eq!(paid.pending_stars(1, 10), 2, "only the waiting entry");
    assert_eq!(paid.total_pending(), 5);
    assert_eq!(paid.toast().map(PendingPaid::stars), Some(2));
    // The first tap's add is refused late: it comes off the committing
    // entry, not the newer one.
    paid.rollback(RequestId(1));
    assert_eq!(paid.toast().map(PendingPaid::stars), Some(2));
    assert_eq!(paid.total_pending(), 2);
    // Undo only takes the waiting entry.
    assert!(paid.undo(1, 10));
    assert_eq!(paid.total_pending(), 0);
}

#[test]
fn an_unsent_commit_keeps_its_stars_stuck_and_is_sent_again_later() {
    let mut paid = PaidReactions::default();
    paid.add(1, 10, 3, None, 0, None);
    assert!(
        paid.commit_unsent(1, 10, ACCUMULATE_MS),
        "an entry was waiting"
    );
    // TDLib still holds them: reserved, not shown, sent again after a wait.
    assert_eq!(paid.total_pending(), 3);
    assert!(!paid.has_pending());
    assert!(paid.due(ACCUMULATE_MS).is_empty(), "not at once");
    assert_eq!(paid.next_change_ms(ACCUMULATE_MS), Some(2_000));
    let again = paid.due(ACCUMULATE_MS + 2_000);
    assert_eq!(again.len(), 1);
    assert!(
        !paid.commit_unsent(1, 10, ACCUMULATE_MS + 2_000),
        "told once"
    );
    // The next tap on the message shows and commits everything TDLib holds.
    assert_eq!(paid.add(1, 10, 2, None, 1_000, None), Some(5));
    assert_eq!(paid.toast().map(PendingPaid::stars), Some(5));
    assert_eq!(paid.total_pending(), 5);
}

#[test]
fn seconds_left_counts_down_to_one() {
    let mut paid = PaidReactions::default();
    paid.add(1, 10, 1, None, 0, None);
    let p = paid.toast().unwrap();
    assert_eq!(p.seconds_left(0), 6);
    assert_eq!(p.seconds_left(1_000), 5);
    assert_eq!(p.seconds_left(5_499), 1);
    assert_eq!(p.seconds_left(9_000), 1);
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
    paid.add(1, 10, 8, None, 0, None);
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
    paid.add(1, 10, 1, None, 0, None);
    let shown = |now_ms: u64| paid.toast().map(|p| p.seconds_left(now_ms));
    let mut now_ms = 0;
    let mut seconds = vec![shown(now_ms)];
    while let Some(wait) = paid.next_change_ms(now_ms).filter(|wait| *wait > 0) {
        assert_eq!(
            shown(now_ms + wait - 1),
            shown(now_ms),
            "no change before it"
        );
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
    paid.add(1, 10, 1, None, 0, None);
    paid.add(1, 11, 1, None, 300, None);
    assert_eq!(paid.next_change_ms(300), Some(200));
    assert_eq!(paid.next_change_ms(ACCUMULATE_MS), Some(0));
}

#[test]
fn a_commit_keeps_its_stars_reserved_until_tdlib_answers() {
    let mut paid = PaidReactions::default();
    paid.add(1, 10, 5, None, 0, None);
    assert_eq!(paid.due(ACCUMULATE_MS).len(), 1);
    paid.begin_commit(1, 10, RequestId(7));
    assert!(!paid.has_pending());
    assert!(paid.due(u64::MAX).is_empty());
    assert_eq!(paid.total_pending(), 5);
    assert!(paid.in_flight());
    // An answer to some other request settles nothing.
    paid.settled(RequestId(8));
    assert_eq!(paid.total_pending(), 5);
    paid.settled(RequestId(7));
    assert_eq!(paid.total_pending(), 0);
    assert!(!paid.in_flight());
}

#[test]
fn a_refused_commit_releases_the_stars_and_never_retries() {
    let mut paid = PaidReactions::default();
    paid.add(1, 10, 5, None, 0, None);
    paid.due(ACCUMULATE_MS);
    paid.begin_commit(1, 10, RequestId(7));
    // A tap while the commit is out is a separate waiting entry.
    assert_eq!(paid.add(1, 10, 2, None, ACCUMULATE_MS + 100, None), Some(2));
    assert_eq!(paid.total_pending(), 7);
    paid.refused(RequestId(7), 0);
    // Only the newer tap is still held: TDLib took the refused Stars out of
    // the message with the commit and unlocked them on the error.
    assert_eq!(paid.total_pending(), 2);
    assert_eq!(paid.toast().map(PendingPaid::stars), Some(2));
    assert!(!paid.in_flight());
    paid.begin_commit(1, 10, RequestId(8));
    paid.settled(RequestId(8));
    assert_eq!(paid.total_pending(), 0);
}

#[test]
fn a_refused_undo_joins_the_next_tap_of_its_message() {
    let mut paid = PaidReactions::default();
    paid.add(1, 10, 3, None, 0, None);
    paid.begin_undo(1, 10, RequestId(4));
    paid.refused(RequestId(4), 0);
    // TDLib removed nothing: the next tap commits them together.
    assert_eq!(paid.add(1, 10, 2, None, 1_000, None), Some(5));
    assert_eq!(paid.toast().map(PendingPaid::stars), Some(5));
    paid.begin_commit(1, 10, RequestId(5));
    paid.settled(RequestId(5));
    assert_eq!(paid.total_pending(), 0);
}

#[test]
fn an_answered_commit_also_settles_stars_an_earlier_failure_left_stuck() {
    let mut paid = PaidReactions::default();
    paid.add(1, 10, 5, None, 0, None);
    paid.commit_unsent(1, 10, ACCUMULATE_MS);
    assert_eq!(paid.total_pending(), 5);
    // A stuck entry cannot be re-committed alone; once any later commit of
    // the message is answered, TDLib has spent or dropped all of it.
    paid.add(2, 20, 1, None, 0, None);
    paid.begin_commit(2, 20, RequestId(3));
    paid.settled(RequestId(3));
    assert_eq!(paid.total_pending(), 5, "another message is untouched");
}

#[test]
fn a_low_balance_commit_error_does_not_loop() {
    let mut paid = PaidReactions::default();
    paid.add(1, 10, 5, None, 0, None);
    assert_eq!(paid.due(ACCUMULATE_MS).len(), 1);
    paid.begin_commit(1, 10, RequestId(1));
    paid.refused(RequestId(1), 0);
    assert_eq!(paid.total_pending(), 0);
    assert!(paid.due(u64::MAX).is_empty());
    assert!(paid.next_change_ms(ACCUMULATE_MS * 3).is_none());
}

#[test]
fn a_refused_undo_keeps_the_stars_stuck_and_does_not_commit() {
    let mut paid = PaidReactions::default();
    paid.add(1, 10, 3, None, 0, None);
    paid.begin_undo(1, 10, RequestId(4));
    assert!(paid.toast().is_none());
    assert_eq!(paid.total_pending(), 3);
    paid.refused(RequestId(4), 0);
    assert_eq!(paid.total_pending(), 3);
    assert!(paid.toast().is_none());
    // The undo, not a commit, is what is sent again.
    assert!(paid.due(1_000).is_empty());
    let again = paid.due(2_000);
    assert_eq!(again.len(), 1);
    assert!(matches!(
        again.first().map(|p| p.stage),
        Some(PaidStage::Stuck {
            settle: Settle::Undo,
            ..
        })
    ));
}

/// A stuck entry is sent again a few times and then let go, so a send that
/// never works does not hold its Stars against the balance for good.
#[test]
fn a_stuck_entry_is_let_go_after_its_tries() {
    let mut paid = PaidReactions::default();
    paid.add(1, 10, 3, None, 0, None);
    let mut now = ACCUMULATE_MS;
    paid.commit_unsent(1, 10, now);
    for _ in 1..MAX_TRIES {
        now = now.saturating_add(60_000);
        assert_eq!(paid.due(now).len(), 1, "still sent again");
        paid.commit_unsent(1, 10, now);
        assert_eq!(paid.total_pending(), 3);
    }
    now = now.saturating_add(60_000);
    assert_eq!(paid.due(now).len(), 1);
    paid.commit_unsent(1, 10, now);
    assert_eq!(paid.total_pending(), 0, "let go");
    assert!(paid.due(u64::MAX).is_empty());
    assert!(paid.next_change_ms(now).is_none());
}

/// An undo TDLib keeps refusing is not sent again for good either.
#[test]
fn a_refused_undo_is_let_go_after_its_tries() {
    let mut paid = PaidReactions::default();
    paid.add(1, 10, 3, None, 0, None);
    let mut now = 0;
    paid.begin_undo(1, 10, RequestId(1));
    paid.refused(RequestId(1), now);
    for n in 2..=MAX_TRIES + 1 {
        now += 60_000;
        assert_eq!(paid.due(now).len(), 1);
        paid.begin_undo(1, 10, RequestId(u64::from(n)));
        paid.refused(RequestId(u64::from(n)), now);
    }
    assert_eq!(paid.total_pending(), 0);
}

/// The senders of a chat are asked for once; a failed ask may be tried
/// again, and an answer for one chat is not another's.
#[test]
fn the_senders_of_a_chat_are_asked_again_only_after_a_failure() {
    use crate::telegram::envelope::MessageSender;
    let mut paid = PaidReactions::default();
    assert!(paid.can_ask_senders(5));
    paid.senders_asked(5, RequestId(1));
    assert!(!paid.can_ask_senders(5), "in flight");
    paid.senders_failed(5, Some(RequestId(1)));
    assert!(paid.can_ask_senders(5), "failed: asked again");
    paid.senders_asked(5, RequestId(2));
    // An older ask's late answer and error change nothing.
    paid.set_senders(
        5,
        Some(RequestId(1)),
        &[MessageSender::Chat { chat_id: -1 }],
    );
    paid.senders_failed(5, Some(RequestId(1)));
    assert!(!paid.can_ask_senders(5), "still waiting for the newer ask");
    assert!(paid.channels_for(5).is_empty());
    paid.set_senders(
        5,
        Some(RequestId(2)),
        &[MessageSender::Chat { chat_id: -9 }],
    );
    assert!(!paid.can_ask_senders(5), "answered");
    assert_eq!(paid.channels_for(5), &[-9]);
    assert!(paid.channels_for(6).is_empty());
}
