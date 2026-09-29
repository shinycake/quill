//! Clickable story areas (Phase 9.8): `story.areas` parsing —
//! `storyArea` / `StoryAreaType` (`schema/td_api.tl:6566`).

use super::envelope::{GeoLocation, geo_location, int53};
use serde_json::Value;

/// Phase 9.8: one `storyArea` (TDLib 1.8.67, `schema/td_api.tl:6566`) — a
/// clickable rectangle on story media.
#[derive(Debug, Clone, PartialEq)]
pub struct StoryAreaView {
    /// Position/size fractions of the media (0.0–1.0) from
    /// `storyAreaPosition` (`schema/td_api.tl:6530`). `x`/`y` are the
    /// rectangle's CENTER, not its top-left corner: the UI places the chip
    /// with `left = x*360 − chip_w/2`, `top = y*640 − chip_h/2`, where
    /// `chip_w = (width*360).max(48)` and `chip_h = (height*640).max(24)`.
    /// `rotation_angle` / `corner_radius_percentage` are not rendered in
    /// this slice.
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub kind: StoryAreaKind,
}

/// Phase 9.8: `StoryAreaType` (`schema/td_api.tl:6533`) — what an area
/// points to. `Eq` is not derived because weather carries a `double`
/// temperature.
#[derive(Debug, Clone, PartialEq)]
pub enum StoryAreaKind {
    /// `storyAreaTypeLocation` (`schema/td_api.tl:6536`) — `location`
    /// plus `locationAddress` (`schema/td_api.tl:4633`) joined as
    /// "country, state, city, street".
    Location {
        location: GeoLocation,
        address: String,
    },
    /// `storyAreaTypeVenue` (`schema/td_api.tl:6539`) — same fields the
    /// message-venue parser keeps.
    Venue {
        title: String,
        address: String,
        location: GeoLocation,
    },
    /// `storyAreaTypeSuggestedReaction` (`schema/td_api.tl:6546`) — the
    /// emoji plus how often it was added. Same call as Phase 9.2: only
    /// `reactionTypeEmoji` renders; custom/paid reactions are dropped at
    /// parse time.
    SuggestedReaction { emoji: String, total_count: i32 },
    /// `storyAreaTypeMessage` (`schema/td_api.tl:6549`).
    Message { chat_id: i64, message_id: i64 },
    /// `storyAreaTypeLink` (`schema/td_api.tl:6552`) — HTTP or `tg://`;
    /// only HTTP passes the platform open gate (see
    /// `crate::platform::open_external_url`).
    Link { url: String },
    /// `storyAreaTypeWeather` (`schema/td_api.tl:6558`) — Celsius
    /// temperature and the weather emoji. `background_color` is dropped.
    Weather { temperature: f64, emoji: String },
    /// `storyAreaTypeUpgradedGift` (`schema/td_api.tl:6562`) — unique
    /// name of the upgraded gift.
    Gift { gift_name: String },
    /// Unknown/future `@type` values: kept so the tap still hit-tests
    /// and says it's unsupported, instead of silently dropping.
    Unsupported { type_name: String },
}

/// Phase 9.8: `story.areas` — `vector<storyArea>` (`schema/td_api.tl:6566`).
/// Unknown position/type payloads are dropped; unknown *types* keep a
/// placeholder so the tap hit-tests.
pub(crate) fn parse_story_areas(value: Option<&Value>) -> Vec<StoryAreaView> {
    value
        .and_then(Value::as_array)
        .map(|areas| areas.iter().filter_map(parse_story_area).collect())
        .unwrap_or_default()
}

fn parse_story_area(value: &Value) -> Option<StoryAreaView> {
    // Same null guard as `parse_location_address`: a present-but-null
    // or non-object position is dropped with the area.
    let position = value
        .get("position")
        .filter(|position| position.is_object())?;
    Some(StoryAreaView {
        x: position
            .get("x_percentage")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        y: position
            .get("y_percentage")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        width: position
            .get("width_percentage")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        height: position
            .get("height_percentage")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        kind: parse_story_area_kind(value.get("type")?)?,
    })
}

