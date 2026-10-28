use super::*;
use std::time::Duration;

fn message(id: i32, sender: i64, date: i32, stars: i64) -> ParsedGroupCallMessage {
    ParsedGroupCallMessage {
        message_id: id,
        sender_id: MessageSender::User { user_id: sender },
        date,
        text: "hi".to_string(),
        entities: Vec::new(),
        paid_message_star_count: stars,
        is_from_owner: false,
        can_be_deleted: false,
    }
}

#[test]
fn levels_sort_lowest_first_and_gain_a_free_level() {
    let levels = normalize_levels(vec![
        GroupCallMessageLevel {
            min_star_count: 100,
            ..level(100)
        },
        GroupCallMessageLevel {
            min_star_count: 50,
            ..level(50)
        },
    ]);
    let stars: Vec<i64> = levels.iter().map(|l| l.min_star_count).collect();
    assert_eq!(stars, vec![0, 50, 100]);
    assert_eq!(normalize_levels(Vec::new()), fallback_levels());
}

fn level(stars: i64) -> GroupCallMessageLevel {
    GroupCallMessageLevel {
        min_star_count: stars,
        pin_duration: 60,
        max_text_length: 60,
        max_custom_emoji_count: 1,
        first_color: 0,
        second_color: 0,
        background_color: 0,
    }
}

#[test]
fn a_comment_gets_the_last_level_it_reaches() {
    let levels = fallback_levels();
    assert_eq!(level_for_stars(&levels, 0).min_star_count, 0);
    assert_eq!(level_for_stars(&levels, 49).min_star_count, 10);
    assert_eq!(level_for_stars(&levels, 50).first_color, 0x46A3EB);
    assert_eq!(level_for_stars(&levels, 1_000_000).min_star_count, 10_000);
    assert_eq!(level_for_stars(&[], 60).min_star_count, 50);
}

#[test]
fn longer_text_and_more_emoji_need_more_stars() {
    let levels = fallback_levels();
    assert_eq!(stars_required(&levels, "hello"), 0);
    assert_eq!(stars_required(&levels, &"a".repeat(31)), 10);
    assert_eq!(stars_required(&levels, "hi 😀"), 10);
    assert_eq!(stars_required(&levels, "😀😀"), 50);
    assert_eq!(stars_required(&levels, &"a".repeat(401)), 10_001);
}

#[test]
fn a_paid_comment_is_pinned_for_its_level() {
    let levels = fallback_levels();
    assert_eq!(pin_finish(&levels, 1000, 0), 0);
    assert_eq!(pin_finish(&levels, 0, 50), 0);
    assert_eq!(pin_finish(&levels, 1000, 50), 1120);
}

#[test]
fn line_feeds_become_spaces() {
    assert_eq!(flatten_comment(" one\ntwo\r\nthree "), "one two  three");
}

#[test]
fn old_comments_leave_only_after_their_pin() {
    let levels = fallback_levels();
    let mut messages: Vec<ParsedGroupCallMessage> = (1..=102)
        .map(|id| message(id, 7, 1000, if id == 1 { 2_000 } else { 0 }))
        .collect();
    retain_comments(&mut messages, &levels, 1500);
    // Two over: #1 is still pinned (1800 s), #2 is free and leaves.
    let first: Vec<i32> = messages.iter().take(2).map(|m| m.message_id).collect();
    assert_eq!(first, vec![1, 3]);
    assert_eq!(messages.len(), 101);
    retain_comments(&mut messages, &levels, 4000);
    assert_eq!(messages.len(), 100);
    assert_eq!(messages.first().map(|m| m.message_id), Some(3));
}

#[test]
fn hidden_comments_show_a_dot_for_new_ones() {
    let me = Some(MessageSender::User { user_id: 1 });
    let mut tracker = CommentsTracker::default();
    assert_eq!(tracker.state(), CommentsState::Empty);
    tracker.on_messages(&[message(1, 7, 1, 0)], me);
    assert_eq!(tracker.state(), CommentsState::Shown);
    tracker.toggle();
    assert_eq!(tracker.state(), CommentsState::Hidden);
    // Your own comment is not new.
    tracker.on_messages(&[message(1, 7, 1, 0), message(2, 1, 2, 0)], me);
    assert_eq!(tracker.state(), CommentsState::Hidden);
    tracker.on_messages(
        &[
            message(1, 7, 1, 0),
            message(2, 1, 2, 0),
            message(3, 8, 3, 0),
        ],
        me,
    );
    assert_eq!(tracker.state(), CommentsState::WithNew);
    tracker.toggle();
    assert_eq!(tracker.state(), CommentsState::Shown);
    tracker.on_messages(&[], me);
    assert_eq!(tracker.state(), CommentsState::Empty);
}

