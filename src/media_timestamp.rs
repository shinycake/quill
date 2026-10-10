//! What a media-timestamp link (`1:23` in a caption or a reply) seeks.
//!
//! Telegram Desktop turns the link into a seek of the message's own
//! voice message, song or video, of the one it replies to, or of a
//! YouTube preview, which opens with the start time appended
//! (`history_view_media.cpp`: `DurationForTimestampLinks`,
//! `TimestampLinkBase`; `local_url_handlers.cpp`: `OpenMediaTimestamp`).
//! TDLib already marks the timestamps in the text, so only the target is
//! decided here.

use crate::telegram::envelope::{LinkPreview, LinkPreviewKind, MessageContent};

/// What a timestamp link acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeekTarget {
    VoiceNote,
    Audio,
    /// A video, opened in the viewer at the moment.
    Video,
    /// A video page in the browser, at the URL carrying the moment.
    Web(String),
}

/// The target `content` offers for a seek to `seconds`, if any.
pub fn seek_target(content: &MessageContent, seconds: i32) -> Option<SeekTarget> {
    match content {
        MessageContent::VoiceNote(_) => Some(SeekTarget::VoiceNote),
        MessageContent::Audio(_) => Some(SeekTarget::Audio),
        MessageContent::Video(_) => Some(SeekTarget::Video),
        MessageContent::Text(text) => text
            .link_preview
            .as_ref()
            .and_then(|preview| youtube_start_url(preview, seconds))
            .map(SeekTarget::Web),
        _ => None,
    }
}

/// Where a seek lands in a clip `duration` seconds long (0 when unknown,
/// which leaves the moment alone).
pub fn clamp_seek(seconds: i32, duration: i32) -> f64 {
    let seconds = seconds.max(0);
    let seconds = if duration > 0 {
        seconds.min(duration)
    } else {
        seconds
    };
    f64::from(seconds)
}

/// The preview's video page with `t=<seconds>` as its start time, for
/// YouTube video previews only (Desktop does the same).
pub fn youtube_start_url(preview: &LinkPreview, seconds: i32) -> Option<String> {
    if !matches!(
        preview.kind,
        LinkPreviewKind::EmbeddedPlayer { audio: false, .. }
    ) {
        return None;
    }
    let youtube = preview.site_name.eq_ignore_ascii_case("youtube") || is_youtube(&preview.url);
    youtube.then(|| with_start_time(&preview.url, seconds))
}

fn is_youtube(url: &str) -> bool {
    let host = url
        .split_once("://")
        .map_or(url, |(_, rest)| rest)
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    host == "youtu.be" || host == "youtube.com" || host.ends_with(".youtube.com")
}

/// `url` with its `t` parameter set to `seconds`, replacing an earlier
/// one and keeping the fragment.
pub fn with_start_time(url: &str, seconds: i32) -> String {
    let (base, fragment) = match url.split_once('#') {
        Some((base, fragment)) => (base, Some(fragment)),
        None => (url, None),
    };
    let (path, query) = base.split_once('?').unwrap_or((base, ""));
    let mut params: Vec<&str> = query
        .split('&')
        .filter(|param| !param.is_empty() && !param.starts_with("t="))
        .collect();
    let time = format!("t={}", seconds.max(0));
    params.push(&time);
    let mut out = format!("{path}?{}", params.join("&"));
    if let Some(fragment) = fragment {
        out.push('#');
        out.push_str(fragment);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{LinkPreview, LinkPreviewKind, clamp_seek, with_start_time, youtube_start_url};

    fn preview(url: &str, site: &str, audio: bool) -> LinkPreview {
        LinkPreview {
            url: url.into(),
            display_url: url.into(),
            site_name: site.into(),
            title: String::new(),
            description: String::new(),
            show_large_media: false,
            has_large_media: false,
            show_media_above_description: false,
            show_above_text: false,
            instant_view_version: 0,
            photo: None,
            kind: LinkPreviewKind::EmbeddedPlayer {
                url: url.into(),
                duration_secs: 0,
                audio,
            },
            view_button: None,
        }
    }

    #[test]
    fn start_time_replaces_an_earlier_one_and_keeps_the_fragment() {
        assert_eq!(
            with_start_time("https://youtu.be/abc", 90),
            "https://youtu.be/abc?t=90"
        );
        assert_eq!(
            with_start_time("https://www.youtube.com/watch?v=abc&t=5&x=1#top", 90),
            "https://www.youtube.com/watch?v=abc&x=1&t=90#top"
        );
        assert_eq!(
            with_start_time("https://youtu.be/abc?t=5", -4),
            "https://youtu.be/abc?t=0"
        );
    }

    #[test]
    fn only_youtube_video_previews_take_a_start_time() {
        let video = preview("https://www.youtube.com/watch?v=abc", "YouTube", false);
        assert_eq!(
            youtube_start_url(&video, 75).as_deref(),
            Some("https://www.youtube.com/watch?v=abc&t=75")
        );
        // Named by host alone.
        let short = preview("https://youtu.be/abc", "", false);
        assert!(youtube_start_url(&short, 1).is_some());
        // Other sites and audio players are left alone.
        assert!(youtube_start_url(&preview("https://vimeo.com/1", "Vimeo", false), 1).is_none());
        assert!(youtube_start_url(&preview("https://notyoutube.com/1", "", false), 1).is_none());
        assert!(youtube_start_url(&preview("https://youtu.be/abc", "YouTube", true), 1).is_none());
    }

    #[test]
    fn seeks_are_clamped_to_the_clip() {
        assert_eq!(clamp_seek(30, 120), 30.0);
        assert_eq!(clamp_seek(500, 120), 120.0);
        assert_eq!(clamp_seek(-5, 120), 0.0);
        assert_eq!(clamp_seek(500, 0), 500.0);
    }
}
