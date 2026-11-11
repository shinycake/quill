use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, RequestId};
use crate::live_story::CommentsState;
use crate::paid_reaction::PaidReactor;
use crate::state::{ActiveGroupCall, LiveStoryState, Session};
use crate::telegram::envelope::{
    GroupCallMessageLevel, LiveStoryDonors, LiveStoryPayload, MessageSender, ParsedGroupCallMessage,
};
use std::sync::Arc;

fn session() -> Session {
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let mut session = Session::new(AccountKey::primary(), sink);
    session.my_user_id = Some(1);
    let mut call = ActiveGroupCall::fresh(42);
    call.is_live_story = true;
    call.paid_message_star_count = 5;
    session.calls.story_stream = Some(call);
    session.begin_live_story(7, 6, 42);
    session
}

fn user(user_id: i64) -> MessageSender {
    MessageSender::User { user_id }
}

fn comment(id: i32, sender: i64, text: &str, stars: i64) -> ParsedGroupCallMessage {
    ParsedGroupCallMessage {
        message_id: id,
        sender_id: user(sender),
        date: 1,
        text: text.to_string(),
        entities: Vec::new(),
        paid_message_star_count: stars,
        is_from_owner: false,
        can_be_deleted: false,
    }
}

fn arrive(session: &mut Session, message: ParsedGroupCallMessage) {
    if let Some(call) = session.calls.story_stream.as_mut() {
        call.messages.push(message.clone());
    }
    session.after_live_story_message(42, &message);
}

fn live(session: &Session) -> &LiveStoryState {
    session.stories.live.as_ref().expect("watching")
}

#[test]
fn begin_is_idempotent_and_other_calls_are_ignored() {
    let mut session = session();
    if let Some(l) = session.live_story_mut(42) {
        l.streamer.begin();
    }
    session.begin_live_story(7, 6, 42);
    assert!(live(&session).streamer.is_loading());
    assert!(session.live_story_mut(43).is_none());
    session.apply_live_story_payload(
        LiveStoryPayload::UpdateTopDonors {
            group_call_id: 43,
            donors: LiveStoryDonors {
                total_star_count: 9,
                top_donors: Vec::new(),
            },
        },
        None,
    );
    assert_eq!(live(&session).stars_total(), 0);
    session.end_live_story();
    assert!(session.stories.live.is_none());
}

#[test]
fn donors_count_my_pending_stars() {
    let mut session = session();
    session.apply_live_story_payload(
        LiveStoryPayload::UpdateTopDonors {
            group_call_id: 42,
            donors: LiveStoryDonors {
                total_star_count: 30,
                top_donors: vec![PaidReactor {
                    sender: Some(user(1)),
                    star_count: 10,
                    is_top: true,
                    is_me: true,
                    is_anonymous: false,
                }],
            },
        },
        None,
    );
    if let Some(live) = session.live_story_mut(42) {
        live.pending_reaction = Some(crate::live_story::PendingLiveReaction::add(None, 3, 0));
    }
    assert_eq!(live(&session).stars_total(), 33);
    assert_eq!(live(&session).my_stars(), 13);
}

#[test]
fn levels_update_replaces_the_fallback() {
    let mut session = session();
    assert!(!session.live_levels().is_empty());
    let level = GroupCallMessageLevel {
        min_star_count: 100,
        pin_duration: 60,
        max_text_length: 50,
        max_custom_emoji_count: 1,
        first_color: 1,
        second_color: 2,
        background_color: 3,
    };
    session.apply_live_story_payload(LiveStoryPayload::UpdateMessageLevels(vec![level]), None);
    let mins: Vec<i64> = session
        .live_levels()
        .iter()
        .map(|l| l.min_star_count)
        .collect();
    assert_eq!(mins, vec![0, 100]);
}

#[test]
fn others_reactions_fly_up_mine_do_not_and_only_four_wait() {
    let mut session = session();
    for stars in 1..=6 {
        session.apply_live_story_payload(
            LiveStoryPayload::UpdateNewPaidReaction {
                group_call_id: 42,
                sender_id: user(9),
                star_count: stars,
            },
            None,
        );
    }
    session.apply_live_story_payload(
        LiveStoryPayload::UpdateNewPaidReaction {
            group_call_id: 42,
            sender_id: user(1),
            star_count: 50,
        },
        None,
    );
    let stars: Vec<i64> = live(&session).effects.iter().map(|e| e.stars).collect();
    assert_eq!(stars, vec![3, 4, 5, 6]);
}

