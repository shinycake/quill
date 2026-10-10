//! Editing sent messages: text edits, media replacement and the link
//! preview choice.

use super::*;

/// Which TDLib edit constructor an own-message edit uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposerEditKind {
    /// `editMessageText` + `inputMessageText`.
    Text,
    /// `editMessageCaption` for outgoing photo/document captions.
    Caption,
}

/// Composer edit mode (tdesktop `FieldHeader::editMessage` / `_editMsgId`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerEdit {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub original_text: String,
    pub kind: ComposerEditKind,
    /// M1: the target is a scheduled send (`session.scheduled_messages`),
    /// not a history message. `edit_snapshot` validates against the
    /// scheduled list in that case.
    pub scheduled: bool,
    /// MED4: caption position for caption edits
    /// (`editMessageCaption.show_caption_above_media`, schema:12338).
    pub caption_above: bool,
    /// B5: what can be replaced (`editMessageMedia`) and the pending
    /// replacement file.
    pub media_edit: MediaEdit,
    /// B5: link-preview choices for `editMessageText` (tdesktop's
    /// "Link Preview Settings"); ignored for caption edits.
    pub link_preview: LinkPreviewChoice,
}

/// B5: the media currently on an edited message, as far as tdesktop's
/// `Media::allowsEditMedia` + `ComputeAlbumType` care.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditableMedia {
    Photo,
    Video,
    Animation,
    Document,
    Audio,
}

impl EditableMedia {
    /// The album type its group is (tdesktop `Ui::AlbumType`).
    fn album_type(self) -> AlbumType {
        match self {
            Self::Photo | Self::Video | Self::Animation => AlbumType::PhotoVideo,
            Self::Audio => AlbumType::Music,
            Self::Document => AlbumType::File,
        }
    }

    /// Photos, videos and animations carry `show_caption_above_media`.
    pub fn has_caption_position(self) -> bool {
        matches!(self, Self::Photo | Self::Video | Self::Animation)
    }
}

/// tdesktop `Ui::AlbumType`: what an album of this kind may contain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AlbumType {
    PhotoVideo,
    Music,
    File,
}

/// The kind a replacement file is sent as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditMediaKind {
    Photo,
    Video,
    Document,
    Audio,
}

/// `editMessageMedia` payload source: a picked or pasted file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditMediaReplacement {
    pub path: PathBuf,
    pub file_name: String,
    pub kind: EditMediaKind,
    pub spoiler: bool,
}

impl EditMediaReplacement {
    /// Path string safe to embed in `inputFileLocal`.
    pub fn send_path_str(&self) -> Option<String> {
        if !is_explicit_send_path(&self.path, &self.path) {
            return None;
        }
        Some(self.path.to_string_lossy().into_owned())
    }
}

/// B5: per-message media-edit state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MediaEdit {
    pub media: Option<EditableMedia>,
    /// The message belongs to an album (`media_album_id != 0`); the
    /// replacement must then fit that album's type.
    pub in_album: bool,
    pub replacement: Option<EditMediaReplacement>,
}

/// Audio extensions that go as `inputMessageAudio` (tdesktop treats
/// songs as `Music` in albums).
fn is_audio_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|ext| matches!(ext.as_str(), "mp3" | "m4a" | "ogg" | "oga" | "flac" | "wav"))
}

/// tdesktop `Core::IsMimeSticker`: webp and Lottie stickers can't replace
/// media (`lng_edit_media_invalid_file`).
fn is_sticker_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|ext| matches!(ext.as_str(), "webp" | "tgs" | "webm"))
}

/// tdesktop `lng_edit_media_invalid_file`.
pub const EDIT_MEDIA_INVALID_FILE: &str = "Sorry, no way to use this file.";
/// tdesktop `lng_edit_media_album_error`.
pub const EDIT_MEDIA_ALBUM_ERROR: &str = "This file cannot be saved as a part of an album.";

/// B5: link-preview half of `editMessageText` / `sendMessage`
/// (`linkPreviewOptions`, tdesktop `WebPageDraft`). `link_index` picks
/// which URL of the text drives the preview ("Click on a link to
/// generate its preview.").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LinkPreviewChoice {
    pub disabled: bool,
    pub above_text: bool,
    pub media: PreviewMediaSize,
    pub link_index: usize,
}

