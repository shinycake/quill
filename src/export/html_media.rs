//! The media part of a message in the HTML pages: saved files with their
//! thumbnails, tdesktop's notes for files left out, and the plain
//! descriptions of locations, contacts, polls and dice.
use super::html::{Item, Outcome};
use super::model::{Body, MediaRef};
use super::settings::{MediaKind, Skip};
use super::text::{escape, render};

/// tdesktop's `kPhotoMaxWidth` / `kPhotoMaxHeight`: the largest a picture
/// is laid out in a message.
const MEDIA_MAX: i32 = 520;

/// Percent-encode a relative path for use in `href` / `src`.
pub fn url_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~' | b'@') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// `src` for a path relative to the page's own folder.
fn src(path: &str) -> String {
    escape(&url_path(path))
}

fn kind_noun(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Photo => "Photo",
        MediaKind::Video => "Video",
        MediaKind::VoiceMessage => "Voice message",
        MediaKind::VideoMessage => "Video message",
        MediaKind::Sticker => "Sticker",
        MediaKind::Gif => "GIF",
        MediaKind::File => "File",
    }
}

/// tdesktop's wording for a file left out of the export.
pub fn skip_note(kind: MediaKind, skip: Skip) -> String {
    noun_skip_note(kind_noun(kind), skip)
}

/// The same note for any noun ("Photo", "Story", "File").
pub fn noun_skip_note(noun: &str, skip: Skip) -> String {
    match skip {
        Skip::NotIncluded => {
            format!("({noun} not included. Change data exporting settings to download.)")
        }
        Skip::TooLarge => {
            format!("({noun} exceeds maximum size. Change data exporting settings to download.)")
        }
        Skip::Unavailable => format!("({noun} unavailable, please try again later)"),
    }
}

pub fn human_size(bytes: i64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes.max(0) as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{:.1} KB", b / KB)
    } else if b < KB * KB * KB {
        format!("{:.1} MB", b / KB / KB)
    } else {
        format!("{:.2} GB", b / KB / KB / KB)
    }
}

