//! The small rounded preview in the reply / edit bar above the composer
//! (tdesktop `FieldHeader::paintEditOrReplyToMessage`,
//! `history/view/controls/history_view_compose_controls.cpp:788`): a
//! `st::historyReplyPreview` (32 px) square of the replied-to or edited
//! media, cropped to fill and rounded by `ImageRoundRadius::Small` (4 px).

use super::app::QuillApp;
use super::message_media::photo_display_path;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::{ChatId, FileId, MessageId};
use quill::local_path::sandboxed_display_path;
use quill::telegram::envelope::{MessageContent, MiniThumbnail};
use std::sync::Arc;

/// `st::historyReplyPreview`.
const PREVIEW_SIZE: f32 = 32.;

/// Where a message's preview picture can come from, best first.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct ThumbSources<'a> {
    /// Local files to try in order (the first one on disk wins).
    pub(crate) files: Vec<FileId>,
    /// The inline blurred stand-in while no file is on disk.
    pub(crate) mini: Option<&'a MiniThumbnail>,
}

/// The picture sources of media a reply or edit can point at; none for
/// text, files without a preview, and spoiler or secret media (tdesktop
/// covers those; Quill shows no preview rather than leak one).
pub(crate) fn thumb_sources(content: &MessageContent) -> ThumbSources<'_> {
    let ids = |ids: &[Option<FileId>]| ids.iter().flatten().copied().collect();
    match content {
        MessageContent::Photo(photo) if !photo.is_secret && !photo.has_spoiler => ThumbSources {
            files: photo
                .thumb_size()
                .into_iter()
                .chain(photo.largest_size())
                .map(|size| size.file_id)
                .collect(),
            mini: photo.minithumbnail.as_ref().filter(|m| !m.data.is_empty()),
        },
        MessageContent::Video(video) if !video.is_secret && !video.has_spoiler => ThumbSources {
            files: ids(&[video.thumb_file_id()]),
            mini: None,
        },
        MessageContent::Animation(animation) if !animation.is_secret && !animation.has_spoiler => {
            ThumbSources {
                files: ids(&[animation.thumb_file_id()]),
                mini: None,
            }
        }
        MessageContent::VideoNote(note) => ThumbSources {
            files: ids(&[note.thumb_file_id()]),
            mini: None,
        },
        MessageContent::Sticker(sticker) => ThumbSources {
            files: ids(&[sticker.thumb_file_id]),
            mini: None,
        },
        _ => ThumbSources::default(),
    }
}

impl QuillApp {
    /// The 32 px preview for the reply / edit bar of `(chat_id,
    /// message_id)`, when that message is media with a picture to show.
    pub(super) fn bar_thumbnail(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let message = session
            .histories
            .get(&chat_id.0)?
            .messages
            .get(&message_id.0)?;
        let sources = thumb_sources(&message.content);
        let roots = self.media_display_roots();
        let path = match &message.content {
            // Whatever size is already downloaded, thumbnail first.
            MessageContent::Photo(photo) => photo_display_path(photo, &session.media.files, &roots),
            _ => None,
        }
        .or_else(|| {
            sources.files.iter().find_map(|id| {
                session
                    .media
                    .files
                    .get(&id.0)
                    .and_then(|file| file.usable_path())
                    .and_then(|path| sandboxed_display_path(path, &roots))
            })
        });
        let frame = |this: Div| {
            this.size(px(PREVIEW_SIZE))
                .flex_none()
                .rounded(px(4.))
                .overflow_hidden()
                .bg(cx.theme().muted)
        };
        let picture = match (path, sources.mini) {
            (Some(path), _) => img(path)
                .size_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            (None, Some(mini)) => img(ImageSource::Image(Arc::new(gpui_kit::Image::from_bytes(
                gpui_kit::ImageFormat::Jpeg,
                mini.data.clone(),
            ))))
            .size_full()
            .object_fit(ObjectFit::Cover)
            .into_any_element(),
            (None, None) => return None,
        };
        Some(
            frame(div())
                .id(("bar-thumb", message_id.0 as u64))
                .child(picture)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::ui::composer_thumb::{ThumbSources, thumb_sources};
    use quill::telegram::envelope::MessageContent;

    #[test]
    fn text_has_no_preview() {
        let text = MessageContent::Text("hello".into());
        assert_eq!(thumb_sources(&text), ThumbSources::default());
    }
}
