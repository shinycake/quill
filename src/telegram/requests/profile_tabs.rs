//! Request builders for the shared-media gallery's profile tabs.
use super::{message_send_options, message_topic_value, send_reply_value};
use crate::composer::SendOptions;
use crate::ids::{ChatId, RequestId};
use serde_json::{Value, json};

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

/// `isProfileAudio file_id:int32 = Ok` (schema 1.8.68, line 14995): whether
/// a file is among the current user's saved music. TDLib answers with a 404
/// error when it is not (tdesktop reads `account.getSavedMusicIds`).
pub fn is_profile_audio(extra: RequestId, file_id: i32) -> String {
    json!({
        "@type": "isProfileAudio",
        "@extra": extra.as_extra(),
        "file_id": file_id,
    })
    .to_string()
}

/// `removeProfileAudio file_id:int32 = Ok` (schema 1.8.68, line 15006):
/// tdesktop's `SavedMusic::remove`.
pub fn remove_profile_audio(extra: RequestId, file_id: i32) -> String {
    json!({
        "@type": "removeProfileAudio",
        "@extra": extra.as_extra(),
        "file_id": file_id,
    })
    .to_string()
}

/// `setProfileAudioPosition file_id:int32 after_file_id:int32 = Ok` (schema
/// 1.8.68, line 15003): tdesktop's `SavedMusic::reorder`. `after_file_id` 0
/// moves the track to the beginning.
pub fn set_profile_audio_position(extra: RequestId, file_id: i32, after_file_id: i32) -> String {
    json!({
        "@type": "setProfileAudioPosition",
        "@extra": extra.as_extra(),
        "file_id": file_id,
        "after_file_id": after_file_id,
    })
    .to_string()
}

/// `sendMessage` + `inputMessageAudio` with `inputFileId`: a saved track
/// sent as a new audio message. tdesktop's "Save to Saved Messages" on the
/// Playlist page forwards the track's document to yourself; a track has no
/// message to forward in TDLib, so the same file goes out as a new message.
pub fn send_saved_audio(
    extra: RequestId,
    chat_id: ChatId,
    file_id: i32,
    duration: i32,
    title: &str,
    performer: &str,
) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(None),
        "reply_to": send_reply_value(None),
        "options": message_send_options(&SendOptions::default()),
        "input_message_content": {
            "@type": "inputMessageAudio",
            "audio": {
                "@type": "inputAudio",
                "audio": { "@type": "inputFileId", "id": file_id },
                "album_cover_thumbnail": Value::Null,
                "duration": duration,
                "title": title,
                "performer": performer,
            },
            "caption": { "@type": "formattedText", "text": "", "entities": [] },
        },
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_track_is_sent_by_its_file_id() {
        let req: Value =
            serde_json::from_str(&send_saved_audio(RequestId(4), ChatId(7), 9, 120, "T", "P"))
                .unwrap();
        assert_eq!(req["@type"], "sendMessage");
        assert_eq!(req["chat_id"], 7);
        let content = &req["input_message_content"];
        assert_eq!(content["@type"], "inputMessageAudio");
        assert_eq!(content["audio"]["audio"]["@type"], "inputFileId");
        assert_eq!(content["audio"]["audio"]["id"], 9);
        assert_eq!(content["audio"]["duration"], 120);
    }

    #[test]
    fn saved_music_management_requests_match_the_schema() {
        let schema = include_str!("../../../schema/td_api.tl");
        for line in [
            "isProfileAudio file_id:int32 = Ok;",
            "removeProfileAudio file_id:int32 = Ok;",
            "setProfileAudioPosition file_id:int32 after_file_id:int32 = Ok;",
        ] {
            assert!(schema.contains(line), "{line}");
        }
        let is: serde_json::Value =
            serde_json::from_str(&is_profile_audio(RequestId(1), 9)).unwrap();
        assert_eq!(is["@type"], "isProfileAudio");
        assert_eq!(is["file_id"], 9);
        let remove: serde_json::Value =
            serde_json::from_str(&remove_profile_audio(RequestId(2), 9)).unwrap();
        assert_eq!(remove["@type"], "removeProfileAudio");
        assert_eq!(remove["file_id"], 9);
        let moved: serde_json::Value =
            serde_json::from_str(&set_profile_audio_position(RequestId(3), 9, 4)).unwrap();
        assert_eq!(moved["@type"], "setProfileAudioPosition");
        assert_eq!(moved["file_id"], 9);
        assert_eq!(moved["after_file_id"], 4);
    }

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