#[test]
fn a_pinned_click_shows_hidden_comments() {
    let mut tracker = CommentsTracker::default();
    tracker.show_from_pinned();
    assert_eq!(tracker.state(), CommentsState::Empty);
    tracker.on_messages(&[message(1, 7, 1, 0)], None);
    tracker.toggle();
    tracker.show_from_pinned();
    assert_eq!(tracker.state(), CommentsState::Shown);
}

#[test]
fn taps_add_up_and_restart_the_timer() {
    let first = PendingLiveReaction::add(None, 1, 1_000);
    assert_eq!(first.commit_at_ms, 6_500);
    let second = PendingLiveReaction::add(Some(first), 1, 3_000);
    assert_eq!(second.stars, 2);
    assert_eq!(second.commit_at_ms, 8_500);
    assert!(!second.due(8_499));
    assert!(second.due(8_500));
    assert_eq!(second.seconds_left(3_000), 6);
    assert_eq!(second.seconds_left(9_000), 1);
    assert!((second.elapsed_fraction(3_000)).abs() < f32::EPSILON);
    assert!((second.elapsed_fraction(9_000) - 1.0).abs() < f32::EPSILON);
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

#[test]
fn a_star_effect_fades_in_rises_and_fades_out() {
    let start = star_effect_frame(0.0, 0.5);
    assert!(close(start.opacity, 0.0));
    assert!(close(start.scale, 0.3));
    assert!(close(start.top, 192.), "starts at the canvas bottom");
    let fading_in = star_effect_frame(0.0625, 0.5);
    assert!(close(fading_in.opacity, 0.5));
    assert!(close(fading_in.scale, 0.5));
    let middle = star_effect_frame(0.5, 0.5);
    assert!(close(middle.opacity, 1.0));
    assert!(close(middle.scale, 0.7));
    assert!(close(middle.top, 96.));
    let late = star_effect_frame(0.95, 0.5);
    assert!(close(late.opacity, 0.4));
    assert!(close(late.scale, 0.46));
    assert!(late.top < middle.top, "it keeps rising");
    let end = star_effect_frame(1.0, 0.5);
    assert!(close(end.top, 0.0));
    assert!(close(end.opacity, 0.0));
}

#[test]
fn the_shift_spreads_effects_across_the_canvas() {
    assert!(close(star_effect_frame(0.5, 0.0).left, 0.0));
    assert!(close(star_effect_frame(0.5, 1.0).left, 64.));
    assert!(close(star_effect_frame(0.5, 0.25).left, 16.));
    assert!(close(star_effect_shift(360), 0.0));
    assert!(close(star_effect_shift(359), 359. / 360.));
}

#[test]
fn bursts_are_tried_from_the_seeds_pick_round() {
    assert_eq!(burst_order(4, 3).collect::<Vec<_>>(), vec![1, 2, 0]);
    assert_eq!(burst_order(u64::MAX, 2).collect::<Vec<_>>(), vec![1, 0]);
    assert_eq!(burst_order(7, 0).count(), 0);
}

#[test]
fn an_effect_plays_the_first_burst_ready_when_it_started_in_seed_order() {
    let start = Instant::now();
    let at = |ms| start + Duration::from_millis(ms);
    let ready = [Some(at(0)), None, Some(at(50)), Some(at(10))];
    // Seed 2 tries 2, 3, 0, 1: 2 was not ready at 20 ms, 3 was.
    assert_eq!(ready_burst(2, &ready, at(20)), Some(3));
    assert_eq!(ready_burst(2, &ready, at(50)), Some(2));
    // Seed 1's own is missing; 2 and 3 were not ready at 5 ms, 0 was.
    assert_eq!(ready_burst(1, &ready, at(5)), Some(0));
    // Ready only after the effect started: it plays none.
    assert_eq!(ready_burst(0, &[Some(at(30))], at(20)), None);
    assert_eq!(ready_burst(0, &[None, None], at(20)), None);
    assert_eq!(ready_burst(5, &[], at(20)), None);
}
