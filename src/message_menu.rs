//! What the message context menu offers per message type, as plain data.
//!
//! The menu follows Telegram Desktop's `FillContextMenuItems` order
//! (`history/view/history_view_context_menu.cpp`): Reply, Copy Selected,
//! then the "top" actions (Go To Message, View Comments, Edit, Pin), then
//! the media's own actions (`AddPhotoActions` / `AddDocumentActions`),
//! Copy Text, Translate, the link copy, Copy Post/Message Link, Forward,
//! Send Now, Delete, Report, Select, and last the "who saw it" row and
//! the date line. The orders below are the single source for that
//! sequence; the UI sorts its rows by them.

use crate::ids::FileId;
use crate::local_time::{CivilTime, day_label, hhmm};
use crate::telegram::envelope::{MessageContent, MessageReadDate};

/// Sort keys for the rows of the message menu, in Telegram Desktop's order.
pub mod order {
    pub const OPEN_LINK: u8 = 4;
    pub const REPLY: u8 = 10;
    pub const COPY_SELECTED: u8 = 12;
    /// Batch 7 (translation) puts "Translate Selected Text" here.
    pub const TRANSLATE_SELECTED: u8 = 13;
    pub const GO_TO_MESSAGE: u8 = 14;
    pub const VIEW_COMMENTS: u8 = 15;
    pub const EDIT: u8 = 20;
    pub const PIN: u8 = 30;
    /// First slot of the media block; each action adds its index.
    pub const MEDIA: u8 = 32;
    pub const COPY_TEXT: u8 = 40;
    pub const COPY_LINK: u8 = 41;
    /// Batch 7 (translation) puts "Translate" here.
    pub const TRANSLATE: u8 = 42;
    pub const COPY_POST_LINK: u8 = 45;
    pub const FORWARD: u8 = 50;
    pub const STOP_POLL: u8 = 55;
    pub const SEND_NOW: u8 = 56;
    pub const RETRY: u8 = 58;
    pub const DELETE: u8 = 60;
    pub const REPORT: u8 = 65;
    pub const SELECT: u8 = 70;
    pub const RESCHEDULE: u8 = 72;
    /// The "N Seen" / "N Reacted" row and the date line follow a separator.
    pub const AUDIENCE: u8 = 90;
    pub const SENT: u8 = 92;
}

/// The kinds of message content that carry a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Photo,
    Video,
    Animation,
    Document,
    Audio,
    Voice,
    VideoNote,
    Sticker,
}

/// The file behind a media message and how the menu names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaTarget {
    pub kind: MediaKind,
    pub file_id: FileId,
    /// The file's own name ("" when TDLib sends none).
    pub file_name: String,
    /// What "Copy Filename" copies: a document's file name, an audio's
    /// "Performer - Title"; nothing for stickers, GIFs, videos and voice.
    pub copy_name: Option<String>,
    pub set_id: i64,
    pub duration: i32,
    pub title: String,
    pub performer: String,
}

/// The media file of a message, if it has one.
pub fn media_target(content: &MessageContent) -> Option<MediaTarget> {
    let empty = |kind, file_id: FileId| MediaTarget {
        kind,
        file_id,
        file_name: String::new(),
        copy_name: None,
        set_id: 0,
        duration: 0,
        title: String::new(),
        performer: String::new(),
    };
    let target = match content {
        MessageContent::Photo(photo) => empty(MediaKind::Photo, photo.open_file_id()?),
        MessageContent::Video(video) => MediaTarget {
            file_name: video.file_name.clone(),
            duration: video.duration,
            ..empty(MediaKind::Video, video.file_id)
        },
        MessageContent::Animation(animation) => MediaTarget {
            file_name: animation.file_name.clone(),
            duration: animation.duration,
            ..empty(MediaKind::Animation, animation.file_id)
        },
        MessageContent::Document(document) => MediaTarget {
            file_name: document.file_name.clone(),
            copy_name: Some(document.file_name.clone()).filter(|name| !name.is_empty()),
            ..empty(MediaKind::Document, document.file_id)
        },
        MessageContent::Audio(audio) => MediaTarget {
            file_name: audio.file_name.clone(),
            copy_name: Some(song_name(&audio.performer, &audio.title, &audio.file_name))
                .filter(|name| !name.is_empty()),
            duration: audio.duration,
            title: audio.title.clone(),
            performer: audio.performer.clone(),
            ..empty(MediaKind::Audio, audio.file_id)
        },
        MessageContent::VoiceNote(voice) => MediaTarget {
            duration: voice.duration,
            ..empty(MediaKind::Voice, voice.file_id)
        },
        MessageContent::VideoNote(note) => MediaTarget {
            duration: note.duration,
            ..empty(MediaKind::VideoNote, note.file_id)
        },
        MessageContent::Sticker(sticker) => MediaTarget {
            set_id: sticker.set_id,
            ..empty(MediaKind::Sticker, sticker.file_id)
        },
        _ => return None,
    };
    (target.file_id.0 > 0).then_some(target)
}

