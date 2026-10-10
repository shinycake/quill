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
    /// "Reply with timecode" follows Reply on a playing voice message.
    pub const REPLY_TIMECODE: u8 = 11;
    /// "Reply in Another Chat" sits with the reply rows (stable sort keeps
    /// the push order inside the shared slot).
    pub const REPLY_ELSEWHERE: u8 = 11;
    pub const COPY_SELECTED: u8 = 12;
    /// "Translate Selected Text" (`ui/translate_ui.rs`).
    pub const TRANSLATE_SELECTED: u8 = 13;
    pub const GO_TO_MESSAGE: u8 = 14;
    pub const VIEW_COMMENTS: u8 = 15;
    pub const EDIT: u8 = 20;
    /// "Add Fact Check" / "Edit Fact Check" follows Edit.
    pub const FACT_CHECK: u8 = 21;
    pub const PIN: u8 = 30;
    /// First slot of the media block; each action adds its index.
    pub const MEDIA: u8 = 32;
    pub const COPY_TEXT: u8 = 40;
    pub const COPY_LINK: u8 = 41;
    /// "Translate" (`ui/translate_ui.rs`).
    pub const TRANSLATE: u8 = 42;
    pub const COPY_POST_LINK: u8 = 45;
    pub const FORWARD: u8 = 50;
    /// "Poll Stats" (`getPollVoteStatistics`).
    pub const POLL_STATS: u8 = 53;
    /// "Retract vote" sits above "Stop Poll" (`AddPollActions`).
    pub const RETRACT_VOTE: u8 = 54;
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
    /// "This message contains emoji from X pack" closes the menu.
    pub const EMOJI_PACKS: u8 = 94;
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
    /// The song or voice message fits the notification-tone limits.
    pub tone_ok: bool,
}

/// How long, how big and how many notification tones Telegram accepts
/// (`notification_sound_*_max`; Telegram Desktop's `Api::Ringtones` defaults
/// when the server sends none).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToneLimits {
    pub max_size: i64,
    pub max_duration: i32,
    pub max_count: usize,
}

impl Default for ToneLimits {
    fn default() -> Self {
        Self {
            max_size: 100 * 1024,
            max_duration: 5,
            max_count: 100,
        }
    }
}

/// `AddSaveSoundForNotifications`: only songs and voice messages that fit
/// the limits, while the saved list still has room.
pub fn tone_offered(
    target: &MediaTarget,
    size: i64,
    saved_count: usize,
    limits: ToneLimits,
) -> bool {
    matches!(target.kind, MediaKind::Audio | MediaKind::Voice)
        && size <= limits.max_size
        && target.duration <= limits.max_duration
        && saved_count < limits.max_count
}