impl LinkPreviewChoice {
    /// True when the choice is the default (send `link_preview_options: null`).
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The URL the options pin, clamped to the links present.
    pub fn chosen_url(&self, text: &str) -> String {
        let urls = find_urls(text);
        urls.get(self.link_index)
            .or_else(|| urls.last())
            .cloned()
            .unwrap_or_default()
    }
}

impl ComposerEdit {
    /// B5: mark the edited message as part of an album.
    pub fn in_album(mut self, in_album: bool) -> Self {
        self.media_edit.in_album = in_album;
        self
    }

    /// tdesktop `HistoryItem::allowsEditMedia`: photos, videos, GIFs,
    /// documents and music can be replaced; voice and video notes,
    /// stickers and self-destructing media cannot.
    pub fn allows_replace(&self) -> bool {
        self.media_edit.media.is_some() && !self.scheduled
    }

    /// tdesktop `ChooseReplacement::checkResult` + `canBeInAlbumType`:
    /// turn a picked file into a replacement, or the toast to show.
    /// `as_file` is the "Send as a document" checkbox, only meaningful
    /// outside albums (`CanToggleCompressed`).
    pub fn replacement_for(
        &self,
        candidate: &Path,
        as_file: bool,
    ) -> Result<EditMediaReplacement, &'static str> {
        let Some(current) = self.media_edit.media else {
            return Err(EDIT_MEDIA_INVALID_FILE);
        };
        if is_sticker_path(candidate) {
            return Err(EDIT_MEDIA_INVALID_FILE);
        }
        let path = pick_send_path(candidate).ok_or(EDIT_MEDIA_INVALID_FILE)?;
        let audio = is_audio_path(&path);
        let media = media_kind_for(&path);
        let kind = if self.media_edit.in_album {
            match current.album_type() {
                AlbumType::PhotoVideo => match media {
                    Some(AttachmentKind::Photo) => EditMediaKind::Photo,
                    Some(AttachmentKind::Video) => EditMediaKind::Video,
                    _ => return Err(EDIT_MEDIA_ALBUM_ERROR),
                },
                AlbumType::Music if audio => EditMediaKind::Audio,
                AlbumType::Music => return Err(EDIT_MEDIA_ALBUM_ERROR),
                AlbumType::File => EditMediaKind::Document,
            }
        } else if audio {
            EditMediaKind::Audio
        } else {
            match (media, as_file) {
                (Some(AttachmentKind::Photo), false) => EditMediaKind::Photo,
                (Some(AttachmentKind::Video), false) => EditMediaKind::Video,
                _ => EditMediaKind::Document,
            }
        };
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();
        Ok(EditMediaReplacement {
            path,
            file_name,
            kind,
            spoiler: false,
        })
    }

    /// Whether "Send as a document" is offered for a replacement
    /// (tdesktop `CanToggleCompressed`: outside albums, for photo/video files).
    pub fn can_toggle_as_file(&self, replacement: &EditMediaReplacement) -> bool {
        !self.media_edit.in_album
            && replacement.kind != EditMediaKind::Audio
            && media_kind_for(&replacement.path).is_some()
    }

    /// Whether the (possibly replaced) media carries the caption-position flag.
    pub fn caption_position_applies(&self) -> bool {
        match self.media_edit.replacement.as_ref() {
            Some(rep) => matches!(rep.kind, EditMediaKind::Photo | EditMediaKind::Video),
            None => self
                .media_edit
                .media
                .is_some_and(EditableMedia::has_caption_position),
        }
    }
}

