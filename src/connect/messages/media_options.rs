//! Connect driver: the send box's media options (tdesktop `SendFilesBox`):
//! photos scaled to the quality limit, a video's cover and start frame, a
//! GIF sent with its caption, and paid media.
use super::*;
use crate::composer::{
    ComposerAttachment, MediaSendOptions, PAID_MEDIA_MAX_STARS, cover_send_path, paid_media_fits,
    photo_for_send, session_allows_cover, session_allows_paid_media,
};
use crate::telegram::requests::{
    LocalAnimation, PaidMediaItem, input_message_paid_media, send_paid_media, with_video_options,
};
use std::path::Path;

/// The path a file goes out with. A photo is scaled to the limit of the
/// "High Quality" choice first (`photo_for_send`), anything else goes as
/// picked.
pub(super) fn media_send_path(
    attachment: &ComposerAttachment,
    media: MediaSendOptions,
) -> Option<String> {
    let path = attachment.send_path_str()?;
    if attachment.kind != AttachmentKind::Photo {
        return Some(path);
    }
    let prepared = photo_for_send(&attachment.path, media.hd_photos);
    if prepared == attachment.path {
        Some(path)
    } else {
        Some(prepared.to_string_lossy().into_owned())
    }
}

/// The cover picked for a video, when the chat takes covers.
fn cover_path(
    session: &crate::state::Session,
    chat_id: ChatId,
    attachment: &ComposerAttachment,
) -> Option<String> {
    let cover = attachment.extras.cover.as_deref()?;
    if !session_allows_cover(session, chat_id) {
        return None;
    }
    let path = cover_send_path(cover)?;
    Some(path.to_string_lossy().into_owned())
}

/// Size and length of a GIF file (or an edited video sent as one). TDLib
/// takes the real values from the file, so an unreadable probe sends zeros.
pub(super) fn local_animation(path: &Path) -> LocalAnimation {
    crate::video::probe_with_ffprobe(path, false).map_or(
        LocalAnimation {
            duration: 0,
            width: 0,
            height: 0,
        },
        |probe| LocalAnimation {
            duration: probe.duration,
            width: probe.width,
            height: probe.height,
        },
    )
}

impl<S: JsonSender> ConnectDriver<S> {
    /// Put the video's cover and start frame on a built `inputMessageVideo`.
    pub(super) fn with_attachment_video_options(
        &self,
        chat_id: ChatId,
        attachment: &ComposerAttachment,
        content: serde_json::Value,
    ) -> serde_json::Value {
        let cover = cover_path(&self.session, chat_id, attachment);
        with_video_options(content, cover.as_deref(), attachment.extras.start_timestamp)
    }

    /// [`Self::with_attachment_video_options`] for a built `sendMessage`.
    pub(super) fn with_video_options_json(
        &self,
        chat_id: ChatId,
        attachment: &ComposerAttachment,
        json: String,
    ) -> String {
        match serde_json::from_str::<serde_json::Value>(&json) {
            Ok(mut value) => {
                let content = value["input_message_content"].take();
                value["input_message_content"] =
                    self.with_attachment_video_options(chat_id, attachment, content);
                value.to_string()
            }
            Err(_) => json,
        }
    }

    /// `sendMessage` + `inputMessagePaidMedia`: the photos and videos of the
    /// snapshot go out as one message behind a price in Stars. Only for a
    /// channel that allows paid media (tdesktop `canPostPaidMedia`).
    pub(super) fn send_paid_media_snapshot(
        &mut self,
        snapshot: &ComposerSnapshot,
    ) -> Result<RequestId, ConnectSendError> {
        let chat_id = snapshot.chat_id();
        let stars = snapshot.media.paid_stars;
        if !(1..=PAID_MEDIA_MAX_STARS).contains(&stars)
            || !session_allows_paid_media(&self.session, chat_id)
            || self.topic_send_is_closed(chat_id)
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let attachments: &[ComposerAttachment] = if !snapshot.album.is_empty() {
            &snapshot.album
        } else {
            snapshot.attachment.as_slice()
        };
        if !paid_media_fits(attachments, true) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let caption = snapshot.caption();
        self.check_caption_length(caption)?;
        let mut paths = Vec::with_capacity(attachments.len());
        let mut videos = Vec::with_capacity(attachments.len());
        for attachment in attachments {
            paths.push(
                media_send_path(attachment, snapshot.media)
                    .ok_or(ConnectSendError::InvalidRequest)?,
            );
            videos.push(match attachment.kind {
                AttachmentKind::Video => Some(
                    crate::video::probe_local_video(&attachment.path)
                        .map_err(|_| ConnectSendError::InvalidRequest)?,
                ),
                _ => None,
            });
        }
        let covers: Vec<Option<String>> = attachments
            .iter()
            .map(|attachment| cover_path(&self.session, chat_id, attachment))
            .collect();
        let items: Vec<PaidMediaItem<'_>> = attachments
            .iter()
            .enumerate()
            .map(|(index, attachment)| match videos[index] {
                Some(probe) => PaidMediaItem::Video {
                    path: &paths[index],
                    duration: probe.duration,
                    width: probe.width,
                    height: probe.height,
                    supports_streaming: probe.supports_streaming,
                    cover: covers[index].as_deref(),
                    start_timestamp: attachment.extras.start_timestamp,
                },
                None => PaidMediaItem::Photo {
                    path: &paths[index],
                },
            })
            .collect();
        let is_secret = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }));
        let content = input_message_paid_media(
            stars,
            &items,
            caption,
            snapshot.caption_above_media,
            is_secret,
        );
        let extra = self
            .session
            .request(RequestPurpose::SendMessage, Some(chat_id));
        let topic_id = self.send_topic(chat_id);
        let json = send_paid_media(extra, chat_id, topic_id, snapshot.send_reply(), content);
        let json = self.thread_routed(chat_id, json);
        match self.sender.send_json(&json) {
            Ok(()) => {
                let _ = self.cancel_outgoing_typing();
                Ok(extra)
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }
}