/// "Performer - Title", falling back to whichever half exists, then the
/// file name (Telegram Desktop `FormatSongNameFor`).
pub fn song_name(performer: &str, title: &str, file_name: &str) -> String {
    match (performer.trim(), title.trim()) {
        ("", "") => file_name.to_string(),
        ("", title) => title.to_string(),
        (performer, "") => performer.to_string(),
        (performer, title) => format!("{performer} - {title}"),
    }
}

/// What is known about the media right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MediaFacts {
    /// A download of the file is running (the menu then only offers to
    /// cancel it).
    pub downloading: bool,
    /// The file is on disk.
    pub local: bool,
    /// Saving the content is allowed (`messageProperties.can_be_saved`,
    /// which is false in a protected chat).
    pub can_save: bool,
    /// The sticker's set is installed.
    pub set_installed: bool,
    /// The sticker is one of the favorites (`None` until they load).
    pub favorite: Option<bool>,
    /// The animation is already among the saved GIFs (`None` until they
    /// load).
    pub gif_saved: Option<bool>,
    /// The message is in Saved Messages (no "Save to Saved Messages").
    pub in_saved_messages: bool,
}

/// One media action. Telegram Desktop's labels are in [`MediaAction::label`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaAction {
    CancelDownload,
    OpenGif,
    SaveGif,
    ViewStickerSet { installed: bool },
    ToggleFavorite { remove: bool },
    ShowInFolder,
    /// A song: "Save to..." with Profile, Saved Messages and Downloads.
    SaveTo,
    SaveAs,
    CopyImage,
    CopyFilename,
}

impl MediaAction {
    pub fn label(self, finder: bool) -> &'static str {
        match self {
            Self::CancelDownload => "Cancel Download",
            Self::OpenGif => "Open GIF",
            Self::SaveGif => "Add to GIFs",
            Self::ViewStickerSet { installed: true } => "View Sticker Set",
            Self::ViewStickerSet { installed: false } => "Add Stickers",
            Self::ToggleFavorite { remove: false } => "Add to Favorites",
            Self::ToggleFavorite { remove: true } => "Remove from Favorites",
            Self::ShowInFolder if finder => "Show in Finder",
            Self::ShowInFolder => "Show in Folder",
            Self::SaveTo => "Save to...",
            Self::SaveAs => "Save As...",
            Self::CopyImage => "Copy Image",
            Self::CopyFilename => "Copy Filename",
        }
    }

    /// Stable element id for the row.
    pub fn id(self) -> &'static str {
        match self {
            Self::CancelDownload => "menu-cancel-download",
            Self::OpenGif => "menu-open-gif",
            Self::SaveGif => "menu-save-gif",
            Self::ViewStickerSet { .. } => "menu-sticker-set",
            Self::ToggleFavorite { .. } => "menu-favorite-sticker",
            Self::ShowInFolder => "menu-show-in-folder",
            Self::SaveTo => "menu-save-to",
            Self::SaveAs => "menu-save-as",
            Self::CopyImage => "menu-copy-image",
            Self::CopyFilename => "menu-copy-filename",
        }
    }
}

