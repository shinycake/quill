//! Request builders for the shared-media gallery's profile tabs.
use crate::ids::RequestId;
use serde_json::json;

/// Message-count filters the gallery's tab bar needs on top of
/// `MEDIA_COUNT_FILTERS` (its keys continue at 7): polls, and voice
/// messages together with round videos (tdesktop's `RoundVoiceFile`).
pub const EXTRA_COUNT_FILTERS: [&str; 2] = [
    "searchMessagesFilterPoll",
    "searchMessagesFilterVoiceAndVideoNote",
];

/// `getUserProfileAudios user_id:int53 offset:int32 limit:int32 = Audios`
/// (schema 1.8.68, line 14992): the tracks a user saved to their profile.
pub fn get_user_profile_audios(extra: RequestId, user_id: i64, offset: i32, limit: i32) -> String {
    json!({
        "@type": "getUserProfileAudios",
        "@extra": extra.as_extra(),
        "user_id": user_id,
        "offset": offset,
        "limit": limit,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_audios_request_shape() {
        let req: serde_json::Value =
            serde_json::from_str(&get_user_profile_audios(RequestId(3), 42, 20, 20)).unwrap();
        assert_eq!(req["@type"], "getUserProfileAudios");
        assert_eq!(req["user_id"], 42);
        assert_eq!(req["offset"], 20);
        assert_eq!(req["limit"], 20);
    }

    #[test]
    fn extra_filters_are_real_constructors() {
        let schema = include_str!("../../../schema/td_api.tl");
        for filter in EXTRA_COUNT_FILTERS {
            assert!(schema.contains(&format!("{filter} = SearchMessagesFilter;")));
        }
    }
}
