//! Story statistics, public forwards and public story search request
//! builders (TDLib 1.8.68, `schema/td_api.tl`).

use crate::ids::{ChatId, RequestId};
use serde_json::json;

/// `getStoryStatistics chat_id:int53 story_id:int32 is_dark:Bool =
/// StoryStatistics` (schema line 16365). Only valid when
/// `story.can_get_statistics`. Response is `storyStatistics`.
pub fn get_story_statistics(
    extra: RequestId,
    chat_id: ChatId,
    story_id: i32,
    is_dark: bool,
) -> String {
    json!({
        "@type": "getStoryStatistics",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "story_id": story_id,
        "is_dark": is_dark
    })
    .to_string()
}

/// `getStoryPublicForwards story_poster_chat_id:int53 story_id:int32
/// offset:string limit:int32 = PublicForwards` (schema line 14241). The
/// limit is 1..=100. Response is `publicForwards`.
pub fn get_story_public_forwards(
    extra: RequestId,
    chat_id: ChatId,
    story_id: i32,
    offset: &str,
    limit: i32,
) -> String {
    json!({
        "@type": "getStoryPublicForwards",
        "@extra": extra.as_extra(),
        "story_poster_chat_id": chat_id.0,
        "story_id": story_id,
        "offset": offset,
        "limit": limit.clamp(1, 100)
    })
    .to_string()
}

/// `searchPublicStoriesByTag story_poster_chat_id:int53 tag:string
/// offset:string limit:int32 = FoundStories` (schema line 12323).
/// `poster_chat_id` 0 searches every chat. The tag is sent without the
/// leading `#` or `$`: the schema takes the bare "hashtag or cashtag".
pub fn search_public_stories_by_tag(
    extra: RequestId,
    poster_chat_id: i64,
    tag: &str,
    offset: &str,
    limit: i32,
) -> String {
    json!({
        "@type": "searchPublicStoriesByTag",
        "@extra": extra.as_extra(),
        "story_poster_chat_id": poster_chat_id,
        "tag": tag.trim_start_matches(['#', '$']),
        "offset": offset,
        "limit": limit.clamp(1, 100)
    })
    .to_string()
}

/// A `locationAddress` (schema line 4947).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoryLocationAddress {
    pub country_code: String,
    pub state: String,
    pub city: String,
    pub street: String,
}

/// `searchPublicStoriesByLocation address:locationAddress offset:string
/// limit:int32 = FoundStories` (schema line 12329).
pub fn search_public_stories_by_location(
    extra: RequestId,
    address: &StoryLocationAddress,
    offset: &str,
    limit: i32,
) -> String {
    json!({
        "@type": "searchPublicStoriesByLocation",
        "@extra": extra.as_extra(),
        "address": {
            "@type": "locationAddress",
            "country_code": address.country_code,
            "state": address.state,
            "city": address.city,
            "street": address.street
        },
        "offset": offset,
        "limit": limit.clamp(1, 100)
    })
    .to_string()
}

/// `searchPublicStoriesByVenue venue_provider:string venue_id:string
/// offset:string limit:int32 = FoundStories` (schema line 12336).
pub fn search_public_stories_by_venue(
    extra: RequestId,
    venue_provider: &str,
    venue_id: &str,
    offset: &str,
    limit: i32,
) -> String {
    json!({
        "@type": "searchPublicStoriesByVenue",
        "@extra": extra.as_extra(),
        "venue_provider": venue_provider,
        "venue_id": venue_id,
        "offset": offset,
        "limit": limit.clamp(1, 100)
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        StoryLocationAddress, get_story_public_forwards, get_story_statistics,
        search_public_stories_by_location, search_public_stories_by_tag,
        search_public_stories_by_venue,
    };
    use crate::ids::{ChatId, RequestId};
    use serde_json::Value;

    fn parse(json: &str) -> Value {
        serde_json::from_str(json).expect("valid json")
    }

    #[test]
    fn statistics_and_forwards_shapes() {
        let v = parse(&get_story_statistics(RequestId(1), ChatId(-100), 7, true));
        assert_eq!(v["@type"], "getStoryStatistics");
        assert_eq!(v["chat_id"], -100);
        assert_eq!(v["story_id"], 7);
        assert_eq!(v["is_dark"], true);
        let v = parse(&get_story_public_forwards(
            RequestId(2),
            ChatId(-100),
            7,
            "abc",
            500,
        ));
        assert_eq!(v["@type"], "getStoryPublicForwards");
        assert_eq!(v["story_poster_chat_id"], -100);
        assert_eq!(v["offset"], "abc");
        assert_eq!(v["limit"], 100);
    }

    #[test]
    fn search_shapes() {
        let v = parse(&search_public_stories_by_tag(
            RequestId(3),
            0,
            "#sunset",
            "",
            30,
        ));
        assert_eq!(v["@type"], "searchPublicStoriesByTag");
        assert_eq!(v["story_poster_chat_id"], 0);
        assert_eq!(v["tag"], "sunset");
        let v = parse(&search_public_stories_by_tag(
            RequestId(3),
            0,
            "$TON",
            "",
            0,
        ));
        assert_eq!(v["tag"], "TON");
        assert_eq!(v["limit"], 1);
        let address = StoryLocationAddress {
            country_code: "IL".into(),
            city: "Haifa".into(),
            ..Default::default()
        };
        let v = parse(&search_public_stories_by_location(
            RequestId(4),
            &address,
            "o",
            20,
        ));
        assert_eq!(v["@type"], "searchPublicStoriesByLocation");
        assert_eq!(v["address"]["@type"], "locationAddress");
        assert_eq!(v["address"]["country_code"], "IL");
        assert_eq!(v["address"]["state"], "");
        let v = parse(&search_public_stories_by_venue(
            RequestId(5),
            "foursquare",
            "v1",
            "",
            20,
        ));
        assert_eq!(v["@type"], "searchPublicStoriesByVenue");
        assert_eq!(v["venue_provider"], "foursquare");
        assert_eq!(v["venue_id"], "v1");
    }
}