/// The media actions for a message, in Telegram Desktop's order
/// (`AddPhotoActions`, `AddDocumentActions`).
pub fn media_actions(target: &MediaTarget, facts: &MediaFacts) -> Vec<MediaAction> {
    let mut actions = Vec::new();
    if target.kind == MediaKind::Photo {
        if facts.can_save {
            actions.push(MediaAction::SaveAs);
            actions.push(MediaAction::CopyImage);
        }
        return actions;
    }
    if facts.downloading {
        return vec![MediaAction::CancelDownload];
    }
    if target.kind == MediaKind::Animation {
        // A GIF that is not on disk (not autoplayed) can be opened.
        if !facts.local {
            actions.push(MediaAction::OpenGif);
        }
        if facts.can_save && facts.gif_saved != Some(true) {
            actions.push(MediaAction::SaveGif);
        }
    }
    if target.kind == MediaKind::Sticker {
        if target.set_id != 0 {
            actions.push(MediaAction::ViewStickerSet {
                installed: facts.set_installed,
            });
        }
        if let Some(favorite) = facts.favorite {
            actions.push(MediaAction::ToggleFavorite { remove: favorite });
        }
    }
    if facts.local {
        actions.push(MediaAction::ShowInFolder);
    }
    if facts.can_save {
        if target.kind == MediaKind::Audio && target.duration > 0 {
            actions.push(MediaAction::SaveTo);
        } else {
            actions.push(MediaAction::SaveAs);
        }
        if target.copy_name.is_some() {
            actions.push(MediaAction::CopyFilename);
        }
    }
    actions
}

/// What the "who saw it" row means for a message (Telegram Desktop
/// `DetectSeenType`): voice notes are "listened", round videos "watched".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeenKind {
    Seen,
    Listened,
    Watched,
}

pub fn seen_kind(content: &MessageContent) -> SeenKind {
    match content {
        MessageContent::VoiceNote(_) => SeenKind::Listened,
        MessageContent::VideoNote(_) => SeenKind::Watched,
        _ => SeenKind::Seen,
    }
}

/// "3 Seen", "1 Listened", "Nobody Viewed" ... (`lng_context_seen_*`).
pub fn seen_label(kind: SeenKind, count: usize) -> String {
    match (kind, count) {
        (SeenKind::Seen, 0) => "Nobody Viewed".into(),
        (SeenKind::Seen, n) => format!("{n} Seen"),
        // Telegram Desktop words the round-video row "Listened" too.
        (SeenKind::Listened | SeenKind::Watched, 0) => "Nobody Listened".into(),
        (SeenKind::Listened | SeenKind::Watched, n) => format!("{n} Listened"),
    }
}

/// "3 Reacted", "Nobody Reacted" (`lng_context_seen_reacted*`).
pub fn reacted_label(count: usize) -> String {
    match count {
        0 => "Nobody Reacted".into(),
        n => format!("{n} Reacted"),
    }
}

/// "today at 12:34", "yesterday at 12:34", "Oct 6 at 12:34"
/// (`FormatReadDate`).
pub fn read_date_label(date: &CivilTime, now: &CivilTime) -> String {
    let time = hhmm(date);
    match now.day_number() - date.day_number() {
        0 => format!("today at {time}"),
        1 => format!("yesterday at {time}"),
        _ => format!(
            "{} at {time}",
            crate::local_time::month_name(date.month)
                .chars()
                .take(3)
                .collect::<String>()
                + &format!(" {}", date.day)
        ),
    }
}

/// The private-chat "seen" line for an outgoing message
/// (`getMessageReadDate`).
pub fn read_status_label(read: MessageReadDate, now: &CivilTime) -> String {
    match read {
        MessageReadDate::Read(unix) => {
            let date = crate::local_time::civil_local(i64::from(unix));
            format!("Seen {}", read_date_label(&date, now))
        }
        MessageReadDate::Unread => "Not seen yet".into(),
        MessageReadDate::TooOld => "Seen a long time ago".into(),
        MessageReadDate::UserPrivacyRestricted | MessageReadDate::MyPrivacyRestricted => {
            "Seen time hidden".into()
        }
    }
}

/// "Sent today at 12:34" (`lng_context_sent_today` and its siblings).
pub fn sent_label(date: &CivilTime, now: &CivilTime) -> String {
    let time = hhmm(date);
    match now.day_number() - date.day_number() {
        0 => format!("Sent today at {time}"),
        1 => format!("Sent yesterday at {time}"),
        _ => format!("Sent {} at {time}", day_label(date, now)),
    }
}

/// Telegram Desktop's wording for the link item: a supergroup gets "Copy
/// Message Link", a channel "Copy Post Link"
/// (`lng_context_copy_message_link` / `lng_context_copy_post_link`).
pub fn copy_link_label(is_channel: bool) -> &'static str {
    if is_channel {
        "Copy Post Link"
    } else {
        "Copy Message Link"
    }
}

