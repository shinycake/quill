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
    if let Some(probe) = probe_with_ffprobe(path) {
        return probe;
    }
    // Linux CI has no ffmpeg. MPEG-1 Layer III (the demo track, and the usual
    // picked mp3) is read from the ID3 tag and frame headers, the same way
    // video falls back to MPEG-4 boxes when `ffprobe` is missing.
    probe_mp3(path)
}

fn probe_with_ffprobe(path: &Path) -> Option<Result<AudioProbe, String>> {
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
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    Some(probe_from_ffprobe_json(&value))
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
    if let Some(cover) = write_album_cover_ffmpeg(src) {
        return Some(cover);
    }
    write_album_cover_mp3(src)
}

fn write_album_cover_ffmpeg(src: &Path) -> Option<AudioCover> {
    let dest = cover_dest()?;
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

fn cover_dest() -> Option<PathBuf> {
    let dir = std::env::temp_dir().join("quill-audio-covers");
    std::fs::create_dir_all(&dir).ok()?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(dir.join(format!("{}-{nanos}.jpg", std::process::id())))
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

/// MPEG-1 Layer III: ID3v2 `TIT2` / `TPE1` plus frame-header duration.
/// Used when `ffprobe` is not installed (linux CI).
fn probe_mp3(path: &Path) -> Result<AudioProbe, String> {
    let bytes = std::fs::read(path).map_err(|_| "could not read audio".to_string())?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err("could not read audio".into());
    }
    let (title, performer, audio_at) = id3v2_tags(&bytes).unwrap_or_default();
    let seconds =
        mpeg1_layer3_seconds(&bytes[audio_at..]).ok_or("could not read audio duration")?;
    Ok(AudioProbe {
        duration: duration_seconds(seconds),
        title,
        performer,
    })
}

fn id3v2_tags(bytes: &[u8]) -> Option<(String, String, usize)> {
    if bytes.len() < 10 || &bytes[0..3] != b"ID3" {
        return None;
    }
    let version = bytes[3];
    if version != 3 && version != 4 {
        return None;
    }
    let tag_size = synchsafe(&bytes[6..10])?;
    let end = 10usize.checked_add(tag_size)?;
    if end > bytes.len() {
        return None;
    }
    let mut title = String::new();
    let mut performer = String::new();
    let mut offset = 10usize;
    while offset + 10 <= end {
        let id = &bytes[offset..offset + 4];
        if id == b"\0\0\0\0" {
            break;
        }
        let frame_size = if version == 4 {
            synchsafe(&bytes[offset + 4..offset + 8])?
        } else {
            u32::from_be_bytes(bytes[offset + 4..offset + 8].try_into().ok()?) as usize
        };
        let body_at = offset + 10;
        let body_end = body_at.checked_add(frame_size)?;
        if body_end > end {
            break;
        }
        let text = id3_text(&bytes[body_at..body_end]);
        if id == b"TIT2" && title.is_empty() {
            title = text;
        } else if id == b"TPE1" && performer.is_empty() {
            performer = text;
        }
        offset = body_end;
    }
    Some((title, performer, end))
}

fn synchsafe(bytes: &[u8]) -> Option<usize> {
    if bytes.len() != 4 || bytes.iter().any(|b| b & 0x80 != 0) {
        return None;
    }
    Some(
        ((bytes[0] as usize) << 21)
            | ((bytes[1] as usize) << 14)
            | ((bytes[2] as usize) << 7)
            | (bytes[3] as usize),
    )
}

fn id3_text(body: &[u8]) -> String {
    if body.is_empty() {
        return String::new();
    }
    let encoding = body[0];
    let raw = &body[1..];
    let text = match encoding {
        0 => latin1(raw),
        1 => utf16_bom(raw),
        2 => utf16_be(raw),
        3 => String::from_utf8_lossy(raw).into_owned(),
        _ => return String::new(),
    };
    text.trim_matches('\0')
        .trim()
        .chars()
        .take(TAG_LIMIT)
        .collect()
}

fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|b| char::from(*b)).collect()
}

fn utf16_bom(bytes: &[u8]) -> String {
    if bytes.len() < 2 {
        return String::new();
    }
    let be = bytes[0] == 0xFE && bytes[1] == 0xFF;
    let le = bytes[0] == 0xFF && bytes[1] == 0xFE;
    if !be && !le {
        return String::new();
    }
    decode_utf16(&bytes[2..], be)
}

fn utf16_be(bytes: &[u8]) -> String {
    decode_utf16(bytes, true)
}