impl ComposerEdit {
    /// Own outgoing, already-sent messages only. Incoming / pending / unsupported
    /// stay out of this slice (schema `messageProperties.can_be_edited` is the
    /// live gate; official clients call `getMessageProperties`).
    pub fn from_own_content(
        chat_id: ChatId,
        message_id: MessageId,
        is_outgoing: bool,
        pending: bool,
        content: &MessageContent,
    ) -> Option<Self> {
        if !is_outgoing || pending {
            return None;
        }
        let (kind, original_text) = match content {
            MessageContent::Text(text) => (ComposerEditKind::Text, text.text.clone()),
            MessageContent::Photo(photo) => (ComposerEditKind::Caption, photo.caption.clone()),
            MessageContent::Document(doc) => (ComposerEditKind::Caption, doc.caption.clone()),
            MessageContent::VoiceNote(note) => (ComposerEditKind::Caption, note.caption.clone()),
            MessageContent::Animation(animation) => {
                (ComposerEditKind::Caption, animation.caption.clone())
            }
            MessageContent::Video(video) => (ComposerEditKind::Caption, video.caption.clone()),
            MessageContent::Audio(audio) => (ComposerEditKind::Caption, audio.caption.clone()),
            MessageContent::VideoNote(_)
            | MessageContent::Sticker(_)
            | MessageContent::Poll(_)
            | MessageContent::Checklist(_)
            | MessageContent::Location(_)
            | MessageContent::Venue(_)
            | MessageContent::Contact(_)
            | MessageContent::Dice(_)
            // Phase B4: timer-change service rows are not editable.
            | MessageContent::ChatTtlChanged { .. }
            // Phase C2f: group-call invitations are not editable.
            | MessageContent::GroupCallInvitation { .. }
            // Phase C2i: call entries are not editable.
            | MessageContent::Call { .. }
            // Phase S1: screenshot-taken service rows are not editable.
            | MessageContent::ScreenshotTaken
            // Slice C2k: community service rows are not editable.
            | MessageContent::ChatAddedToCommunity { .. }
            | MessageContent::ChatRemovedFromCommunity
            // Slice G9: community service rows are not editable.
            | MessageContent::ChatJoinFromCommunity { .. }
            // M2: rich messages are edited in the rich editor, not here.
            | MessageContent::RichMessage(_)
            // B1: games are not editable.
            | MessageContent::Game(_)
            // Slice P1: invoices and payment notices are not editable.
            | MessageContent::Invoice(_)
            | MessageContent::PaymentSuccessful(_)
            | MessageContent::PaymentReceived(_)
            | MessageContent::Action(_)
            | MessageContent::Unsupported { .. } => return None,
        };
        Some(Self {
            chat_id,
            message_id,
            original_text,
            kind,
            scheduled: false,
            media_edit: MediaEdit {
                media: match content {
                    MessageContent::Photo(_) => Some(EditableMedia::Photo),
                    MessageContent::Video(_) => Some(EditableMedia::Video),
                    MessageContent::Animation(_) => Some(EditableMedia::Animation),
                    MessageContent::Document(_) => Some(EditableMedia::Document),
                    MessageContent::Audio(_) => Some(EditableMedia::Audio),
                    _ => None,
                },
                in_album: false,
                replacement: None,
            },
            link_preview: match content {
                // tdesktop seeds the draft with the message's own preview.
                MessageContent::Text(text) => LinkPreviewChoice {
                    above_text: text
                        .link_preview
                        .as_ref()
                        .is_some_and(|preview| preview.show_above_text),
                    ..LinkPreviewChoice::default()
                },
                _ => LinkPreviewChoice::default(),
            },
            // MED4: preserve the message's caption position on edit
            // (`editMessageCaption.show_caption_above_media`,
            // schema:12338). The composer toggle can flip it.
            caption_above: match content {
                MessageContent::Photo(photo) => photo.show_caption_above_media,
                MessageContent::Video(video) => video.show_caption_above_media,
                MessageContent::Animation(animation) => animation.show_caption_above_media,
                _ => false,
            },
        })
    }
}

/// Enter edit: stash the current field as the normal draft (tdesktop
/// `DraftType::Normal`) and load the message text into the field.
pub fn begin_edit_draft(
    current_text: String,
    edit: ComposerEdit,
) -> (ComposerEdit, String, String) {
    let field = edit.original_text.clone();
    (edit, field, current_text)
}

/// tdesktop `ComposeControls::cancelEditMessage`: clear the edit header,
/// then `applyDraft()` restores the **normal** draft — not the typed edit.
pub fn cancel_edit_draft(
    edit: Option<ComposerEdit>,
    saved_draft: String,
) -> (Option<ComposerEdit>, String) {
    let _ = edit;
    (None, saved_draft)
}