fn parse_story_area_kind(value: &Value) -> Option<StoryAreaKind> {
    match value.get("@type").and_then(Value::as_str) {
        Some("storyAreaTypeLocation") => {
            let location = geo_location(value.get("location"))?;
            Some(StoryAreaKind::Location {
                location,
                address: parse_location_address(value.get("address")),
            })
        }
        Some("storyAreaTypeVenue") => {
            let venue = value.get("venue")?;
            Some(StoryAreaKind::Venue {
                title: venue
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                address: venue
                    .get("address")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                location: geo_location(venue.get("location"))?,
            })
        }
        Some("storyAreaTypeSuggestedReaction") => {
            // Same call as Phase 9.2: only `reactionTypeEmoji` renders;
            // custom-emoji and paid reactions are dropped.
            let reaction = value.get("reaction_type")?;
            if reaction.get("@type").and_then(Value::as_str) != Some("reactionTypeEmoji") {
                return None;
            }
            let emoji = reaction
                .get("emoji")
                .and_then(Value::as_str)
                .filter(|emoji| !emoji.is_empty())?;
            Some(StoryAreaKind::SuggestedReaction {
                emoji: emoji.to_string(),
                total_count: value
                    .get("total_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
            })
        }
        Some("storyAreaTypeMessage") => Some(StoryAreaKind::Message {
            chat_id: int53(value.get("chat_id")).ok()?,
            message_id: int53(value.get("message_id")).ok()?,
        }),
        Some("storyAreaTypeLink") => {
            let url = value
                .get("url")
                .and_then(Value::as_str)
                .filter(|url| !url.is_empty())?;
            Some(StoryAreaKind::Link {
                url: url.to_string(),
            })
        }
        Some("storyAreaTypeWeather") => Some(StoryAreaKind::Weather {
            temperature: value
                .get("temperature")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            emoji: value
                .get("emoji")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        Some("storyAreaTypeUpgradedGift") => {
            let gift_name = value
                .get("gift_name")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())?;
            Some(StoryAreaKind::Gift {
                gift_name: gift_name.to_string(),
            })
        }
        Some(type_name) => Some(StoryAreaKind::Unsupported {
            type_name: type_name.to_string(),
        }),
        None => None,
    }
}

/// Phase 9.8: `locationAddress` (`schema/td_api.tl:4633`) — join the
/// non-empty parts as "country, state, city, street".
fn parse_location_address(value: Option<&Value>) -> String {
    let value = match value.filter(|value| !value.is_null()) {
        Some(value) => value,
        None => return String::new(),
    };
    ["country_code", "state", "city", "street"]
        .iter()
        .filter_map(|key| value.get(key).and_then(Value::as_str))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod story_areas_tests {
    use super::*;
    use crate::telegram::envelope::{EnvelopePayload, parse_envelope};

    #[test]
    fn story_areas_parse_all_types() {
        // `story.areas` (schema 1.8.67 line 6566): one of every
        // `StoryAreaType`, plus an unknown type and a custom-emoji
        // suggested reaction (dropped, same call as Phase 9.2).
        let area = |position: &str, typ: &str| -> String {
            format!(r#"{{"@type":"storyArea","position":{position},"type":{typ}}}"#)
        };
        let position = r#"{"@type":"storyAreaPosition","x_percentage":0.1,"y_percentage":0.2,"width_percentage":0.3,"height_percentage":0.05,"rotation_angle":0.0,"corner_radius_percentage":0.0}"#;
        let areas = [
            area(
                position,
                r#"{"@type":"storyAreaTypeLocation","location":{"@type":"location","latitude":37.7955,"longitude":-122.3937,"horizontal_accuracy":0.0},"address":{"@type":"locationAddress","country_code":"US","state":"CA","city":"San Francisco","street":"1 Ferry Building"}}"#,
            ),
            area(
                position,
                r#"{"@type":"storyAreaTypeVenue","venue":{"@type":"venue","location":{"@type":"location","latitude":37.7955,"longitude":-122.3937,"horizontal_accuracy":0.0},"title":"Ferry Building","address":"1 Ferry Building, San Francisco","provider":"foursquare","id":"x","type":"Food"}}"#,
            ),
            area(
                position,
                r#"{"@type":"storyAreaTypeSuggestedReaction","reaction_type":{"@type":"reactionTypeEmoji","emoji":"🔥"},"total_count":12,"is_dark":false,"is_flipped":false}"#,
            ),
            // Custom-emoji suggested reaction: dropped.
            area(
                position,
                r#"{"@type":"storyAreaTypeSuggestedReaction","reaction_type":{"@type":"reactionTypeCustomEmoji","custom_emoji_id":"123"},"total_count":3,"is_dark":false,"is_flipped":false}"#,
            ),
            area(
                position,
                r#"{"@type":"storyAreaTypeMessage","chat_id":11,"message_id":42}"#,
            ),
            area(
                position,
                r#"{"@type":"storyAreaTypeLink","url":"https://t.me/quill"}"#,
            ),
            area(
                position,
                r#"{"@type":"storyAreaTypeWeather","temperature":21.5,"emoji":"☀️","background_color":-16777216}"#,
            ),
            area(
                position,
                r#"{"@type":"storyAreaTypeUpgradedGift","gift_name":"Ion Gem"}"#,
            ),
            area(position, r#"{"@type":"storyAreaTypeFuture"}"#),
        ]
        .join(",");
        let json = format!(
            r#"{{"@type":"story","id":5,"poster_chat_id":11,"date":1700000000,"content":{{"@type":"storyContentPhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[]}}}},"areas":[{areas}],"caption":{{"@type":"formattedText","text":"","entities":[]}}}}"#,
        );
        let env = parse_envelope(&json).unwrap();
        match env.payload {
            EnvelopePayload::Story { story, .. } => {
                assert_eq!(story.areas.len(), 8);
                let area = &story.areas[0];
                assert_eq!(
                    (area.x, area.y, area.width, area.height),
                    (0.1, 0.2, 0.3, 0.05)
                );
                match &area.kind {
                    StoryAreaKind::Location { location, address } => {
                        assert!((location.latitude() - 37.7955).abs() < 1e-9);
                        assert!((location.longitude() + 122.3937).abs() < 1e-9);
                        assert_eq!(address, "US, CA, San Francisco, 1 Ferry Building");
                    }
                    other => panic!("{other:?}"),
                }
                match &story.areas[1].kind {
                    StoryAreaKind::Venue {
                        title,
                        address,
                        location,
                    } => {
                        assert_eq!(title, "Ferry Building");
                        assert_eq!(address, "1 Ferry Building, San Francisco");
                        assert!((location.latitude() - 37.7955).abs() < 1e-9);
                    }
                    other => panic!("{other:?}"),
                }
                assert_eq!(
                    story.areas[2].kind,
                    StoryAreaKind::SuggestedReaction {
                        emoji: "🔥".to_string(),
                        total_count: 12,
                    }
                );
                // index 3 is the dropped custom-emoji reaction; the
                // message area shifts into its slot.
                assert_eq!(
                    story.areas[3].kind,
                    StoryAreaKind::Message {
                        chat_id: 11,
                        message_id: 42,
                    }
                );
                assert_eq!(
                    story.areas[4].kind,
                    StoryAreaKind::Link {
                        url: "https://t.me/quill".to_string(),
                    }
                );
                assert_eq!(
                    story.areas[5].kind,
                    StoryAreaKind::Weather {
                        temperature: 21.5,
                        emoji: "☀️".to_string(),
                    }
                );
                assert_eq!(
                    story.areas[6].kind,
                    StoryAreaKind::Gift {
                        gift_name: "Ion Gem".to_string(),
                    }
                );
                assert_eq!(
                    story.areas[7].kind,
                    StoryAreaKind::Unsupported {
                        type_name: "storyAreaTypeFuture".to_string(),
                    }
                );
            }
            other => panic!("{other:?}"),
        }
    }
}