fn extension(path: &str) -> String {
    path.rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// `width="…" height="…"` that fit `MEDIA_MAX`, keeping the aspect ratio,
/// so pages do not jump while pictures load. Empty when the size is unknown.
fn dimensions(media: &MediaRef) -> String {
    let (w, h) = (media.width, media.height);
    if w <= 0 || h <= 0 {
        return String::new();
    }
    let scale = (f64::from(MEDIA_MAX) / f64::from(w.max(h))).min(1.0);
    let fit = |v: i32| ((f64::from(v) * scale).round() as i32).max(1);
    format!(" width=\"{}\" height=\"{}\"", fit(w), fit(h))
}

fn saved_media(media: &MediaRef, path: &str, thumb: Option<&str>) -> String {
    let file = src(path);
    let ext = extension(path);
    let is_image = matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp" | "gif");
    let is_video = matches!(ext.as_str(), "mp4" | "mov" | "webm" | "m4v");
    let is_audio = matches!(ext.as_str(), "ogg" | "oga" | "mp3" | "m4a" | "opus" | "wav");
    let size = dimensions(media);
    let poster = thumb.map_or(String::new(), |t| format!(" poster=\"{}\"", src(t)));
    let label = if media.label.is_empty() {
        String::new()
    } else {
        format!("<div class=\"details\">{}</div>\n", escape(&media.label))
    };
    let body = match media.kind {
        MediaKind::Photo | MediaKind::Sticker | MediaKind::Gif if is_image => format!(
            "<a class=\"media image\" href=\"{file}\"><img src=\"{file}\"{size} loading=\"lazy\" alt=\"\"></a>"
        ),
        MediaKind::Gif if is_video => format!(
            "<video class=\"media\" src=\"{file}\"{size}{poster} autoplay loop muted playsinline></video>"
        ),
        _ if is_video => format!(
            "<video class=\"media\" src=\"{file}\"{size}{poster} controls preload=\"none\"></video>"
        ),
        _ if is_audio => {
            let cover = thumb.map_or(String::new(), |t| {
                format!("<img class=\"thumb\" src=\"{}\" alt=\"\">", src(t))
            });
            format!(
                "{cover}<audio class=\"media\" src=\"{file}\" controls preload=\"none\"></audio>"
            )
        }
        _ => {
            let preview = thumb.map_or(String::new(), |t| {
                format!("<img class=\"thumb\" src=\"{}\" alt=\"\">", src(t))
            });
            format!(
                "<a class=\"media file clearfix\" href=\"{file}\">{preview}{}</a>",
                escape(path)
            )
        }
    };
    format!(
        "<div class=\"media_wrap clearfix\">\n{body}\n{label}<div class=\"details\">{}</div>\n</div>\n",
        human_size(media.size)
    )
}

fn map_link(label: &str, detail: &str, lat: f64, lon: f64) -> String {
    let detail = if detail.is_empty() {
        String::new()
    } else {
        format!("<div class=\"details\">{}</div>", escape(detail))
    };
    format!(
        "<div class=\"media_wrap\"><div class=\"title\">{label}</div>{detail}\
         <a href=\"https://www.openstreetmap.org/?mlat={lat}&amp;mlon={lon}\">{lat:.5}, {lon:.5}</a></div>\n"
    )
}

/// The media and text of one message.
pub fn body_html(item: &Item<'_>) -> String {
    let msg = item.message;
    let mut out = String::new();
    match &msg.body {
        Body::Media(media) => match item.outcome {
            Some(Outcome::Saved { path, thumb }) => {
                out.push_str(&saved_media(media, path, thumb.as_deref()));
            }
            Some(Outcome::Skipped(skip)) => out.push_str(&format!(
                "<div class=\"media_wrap\"><div class=\"details\">{}</div></div>\n",
                escape(&skip_note(media.kind, *skip))
            )),
            None => {}
        },
        Body::Location {
            latitude,
            longitude,
        } => out.push_str(&map_link("Location", "", *latitude, *longitude)),
        Body::Venue {
            title,
            address,
            latitude,
            longitude,
        } => {
            let detail = if address.is_empty() {
                title.clone()
            } else {
                format!("{title}, {address}")
            };
            out.push_str(&map_link("Venue", &detail, *latitude, *longitude));
        }
        Body::Contact { name, phone } => out.push_str(&format!(
            "<div class=\"media_wrap\"><div class=\"title\">{}</div><div class=\"details\">{}</div></div>\n",
            escape(name),
            escape(phone)
        )),
        Body::Poll {
            question,
            options,
            total_voters,
        } => {
            out.push_str(&format!(
                "<div class=\"media_wrap\"><div class=\"title\">Poll: {}</div>\n",
                escape(question)
            ));
            for (text, votes) in options {
                out.push_str(&format!(
                    "<div class=\"details\">- {} ({votes})</div>\n",
                    escape(text)
                ));
            }
            out.push_str(&format!(
                "<div class=\"details\">Total votes: {total_voters}</div></div>\n"
            ));
        }
        Body::Dice { emoji, value } => out.push_str(&format!(
            "<div class=\"media_wrap\"><div class=\"title\">{} {value}</div></div>\n",
            escape(emoji)
        )),
        Body::Other => out.push_str(
            "<div class=\"media_wrap\"><div class=\"details\">(This message type is not exported)</div></div>\n",
        ),
        Body::Text | Body::Service { .. } => {}
    }
    if !msg.text.is_empty() {
        out.push_str(&format!(
            "<div class=\"text\">{}</div>\n",
            render(&msg.text, &msg.entities)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::model::{ExportMessage, ThumbRef, Who};

    fn media_msg(kind: MediaKind, width: i32, height: i32) -> ExportMessage {
        ExportMessage {
            id: 1,
            date: 1_700_000_000,
            edited: 0,
            outgoing: false,
            sender: Some(Who::User(1)),
            from: Some("Ann".into()),
            reply_to: None,
            forward: None,
            forwarded_from: None,
            text: String::new(),
            entities: Vec::new(),
            body: Body::Media(MediaRef {
                kind,
                file_id: 1,
                size: 2048,
                file_name: String::new(),
                mime: String::new(),
                duration: 0,
                label: String::new(),
                width,
                height,
                thumb: Some(ThumbRef {
                    file_id: 2,
                    size: 10,
                }),
            }),
        }
    }

    fn saved(message: &ExportMessage, path: &str, thumb: Option<&str>) -> String {
        let outcome = Outcome::Saved {
            path: path.into(),
            thumb: thumb.map(str::to_string),
        };
        body_html(&Item {
            message,
            outcome: Some(&outcome),
        })
    }

    #[test]
    fn videos_use_their_thumbnail_as_poster_and_fit_the_layout() {
        let video = media_msg(MediaKind::Video, 1920, 1080);
        let html = saved(
            &video,
            "video_files/a b.mp4",
            Some("video_files/a b.mp4_thumb.jpg"),
        );
        assert!(html.contains("src=\"video_files/a%20b.mp4\" width=\"520\" height=\"293\""));
        assert!(html.contains("poster=\"video_files/a%20b.mp4_thumb.jpg\" controls"));
        let round = media_msg(MediaKind::VideoMessage, 384, 384);
        let html = saved(&round, "round_video_messages/r.mp4", Some("t.jpg"));
        assert!(html.contains("width=\"384\" height=\"384\" poster=\"t.jpg\""));
        // Without a thumbnail or size, the player still shows.
        let gif = media_msg(MediaKind::Gif, 0, 0);
        let html = saved(&gif, "animations/g.mp4", None);
        assert!(html.contains("<video class=\"media\" src=\"animations/g.mp4\" autoplay loop"));
    }

    #[test]
    fn files_and_songs_show_their_preview_image() {
        let doc = media_msg(MediaKind::File, 0, 0);
        let html = saved(&doc, "files/r.pdf", Some("files/r.pdf_thumb.jpg"));
        assert!(html.contains(
            "<a class=\"media file clearfix\" href=\"files/r.pdf\"><img class=\"thumb\" src=\"files/r.pdf_thumb.jpg\""
        ));
        let song = saved(&doc, "files/s.mp3", Some("files/s.mp3_thumb.jpg"));
        assert!(
            song.contains("<img class=\"thumb\" src=\"files/s.mp3_thumb.jpg\" alt=\"\"><audio")
        );
    }

    #[test]
    fn photos_get_layout_sizes() {
        let photo = media_msg(MediaKind::Photo, 800, 1600);
        let html = saved(&photo, "photos/p.jpg", None);
        assert!(html.contains("<img src=\"photos/p.jpg\" width=\"260\" height=\"520\""));
    }

    #[test]
    fn skip_notes_use_tdesktops_words() {
        assert_eq!(
            noun_skip_note("Story", Skip::TooLarge),
            "(Story exceeds maximum size. Change data exporting settings to download.)"
        );
        assert_eq!(
            skip_note(MediaKind::VoiceMessage, Skip::Unavailable),
            "(Voice message unavailable, please try again later)"
        );
    }
}
