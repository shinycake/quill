//! `audios` (`getUserProfileAudios`, schema line 566): the tracks a user
//! saved to their profile, listed in the gallery's Playlist tab.
use super::*;
use crate::ids::FileId;
use serde_json::Value;

/// One saved track. Only what the list shows is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileAudio {
    pub file_id: FileId,
    pub title: String,
    pub performer: String,
    pub file_name: String,
    pub duration: i32,
}

/// One page of `audios`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileAudiosPage {
    pub total_count: i32,
    pub audios: Vec<ProfileAudio>,
    /// Files of the tracks, for the downloader.
    pub files: Vec<ParsedFile>,
}

pub(crate) fn parse_profile_audios(value: &Value) -> ProfileAudiosPage {
    let mut page = ProfileAudiosPage {
        total_count: json_i32(value.get("total_count"), 0),
        ..Default::default()
    };
    let Some(entries) = value.get("audios").and_then(Value::as_array) else {
        return page;
    };
    for entry in entries {
        // The `audio` object has the same shape inside `messageAudio`.
        let wrapped = serde_json::json!({ "audio": entry });
        let (content, files) = parse_message_audio(&wrapped);
        if let MessageContent::Audio(audio) = content {
            page.audios.push(ProfileAudio {
                file_id: audio.file_id,
                title: audio.title,
                performer: audio.performer,
                file_name: audio.file_name,
                duration: audio.duration,
            });
            page.files.extend(files);
        }
    }
    page
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_tracks_and_total() {
        let page = parse_profile_audios(&json!({
            "@type": "audios",
            "total_count": 3,
            "audios": [
                {"@type": "audio", "duration": 214, "title": "Night Drive", "performer": "Ada",
                 "file_name": "night.mp3", "mime_type": "audio/mpeg",
                 "audio": {"@type": "file", "id": 41, "size": 10, "expected_size": 10,
                           "local": {"@type": "localFile", "path": "", "is_downloading_completed": false,
                                     "is_downloading_active": false},
                           "remote": {"@type": "remoteFile", "id": "x"}}},
                {"@type": "audio", "duration": 5, "title": "", "performer": "", "file_name": "b.ogg"}
            ]
        }));
        assert_eq!(page.total_count, 3);
        assert_eq!(page.audios.len(), 2);
        assert_eq!(page.audios[0].title, "Night Drive");
        assert_eq!(page.audios[0].performer, "Ada");
        assert_eq!(page.audios[0].duration, 214);
        assert_eq!(page.audios[0].file_id.0, 41);
        assert_eq!(page.audios[1].file_name, "b.ogg");
        assert_eq!(page.files.len(), 1);
        assert_eq!(parse_profile_audios(&json!({})).audios.len(), 0);
    }
}
