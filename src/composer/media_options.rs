//! Send-box options for photos and videos (tdesktop `SendFilesBox`): the
//! high quality toggle, a GIF sent with a caption, the price of paid media
//! and a video's cover and edits.
//!
//! tdesktop shows "High Quality" for a compressed photo larger than 1280 px
//! and then keeps 2560 px instead of scaling the photo down to 1280 px
//! (`PhotoSideLimit`). Paid media is for channels the account can post to
//! that allow it, up to ten photos and videos in one message.

use super::*;
use crate::video_edit::VideoEdits;

/// tdesktop `kStandardPhotoSideLimit`: a compressed photo is scaled down to
/// this many pixels on its longer side.
pub const PHOTO_SIDE_STANDARD: u32 = 1280;
/// tdesktop `PhotoSideLimit(true)`: the limit with "High Quality" on.
pub const PHOTO_SIDE_LARGE: u32 = 2560;
/// tdesktop's default of `stars_paid_post_amount_max`: the dearest price a
/// paid media message can have, in Stars.
pub const PAID_MEDIA_MAX_STARS: i64 = 10_000;

/// Scaling one photo with ffmpeg may take this long.
const PHOTO_SCALE_LIMIT: std::time::Duration = std::time::Duration::from_secs(30);

/// The longer side a photo is sent with.
pub fn photo_side_limit(high_quality: bool) -> u32 {
    if high_quality {
        PHOTO_SIDE_LARGE
    } else {
        PHOTO_SIDE_STANDARD
    }
}

/// tdesktop `PreparedFile::canUseHighQualityPhoto`: only a photo larger
/// than the standard limit gains from the toggle.
pub fn can_use_high_quality(width: u32, height: u32) -> bool {
    width.max(height) > PHOTO_SIDE_STANDARD
}

/// The size a photo is sent with: scaled down to `limit` on its longer
/// side, never up.
pub fn fitted_photo_size(width: u32, height: u32, limit: u32) -> (u32, u32) {
    let longer = width.max(height);
    if limit == 0 || longer <= limit || longer == 0 {
        return (width, height);
    }
    let scale = |side: u32| {
        let scaled =
            (u64::from(side) * u64::from(limit) + u64::from(longer) / 2) / u64::from(longer);
        scaled.max(1) as u32
    };
    (scale(width), scale(height))
}

/// The send-box choices that are the same for every file of a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MediaSendOptions {
    /// "High Quality": keep photos up to `PHOTO_SIDE_LARGE`.
    pub hd_photos: bool,
    /// Stars to unlock the media; 0 is a normal message.
    pub paid_stars: i64,
}

/// What the user set on one attachment besides the file itself.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AttachmentExtras {
    /// Send the file as a GIF (`inputMessageAnimation`) when it is sent on
    /// its own: a `.gif` file, a video without sound (tdesktop
    /// `modifications.gif = !hasAudio`) or a video turned into one in the
    /// video editor. In an album it goes as a plain video.
    pub gif: bool,
    /// The cover picked with "Edit Cover" (`inputVideo.cover`).
    pub cover: Option<PathBuf>,
    /// `inputVideo.start_timestamp` in seconds: the cover frame chosen in
    /// the video editor.
    pub start_timestamp: i32,
    /// The file the video editor started from, kept so the edit can be
    /// opened again; `path` is the edited copy.
    pub source: Option<PathBuf>,
    /// The edits `path` was made with.
    pub edits: Option<VideoEdits>,
    /// `gif` came from taking the sound out of a video that had it
    /// (tdesktop `sendsVideoAsGif`): such a video leaves an album and goes
    /// out alone. A video that never had sound stays in the album.
    pub sound_removed: bool,
    /// The thumbnail Quill made for a picture sent as a file
    /// (`inputDocument.thumbnail`; tdesktop `FinalizeFileThumbnail`).
    pub file_thumbnail: Option<FileThumbnail>,
    /// A photo edit is rendering into a new file for this one: the latest
    /// Done's number. Until it lands the file must not be sent.
    pub photo_render: Option<u64>,
}