/// Telegram Desktop's line under a menu with nothing to copy or forward
/// (`lng_context_noforwards_info_*`).
pub fn noforwards_info(is_channel: bool, is_group: bool, is_bot: bool, mine: bool) -> &'static str {
    if is_channel {
        "Copying and forwarding is not allowed in this channel."
    } else if is_group {
        "Copying and forwarding is not allowed in this group."
    } else if is_bot {
        "Copying and forwarding is not allowed from this bot."
    } else if mine {
        "You disabled copying and forwarding in this chat."
    } else {
        "Copying and forwarding is not allowed in this chat."
    }
}

#[cfg(test)]
mod tests {
    use crate::local_time::civil_at;
    use crate::message_menu::{
        MediaAction, MediaFacts, MediaKind, MediaTarget, copy_link_label, media_actions,
        media_target, read_date_label, read_status_label, reacted_label, seen_kind, seen_label,
        sent_label, song_name,
    };
    use crate::telegram::envelope::{MessageContent, MessageReadDate};

    // 2026-09-28 21:42 UTC, a Monday.
    const NOW: i64 = 1_790_631_720;

    fn target(kind: MediaKind) -> MediaTarget {
        MediaTarget {
            kind,
            file_id: crate::ids::FileId(5),
            file_name: "name.bin".into(),
            copy_name: Some("name.bin".into()),
            set_id: 0,
            duration: 0,
            title: String::new(),
            performer: String::new(),
        }
    }

    fn local() -> MediaFacts {
        MediaFacts {
            local: true,
            can_save: true,
            ..MediaFacts::default()
        }
    }

    #[test]
    fn photo_offers_save_as_and_copy_image_when_saving_is_allowed() {
        assert_eq!(
            media_actions(&target(MediaKind::Photo), &local()),
            vec![MediaAction::SaveAs, MediaAction::CopyImage]
        );
        let protected = MediaFacts {
            can_save: false,
            ..local()
        };
        assert!(media_actions(&target(MediaKind::Photo), &protected).is_empty());
    }

    #[test]
    fn downloading_documents_only_offer_cancel() {
        let facts = MediaFacts {
            downloading: true,
            ..local()
        };
        for kind in [MediaKind::Document, MediaKind::Video, MediaKind::Audio] {
            assert_eq!(
                media_actions(&target(kind), &facts),
                vec![MediaAction::CancelDownload]
            );
        }
    }

    #[test]
    fn document_in_telegram_desktop_order() {
        assert_eq!(
            media_actions(&target(MediaKind::Document), &local()),
            vec![
                MediaAction::ShowInFolder,
                MediaAction::SaveAs,
                MediaAction::CopyFilename
            ]
        );
        // Not downloaded: nothing to show in the folder.
        let remote = MediaFacts {
            local: false,
            ..local()
        };
        assert_eq!(
            media_actions(&target(MediaKind::Document), &remote),
            vec![MediaAction::SaveAs, MediaAction::CopyFilename]
        );
    }

    #[test]
    fn video_and_voice_have_no_copy_filename() {
        let mut video = target(MediaKind::Video);
        video.copy_name = None;
        assert_eq!(
            media_actions(&video, &local()),
            vec![MediaAction::ShowInFolder, MediaAction::SaveAs]
        );
    }

    #[test]
    fn gif_offers_open_when_not_on_disk_and_add_to_gifs_once() {
        let mut animation = target(MediaKind::Animation);
        animation.copy_name = None;
        let remote = MediaFacts {
            local: false,
            gif_saved: Some(false),
            ..local()
        };
        assert_eq!(
            media_actions(&animation, &remote),
            vec![
                MediaAction::OpenGif,
                MediaAction::SaveGif,
                MediaAction::SaveAs
            ]
        );
        let saved = MediaFacts {
            gif_saved: Some(true),
            ..local()
        };
        assert_eq!(
            media_actions(&animation, &saved),
            vec![MediaAction::ShowInFolder, MediaAction::SaveAs]
        );
    }