/// Enter edit without dropping the normal draft's reply. Flush `saved_reply`
/// with the stashed text first; cancel/finish restores both.
pub fn begin_edit_keeping_reply(
    current_text: String,
    edit: ComposerEdit,
    reply: Option<ComposerReplyTo>,
) -> (ComposerEdit, String, String, Option<ComposerReplyTo>) {
    let (edit, field, saved) = begin_edit_draft(current_text, edit);
    (edit, field, saved, reply)
}

/// Cancel edit: normal draft text and its reply come back together.
pub fn cancel_edit_keeping_reply(
    saved_draft: String,
    saved_reply: Option<ComposerReplyTo>,
) -> (String, Option<ComposerReplyTo>) {
    (saved_draft, saved_reply)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit_with(media: Option<EditableMedia>, in_album: bool) -> ComposerEdit {
        ComposerEdit {
            chat_id: ChatId(1),
            message_id: MessageId(2),
            original_text: String::new(),
            kind: ComposerEditKind::Caption,
            scheduled: false,
            caption_above: false,
            media_edit: MediaEdit {
                media,
                in_album,
                replacement: None,
            },
            link_preview: LinkPreviewChoice::default(),
        }
    }

    fn temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("quill-edit-media-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, [1u8]).unwrap();
        path
    }

    #[test]
    fn replace_is_offered_only_for_replaceable_media() {
        // tdesktop `allowsEditMedia`: not voice, video notes, stickers.
        assert!(edit_with(Some(EditableMedia::Photo), false).allows_replace());
        assert!(edit_with(Some(EditableMedia::Audio), false).allows_replace());
        assert!(!edit_with(None, false).allows_replace());
        let mut scheduled = edit_with(Some(EditableMedia::Photo), false);
        scheduled.scheduled = true;
        assert!(!scheduled.allows_replace());
    }

    #[test]
    fn replacement_outside_an_album_follows_the_file_and_as_file() {
        let edit = edit_with(Some(EditableMedia::Document), false);
        let png = temp_file("a.png");
        assert_eq!(
            edit.replacement_for(&png, false).unwrap().kind,
            EditMediaKind::Photo
        );
        let as_file = edit.replacement_for(&png, true).unwrap();
        assert_eq!(as_file.kind, EditMediaKind::Document);
        assert!(edit.can_toggle_as_file(&as_file));
        let mp4 = temp_file("b.mp4");
        assert_eq!(
            edit.replacement_for(&mp4, false).unwrap().kind,
            EditMediaKind::Video
        );
        let mp3 = temp_file("c.mp3");
        let audio = edit.replacement_for(&mp3, true).unwrap();
        assert_eq!(audio.kind, EditMediaKind::Audio);
        assert!(!edit.can_toggle_as_file(&audio));
        let txt = temp_file("d.txt");
        assert_eq!(
            edit.replacement_for(&txt, false).unwrap().kind,
            EditMediaKind::Document
        );
    }

    #[test]
    fn replacement_rejects_stickers_and_missing_files() {
        let edit = edit_with(Some(EditableMedia::Photo), false);
        assert_eq!(
            edit.replacement_for(&temp_file("s.webp"), false),
            Err(EDIT_MEDIA_INVALID_FILE)
        );
        assert_eq!(
            edit.replacement_for(&temp_file("s.tgs"), false),
            Err(EDIT_MEDIA_INVALID_FILE)
        );
        assert_eq!(
            edit.replacement_for(Path::new("/nonexistent/x.png"), false),
            Err(EDIT_MEDIA_INVALID_FILE)
        );
        assert_eq!(
            edit_with(None, false).replacement_for(&temp_file("e.png"), false),
            Err(EDIT_MEDIA_INVALID_FILE)
        );
    }

    #[test]
    fn album_items_only_accept_their_albums_type() {
        let png = temp_file("p.png");
        let mp3 = temp_file("m.mp3");
        let txt = temp_file("t.txt");
        // Photo/video album: photo and video only, never as a file.
        let media = edit_with(Some(EditableMedia::Video), true);
        assert_eq!(
            media.replacement_for(&png, true).unwrap().kind,
            EditMediaKind::Photo
        );
        assert_eq!(
            media.replacement_for(&txt, false),
            Err(EDIT_MEDIA_ALBUM_ERROR)
        );
        assert_eq!(
            media.replacement_for(&mp3, false),
            Err(EDIT_MEDIA_ALBUM_ERROR)
        );
        assert!(!media.can_toggle_as_file(&media.replacement_for(&png, false).unwrap()));
        // Music album: audio only.
        let music = edit_with(Some(EditableMedia::Audio), true);
        assert_eq!(
            music.replacement_for(&mp3, false).unwrap().kind,
            EditMediaKind::Audio
        );
        assert_eq!(
            music.replacement_for(&png, false),
            Err(EDIT_MEDIA_ALBUM_ERROR)
        );
        // File album: anything goes as a document.
        let files = edit_with(Some(EditableMedia::Document), true);
        assert_eq!(
            files.replacement_for(&png, false).unwrap().kind,
            EditMediaKind::Document
        );
    }

    #[test]
    fn caption_position_follows_the_replacement_kind() {
        let mut edit = edit_with(Some(EditableMedia::Photo), false);
        assert!(edit.caption_position_applies());
        edit.media_edit.replacement = edit.replacement_for(&temp_file("k.txt"), false).ok();
        assert!(!edit.caption_position_applies());
        assert!(!edit_with(Some(EditableMedia::Document), false).caption_position_applies());
        assert!(edit_with(Some(EditableMedia::Animation), false).caption_position_applies());
    }

    #[test]
    fn text_edit_seeds_preview_position_from_the_message() {
        let edit = ComposerEdit::from_own_content(
            ChatId(1),
            MessageId(2),
            true,
            false,
            &MessageContent::Text("hi".into()),
        )
        .unwrap();
        assert!(edit.link_preview.is_default());
        assert!(!edit.allows_replace());
    }

    #[test]
    fn own_outgoing_text_can_enter_edit_incoming_cannot() {
        let outgoing = ComposerEdit::from_own_content(
            ChatId(11),
            MessageId(102),
            true,
            false,
            &MessageContent::Text("Reply from the session reducer.".into()),
        )
        .unwrap();
        assert_eq!(outgoing.kind, ComposerEditKind::Text);
        assert_eq!(outgoing.original_text, "Reply from the session reducer.");
        assert!(
            ComposerEdit::from_own_content(
                ChatId(11),
                MessageId(101),
                false,
                false,
                &MessageContent::Text("incoming".into()),
            )
            .is_none()
        );
        assert!(
            ComposerEdit::from_own_content(
                ChatId(11),
                MessageId(-1),
                true,
                true,
                &MessageContent::Text("pending".into()),
            )
            .is_none()
        );
    }

    #[test]
    fn cancel_edit_restores_unrelated_draft() {
        let edit = ComposerEdit::from_own_content(
            ChatId(11),
            MessageId(102),
            true,
            false,
            &MessageContent::Text("original outgoing".into()),
        )
        .unwrap();
        let (edit, field, saved) = begin_edit_draft("keep this draft".into(), edit);
        assert_eq!(field, "original outgoing");
        assert_eq!(saved, "keep this draft");
        let (cleared, restored) = cancel_edit_draft(Some(edit), saved);
        assert_eq!(cleared, None);
        assert_eq!(restored, "keep this draft");
    }

    #[test]
    fn edit_keeps_reply_on_the_normal_draft() {
        let edit = ComposerEdit::from_own_content(
            ChatId(11),
            MessageId(102),
            true,
            false,
            &MessageContent::Text("original outgoing".into()),
        )
        .unwrap();
        let reply = ComposerReplyTo::new(ChatId(11), MessageId(101), "quoted");
        let (edit, field, saved, stashed) =
            begin_edit_keeping_reply("keep this draft".into(), edit, Some(reply.clone()));
        assert_eq!(field, "original outgoing");
        assert_eq!(saved, "keep this draft");
        assert_eq!(stashed, Some(reply.clone()));
        let _ = edit;
        let (restored, reply_back) = cancel_edit_keeping_reply(saved, stashed);
        assert_eq!(restored, "keep this draft");
        assert_eq!(reply_back, Some(reply));
    }
}
