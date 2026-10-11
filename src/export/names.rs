//! Output file names, following tdesktop's scheme: `photo_1@14-11-2023_22-13-20.jpg`,
//! `audio_2@…ogg`, `video_3@…mp4`, `file_4@…` for unnamed files, and the
//! original name for files that have one.
use super::model::MediaRef;
use super::settings::MediaKind;
use std::collections::HashSet;

/// `@DD-MM-YYYY_HH-MM-SS` (tdesktop's `PrepareFileNameDatePart`), in the
/// wall clock `offset_secs` east of UTC. Empty for a zero date.
pub fn date_part(date: i32, offset_secs: i32) -> String {
    if date == 0 {
        return String::new();
    }
    let unix = i64::from(date);
    let time = crate::local_time::civil_at(unix, offset_secs);
    let second = (unix + i64::from(offset_secs)).rem_euclid(60);
    format!(
        "@{:02}-{:02}-{:04}_{:02}-{:02}-{:02}",
        time.day, time.month, time.year, time.hour, time.minute, second
    )
}

/// A file name safe on every platform: path separators, control and
/// reserved characters become `_`; leading dots and surrounding spaces go.
pub fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_start_matches('.').trim();
    trimmed.chars().take(120).collect()
}

fn mime_extension(mime: &str) -> Option<&'static str> {
    Some(match mime.to_ascii_lowercase().as_str() {
        "image/jpeg" => ".jpg",
        "image/png" => ".png",
        "image/webp" => ".webp",
        "image/gif" => ".gif",
        "video/mp4" => ".mp4",
        "video/quicktime" => ".mov",
        "video/webm" => ".webm",
        "audio/mpeg" | "audio/mp3" => ".mp3",
        "audio/ogg" => ".ogg",
        "audio/mp4" | "audio/m4a" => ".m4a",
        "application/pdf" => ".pdf",
        "application/zip" => ".zip",
        "text/plain" => ".txt",
        _ => return None,
    })
}

/// `.ext` of a path, lowercase, when it looks like a real extension.
fn path_extension(path: &str) -> Option<String> {
    let ext = std::path::Path::new(path).extension()?.to_str()?;
    (!ext.is_empty() && ext.len() <= 8 && ext.chars().all(|c| c.is_ascii_alphanumeric()))
        .then(|| format!(".{}", ext.to_ascii_lowercase()))
}

/// Hands out unique relative paths (`folder/name`) in the order media is
/// written, so numbering follows the chat oldest first.
#[derive(Default)]
pub struct Namer {
    photos: u32,
    videos: u32,
    audios: u32,
    files: u32,
    used: HashSet<String>,
}

impl Namer {
    /// The relative path for `media` sent at `date`. `source` is TDLib's
    /// local path, used only to learn the extension.
    pub fn path(&mut self, media: &MediaRef, date: i32, offset_secs: i32, source: &str) -> String {
        let when = date_part(date, offset_secs);
        let ext = path_extension(source)
            .or_else(|| path_extension(&media.file_name))
            .or_else(|| mime_extension(&media.mime).map(str::to_string));
        let named = sanitize(&media.file_name);
        let name = match media.kind {
            MediaKind::Photo => {
                self.photos += 1;
                format!("photo_{}{when}.jpg", self.photos)
            }
            _ if !named.is_empty() => named,
            MediaKind::VoiceMessage => {
                self.audios += 1;
                let ext = if media.mime.eq_ignore_ascii_case("audio/mp3")
                    || media.mime.eq_ignore_ascii_case("audio/mpeg")
                {
                    ".mp3"
                } else {
                    ".ogg"
                };
                format!("audio_{}{when}{ext}", self.audios)
            }
            MediaKind::Video => {
                self.videos += 1;
                format!(
                    "video_{}{when}{}",
                    self.videos,
                    ext.as_deref().unwrap_or(".mov")
                )
            }
            _ => {
                self.files += 1;
                format!(
                    "file_{}{when}{}",
                    self.files,
                    ext.as_deref().unwrap_or(".unknown")
                )
            }
        };
        self.unique(media.kind.folder(), &name)
    }

    /// Where the preview image of the file saved at `main` goes: next to
    /// it, as tdesktop's `<path>_thumb.jpg`.
    pub fn thumb_path(&mut self, main: &str) -> String {
        let path = format!("{main}_thumb.jpg");
        self.used.insert(path.to_lowercase());
        path
    }