    #[test]
    fn sticker_offers_its_set_and_favorites_before_the_file_actions() {
        let mut sticker = target(MediaKind::Sticker);
        sticker.set_id = 77;
        sticker.copy_name = None;
        let facts = MediaFacts {
            set_installed: false,
            favorite: Some(false),
            ..local()
        };
        assert_eq!(
            media_actions(&sticker, &facts),
            vec![
                MediaAction::ViewStickerSet { installed: false },
                MediaAction::ToggleFavorite { remove: false },
                MediaAction::ShowInFolder,
                MediaAction::SaveAs
            ]
        );
        let installed = MediaFacts {
            set_installed: true,
            favorite: Some(true),
            ..local()
        };
        let labels: Vec<_> = media_actions(&sticker, &installed)
            .into_iter()
            .map(|a| a.label(false))
            .collect();
        assert_eq!(
            labels,
            [
                "View Sticker Set",
                "Remove from Favorites",
                "Show in Folder",
                "Save As..."
            ]
        );
        // A sticker without a set offers neither.
        sticker.set_id = 0;
        assert!(!media_actions(&sticker, &facts).iter().any(|a| matches!(
            a,
            MediaAction::ViewStickerSet { .. }
        )));
    }

    #[test]
    fn songs_save_to_a_destination_instead_of_save_as() {
        let mut song = target(MediaKind::Audio);
        song.duration = 200;
        song.copy_name = Some("Artist - Song".into());
        assert_eq!(
            media_actions(&song, &local()),
            vec![
                MediaAction::ShowInFolder,
                MediaAction::SaveTo,
                MediaAction::CopyFilename
            ]
        );
    }

    #[test]
    fn finder_wording_on_macos() {
        assert_eq!(MediaAction::ShowInFolder.label(true), "Show in Finder");
        assert_eq!(MediaAction::ShowInFolder.label(false), "Show in Folder");
    }

    #[test]
    fn text_has_no_media_target() {
        assert!(media_target(&MessageContent::Service("x".into())).is_none());
    }

    #[test]
    fn song_names() {
        assert_eq!(song_name("A", "B", "f.mp3"), "A - B");
        assert_eq!(song_name("", "B", "f.mp3"), "B");
        assert_eq!(song_name("A", "", "f.mp3"), "A");
        assert_eq!(song_name("", "", "f.mp3"), "f.mp3");
    }

    #[test]
    fn audience_labels_follow_telegram_desktop() {
        assert_eq!(seen_label(seen_kind(&MessageContent::Service("".into())), 3), "3 Seen");
        assert_eq!(seen_label(crate::message_menu::SeenKind::Seen, 0), "Nobody Viewed");
        assert_eq!(seen_label(crate::message_menu::SeenKind::Listened, 2), "2 Listened");
        assert_eq!(seen_label(crate::message_menu::SeenKind::Watched, 0), "Nobody Listened");
        assert_eq!(reacted_label(0), "Nobody Reacted");
        assert_eq!(reacted_label(4), "4 Reacted");
    }

    #[test]
    fn dates_say_today_yesterday_or_the_day() {
        let now = civil_at(NOW, 0);
        let today = civil_at(NOW - 3600, 0);
        let yesterday = civil_at(NOW - 86_400, 0);
        let older = civil_at(NOW - 5 * 86_400, 0);
        assert_eq!(read_date_label(&today, &now), "today at 20:42");
        assert_eq!(read_date_label(&yesterday, &now), "yesterday at 21:42");
        assert_eq!(read_date_label(&older, &now), "Sep 23 at 21:42");
        assert_eq!(sent_label(&today, &now), "Sent today at 20:42");
        assert_eq!(sent_label(&yesterday, &now), "Sent yesterday at 21:42");
        assert_eq!(sent_label(&older, &now), "Sent September 23 at 21:42");
    }

    #[test]
    fn read_status_covers_every_privacy_state() {
        let now = civil_at(NOW, 0);
        assert_eq!(read_status_label(MessageReadDate::Unread, &now), "Not seen yet");
        assert_eq!(
            read_status_label(MessageReadDate::UserPrivacyRestricted, &now),
            "Seen time hidden"
        );
        assert_eq!(
            read_status_label(MessageReadDate::MyPrivacyRestricted, &now),
            "Seen time hidden"
        );
        assert_eq!(
            read_status_label(MessageReadDate::TooOld, &now),
            "Seen a long time ago"
        );
    }

    #[test]
    fn link_wording() {
        assert_eq!(copy_link_label(true), "Copy Post Link");
        assert_eq!(copy_link_label(false), "Copy Message Link");
    }
}