/// A JPEG thumbnail written next to the media cache for upload with a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileThumbnail {
    pub path: PathBuf,
    pub width: i32,
    pub height: i32,
}

/// tdesktop `kThumbnailSize`: the long side of a file's thumbnail.
pub const FILE_THUMBNAIL_SIDE: u32 = 320;

/// tdesktop `FileThumbnailUploadRequired`: Telegram makes the thumbnail
/// itself only for small JPEG, GIF, PNG and WebP files (and MP4 videos), so
/// any other picture sent as a file uploads one Quill made.
pub fn file_thumbnail_upload_required(path: &Path, size: u64) -> bool {
    const SERVER_THUMBNAIL_MAX: u64 = 5 * 1024 * 1024;
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    size > SERVER_THUMBNAIL_MAX
        || !matches!(
            extension.as_str(),
            "jpg" | "jpeg" | "jfif" | "gif" | "png" | "webp" | "mp4"
        )
}

/// A `.gif` file: it is sent as an animation, not a video or a file.
pub fn is_gif_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gif"))
}

/// tdesktop `PreparedList::canChangePrice` for the files in the tray: at
/// most ten photos and videos, sent compressed (and as one album when
/// there are several). An animated `.gif` file is not paid media.
pub fn paid_media_fits(list: &[ComposerAttachment], grouped: bool) -> bool {
    !list.is_empty()
        && list.len() <= crate::album::ALBUM_MAX_ITEMS
        && (list.len() == 1 || grouped)
        && list.iter().all(|attachment| {
            matches!(
                attachment.kind,
                AttachmentKind::Photo | AttachmentKind::Video
            ) && !is_gif_path(&attachment.path)
        })
}

/// tdesktop `ChannelData::canPostPaidMedia`: a channel the account can post
/// to that allows paid media (`supergroupFullInfo.has_paid_media_allowed`).
pub fn session_allows_paid_media(session: &crate::state::Session, chat_id: ChatId) -> bool {
    let Some(chat) = session.chats.get(&chat_id.0) else {
        return false;
    };
    chat.is_channel()
        && chat.can_post()
        && session
            .chat_supergroup(chat_id)
            .and_then(|id| session.supergroup_full_info(id))
            .is_some_and(|info| info.admin.has_paid_media_allowed)
}

/// What was typed in the "Paid Content" box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceInput {
    /// Empty or zero: the media is not paid (tdesktop applies a zero price).
    Clear,
    /// A price from 1 Star up to the limit.
    Price(i64),
    /// Over the limit (tdesktop `field->showError()`).
    TooHigh,
}

/// Read the price field against `max` (the TDLib option, or
/// `PAID_MEDIA_MAX_STARS`). Anything but digits is ignored, so "1 000" is 1000.
pub fn parse_paid_price(text: &str, max: i64) -> PriceInput {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return PriceInput::Clear;
    }
    match digits.parse::<i64>() {
        Ok(0) => PriceInput::Clear,
        Ok(price) if price <= max => PriceInput::Price(price),
        _ => PriceInput::TooHigh,
    }
}

/// ffmpeg arguments that scale the image `input` to `size` and write
/// `output` (JPEG, or PNG to keep transparency).
pub fn photo_scale_args(input: &str, output: &str, size: (u32, u32), png: bool) -> Vec<String> {
    let mut args: Vec<String> = ["-y", "-i", input, "-vf"]
        .iter()
        .map(|arg| (*arg).to_string())
        .collect();
    args.push(format!("scale={}:{}:flags=lanczos", size.0, size.1));
    args.extend(["-frames:v", "1"].iter().map(|arg| (*arg).to_string()));
    if !png {
        args.extend(["-q:v", "2"].iter().map(|arg| (*arg).to_string()));
    }
    args.push(output.to_string());
    args
}

