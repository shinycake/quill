//! Local music files for `inputMessageAudio` (TDLib 1.8.67).
//!
//! tdesktop’s attach path treats a music file as audio, not a voice note and
//! not a generic document: ffmpeg reads duration, title, and performer, and
//! an embedded album picture becomes the cover thumbnail. Unigram sends the
//! same shape as `InputMessageAudio` / `InputAudio` / `InputFileLocal`.
//!
//! `inputAudio` fields are `audio`, `album_cover_thumbnail`, `duration`,
//! `title` (0–64), and `performer` (0–64). There is no `file_name` or
//! `mime_type` on the send constructor — those exist on the received `audio`
//! object. The picked path’s name is what TDLib stores as the file name, and
//! the server detects the MIME type.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Probed fields that exist on `inputAudio`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioProbe {
    pub duration: i32,
    pub title: String,
    pub performer: String,
}

/// JPEG album picture for `inputAudio.album_cover_thumbnail`.
/// Width and height stay at most 320 (schema: usually shouldn't exceed 320).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioCover {
    pub path: PathBuf,
    pub width: i32,
    pub height: i32,
}

const TAG_LIMIT: usize = 64;

/// Read duration, title, and performer from a user-picked local audio file.
/// Title and performer come from container tags (`title`, `artist` /
/// `album_artist` / `performer`) and are truncated to 64 characters. Missing
/// tags stay empty; the history row then shows the file name.
pub fn probe_local_audio(path: &Path) -> Result<AudioProbe, String> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration:format_tags=title,artist,album_artist,performer",
            "-show_entries",
            "stream=codec_type,duration:stream_disposition=attached_pic",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|_| "ffprobe is not available".to_string())?;
    if !output.status.success() {
        return Err("could not read audio".into());
    }
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|_| "could not read audio".to_string())?;
    probe_from_ffprobe_json(&value)
}

fn probe_from_ffprobe_json(value: &serde_json::Value) -> Result<AudioProbe, String> {
    let streams = value
        .get("streams")
        .and_then(|streams| streams.as_array())
        .ok_or_else(|| "could not read audio".to_string())?;
    let has_audio = streams
        .iter()
        .any(|stream| stream.get("codec_type").and_then(|kind| kind.as_str()) == Some("audio"));
    if !has_audio {
        return Err("unsupported audio".into());
    }
    let has_real_video = streams.iter().any(|stream| {
        stream.get("codec_type").and_then(|kind| kind.as_str()) == Some("video")
            && stream
                .get("disposition")
                .and_then(|disp| disp.get("attached_pic"))
                .and_then(|flag| flag.as_i64())
                != Some(1)
    });
    if has_real_video {
        return Err("file is a video, not an audio track".into());
    }
    let format = value.get("format");
    let duration = streams
        .iter()
        .find(|stream| stream.get("codec_type").and_then(|kind| kind.as_str()) == Some("audio"))
        .and_then(|stream| json_seconds(stream.get("duration")))
        .or_else(|| json_seconds(format.and_then(|format| format.get("duration"))))
        .ok_or_else(|| "could not read audio duration".to_string())?;
    let tags = format.and_then(|format| format.get("tags"));
    let title = tag_text(tags, &["title"]);
    let performer = tag_text(tags, &["artist", "album_artist", "performer"]);
    Ok(AudioProbe {
        duration: duration_seconds(duration),
        title,
        performer,
    })
}

fn tag_text(tags: Option<&serde_json::Value>, keys: &[&str]) -> String {
    let Some(tags) = tags else {
        return String::new();
    };
    for key in keys {
        let found = tags.as_object().and_then(|map| {
            map.iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(key))
                .and_then(|(_, value)| value.as_str())
        });
        if let Some(text) = found {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return trimmed.chars().take(TAG_LIMIT).collect();
            }
        }
    }
    String::new()
}

fn json_seconds(value: Option<&serde_json::Value>) -> Option<f64> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    if let Some(number) = value.as_f64() {
        return number.is_finite().then_some(number);
    }
    let text = value.as_str()?.trim();
    if text.is_empty() || text.eq_ignore_ascii_case("N/A") {
        return None;
    }
    text.parse::<f64>().ok().filter(|number| number.is_finite())
}

