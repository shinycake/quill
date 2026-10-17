//! "Edit Image" and "Edit Cover" for the media of an own message being
//! edited (tdesktop `EditCaptionBox::showMenu` and
//! `MediaEditManager::CanEditVideoCover`).
//!
//! tdesktop offers these in the media preview menu of the edit box, not in
//! the message context menu: "Edit Image" opens its photo editor on the
//! photo (or on the replacement file), "Edit Cover" picks an image that
//! becomes the video's cover. Both are applied when the edit is saved, as
//! an `editMessageMedia`. tdesktop's video editor ("Edit Video") works on a
//! prepared local file only, so it is offered for a staged replacement
//! video (`video_edit.rs`; `docs/decisions/codex-send-box-options.md`).

use super::*;
use crate::ids::FileId;
use crate::telegram::envelope::{ChatKind, VideoContent};

/// One entry of the edit-media menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditMediaTool {
    /// tdesktop `lng_context_draw`: crop, rotate, draw, stickers.
    EditImage,
    /// tdesktop `lng_context_edit_cover`: choose a new cover for a video.
    EditCover,
    /// tdesktop `lng_context_edit_video`: trim, cover frame, quality and
    /// GIF for a video file staged as the replacement.
    EditVideo,
}

impl EditMediaTool {
    /// Menu label (tdesktop `lng_context_draw` is "Edit Image").
    pub fn label(self) -> &'static str {
        match self {
            Self::EditImage => "Edit Image",
            Self::EditCover => "Edit Cover",
            Self::EditVideo => "Edit Video",
        }
    }
}

/// tdesktop `lng_context_clear_cover`.
pub const CLEAR_COVER_LABEL: &str = "Clear Cover";

/// The video an edited message already carries, enough to resend it by
/// file id with a new cover (`inputFileId`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingVideo {
    pub file_id: FileId,
    pub duration: i32,
    pub width: i32,
    pub height: i32,
    pub supports_streaming: bool,
    pub start_timestamp: i32,
    pub has_spoiler: bool,
}

impl ExistingVideo {
    /// `None` for self-destructing videos (no cover support) and for
    /// videos without a file or dimensions (tdesktop
    /// `CanEditVideoCover`: `!document->dimensions.isEmpty()`).
    pub fn from_content(video: &VideoContent) -> Option<Self> {
        let file_id = video.play_file_id()?;
        (!video.is_secret && video.width > 0 && video.height > 0).then_some(Self {
            file_id,
            duration: video.duration,
            width: video.width,
            height: video.height,
            supports_streaming: video.supports_streaming,
            start_timestamp: video.start_timestamp,
            has_spoiler: video.has_spoiler,
        })
    }
}

/// tdesktop `CanEditVideoCover`: the peer is a channel, Saved Messages or
/// a bot. Secret chats and groups never qualify.
pub fn cover_peer_allows(kind: &ChatKind, is_saved_messages: bool, is_bot: bool) -> bool {
    match kind {
        ChatKind::Supergroup { is_channel, .. } => *is_channel,
        ChatKind::Private { .. } => is_saved_messages || is_bot,
        _ => false,
    }
}

/// [`cover_peer_allows`] for a chat of a session.
pub fn session_allows_cover(session: &crate::state::Session, chat_id: ChatId) -> bool {
    let Some(chat) = session.chats.get(&chat_id.0) else {
        return false;
    };
    let is_bot = match &chat.kind {
        ChatKind::Private { user_id } => session.is_bot_user(user_id.0),
        _ => false,
    };
    cover_peer_allows(&chat.kind, session.is_saved_messages(chat_id), is_bot)
}

/// A cover must be an image (tdesktop `FileDialog::ImagesFilter`).
pub fn cover_send_path(candidate: &Path) -> Option<PathBuf> {
    let path = pick_send_path(candidate)?;
    matches!(media_kind_for(&path), Some(AttachmentKind::Photo)).then_some(path)
}

impl ComposerEdit {
    /// The video the cover would belong to: the staged replacement video,
    /// else the video already on the message.
    fn cover_applies(&self) -> bool {
        match self.media_edit.replacement.as_ref() {
            Some(rep) => rep.kind == EditMediaKind::Video,
            None => self.media_edit.video.is_some(),
        }
    }

    /// The staged cover, only while it still applies (a replacement that
    /// is no longer a video drops it).
    pub fn effective_cover(&self) -> Option<&Path> {
        self.media_edit
            .cover
            .as_deref()
            .filter(|_| self.allows_replace() && self.cover_applies())
    }

