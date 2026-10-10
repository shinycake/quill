//! Story statistics, public forwards and found stories
//! (`storyStatistics`, `publicForwards`, `foundStories`; schema 1.8.68).

use super::*;
use serde_json::Value;

/// `storyStatistics` (schema line 10622): the two charts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryStatisticsView {
    pub interaction_graph: StatisticalGraph,
    pub reaction_graph: StatisticalGraph,
}

pub(crate) fn parse_story_statistics(value: &Value) -> Result<StoryStatisticsView, ParseError> {
    Ok(StoryStatisticsView {
        interaction_graph: StatisticalGraph::parse(
            value
                .get("story_interaction_graph")
                .ok_or(ParseError::MissingField)?,
        )?,
        reaction_graph: StatisticalGraph::parse(
            value
                .get("story_reaction_graph")
                .ok_or(ParseError::MissingField)?,
        )?,
    })
}

/// What a `PublicForward` points at: a channel message or a story repost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicForwardKind {
    Message { chat_id: i64, message_id: i64 },
    Story { chat_id: i64, story_id: i32 },
}

/// One `publicForwardMessage` / `publicForwardStory` (schema lines 7180, 7183).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicForwardView {
    pub kind: PublicForwardKind,
    pub date: i32,
}

/// `publicForwards` (schema line 7190).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PublicForwardsView {
    pub total_count: i32,
    pub forwards: Vec<PublicForwardView>,
    pub next_offset: String,
}

pub(crate) fn parse_public_forwards(value: &Value) -> PublicForwardsView {
    let forwards = value
        .get("forwards")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(parse_public_forward).collect())
        .unwrap_or_default();
    PublicForwardsView {
        total_count: value
            .get("total_count")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .sat_i32(),
        forwards,
        next_offset: json_field_str(value, "next_offset"),
    }
}

fn parse_public_forward(value: &Value) -> Option<PublicForwardView> {
    match value.get("@type").and_then(Value::as_str)? {
        "publicForwardMessage" => {
            let message = value.get("message")?;
            Some(PublicForwardView {
                kind: PublicForwardKind::Message {
                    chat_id: int53(message.get("chat_id")).ok()?,
                    message_id: int53(message.get("id")).ok()?,
                },
                date: message
                    .get("date")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .sat_i32(),
            })
        }
        "publicForwardStory" => {
            let story = value.get("story")?;
            Some(PublicForwardView {
                kind: PublicForwardKind::Story {
                    chat_id: int53(story.get("poster_chat_id")).ok()?,
                    story_id: story.get("id")?.as_i64()?.sat_i32(),
                },
                date: story
                    .get("date")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .sat_i32(),
            })
        }
        _ => None,
    }
}

/// `foundStories` (schema line 7086): one page of a public story search.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FoundStoriesView {
    pub total_count: i32,
    pub stories: Vec<ParsedStory>,
    pub files: Vec<ParsedFile>,
    pub next_offset: String,
}

pub(crate) fn parse_found_stories(value: &Value) -> FoundStoriesView {
    let mut stories = Vec::new();
    let mut files = Vec::new();
    if let Some(list) = value.get("stories").and_then(Value::as_array) {
        for entry in list {
            if let Some((story, story_files)) = parse_story(entry) {
                stories.push(story);
                files.extend(story_files);
            }
        }
    }
    FoundStoriesView {
        total_count: value
            .get("total_count")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .sat_i32(),
        stories,
        files,
        next_offset: json_field_str(value, "next_offset"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PublicForwardKind, parse_found_stories, parse_public_forwards, parse_story_statistics,
    };
    use crate::telegram::envelope::StatisticalGraph;
    use serde_json::json;

    #[test]
    fn statistics_parse_both_graphs() {
        let view = parse_story_statistics(&json!({
            "@type": "storyStatistics",
            "story_interaction_graph": {"@type": "statisticalGraphData", "json_data": "{}", "zoom_token": ""},
            "story_reaction_graph": {"@type": "statisticalGraphAsync", "token": "t"}
        }))
        .expect("parses");
        assert!(matches!(
            view.interaction_graph,
            StatisticalGraph::Data { .. }
        ));
        assert_eq!(
            view.reaction_graph,
            StatisticalGraph::Async { token: "t".into() }
        );
        assert!(parse_story_statistics(&json!({"@type": "storyStatistics"})).is_err());
    }

    #[test]
    fn public_forwards_keep_both_kinds_and_skip_unknown() {
        let view = parse_public_forwards(&json!({
            "@type": "publicForwards", "total_count": 3, "next_offset": "n1",
            "forwards": [
                {"@type": "publicForwardMessage", "message": {"id": 40, "chat_id": -100, "date": 11}},
                {"@type": "publicForwardStory", "story": {"id": 5, "poster_chat_id": -200, "date": 22}},
                {"@type": "publicForwardFuture"}
            ]
        }));
        assert_eq!(view.total_count, 3);
        assert_eq!(view.next_offset, "n1");
        assert_eq!(view.forwards.len(), 2);
        assert_eq!(
            view.forwards[0].kind,
            PublicForwardKind::Message {
                chat_id: -100,
                message_id: 40
            }
        );
        assert_eq!(
            view.forwards[1].kind,
            PublicForwardKind::Story {
                chat_id: -200,
                story_id: 5
            }
        );
        assert_eq!(view.forwards[1].date, 22);
    }

    #[test]
    fn found_stories_parse_and_skip_bad_entries() {
        let view = parse_found_stories(&json!({
            "@type": "foundStories", "total_count": 9, "next_offset": "x",
            "stories": [
                {"@type": "story", "id": 4, "poster_chat_id": 11, "date": 1, "content": {"@type": "storyContentUnsupported"}},
                {"@type": "story"}
            ]
        }));
        assert_eq!(view.total_count, 9);
        assert_eq!(view.stories.len(), 1);
        assert_eq!(view.stories[0].id, 4);
        assert_eq!(view.next_offset, "x");
    }
}