fn duration_seconds(seconds: f64) -> i32 {
    if !seconds.is_finite() || seconds < 0.0 {
        return 0;
    }
    let rounded = seconds.round() as i64;
    if seconds > 0.0 && rounded == 0 {
        1
    } else {
        i32::try_from(rounded).unwrap_or(i32::MAX)
    }
}

/// Embedded album picture as a JPEG, or `None` when the file has no cover
/// (schema: pass null to skip thumbnail uploading).
pub fn write_album_cover(src: &Path) -> Option<AudioCover> {
    let dir = std::env::temp_dir().join("quill-audio-covers");
    std::fs::create_dir_all(&dir).ok()?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    let dest = dir.join(format!("{}-{nanos}.jpg", std::process::id()));
    let status = Command::new("ffmpeg")
        .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(src)
        .args([
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-vf",
            "scale=320:320:force_original_aspect_ratio=decrease",
            "-q:v",
            "5",
        ])
        .arg(&dest)
        .status()
        .ok()?;
    if !status.success() || !dest.is_file() {
        let _ = std::fs::remove_file(&dest);
        return None;
    }
    let (width, height) = jpeg_size(&dest).unwrap_or((0, 0));
    Some(AudioCover {
        path: dest,
        width,
        height,
    })
}

/// MIME type for a received `audio.mime_type` demo row. Not a field on
/// `inputAudio`; TDLib detects it from the uploaded file.
pub fn mime_type_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "mp3" => "audio/mpeg",
        "m4a" | "mp4" | "aac" => "audio/mp4",
        "flac" => "audio/flac",
        "ogg" | "oga" => "audio/ogg",
        "opus" => "audio/opus",
        "wav" => "audio/wav",
        _ => "audio/mpeg",
    }
}

fn jpeg_size(path: &Path) -> Option<(i32, i32)> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let stream = value.get("streams")?.get(0)?;
    let width = i32::try_from(stream.get("width")?.as_i64()?).ok()?;
    let height = i32::try_from(stream.get("height")?.as_i64()?).ok()?;
    Some((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/screenshots/fixtures/demo-track.mp3")
    }

    #[test]
    fn probe_reads_duration_title_and_performer() {
        let probe = probe_local_audio(&fixture()).unwrap();
        assert_eq!(probe.duration, 2);
        assert_eq!(probe.title, "Night Drive");
        assert_eq!(probe.performer, "Ada Lovelace");
    }

    #[test]
    fn tags_truncate_to_64_characters() {
        let long = "あ".repeat(80);
        let json = serde_json::json!({
            "streams": [{ "codec_type": "audio", "duration": "3.2" }],
            "format": { "duration": "3.2", "tags": { "title": long, "artist": "Ada" } }
        });
        let probe = probe_from_ffprobe_json(&json).unwrap();
        assert_eq!(probe.title.chars().count(), 64);
        assert_eq!(probe.performer, "Ada");
        assert_eq!(probe.duration, 3);
    }

    #[test]
    fn video_file_is_not_an_audio_track() {
        let json = serde_json::json!({
            "streams": [
                { "codec_type": "video", "disposition": { "attached_pic": 0 } },
                { "codec_type": "audio", "duration": "1.0" }
            ],
            "format": { "duration": "1.0" }
        });
        assert!(probe_from_ffprobe_json(&json).is_err());
    }

    #[test]
    fn album_cover_is_a_jpeg_when_embedded() {
        let cover = write_album_cover(&fixture()).expect("embedded cover");
        assert!(cover.path.is_file());
        assert!(cover.width > 0 && cover.width <= 320);
        assert!(cover.height > 0 && cover.height <= 320);
        let _ = std::fs::remove_file(&cover.path);
    }

    #[test]
    fn mime_follows_extension() {
        assert_eq!(mime_type_for_path(Path::new("night.mp3")), "audio/mpeg");
        assert_eq!(mime_type_for_path(Path::new("night.m4a")), "audio/mp4");
        assert_eq!(mime_type_for_path(Path::new("night.flac")), "audio/flac");
        assert_eq!(mime_type_for_path(Path::new("night.ogg")), "audio/ogg");
    }
}
