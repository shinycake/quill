//! Parses TDLib objects for stories, story albums and close friends.
use crate::telegram::envelope::*;
use crate::telegram::envelope_story::parse_story_album;
use serde_json::Value;

/// The stories domain's TDLib types; `Ok(None)` leaves
/// `type_name` to the other domains.
pub(crate) fn parse_stories_payload(
    type_name: &str,
    value: &Value,
) -> Result<Option<EnvelopePayload>, ParseError> {
    let payload = match type_name {
        "updateChatActiveStories" => {
            let active_stories = value
                .get("active_stories")
                .ok_or(ParseError::MissingField)?;
            parse_chat_active_stories(active_stories)
                .map(|active_stories| {
                    EnvelopePayload::Stories(StoriesPayload::UpdateChatActiveStories {
                        active_stories,
                    })
                })
                .ok_or(ParseError::MissingField)
        }
        "chatActiveStories" => parse_chat_active_stories(value)
            .map(|active_stories| {
                EnvelopePayload::Stories(StoriesPayload::ChatActiveStories { active_stories })
            })
            .ok_or(ParseError::MissingField),
        "story" => {
            let (story, files) = parse_story(value).ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::Stories(StoriesPayload::Story {
                story,
                files,
            }))
        }
        "storyAlbums" => {
            let albums = value
                .get("albums")
                .and_then(Value::as_array)
                .map(|albums| albums.iter().filter_map(parse_story_album).collect())
                .unwrap_or_default();
            Ok(EnvelopePayload::Stories(StoriesPayload::StoryAlbums {
                albums,
            }))
        }
        "storyAlbum" => {
            let album = parse_story_album(value).ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::Stories(StoriesPayload::StoryAlbum {
                album,
            }))
        }
        "stories" => {
            let total_count = value
                .get("total_count")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .sat_i32();
            let stories = value
                .get("stories")
                .and_then(Value::as_array)
                .map(|stories| stories.iter().filter_map(parse_story).collect())
                .unwrap_or_default();
            let pinned_story_ids = value
                .get("pinned_story_ids")
                .and_then(Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter_map(|id| id.as_i64().map(|id| id.sat_i32()))
                        .collect()
                })
                .unwrap_or_default();
            Ok(EnvelopePayload::Stories(StoriesPayload::Stories {
                total_count,
                stories,
                pinned_story_ids,
            }))
        }
        "updateStory" => {
            let story = value.get("story").ok_or(ParseError::MissingField)?;
            let (story, files) = parse_story(story).ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::Stories(StoriesPayload::Story {
                story,
                files,
            }))
        }
        "updateStoryDeleted" => Ok(EnvelopePayload::Stories(
            StoriesPayload::UpdateStoryDeleted {
                poster_chat_id: int53(value.get("story_poster_chat_id"))?,
                story_id: value
                    .get("story_id")
                    .and_then(Value::as_i64)
                    .ok_or(ParseError::MissingField)?
                    .sat_i32(),
            },
        )),
        "updateStoryPostSucceeded" => {
            let story = value.get("story").ok_or(ParseError::MissingField)?;
            let (story, files) = parse_story(story).ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::Stories(
                StoriesPayload::UpdateStoryPostSucceeded {
                    story,
                    files,
                    old_story_id: value
                        .get("old_story_id")
                        .and_then(Value::as_i64)
                        .ok_or(ParseError::MissingField)?
                        .sat_i32(),
                },
            ))
        }
        "updateStoryPostFailed" => {
            let story = value.get("story").ok_or(ParseError::MissingField)?;
            let (story, _files) = parse_story(story).ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::Stories(
                StoriesPayload::UpdateStoryPostFailed {
                    story,
                    error: parse_error(value.get("error")),
                },
            ))
        }
        "availableReactions" => {
            let list = |key: &str| -> Vec<_> {
                value
                    .get(key)
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(parse_story_available_reaction)
                            .collect()
                    })
                    .unwrap_or_default()
            };
            Ok(EnvelopePayload::Stories(
                StoriesPayload::StoryAvailableReactions {
                    reactions: list("top_reactions"),
                    recent: list("recent_reactions"),
                    popular: list("popular_reactions"),
                    allow_custom_emoji: value
                        .get("allow_custom_emoji")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                },
            ))
        }
        // Phase 9.3: `canPostStory` answer — one of the
        // `canPostStoryResult*` variants (TDLib 1.8.67, `schema/td_api.tl:8535`
        // – `td_api.tl:8553`).
        "canPostStoryResultOk"
        | "canPostStoryResultPremiumNeeded"
        | "canPostStoryResultBoostNeeded"
        | "canPostStoryResultActiveStoryLimitExceeded"
        | "canPostStoryResultWeeklyLimitExceeded"
        | "canPostStoryResultMonthlyLimitExceeded"
        | "canPostStoryResultLiveStoryIsActive" => {
            let result = parse_can_post_story_result(value).ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::Stories(
                StoriesPayload::CanPostStoryResult { result },
            ))
        }
        "reportStoryResultOk" => Ok(EnvelopePayload::Stories(StoriesPayload::ReportStoryResult(
            ReportStoryResult::Ok,
        ))),
        "reportStoryResultOptionRequired" => Ok(EnvelopePayload::Stories(
            StoriesPayload::ReportStoryResult(ReportStoryResult::OptionRequired {
                title: json_field_str(value, "title"),
                options: parse_report_options(value.get("options")),
            }),
        )),
        "reportStoryResultTextRequired" => Ok(EnvelopePayload::Stories(
            StoriesPayload::ReportStoryResult(ReportStoryResult::TextRequired {
                option_id: json_field_str(value, "option_id"),
                is_optional: value
                    .get("is_optional")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            }),
        )),
        "storyStatistics" => Ok(EnvelopePayload::Stories(StoriesPayload::StoryStatistics {
            statistics: parse_story_statistics(value)?,
        })),
        "publicForwards" => Ok(EnvelopePayload::Stories(StoriesPayload::PublicForwards {
            forwards: parse_public_forwards(value),
        })),
        "foundStories" => Ok(EnvelopePayload::Stories(StoriesPayload::FoundStories {
            found: parse_found_stories(value),
        })),
        "storyInteractions" => Ok(EnvelopePayload::Stories(
            StoriesPayload::StoryInteractions {
                interactions: parse_story_interactions(value),
            },
        )),
        "updateStoryStealthMode" => Ok(EnvelopePayload::Stories(
            StoriesPayload::UpdateStoryStealthMode {
                active_until_date: value
                    .get("active_until_date")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .sat_i32(),
                cooldown_until_date: value
                    .get("cooldown_until_date")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .sat_i32(),
            },
        )),
        _ => return Ok(None),
    };
    payload.map(Some)
}
