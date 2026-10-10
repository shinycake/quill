use quill::ids::{ChatId, RequestId};
use quill::telegram::envelope::StoriesPayload;
use quill::telegram::envelope::{EnvelopePayload, StoryChosenExtraReaction, parse_envelope};
use quill::telegram::requests::set_story_custom_emoji_reaction;

#[test]
fn story_chosen_reaction_custom_emoji_and_paid_parsed() {
    // Phase 9.2+: custom-emoji and paid chosen reactions are no longer
    // dropped — they land in `chosen_reaction_extra`.
    let reaction_json = |reaction: &str| {
        format!(
            r#"{{"@type":"story","id":7,"poster_chat_id":11,"date":1,"content":{{"@type":"storyContentUnsupported"}},"chosen_reaction_type":{reaction},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}"#,
        )
    };
    let env = parse_envelope(&reaction_json(
        r#"{"@type":"reactionTypeCustomEmoji","custom_emoji_id":"123"}"#,
    ))
    .unwrap();
    match env.payload {
        EnvelopePayload::Stories(StoriesPayload::Story { story, .. }) => {
            assert_eq!(story.chosen_reaction_emoji, None);
            assert_eq!(
                story.chosen_reaction_extra,
                Some(StoryChosenExtraReaction::CustomEmoji(123))
            );
        }
        other => panic!("{other:?}"),
    }
    let env = parse_envelope(&reaction_json(r#"{"@type":"reactionTypePaid"}"#)).unwrap();
    match env.payload {
        EnvelopePayload::Stories(StoriesPayload::Story { story, .. }) => {
            assert_eq!(story.chosen_reaction_emoji, None);
            assert_eq!(
                story.chosen_reaction_extra,
                Some(StoryChosenExtraReaction::Paid)
            );
        }
        other => panic!("{other:?}"),
    }
    // Unknown reaction types still parse to (None, None) — never an error.
    let env = parse_envelope(&reaction_json(r#"{"@type":"reactionTypeUnknown"}"#)).unwrap();
    match env.payload {
        EnvelopePayload::Stories(StoriesPayload::Story { story, .. }) => {
            assert_eq!(story.chosen_reaction_emoji, None);
            assert_eq!(story.chosen_reaction_extra, None);
        }
        other => panic!("{other:?}"),
    }
}

/// Phase 9.2+: the viewers list keeps custom-emoji / paid chosen
/// reactions in `reaction_extra` instead of dropping them.
#[test]
fn story_interactions_extra_reactions_parse() {
    let json = r#"{"@type":"storyInteractions","total_count":2,"interactions":[
        {"actor_id":{"@type":"messageSenderUser","user_id":777},"interaction_date":1,"type":{"@type":"storyInteractionTypeView","chosen_reaction_type":{"@type":"reactionTypeCustomEmoji","custom_emoji_id":"987"}}},
        {"actor_id":{"@type":"messageSenderUser","user_id":778},"interaction_date":1,"type":{"@type":"storyInteractionTypeView","chosen_reaction_type":{"@type":"reactionTypePaid"}}}
    ],"next_offset":""}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Stories(StoriesPayload::StoryInteractions { interactions }) => {
            assert_eq!(interactions.interactions.len(), 2);
            assert_eq!(interactions.interactions[0].reaction_emoji, None);
            assert_eq!(
                interactions.interactions[0].reaction_extra,
                Some(StoryChosenExtraReaction::CustomEmoji(987))
            );
            assert_eq!(interactions.interactions[1].reaction_emoji, None);
            assert_eq!(
                interactions.interactions[1].reaction_extra,
                Some(StoryChosenExtraReaction::Paid)
            );
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn set_story_custom_emoji_reaction_shape() {
    // Phase 9.2+: custom-emoji reaction — `reactionTypeCustomEmoji`
    // with the int64 id as a JSON string (schema 1.8.67 lines 2918,
    // 13809).
    let custom = set_story_custom_emoji_reaction(RequestId(73), ChatId(11), 7, 123);
    let v: serde_json::Value = serde_json::from_str(&custom).unwrap();
    assert_eq!(v["@type"], "setStoryReaction");
    assert_eq!(v["@extra"], "73");
    assert_eq!(v["story_poster_chat_id"], 11);
    assert_eq!(v["story_id"], 7);
    assert_eq!(v["reaction_type"]["@type"], "reactionTypeCustomEmoji");
    assert_eq!(v["reaction_type"]["custom_emoji_id"], "123");
    assert_eq!(v["update_recent_reactions"], true);
}
