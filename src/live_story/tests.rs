use super::*;

fn message(id: i32, sender: i64, date: i32, stars: i64) -> ParsedGroupCallMessage {
    ParsedGroupCallMessage {
        message_id: id,
        sender_id: MessageSender::User { user_id: sender },
        date,
        text: "hi".to_string(),
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