    /// `folder/name`, or `folder/name (2)` style when taken.
    pub fn unique(&mut self, folder: &str, name: &str) -> String {
        let (stem, ext) = match name.rfind('.') {
            Some(dot) if dot > 0 => name.split_at(dot),
            _ => (name, ""),
        };
        let mut candidate = format!("{folder}/{name}");
        let mut n = 1;
        while !self.used.insert(candidate.to_lowercase()) {
            n += 1;
            candidate = format!("{folder}/{stem} ({n}){ext}");
        }
        candidate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn media(kind: MediaKind, name: &str, mime: &str) -> MediaRef {
        MediaRef {
            kind,
            file_id: 1,
            size: 1,
            file_name: name.into(),
            mime: mime.into(),
            duration: 0,
            label: String::new(),
            width: 0,
            height: 0,
            thumb: None,
        }
    }

    #[test]
    fn thumbnails_sit_next_to_their_file() {
        let mut namer = Namer::default();
        let video = media(MediaKind::Video, "clip.mp4", "video/mp4");
        let main = namer.path(&video, 0, 0, "");
        assert_eq!(namer.thumb_path(&main), "video_files/clip.mp4_thumb.jpg");
        // A file literally named like the thumbnail can't take its place.
        let clash = media(MediaKind::Video, "clip.mp4_thumb.jpg", "");
        assert_eq!(
            namer.path(&clash, 0, 0, ""),
            "video_files/clip.mp4_thumb (2).jpg"
        );
    }

    #[test]
    fn date_part_uses_the_given_offset() {
        // 2023-11-14 22:13:20 UTC.
        assert_eq!(date_part(1_700_000_000, 0), "@14-11-2023_22-13-20");
        assert_eq!(date_part(1_700_000_000, 3600), "@14-11-2023_23-13-20");
        assert_eq!(date_part(0, 0), "");
    }

    #[test]
    fn photos_are_numbered_in_order() {
        let mut namer = Namer::default();
        let photo = media(MediaKind::Photo, "", "image/jpeg");
        assert_eq!(
            namer.path(&photo, 1_700_000_000, 0, "/x/y/12.jpg"),
            "photos/photo_1@14-11-2023_22-13-20.jpg"
        );
        assert_eq!(
            namer.path(&photo, 1_700_000_001, 0, ""),
            "photos/photo_2@14-11-2023_22-13-21.jpg"
        );
    }

    #[test]
    fn voice_video_and_unnamed_files_follow_tdesktop() {
        let mut namer = Namer::default();
        assert_eq!(
            namer.path(
                &media(MediaKind::VoiceMessage, "", "audio/ogg"),
                0,
                0,
                "/a/b.oga"
            ),
            "voice_messages/audio_1.ogg"
        );
        assert_eq!(
            namer.path(&media(MediaKind::Video, "", "video/mp4"), 0, 0, "/a/b.mp4"),
            "video_files/video_1.mp4"
        );
        assert_eq!(
            namer.path(&media(MediaKind::File, "", ""), 0, 0, ""),
            "files/file_1.unknown"
        );
        assert_eq!(
            namer.path(
                &media(MediaKind::Sticker, "", "image/webp"),
                0,
                0,
                "/s/1.webp"
            ),
            "stickers/file_2.webp"
        );
    }

    #[test]
    fn named_files_keep_their_name_and_never_collide() {
        let mut namer = Namer::default();
        let doc = media(MediaKind::File, "report.pdf", "application/pdf");
        assert_eq!(namer.path(&doc, 0, 0, ""), "files/report.pdf");
        assert_eq!(namer.path(&doc, 0, 0, ""), "files/report (2).pdf");
        assert_eq!(namer.path(&doc, 0, 0, ""), "files/report (3).pdf");
        // Case-insensitive file systems must not merge them either.
        let upper = media(MediaKind::File, "REPORT.PDF", "");
        assert_eq!(namer.path(&upper, 0, 0, ""), "files/REPORT (4).PDF");
    }

    #[test]
    fn sanitize_blocks_path_tricks() {
        assert_eq!(sanitize("../../etc/passwd"), "_.._etc_passwd");
        assert_eq!(sanitize("a:b*c?.txt"), "a_b_c_.txt");
        assert_eq!(sanitize("  .hidden "), "hidden");
        assert_eq!(sanitize("..."), "");
        let long = "x".repeat(300);
        assert_eq!(sanitize(&long).len(), 120);
        let mut namer = Namer::default();
        let evil = media(MediaKind::File, "../../x.sh", "");
        let path = namer.path(&evil, 0, 0, "");
        assert!(path.starts_with("files/") && !path[6..].contains('/'));
    }
}