    /// tdesktop's edit-media menu entries for this edit. `cover_peer_ok`
    /// is [`session_allows_cover`] for the chat.
    pub fn edit_media_tools(&self, cover_peer_ok: bool) -> Vec<EditMediaTool> {
        if !self.allows_replace() {
            return Vec::new();
        }
        let mut tools = Vec::new();
        // `canDraw`: a photo file, or the existing photo not sent as file.
        let image = match self.media_edit.replacement.as_ref() {
            Some(rep) => rep.kind == EditMediaKind::Photo,
            None => self.media_edit.media == Some(EditableMedia::Photo),
        };
        if image {
            tools.push(EditMediaTool::EditImage);
        }
        // `canEditVideo`: only a local video that is not sent as a file.
        if self
            .media_edit
            .replacement
            .as_ref()
            .is_some_and(|rep| rep.kind == EditMediaKind::Video)
        {
            tools.push(EditMediaTool::EditVideo);
        }
        if cover_peer_ok && self.cover_applies() {
            tools.push(EditMediaTool::EditCover);
        }
        tools
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(media: Option<EditableMedia>, video: bool) -> ComposerEdit {
        ComposerEdit {
            chat_id: ChatId(1),
            message_id: MessageId(2),
            kind: ComposerEditKind::Caption,
            scheduled: false,
            caption_above: false,
            media_edit: MediaEdit {
                media,
                video: video.then_some(ExistingVideo {
                    file_id: FileId(5),
                    duration: 3,
                    width: 640,
                    height: 360,
                    supports_streaming: true,
                    start_timestamp: 0,
                    has_spoiler: false,
                }),
                ..MediaEdit::default()
            },
            link_preview: LinkPreviewChoice::default(),
        }
    }

    fn replacement(kind: EditMediaKind) -> EditMediaReplacement {
        EditMediaReplacement {
            path: PathBuf::from("/tmp/x.png"),
            file_name: "x.png".into(),
            kind,
            spoiler: false,
        }
    }

    #[test]
    fn photo_offers_edit_image_only() {
        let edit = edit(Some(EditableMedia::Photo), false);
        assert_eq!(edit.edit_media_tools(true), [EditMediaTool::EditImage]);
    }

    #[test]
    fn photo_sent_as_file_has_no_edit_image() {
        let mut edit = edit(Some(EditableMedia::Photo), false);
        edit.media_edit.replacement = Some(replacement(EditMediaKind::Document));
        assert!(edit.edit_media_tools(true).is_empty());
    }

    #[test]
    fn staged_photo_replacement_offers_edit_image() {
        let mut edit = edit(Some(EditableMedia::Document), false);
        edit.media_edit.replacement = Some(replacement(EditMediaKind::Photo));
        assert_eq!(edit.edit_media_tools(false), [EditMediaTool::EditImage]);
    }

    #[test]
    fn video_cover_needs_a_channel_saved_messages_or_bot() {
        let edit = edit(Some(EditableMedia::Video), true);
        assert_eq!(edit.edit_media_tools(true), [EditMediaTool::EditCover]);
        assert!(edit.edit_media_tools(false).is_empty());
    }

    #[test]
    fn edit_video_is_for_a_staged_replacement_video_only() {
        let mut edit = edit(Some(EditableMedia::Video), true);
        // The video already on the message is not a local file.
        assert!(
            !edit
                .edit_media_tools(true)
                .contains(&EditMediaTool::EditVideo)
        );
        edit.media_edit.replacement = Some(replacement(EditMediaKind::Video));
        assert_eq!(
            edit.edit_media_tools(true),
            [EditMediaTool::EditVideo, EditMediaTool::EditCover]
        );
        // Without a cover peer the video can still be edited.
        assert_eq!(edit.edit_media_tools(false), [EditMediaTool::EditVideo]);
        edit.media_edit.replacement = Some(replacement(EditMediaKind::Document));
        assert!(
            !edit
                .edit_media_tools(true)
                .contains(&EditMediaTool::EditVideo)
        );
        assert_eq!(EditMediaTool::EditVideo.label(), "Edit Video");
    }

    #[test]
    fn video_without_known_size_has_no_cover_tool() {
        let edit = edit(Some(EditableMedia::Video), false);
        assert!(edit.edit_media_tools(true).is_empty());
    }

    #[test]
    fn animations_and_documents_get_no_tools() {
        assert!(
            edit(Some(EditableMedia::Animation), false)
                .edit_media_tools(true)
                .is_empty()
        );
        assert!(
            edit(Some(EditableMedia::Document), false)
                .edit_media_tools(true)
                .is_empty()
        );
    }

    #[test]
    fn scheduled_edits_get_no_tools() {
        let mut edit = edit(Some(EditableMedia::Photo), false);
        edit.scheduled = true;
        assert!(edit.edit_media_tools(true).is_empty());
    }

    #[test]
    fn replacement_video_offers_cover_and_non_video_drops_it() {
        let mut edit = edit(Some(EditableMedia::Document), false);
        edit.media_edit.replacement = Some(replacement(EditMediaKind::Video));
        edit.media_edit.cover = Some(PathBuf::from("/tmp/c.png"));
        assert_eq!(
            edit.edit_media_tools(true),
            [EditMediaTool::EditVideo, EditMediaTool::EditCover]
        );
        assert!(edit.effective_cover().is_some());
        edit.media_edit.replacement = Some(replacement(EditMediaKind::Document));
        assert!(edit.effective_cover().is_none());
    }

    #[test]
    fn cover_peers_match_tdesktop() {
        let channel = ChatKind::Supergroup {
            supergroup_id: 1,
            is_channel: true,
        };
        let group = ChatKind::Supergroup {
            supergroup_id: 1,
            is_channel: false,
        };
        let private = ChatKind::Private {
            user_id: crate::ids::UserId(9),
        };
        assert!(cover_peer_allows(&channel, false, false));
        assert!(!cover_peer_allows(&group, false, false));
        assert!(cover_peer_allows(&private, true, false));
        assert!(cover_peer_allows(&private, false, true));
        assert!(!cover_peer_allows(&private, false, false));
        assert!(!cover_peer_allows(&ChatKind::Unknown, true, true));
    }

    #[test]
    fn cover_must_be_an_existing_image() {
        let dir = crate::scratch::process_dir("quill-cover");
        let png = dir.join("c.png");
        let mp4 = dir.join("c.mp4");
        std::fs::write(&png, [1u8]).unwrap();
        std::fs::write(&mp4, [1u8]).unwrap();
        assert!(cover_send_path(&png).is_some());
        assert!(cover_send_path(&mp4).is_none());
        assert!(cover_send_path(&dir.join("missing.png")).is_none());
    }
}
