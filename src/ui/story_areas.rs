//! `ReadyStoryAreas` screenshot-demo fixture (Phase 9.8): inject a
//! photo story on Demo chat A (id 11) carrying one of every
//! `storyAreaType`, through the real reducer.

use super::demo::{demo_file_json, demo_thumb_png_path};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
/// `ReadyStoryAreas` fixture: inject a photo story on Demo chat A (id 11)
/// carrying one of every `storyAreaType` — location, venue, suggested
/// reaction, message, link, weather, gift — through the real reducer, no
/// live Telegram. The viewer opens on it; the chips show the area labels
/// and each tap performs its action.
pub(crate) fn apply_ready_story_areas(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let photo_file = demo_file_json(91, &demo_thumb_png_path(), true);
    let caption = |text: &str| -> String {
        format!(
            r#"{{"@type":"formattedText","text":{},"entities":[]}}"#,
            serde_json::to_string(text).unwrap()
        )
    };
    let tray = r#"{"@type":"updateChatActiveStories","active_stories":{"@type":"chatActiveStories","chat_id":11,"list":{"@type":"storyListMain"},"order":"30","can_be_archived":false,"max_read_story_id":4,"stories":[{"@type":"storyInfo","story_id":5,"date":1700000000,"is_for_close_friends":false,"is_live":false}]}}"#.to_string();
    let areas = concat!(
        r#"{"@type":"storyArea","position":{"@type":"storyAreaPosition","x_percentage":0.31,"y_percentage":0.11,"width_percentage":0.5,"height_percentage":0.06,"rotation_angle":0.0,"corner_radius_percentage":0.5},"type":{"@type":"storyAreaTypeLocation","location":{"@type":"location","latitude":37.7955,"longitude":-122.3937,"horizontal_accuracy":0.0},"address":{"@type":"locationAddress","country_code":"US","state":"CA","city":"San Francisco","street":"1 Ferry Building"}}},"#,
        r#"{"@type":"storyArea","position":{"@type":"storyAreaPosition","x_percentage":0.69,"y_percentage":0.19,"width_percentage":0.5,"height_percentage":0.06,"rotation_angle":0.0,"corner_radius_percentage":0.5},"type":{"@type":"storyAreaTypeVenue","venue":{"@type":"venue","location":{"@type":"location","latitude":37.7955,"longitude":-122.3937,"horizontal_accuracy":0.0},"title":"Ferry Building","address":"1 Ferry Building, San Francisco","provider":"foursquare","id":"4a1a2b3c","type":"Food"}}},"#,
        r#"{"@type":"storyArea","position":{"@type":"storyAreaPosition","x_percentage":0.21,"y_percentage":0.265,"width_percentage":0.3,"height_percentage":0.05,"rotation_angle":0.0,"corner_radius_percentage":0.5},"type":{"@type":"storyAreaTypeSuggestedReaction","reaction_type":{"@type":"reactionTypeEmoji","emoji":"🔥"},"total_count":12,"is_dark":false,"is_flipped":false}},"#,
        r#"{"@type":"storyArea","position":{"@type":"storyAreaPosition","x_percentage":0.79,"y_percentage":0.265,"width_percentage":0.3,"height_percentage":0.05,"rotation_angle":0.0,"corner_radius_percentage":0.5},"type":{"@type":"storyAreaTypeMessage","chat_id":11,"message_id":42}},"#,
        r#"{"@type":"storyArea","position":{"@type":"storyAreaPosition","x_percentage":0.21,"y_percentage":0.345,"width_percentage":0.3,"height_percentage":0.05,"rotation_angle":0.0,"corner_radius_percentage":0.5},"type":{"@type":"storyAreaTypeLink","url":"https://t.me/quill"}},"#,
        r#"{"@type":"storyArea","position":{"@type":"storyAreaPosition","x_percentage":0.79,"y_percentage":0.345,"width_percentage":0.3,"height_percentage":0.05,"rotation_angle":0.0,"corner_radius_percentage":0.5},"type":{"@type":"storyAreaTypeWeather","temperature":21.5,"emoji":"☀️","background_color":-16777216}},"#,
        r#"{"@type":"storyArea","position":{"@type":"storyAreaPosition","x_percentage":0.31,"y_percentage":0.425,"width_percentage":0.5,"height_percentage":0.05,"rotation_angle":0.0,"corner_radius_percentage":0.5},"type":{"@type":"storyAreaTypeUpgradedGift","gift_name":"Ion Gem"}}"#,
    );
    let story = format!(
        r#"{{"@type":"story","id":5,"poster_chat_id":11,"date":1700000000,"content":{{"@type":"storyContentPhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"y","photo":{photo_file},"width":960,"height":1280,"progressive_sizes":[]}}]}}}},"areas":[{areas}],"caption":{}}}"#,
        caption(
            "Phase 9.8: every story area type as a clickable chip — \
             location, venue, suggested reaction, message, link, weather, gift.",
        ),
    );
    for json in [tray, story] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}
