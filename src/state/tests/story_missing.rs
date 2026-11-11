//! A story `getStory` refused is asked for again once what it waited for
//! arrives: its poster chat, the story itself, a new Ready.
use super::common::*;
use super::*;

fn refuse(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>, chat: i64, story: i32) {
    let extra = session.request_for_story(RequestPurpose::GetStory, ChatId(chat), story);
    session.stories.reply_attempted.insert((chat, story));
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"Chat not found"}}"#,
            extra.0
        ),
    );
}

#[test]
fn a_refused_story_is_asked_again_when_its_poster_chat_loads() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    refuse(&mut session, &seq, &sink, 21, 5);
    assert_eq!(session.story_card_state(21, 5), StoryCardState::Expired);
    // Another poster's refusal is not touched by this chat loading.
    refuse(&mut session, &seq, &sink, 22, 6);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Poster","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
    );
    assert_eq!(session.story_card_state(21, 5), StoryCardState::Loading);
    assert!(!session.stories.reply_attempted.contains(&(21, 5)));
    assert_eq!(session.story_card_state(22, 6), StoryCardState::Expired);
}

#[test]
fn a_refused_story_that_arrives_is_no_longer_missing() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    refuse(&mut session, &seq, &sink, 21, 5);
    assert!(session.stories.missing.contains(&(21, 5)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateStory","story":{"@type":"story","id":5,"poster_chat_id":21,"date":1,"content":{"@type":"storyContentUnsupported"},"caption":{"@type":"formattedText","text":"","entities":[]}}}"#,
    );
    assert!(!session.stories.missing.contains(&(21, 5)));
    assert_eq!(session.story_card_state(21, 5), StoryCardState::Ready);
}

#[test]
fn a_new_ready_asks_the_missing_stories_again() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    refuse(&mut session, &seq, &sink, 21, 5);
    session.stories.reset_active_stories_paging();
    assert!(session.stories.missing.is_empty());
}