#[test]
fn cheap_reaction_rows_leave_and_comments_toggle_follows() {
    let mut session = session();
    assert_eq!(live(&session).comments.state(), CommentsState::Empty);
    arrive(&mut session, comment(1, 9, "", 2));
    let count = session
        .calls
        .story_stream
        .as_ref()
        .map_or(0, |c| c.messages.len());
    assert_eq!(count, 0, "a reaction under the price is not a comment");
    arrive(&mut session, comment(2, 9, "hello", 0));
    assert_eq!(live(&session).comments.state(), CommentsState::Shown);
    arrive(&mut session, comment(3, 9, "", 25));
    assert_eq!(live(&session).effects.last().map(|e| e.stars), Some(25));
    if let Some(call) = session.calls.story_stream.as_mut() {
        call.messages.clear();
    }
    session.after_live_story_messages_deleted(42);
    assert_eq!(live(&session).comments.state(), CommentsState::Empty);
}

#[test]
fn failed_comments_are_marked_only_for_the_watched_call() {
    let mut session = session();
    assert!(session.live_story_message_failed(42, 5));
    assert!(!session.live_story_message_failed(43, 5));
    assert!(live(&session).failed.contains(&5));
}

fn donors(total: i64) -> LiveStoryPayload {
    LiveStoryPayload::UpdateTopDonors {
        group_call_id: 42,
        donors: LiveStoryDonors {
            total_star_count: total,
            top_donors: Vec::new(),
        },
    }
}

fn tap(session: &mut Session, stars: i64) {
    if let Some(l) = session.live_story_mut(42) {
        l.pending_reaction = Some(crate::live_story::PendingLiveReaction::add(
            l.pending_reaction,
            stars,
            0,
        ));
    }
}

fn commit(session: &mut Session, id: u64) -> RequestId {
    let id = RequestId(id);
    if let Some(l) = session.live_story_mut(42) {
        l.commit_reaction(id);
    }
    id
}

#[test]
fn a_donors_answer_older_than_a_commit_keeps_its_stars_in_the_total() {
    let mut session = session();
    session.apply_live_story_payload(donors(10), None);
    tap(&mut session, 3);
    commit(&mut session, 5);
    assert_eq!(live(&session).stars_total(), 13);
    // The answer to a `getLiveStoryTopDonors` asked (id 4) before the
    // commit (id 5) does not count it.
    if let Some(l) = session.live_story_mut(42) {
        l.set_donors(
            LiveStoryDonors {
                total_star_count: 10,
                top_donors: Vec::new(),
            },
            Some(RequestId(4)),
        );
    }
    assert_eq!(live(&session).stars_total(), 13);
    // One asked after it does.
    if let Some(l) = session.live_story_mut(42) {
        l.set_donors(
            LiveStoryDonors {
                total_star_count: 13,
                top_donors: Vec::new(),
            },
            Some(RequestId(6)),
        );
    }
    assert_eq!(live(&session).stars_total(), 13);
    assert!(live(&session).committed.is_empty());
}

#[test]
fn a_pushed_donors_update_counts_the_committed_batches() {
    let mut session = session();
    tap(&mut session, 3);
    commit(&mut session, 5);
    session.apply_live_story_payload(donors(3), None);
    assert_eq!(live(&session).stars_total(), 3);
}

#[test]
fn a_failed_add_leaves_the_batch_that_held_its_stars() {
    let mut session = session();
    session.apply_live_story_payload(donors(0), None);
    // Adds 1 and 2 (request ids), commit 3, add 4, commit 5, add 6.
    tap(&mut session, 1);
    tap(&mut session, 2);
    commit(&mut session, 3);
    tap(&mut session, 4);
    commit(&mut session, 5);
    tap(&mut session, 6);
    assert_eq!(live(&session).stars_total(), 13);
    if let Some(l) = session.live_story_mut(42) {
        // The add of 2 Stars (id 2) failed after its commit went out.
        l.add_failed(RequestId(2), 2);
    }
    assert_eq!(live(&session).stars_total(), 11);
    assert_eq!(live(&session).committed.first().map(|b| b.stars), Some(1));
    assert_eq!(live(&session).committed.get(1).map(|b| b.stars), Some(4));
    if let Some(l) = session.live_story_mut(42) {
        // The add of 6 (id 6) failed while still pending.
        l.add_failed(RequestId(6), 6);
    }
    assert_eq!(live(&session).stars_total(), 5);
    assert!(live(&session).pending_reaction.is_none());
}

#[test]
fn a_failed_commit_drops_only_its_own_batch() {
    let mut session = session();
    tap(&mut session, 2);
    let first = commit(&mut session, 3);
    tap(&mut session, 4);
    let second = commit(&mut session, 5);
    assert_eq!(live(&session).stars_total(), 6);
    if let Some(l) = session.live_story_mut(42) {
        l.commit_failed(first);
    }
    assert_eq!(live(&session).stars_total(), 4);
    if let Some(l) = session.live_story_mut(42) {
        l.commit_failed(second);
    }
    assert_eq!(live(&session).stars_total(), 0);
}