pub fn photo_size_of(path: &Path) -> Option<(u32, u32)> {
    let mut command = crate::media_tools::command("ffprobe");
    command
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
        .arg(path);
    let output =
        crate::child_timeout::output_within(&mut command, crate::child_timeout::PROBE_LIMIT)
            .ok()?;
    if !output.status.success() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let stream = value.get("streams")?.get(0)?;
    let width = u32::try_from(stream.get("width")?.as_u64()?).ok()?;
    let height = u32::try_from(stream.get("height")?.as_u64()?).ok()?;
    Some((width, height))
}

/// The file a photo goes out as (tdesktop `PrepareDetails` with
/// `PhotoSideLimit`): the photo itself when it fits, otherwise a copy
/// scaled down to the limit. Falls back to the original when the copy can't
/// be made, so a send never fails over this. Blocks on ffmpeg.
pub fn photo_for_send(path: &Path, high_quality: bool) -> PathBuf {
    scaled_photo_for_send(path, high_quality)
        .unwrap_or_else(|PhotoScaleTimedOut| path.to_path_buf())
}

/// ffmpeg did not finish scaling a photo in [`PHOTO_SCALE_LIMIT`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhotoScaleTimedOut;

/// [`photo_for_send`], but a scale that ran out of time is an error so the
/// caller can tell the user instead of quietly sending the big original.
/// Any other failure still falls back to the original. A partial copy left
/// by a failed or timed out scale is deleted.
pub fn scaled_photo_for_send(
    path: &Path,
    high_quality: bool,
) -> Result<PathBuf, PhotoScaleTimedOut> {
    let limit = photo_side_limit(high_quality);
    let Some((width, height)) = photo_size_of(path) else {
        return Ok(path.to_path_buf());
    };
    let size = fitted_photo_size(width, height, limit);
    if size == (width, height) {
        return Ok(path.to_path_buf());
    }
    let png = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"));
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let dir = crate::scratch::temp_work_dir("quill-send-photos");
    let output = dir.join(format!("photo-{stamp}.{}", if png { "png" } else { "jpg" }));
    let mut command = crate::media_tools::command("ffmpeg");
    command.args(photo_scale_args(
        &path.to_string_lossy(),
        &output.to_string_lossy(),
        size,
        png,
    ));
    let scaled = std::fs::create_dir_all(&dir)
        .map_err(|_| None)
        .and_then(|()| {
            crate::child_timeout::output_within(&mut command, PHOTO_SCALE_LIMIT)
                .map_err(|err| crate::child_timeout::is_timeout(&err).then_some(PhotoScaleTimedOut))
        });
    match scaled {
        Ok(result) if result.status.success() => {
            Ok(pick_send_path(&output).unwrap_or_else(|| path.to_path_buf()))
        }
        Ok(_) | Err(None) => {
            let _ = std::fs::remove_file(&output);
            Ok(path.to_path_buf())
        }
        Err(Some(timed_out)) => {
            let _ = std::fs::remove_file(&output);
            Err(timed_out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attachment(kind: AttachmentKind) -> ComposerAttachment {
        ComposerAttachment {
            path: PathBuf::from("/tmp/x"),
            kind,
            file_name: "x".into(),
            spoiler: false,
            extras: AttachmentExtras::default(),
        }
    }

    #[test]
    fn file_thumbnails_upload_where_telegram_makes_none() {
        let small = 1024;
        assert!(!file_thumbnail_upload_required(Path::new("a.JPG"), small));
        assert!(!file_thumbnail_upload_required(Path::new("a.webp"), small));
        assert!(file_thumbnail_upload_required(Path::new("a.heic"), small));
        assert!(file_thumbnail_upload_required(Path::new("a.avif"), small));
        assert!(file_thumbnail_upload_required(
            Path::new("a.png"),
            6 * 1024 * 1024
        ));
    }

    #[test]
    fn high_quality_only_helps_photos_over_1280() {
        assert!(!can_use_high_quality(1280, 720));
        assert!(!can_use_high_quality(720, 1280));
        assert!(can_use_high_quality(1281, 100));
        assert!(can_use_high_quality(100, 4000));
        assert_eq!(photo_side_limit(false), 1280);
        assert_eq!(photo_side_limit(true), 2560);
    }

    #[test]
    fn photos_scale_down_to_the_limit_and_never_up() {
        assert_eq!(fitted_photo_size(4000, 3000, 1280), (1280, 960));
        assert_eq!(fitted_photo_size(3000, 4000, 2560), (1920, 2560));
        assert_eq!(fitted_photo_size(1000, 800, 1280), (1000, 800));
        assert_eq!(fitted_photo_size(1280, 1280, 1280), (1280, 1280));
        assert_eq!(fitted_photo_size(10_000, 10, 1280), (1280, 1));
        assert_eq!(fitted_photo_size(0, 0, 1280), (0, 0));
    }

    #[test]
    fn paid_media_needs_ten_or_fewer_compressed_photos_and_videos() {
        let photo = attachment(AttachmentKind::Photo);
        let video = attachment(AttachmentKind::Video);
        let file = attachment(AttachmentKind::Document);
        assert!(paid_media_fits(std::slice::from_ref(&photo), false));
        assert!(paid_media_fits(&[photo.clone(), video.clone()], true));
        assert!(!paid_media_fits(&[photo.clone(), video.clone()], false));
        assert!(!paid_media_fits(&[photo.clone(), file], true));
        assert!(!paid_media_fits(&[], true));
        assert!(!paid_media_fits(&vec![photo.clone(); 11], true));
        assert!(paid_media_fits(&vec![photo.clone(); 10], true));
        let mut gif = video.clone();
        gif.path = PathBuf::from("/tmp/x.gif");
        assert!(!paid_media_fits(&[gif], true));
        // A soundless video is a "GIF" for the animation rule only.
        let mut silent = video;
        silent.extras.gif = true;
        assert!(paid_media_fits(&[silent], true));
        assert!(!paid_media_fits(
            &[attachment(AttachmentKind::VideoNote)],
            true
        ));
    }

    #[test]
    fn the_price_box_accepts_up_to_the_limit_and_clears_on_empty() {
        let max = PAID_MEDIA_MAX_STARS;
        assert_eq!(parse_paid_price("250", max), PriceInput::Price(250));
        assert_eq!(parse_paid_price(" 1 000 ", max), PriceInput::Price(1000));
        assert_eq!(parse_paid_price("10000", max), PriceInput::Price(10_000));
        assert_eq!(parse_paid_price("10001", max), PriceInput::TooHigh);
        assert_eq!(parse_paid_price("10001", 20_000), PriceInput::Price(10_001));
        assert_eq!(parse_paid_price("500", 400), PriceInput::TooHigh);
        assert_eq!(
            parse_paid_price("99999999999999999999", max),
            PriceInput::TooHigh
        );
        assert_eq!(parse_paid_price("", max), PriceInput::Clear);
        assert_eq!(parse_paid_price("0", max), PriceInput::Clear);
        assert_eq!(parse_paid_price("abc", max), PriceInput::Clear);
    }

    #[test]
    fn gif_files_are_recognised_by_extension() {
        assert!(is_gif_path(Path::new("/tmp/Funny.GIF")));
        assert!(!is_gif_path(Path::new("/tmp/clip.mp4")));
        assert!(!is_gif_path(Path::new("/tmp/gif")));
    }

    #[test]
    fn scale_args_pick_the_encoder_by_format() {
        let jpg = photo_scale_args("in.heic", "out.jpg", (1280, 960), false);
        assert!(jpg.contains(&"scale=1280:960:flags=lanczos".to_string()));
        assert!(jpg.contains(&"-q:v".to_string()));
        assert_eq!(jpg.last().unwrap(), "out.jpg");
        let png = photo_scale_args("in.png", "out.png", (1280, 960), true);
        assert!(!png.contains(&"-q:v".to_string()));
    }

    #[test]
    fn a_missing_photo_is_sent_as_it_is() {
        let path = PathBuf::from("/definitely/not/here.jpg");
        assert_eq!(photo_for_send(&path, false), path);
    }
}