fn decode_utf16(bytes: &[u8], be: bool) -> String {
    let (pairs, _) = bytes.as_chunks::<2>();
    let units: Vec<u16> = pairs
        .iter()
        .map(|pair| {
            if be {
                u16::from_be_bytes(*pair)
            } else {
                u16::from_le_bytes(*pair)
            }
        })
        .take_while(|unit| *unit != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

fn mpeg1_layer3_seconds(bytes: &[u8]) -> Option<f64> {
    const BITRATES: [i32; 16] = [
        0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0,
    ];
    const RATES: [i32; 3] = [44100, 48000, 32000];
    let mut offset = 0usize;
    let mut seconds = 0.0f64;
    while offset + 4 <= bytes.len() {
        if bytes[offset] != 0xFF || bytes[offset + 1] & 0xE0 != 0xE0 {
            offset += 1;
            continue;
        }
        let header = u32::from_be_bytes(bytes[offset..offset + 4].try_into().ok()?);
        let version = (header >> 19) & 0b11;
        let layer = (header >> 17) & 0b11;
        let bitrate_i = ((header >> 12) & 0b1111) as usize;
        let rate_i = ((header >> 10) & 0b11) as usize;
        let pad = ((header >> 9) & 1) as i32;
        if version != 0b11 || layer != 0b01 || bitrate_i == 0 || bitrate_i == 15 || rate_i == 3 {
            offset += 1;
            continue;
        }
        let bitrate = BITRATES[bitrate_i] * 1000;
        let rate = RATES[rate_i];
        let frame_len = (144 * bitrate) / rate + pad;
        if frame_len < 4 {
            offset += 1;
            continue;
        }
        let next = offset + frame_len as usize;
        if next > bytes.len() {
            break;
        }
        seconds += 1152.0 / f64::from(rate);
        offset = next;
    }
    (seconds > 0.0).then_some(seconds)
}

fn write_album_cover_mp3(src: &Path) -> Option<AudioCover> {
    let bytes = std::fs::read(src).ok()?;
    if bytes.len() > 32 * 1024 * 1024 {
        return None;
    }
    let jpeg = id3_apic_jpeg(&bytes)?;
    let (width, height) = jpeg_dimensions(&jpeg).unwrap_or((0, 0));
    if width <= 0 || height <= 0 || width > 320 || height > 320 {
        return None;
    }
    let dest = cover_dest()?;
    std::fs::write(&dest, jpeg).ok()?;
    Some(AudioCover {
        path: dest,
        width,
        height,
    })
}

fn id3_apic_jpeg(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() < 10 || &bytes[0..3] != b"ID3" {
        return None;
    }
    let version = bytes[3];
    if version != 3 && version != 4 {
        return None;
    }
    let tag_size = synchsafe(&bytes[6..10])?;
    let end = 10usize.checked_add(tag_size)?;
    if end > bytes.len() {
        return None;
    }
    let mut offset = 10usize;
    while offset + 10 <= end {
        let id = &bytes[offset..offset + 4];
        if id == b"\0\0\0\0" {
            break;
        }
        let frame_size = if version == 4 {
            synchsafe(&bytes[offset + 4..offset + 8])?
        } else {
            u32::from_be_bytes(bytes[offset + 4..offset + 8].try_into().ok()?) as usize
        };
        let body_at = offset + 10;
        let body_end = body_at.checked_add(frame_size)?;
        if body_end > end {
            break;
        }
        if id == b"APIC"
            && let Some(jpeg) = apic_jpeg(&bytes[body_at..body_end])
        {
            return Some(jpeg);
        }
        offset = body_end;
    }
    None
}

fn apic_jpeg(body: &[u8]) -> Option<Vec<u8>> {
    if body.is_empty() {
        return None;
    }
    let encoding = body[0];
    let mime_end = body[1..].iter().position(|b| *b == 0)? + 1;
    let mime = &body[1..mime_end];
    if mime != b"image/jpeg" && mime != b"image/jpg" {
        return None;
    }
    let mut rest = mime_end + 1;
    if rest >= body.len() {
        return None;
    }
    rest += 1;
    match encoding {
        0 | 3 => {
            let end = body[rest..].iter().position(|b| *b == 0)?;
            rest += end + 1;
        }
        1 | 2 => {
            while rest + 1 < body.len() {
                let unit = if encoding == 2 || (body.len() > 2 && body[1] == 0xFE) {
                    u16::from_be_bytes([body[rest], body[rest + 1]])
                } else {
                    u16::from_le_bytes([body[rest], body[rest + 1]])
                };
                rest += 2;
                if unit == 0 {
                    break;
                }
            }
        }
        _ => return None,
    }
    let jpeg = body.get(rest..)?;
    jpeg.starts_with(&[0xFF, 0xD8]).then(|| jpeg.to_vec())
}

fn jpeg_dimensions(bytes: &[u8]) -> Option<(i32, i32)> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut offset = 2usize;
    while offset + 4 < bytes.len() {
        if bytes[offset] != 0xFF {
            return None;
        }
        let marker = bytes[offset + 1];
        if marker == 0xD8 {
            offset += 2;
            continue;
        }
        if marker == 0xD9 || marker == 0xDA {
            return None;
        }
        let len = u16::from_be_bytes([bytes[offset + 2], bytes[offset + 3]]) as usize;
        if len < 2 || offset + 2 + len > bytes.len() {
            return None;
        }
        if matches!(marker, 0xC0..=0xC2) && len >= 7 {
            let height = u16::from_be_bytes([bytes[offset + 5], bytes[offset + 6]]);
            let width = u16::from_be_bytes([bytes[offset + 7], bytes[offset + 8]]);
            return Some((i32::from(width), i32::from(height)));
        }
        offset += 2 + len;
    }
    None
}

fn jpeg_size(path: &Path) -> Option<(i32, i32)> {
    if let Some(size) = std::fs::read(path)
        .ok()
        .and_then(|bytes| jpeg_dimensions(&bytes))
    {
        return Some(size);
    }
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
