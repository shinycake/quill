//! State tests: story messages (`messageStory`) and the stories they need.
use super::common::*;
use super::*;
use crate::telegram::envelope::ServiceAction;

fn story_content(via_mention: bool) -> MessageContent {
    MessageContent::Action(Box::new(ServiceAction::Story {
        via_mention,
        poster_chat_id: 77,
        story_id: 5,
    }))
}

#[test]
fn story_messages_name_their_story() {
    assert_eq!(message_story_key(&story_content(true)), Some((77, 5, true)));
    assert_eq!(
        message_story_key(&story_content(false)),
        Some((77, 5, false))
    );
    assert_eq!(message_story_key(&MessageContent::ScreenshotTaken), None);
    let unnamed = MessageContent::Action(Box::new(ServiceAction::Story {
        via_mention: false,
        poster_chat_id: 0,
        story_id: 0,
    }));
    assert_eq!(message_story_key(&unnamed), None);
}

#[test]
fn card_state_is_loading_until_the_story_lands_or_fails() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    assert_eq!(session.story_card_state(77, 5), StoryCardState::Loading);

    // `getStory` fails: the story is gone.
    let extra = session.request_for_story(RequestPurpose::GetStory, ChatId(77), 5);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":404,"message":"Story not found"}}"#,
            extra.0
        ),
    );
    assert_eq!(session.story_card_state(77, 5), StoryCardState::Expired);

    // Another story arrives and is ready.
    let extra = session.request_for_story(RequestPurpose::GetStory, ChatId(77), 6);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"story","@extra":"{}","id":6,"poster_chat_id":77,"date":1,"content":{{"@type":"storyContentUnsupported"}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}"#,
            extra.0
        ),
    );
    assert_eq!(session.story_card_state(77, 6), StoryCardState::Ready);
}
