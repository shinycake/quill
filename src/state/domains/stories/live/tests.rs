use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::AccountKey;
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
        l.extras_requested = true;
    }
    session.begin_live_story(7, 6, 42);
    assert!(live(&session).extras_requested);
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
