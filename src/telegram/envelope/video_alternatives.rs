//! `messageVideo.alternative_videos`: the other qualities of a video the
//! viewer's quality picker offers (`alternativeVideo id width height codec
//! hls_file video`, schema/td_api.tl).

use super::*;
use crate::ids::FileId;
use serde_json::Value;

/// One alternative rendition of a video. The `video` file is a plain
/// downloadable clip; the HLS playlist is not used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoAlternative {
    pub width: i32,
    pub height: i32,
    /// "h264", "h265", "av1" and so on, as TDLib reports it.
    pub codec: String,
    pub file_id: FileId,
}

/// Parse the `alternative_videos` array. Each usable clip's file is pushed
/// to `files` so its download state tracks like the main clip's.
pub(super) fn parse_video_alternatives(
    value: Option<&Value>,
    files: &mut Vec<ParsedFile>,
) -> Vec<VideoAlternative> {
    let Some(list) = value.and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in list {
        let Ok(file) = parse_file(entry.get("video")) else {
            continue;
        };
        if file.id.0 == 0 {
            continue;
        }
        let file_id = file.id;
        files.push(file);
        out.push(VideoAlternative {
            width: int53_or_zero(entry.get("width")).sat_i32(),
            height: int53_or_zero(entry.get("height")).sat_i32(),
            codec: entry
                .get("codec")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            file_id,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn file(id: i32) -> Value {
        json!({"@type":"file","id":id,"size":10,"expected_size":10,
            "local":{"@type":"localFile","path":"","can_be_downloaded":true,
            "can_be_deleted":false,"is_downloading_active":false,
            "is_downloading_completed":false,"download_offset":0,
            "downloaded_prefix_size":0,"downloaded_size":0},
            "remote":{"@type":"remoteFile","id":"x","unique_id":"y",
            "is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":10}})
    }

    #[test]
    fn parses_clips_and_skips_entries_without_one() {
        let list = json!([
            {"@type":"alternativeVideo","id":1,"width":1280,"height":720,"codec":"h264","hls_file":null,"video":file(41)},
            {"@type":"alternativeVideo","id":2,"width":640,"height":360,"codec":"av1","hls_file":null,"video":null},
        ]);
        let mut files = Vec::new();
        let parsed = parse_video_alternatives(Some(&list), &mut files);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].height, 720);
        assert_eq!(parsed[0].codec, "h264");
        assert_eq!(parsed[0].file_id, FileId(41));
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn absent_list_is_empty() {
        let mut files = Vec::new();
        assert!(parse_video_alternatives(None, &mut files).is_empty());
        assert!(files.is_empty());
    }
}