/// One media action. Telegram Desktop's labels are in [`MediaAction::label`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaAction {
    CancelDownload,
    OpenGif,
    SaveGif,
    ViewStickerSet {
        installed: bool,
    },
    ToggleFavorite {
        remove: bool,
    },
    ShowInFolder,
    /// "Save for Notifications" (`addSavedNotificationSound`).
    SaveForNotifications,
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
            Self::SaveForNotifications => "Save for Notifications",
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
            Self::SaveForNotifications => "menu-save-notification-tone",
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
    if facts.can_save && facts.tone_ok {
        actions.push(MediaAction::SaveForNotifications);
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

/// The footer line about custom emoji packs (`lng_context_animated_emoji`
/// and `lng_context_animated_emoji_many`): the text before the bold part,
/// the bold part, the text after. One known pack is named; otherwise the
/// packs are counted. `None` when the message uses no pack.
pub fn emoji_pack_footer(packs: usize, name: Option<&str>) -> Option<(String, String, String)> {
    let before = "This message contains emoji from ".to_string();
    match (packs, name.filter(|name| !name.is_empty())) {
        (0, _) => None,
        (1, Some(name)) => Some((before, format!("{name} pack"), ".".into())),
        (1, None) => Some((before, "1 pack".into(), ".".into())),
        (count, _) => Some((before, format!("{count} packs"), ".".into())),
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

/// "Edited today at 12:36" (the date row `WhenEdited` shows).
pub fn edited_label(date: &CivilTime, now: &CivilTime) -> String {
    format!("Edited {}", read_date_label(date, now))
}

/// The file name "Save As..." proposes: the file's own name, else the
/// local file's name, with the audio's "Performer - Title" preferred over
/// a bare id-like name.
pub fn suggested_save_name(target: &MediaTarget, local_name: &str) -> String {
    if !target.file_name.is_empty() {
        return target.file_name.clone();
    }
    let extension = std::path::Path::new(local_name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    match (&target.copy_name, extension) {
        (Some(name), "") => name.clone(),
        (Some(name), ext) => format!("{name}.{ext}"),
        (None, _) => local_name.to_string(),
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

/// "Add Fact Check" or, once one exists, "Edit Fact Check"
/// (`lng_context_add_factcheck` / `lng_context_edit_factcheck`).
pub fn fact_check_label(has_fact_check: bool) -> &'static str {
    if has_fact_check {
        "Edit Fact Check"
    } else {
        "Add Fact Check"
    }
}

/// A playback position as Telegram Desktop writes it into a reply
/// (`Ui::FormatDurationText`): "0:07", "12:03" or "1:02:03".
pub fn timecode_text(position_secs: f64) -> String {
    let total = position_secs.max(0.0) as u64;
    let (hours, minutes, seconds) = (total / 3600, total % 3600 / 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// "Reply with timecode" is offered on a voice message that is playing (or
/// paused part-way) when the chat takes replies.
pub fn timecode_offered(content: &MessageContent, playing_now: bool, can_reply: bool) -> bool {
    playing_now && can_reply && matches!(content, MessageContent::VoiceNote(_))
}

/// What the composer receives for the timecode: a space first when the text
/// before the cursor does not end in whitespace, and a space after
/// (`Menu::InsertTextAtCursor`).
pub fn timecode_insertion(before_cursor: &str, timecode: &str) -> String {
    let space_first = before_cursor
        .chars()
        .last()
        .is_some_and(|c| !c.is_whitespace());
    format!("{}{timecode} ", if space_first { " " } else { "" })
}

/// The note after "Copy Post Link" / "Copy Message Link": a public link says
/// so, a private one warns that only members can open it
/// (`lng_channel_public_link_copied` / `lng_context_about_private_link`).
pub fn link_copied_note(public: bool) -> &'static str {
    if public {
        "Link copied to clipboard."
    } else {
        "This link will only work for members of this chat."
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

/// [`noforwards_info`] with the peer named in a private chat the other
/// side restricted (`lng_context_noforwards_info_his`).
pub fn noforwards_text(
    is_channel: bool,
    is_group: bool,
    is_bot: bool,
    mine: bool,
    peer: Option<&str>,
) -> String {
    match peer {
        Some(name) if !is_channel && !is_group && !is_bot && !mine => {
            format!("{name} disabled copying and forwarding in this chat.")
        }
        _ => noforwards_info(is_channel, is_group, is_bot, mine).to_string(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn emoji_pack_footer_names_one_pack_or_counts_them() {
        use super::emoji_pack_footer;
        assert_eq!(emoji_pack_footer(0, Some("x")), None);
        assert_eq!(
            emoji_pack_footer(1, Some("Fun")),
            Some((
                "This message contains emoji from ".into(),
                "Fun pack".into(),
                ".".into()
            ))
        );
        assert_eq!(emoji_pack_footer(1, None).unwrap().1, "1 pack");
        assert_eq!(emoji_pack_footer(1, Some("")).unwrap().1, "1 pack");
        assert_eq!(emoji_pack_footer(3, Some("Fun")).unwrap().1, "3 packs");
    }

    use crate::local_time::civil_at;
    use crate::message_menu::{
        MediaAction, MediaFacts, MediaKind, MediaTarget, ToneLimits, copy_link_label,
        fact_check_label, link_copied_note, media_actions, media_target, reacted_label,
        read_date_label, read_status_label, seen_kind, seen_label, sent_label, song_name,
        timecode_insertion, timecode_offered, timecode_text, tone_offered,
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
        assert!(
            !media_actions(&sticker, &facts)
                .iter()
                .any(|a| matches!(a, MediaAction::ViewStickerSet { .. }))
        );
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
    fn short_songs_and_voice_notes_offer_a_notification_tone() {
        let limits = ToneLimits::default();
        let mut voice = target(MediaKind::Voice);
        voice.duration = 4;
        assert!(tone_offered(&voice, 20_000, 3, limits));
        // Too long, too big, or no room left in the saved list.
        voice.duration = 6;
        assert!(!tone_offered(&voice, 20_000, 3, limits));
        voice.duration = 4;
        assert!(!tone_offered(&voice, 200_000, 3, limits));
        assert!(!tone_offered(&voice, 20_000, 100, limits));
        // Videos and documents never qualify.
        for kind in [MediaKind::Video, MediaKind::Document, MediaKind::VideoNote] {
            assert!(!tone_offered(&target(kind), 100, 0, limits));
        }
    }

    #[test]
    fn notification_tone_sits_between_show_in_folder_and_save() {
        let mut voice = target(MediaKind::Voice);
        voice.copy_name = None;
        let facts = MediaFacts {
            tone_ok: true,
            ..local()
        };
        assert_eq!(
            media_actions(&voice, &facts),
            vec![
                MediaAction::ShowInFolder,
                MediaAction::SaveForNotifications,
                MediaAction::SaveAs
            ]
        );
        // A protected chat offers neither the tone nor Save As.
        let protected = MediaFacts {
            can_save: false,
            ..facts
        };
        assert_eq!(
            media_actions(&voice, &protected),
            vec![MediaAction::ShowInFolder]
        );
    }

    #[test]
    fn finder_wording_on_macos() {
        assert_eq!(MediaAction::ShowInFolder.label(true), "Show in Finder");
        assert_eq!(MediaAction::ShowInFolder.label(false), "Show in Folder");
    }

    #[test]
    fn text_has_no_media_target() {
        assert!(media_target(&MessageContent::ScreenshotTaken).is_none());
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
        assert_eq!(
            seen_label(seen_kind(&MessageContent::ScreenshotTaken), 3),
            "3 Seen"
        );
        assert_eq!(
            seen_label(crate::message_menu::SeenKind::Seen, 0),
            "Nobody Viewed"
        );
        assert_eq!(
            seen_label(crate::message_menu::SeenKind::Listened, 2),
            "2 Listened"
        );
        assert_eq!(
            seen_label(crate::message_menu::SeenKind::Watched, 0),
            "Nobody Listened"
        );
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
        assert_eq!(
            read_status_label(MessageReadDate::Unread, &now),
            "Not seen yet"
        );
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
    fn save_names_prefer_the_files_own_name() {
        let mut t = target(MediaKind::Document);
        assert_eq!(
            crate::message_menu::suggested_save_name(&t, "/cache/x1.bin"),
            "name.bin"
        );
        t.file_name.clear();
        t.copy_name = Some("Artist - Song".into());
        assert_eq!(
            crate::message_menu::suggested_save_name(&t, "/cache/x1.mp3"),
            "Artist - Song.mp3"
        );
        t.copy_name = None;
        assert_eq!(
            crate::message_menu::suggested_save_name(&t, "/cache/photo_7.jpg"),
            "/cache/photo_7.jpg".to_string()
        );
    }

    #[test]
    fn edited_row_reads_like_the_read_date() {
        let now = civil_at(NOW, 0);
        let earlier = civil_at(NOW - 600, 0);
        assert_eq!(
            crate::message_menu::edited_label(&earlier, &now),
            "Edited today at 21:32"
        );
    }

    #[test]
    fn fact_check_wording_follows_the_existing_note() {
        assert_eq!(fact_check_label(false), "Add Fact Check");
        assert_eq!(fact_check_label(true), "Edit Fact Check");
    }

    #[test]
    fn timecodes_read_like_a_player() {
        assert_eq!(timecode_text(0.0), "0:00");
        assert_eq!(timecode_text(7.9), "0:07");
        assert_eq!(timecode_text(723.0), "12:03");
        assert_eq!(timecode_text(3723.0), "1:02:03");
        assert_eq!(timecode_text(-4.0), "0:00");
    }

    #[test]
    fn timecode_row_is_for_a_playing_voice_note_you_can_reply_to() {
        use crate::telegram::envelope::VoiceNoteContent;
        let voice = MessageContent::VoiceNote(VoiceNoteContent {
            duration: 20,
            waveform: Vec::new(),
            mime_type: String::new(),
            caption: String::new(),
            caption_entities: Vec::new(),
            is_listened: false,
            file_id: crate::ids::FileId(3),
            transcription: None,
        });
        assert!(timecode_offered(&voice, true, true));
        assert!(!timecode_offered(&voice, false, true));
        assert!(!timecode_offered(&voice, true, false));
        assert!(!timecode_offered(
            &MessageContent::ScreenshotTaken,
            true,
            true
        ));
    }

    #[test]
    fn timecode_insertion_pads_with_spaces() {
        assert_eq!(timecode_insertion("", "0:07"), "0:07 ");
        assert_eq!(timecode_insertion("see ", "0:07"), "0:07 ");
        assert_eq!(timecode_insertion("see", "0:07"), " 0:07 ");
    }

    #[test]
    fn private_links_warn_that_only_members_can_open_them() {
        assert_eq!(link_copied_note(true), "Link copied to clipboard.");
        assert_eq!(
            link_copied_note(false),
            "This link will only work for members of this chat."
        );
    }

    #[test]
    fn link_wording() {
        assert_eq!(copy_link_label(true), "Copy Post Link");
        assert_eq!(copy_link_label(false), "Copy Message Link");
    }

    #[test]
    fn noforwards_line_names_the_chat() {
        use crate::message_menu::noforwards_text;
        assert_eq!(
            noforwards_text(true, false, false, false, None),
            "Copying and forwarding is not allowed in this channel."
        );
        assert_eq!(
            noforwards_text(false, true, false, false, None),
            "Copying and forwarding is not allowed in this group."
        );
        assert_eq!(
            noforwards_text(false, false, true, false, Some("Bot")),
            "Copying and forwarding is not allowed from this bot."
        );
        assert_eq!(
            noforwards_text(false, false, false, true, Some("Mom")),
            "You disabled copying and forwarding in this chat."
        );
        assert_eq!(
            noforwards_text(false, false, false, false, Some("Mom")),
            "Mom disabled copying and forwarding in this chat."
        );
        assert_eq!(
            noforwards_text(false, false, false, false, None),
            "Copying and forwarding is not allowed in this chat."
        );
    }
}
