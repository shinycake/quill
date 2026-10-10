//! Connect driver: the message send pipeline.
use super::*;
use crate::composer::{
    AttachmentKind, ComposerEdit, ComposerEditKind, ComposerScheduling, ComposerSnapshot,
    DeleteConfirm, EditMediaKind, ForwardDraft, LinkPreviewChoice, SendOptions,
};
use crate::ids::{ChatId, MessageId, RequestId};
use crate::rich::RichBlock;
use crate::state::MessagesPurpose;
use crate::state::{ForwardFlight, RequestPurpose};
use crate::telegram::envelope::ChatKind;
use crate::telegram::requests::{
    AnimationSend, SendReply, StickerSend, VideoNoteSend, VideoNoteThumbnailSend, VideoSend,
    VoiceNoteSend, compose_rich_message_with_ai, compose_text_with_ai, create_rich_message_with_ai,
    delete_messages, edit_media_content, edit_message_caption, edit_message_media,
    edit_message_scheduling_state, edit_message_text, fix_rich_message_with_ai, fix_text_with_ai,
    forward_messages_with_options, get_chat_history, get_chat_scheduled_messages,
    get_full_rich_message, input_message_photo, input_message_video, open_message_content,
    recognize_speech, resend_messages, send_animation, send_document, send_message_album,
    send_photo, send_rich_message, send_sticker, send_text, send_video, send_video_note,
    send_voice_note,
};
use crate::voice::VoiceDraft;

mod ai;
mod delete_forward;
mod edit;
mod export;
mod media_send;

impl<S: JsonSender> ConnectDriver<S> {
    /// Load another page of history for the open chat (`from_message_id` = oldest, or 0).
    pub fn fetch_history(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat_id) = self.session.open_chat else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if self
            .session
            .histories
            .get(&chat_id.0)
            .is_some_and(|h| h.loaded_complete)
        {
            return Ok(None);
        }
        // R5: a failed first page waits for the explicit Retry row
        // (`retry_history`) instead of re-sending on every render.
        if self
            .session
            .histories
            .get(&chat_id.0)
            .is_some_and(|h| h.load_failed && h.messages.is_empty())
        {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetHistory, chat_id)
            || self
                .session
                .requests
                .has_purpose_for_chat(RequestPurpose::GetHistoryAround, chat_id)
        {
            return Ok(None);
        }
        // Opened with unread messages: the first page loads around the read
        // boundary, so the chat starts where the reader left off.
        if let Some(history) = self.session.histories.get(&chat_id.0)
            && history.messages.is_empty()
            && let Some(anchor) = history.unread_anchor
        {
            let extra = self.session.request_history_around(chat_id, anchor);
            if let Err(err) = self.sender.send_json(&get_chat_history(
                extra,
                chat_id,
                anchor,
                HISTORY_AROUND_OFFSET,
                HISTORY_AROUND_LIMIT,
                false,
            )) {
                self.session.requests.take(extra);
                return Err(err);
            }
            return Ok(Some(extra));
        }
        let from = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|h| h.oldest_id())
            .unwrap_or(MessageId(0));
        // The page's `from_message_id` rides on the pending request so the
        // reducer can tell a page that made no progress (end of history)
        // from a merely short one.
        let extra = self
            .session
            .request_for_message(RequestPurpose::GetHistory, chat_id, from);
        self.sender.send_json(&get_chat_history(
            extra,
            chat_id,
            from,
            0,
            HISTORY_PAGE_SIZE,
            false,
        ))?;
        Ok(Some(extra))
    }

    /// Load the page after the window's newest message while the window
    /// stops short of the chat's latest message (`HistoryState::has_newer`).
    pub fn fetch_history_newer(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat_id) = self.session.open_chat else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let Some(from) = self
            .session
            .histories
            .get(&chat_id.0)
            .filter(|h| h.has_newer && !h.newer_failed)
            .and_then(|h| h.newest_id())
        else {
            return Ok(None);
        };
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetHistoryNewer, chat_id)
        {
            return Ok(None);
        }
        let extra =
            self.session
                .request_for_message(RequestPurpose::GetHistoryNewer, chat_id, from);
        // A negative offset returns `-offset` messages newer than `from`
        // plus `from` itself (schema `getChatHistory`).
        if let Err(err) = self.sender.send_json(&get_chat_history(
            extra,
            chat_id,
            from,
            -(HISTORY_PAGE_SIZE - 1),
            HISTORY_PAGE_SIZE,
            false,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Jump the open chat to its latest messages: replace a window that
    /// stops short of them (and drop the unread divider) and load the
    /// newest page.
    pub fn jump_to_latest(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(chat_id) = self.session.open_chat else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let history = self.session.histories.entry(chat_id.0).or_default();
        history.unread_anchor = None;
        if !history.has_newer {
            return Ok(None);
        }
        self.session.reset_history_window(chat_id);
        self.fetch_history()
    }

    /// Parity slice 4: the forum topic a send to `chat_id` is addressed to.
    /// `Some` only when `chat_id` is the open chat and a topic is selected
    /// there; the `sendMessage` request then carries
    /// `topic_id = messageTopicForum{forum_topic_id}` (schema 1.8.67, lines
    /// 12200 and 3004). Story replies keep a null topic — they address the
    /// poster chat, never a topic.
    pub(crate) fn send_topic(&self, chat_id: ChatId) -> Option<i32> {
        if self.session.open_chat == Some(chat_id) {
            self.session.open_topic
        } else {
            None
        }
    }

    /// Parity slice 4: rejects sends into a closed forum topic. The
    /// composer is hidden there; this guards a stale-snapshot race.
    pub(crate) fn topic_send_is_closed(&self, chat_id: ChatId) -> bool {
        self.send_topic(chat_id).is_some_and(|_| {
            self.session
                .open_topic_info(chat_id)
                .is_some_and(|topic| topic.is_closed)
        })
    }

    /// Send `sendMessage` for a snapshot frozen at composer submit.
    /// Caption / path are not logged.
    pub fn send_text_snapshot(
        &mut self,
        snapshot: &ComposerSnapshot,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_snapshot(snapshot)
    }

    /// Send text, photo, document, or local video via `sendMessage` (TDLib 1.8.67).
    /// MED4: caption-length gate against the runtime
    /// `message_caption_length_max` option (TDLib 1.8.67, `schema/td_api.tl:6088`).
    /// Counts Unicode scalar values; TDLib's exact limit unit is not
    /// source-verified (assumption — TDLib remains the final gate, and a
    /// server refusal surfaces in the status note). Plain-text sends use the
    /// separate `message_text_length_max` option (untracked here — out of
    /// this slice).
    fn check_caption_length(&self, caption: &str) -> Result<(), ConnectSendError> {
        let limit = self.session.message_caption_length_max;
        if caption.chars().count() as i64 > i64::from(limit.max(0)) {
            Err(ConnectSendError::CaptionTooLong { limit })
        } else {
            Ok(())
        }
    }

    pub fn send_snapshot(
        &mut self,
        snapshot: &ComposerSnapshot,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if snapshot.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if snapshot.is_media_album() {
            return self.send_album_snapshot(snapshot);
        }
        let chat_id = snapshot.chat_id();
        // Phase 2.3: channel posting is admin-gated (`can_post` derives the
        // right from own membership); the driver rejects non-admin channel
        // sends the same way the hidden composer does.
        let can_post = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.can_post());
        if !can_post {
            return Err(ConnectSendError::InvalidRequest);
        }
        // Parity slice 4: never send into a closed forum topic (the
        // composer is hidden there; this guards a stale-snapshot race).
        if self.topic_send_is_closed(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let caption = snapshot.caption();
        if snapshot.attachment.is_none() && caption.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        // MED4: caption-length gate (runtime option; the counter in the
        // composer shows the same limit). Only the caption-carrying
        // paths are gated — plain text has its own (untracked) limit.
        if snapshot.attachment.is_some() || snapshot.is_media_album() {
            self.check_caption_length(caption)?;
        }
        // Slice G1: quote-carrying reply (`inputTextQuote`).
        let reply_to = snapshot.send_reply();
        // Validate the picked path before allocating `@extra`.
        let media_path = match snapshot.attachment.as_ref() {
            Some(att) => Some(
                att.send_path_str()
                    .ok_or(ConnectSendError::InvalidRequest)?,
            ),
            None => None,
        };
        let video_probe = match snapshot.attachment.as_ref() {
            Some(att) if att.kind == AttachmentKind::Video => Some(
                crate::video::probe_local_video(&att.path)
                    .map_err(|_| ConnectSendError::InvalidRequest)?,
            ),
            _ => None,
        };
        let video_note = match snapshot.attachment.as_ref() {
            Some(att) if att.kind == AttachmentKind::VideoNote => {
                let probe = crate::video::probe_local_video_note(&att.path)
                    .map_err(|_| ConnectSendError::InvalidRequest)?;
                let thumbnail = crate::video::write_video_note_thumbnail(&att.path).map(|thumb| {
                    VideoNoteThumbnailSend {
                        path: thumb.path.to_string_lossy().into_owned(),
                        width: thumb.width,
                        height: thumb.height,
                    }
                });
                Some(VideoNoteSend {
                    duration: probe.duration,
                    length: probe.length,
                    thumbnail,
                })
            }
            _ => None,
        };
        let extra = self
            .session
            .request(RequestPurpose::SendMessage, Some(chat_id));
        // Parity slice 4: sends from a topic view address the open topic.
        let topic_id = self.send_topic(chat_id);
        // Phase B3: TDLib accepts `inputMessagePhoto`/`inputMessageVideo`
        // `self_destruct_type` only in `chatTypePrivate` chats (its runtime
        // check is `dialog_id.get_type() != DialogType::User` → 400, and the
        // schema says "private chats only"). The choice is stripped for
        // every other chat kind here (defense in depth — the composer
        // picker is gated the same way), so a stale snapshot can never
        // turn a secret-chat send into a 400.
        let self_destruct = self
            .session
            .chats
            .get(&chat_id.0)
            .filter(|chat| matches!(chat.kind, ChatKind::Private { .. }))
            .and(snapshot.self_destruct);
        // M1 fix-up: `textEntityTypeBlockQuote` is not supported in secret
        // chats (schema) — strip it from captions here; the text path
        // does the same via `SendOptions::is_secret`.
        let is_secret = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }));
        let spoiler = snapshot.attachment.as_ref().is_some_and(|att| {
            att.spoiler && matches!(att.kind, AttachmentKind::Photo | AttachmentKind::Video)
        });
        // Contains caption / path — do not log `json`.
        let json = match (snapshot.attachment.as_ref(), media_path.as_deref()) {
            (Some(att), Some(path)) => match att.kind {
                AttachmentKind::Photo => send_photo(
                    extra,
                    chat_id,
                    topic_id,
                    path,
                    caption,
                    snapshot.caption_above_media,
                    reply_to,
                    self_destruct,
                    is_secret,
                ),
                AttachmentKind::Document => {
                    send_document(extra, chat_id, topic_id, path, caption, reply_to, is_secret)
                }
                AttachmentKind::Video => {
                    let probe = video_probe.ok_or_else(|| {
                        self.session.requests.take(extra);
                        ConnectSendError::InvalidRequest
                    })?;
                    send_video(
                        extra,
                        chat_id,
                        topic_id,
                        path,
                        &VideoSend {
                            duration: probe.duration,
                            width: probe.width,
                            height: probe.height,
                            supports_streaming: probe.supports_streaming,
                            self_destruct,
                        },
                        caption,
                        snapshot.caption_above_media,
                        reply_to,
                        is_secret,
                    )
                }
                AttachmentKind::VideoNote => {
                    let note = video_note.ok_or_else(|| {
                        self.session.requests.take(extra);
                        ConnectSendError::InvalidRequest
                    })?;
                    send_video_note(extra, chat_id, topic_id, path, &note, reply_to)
                }
            },
            (None, None) => {
                // Phase S1: secret chats never get link previews (TGX
                // default-off; previews are generated on Telegram servers,
                // which can't see E2E content).
                let is_secret = self
                    .session
                    .chats
                    .get(&chat_id.0)
                    .is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }));
                // M1: the composer send options ride the snapshot; secret
                // chats force the preview toggle off regardless, and mark
                // the send so `textEntityTypeBlockQuote` is stripped
                // (unsupported in secret chats).
                let mut send_options = snapshot.send_options;
                if is_secret {
                    send_options.link_preview_disabled = true;
                    send_options.is_secret = true;
                }
                // A lone dice emoji rolls a die (tdesktop does the same).
                match crate::telegram::requests::dice_emoji_in(
                    caption,
                    &self.session.sync.dice_emojis,
                ) {
                    Some(emoji) => crate::telegram::requests::send_dice(
                        extra,
                        chat_id,
                        topic_id,
                        &emoji,
                        reply_to.as_ref(),
                        &send_options,
                    ),
                    None => send_text(extra, chat_id, topic_id, caption, reply_to, &send_options),
                }
            }
            _ => {
                self.session.requests.take(extra);
                return Err(ConnectSendError::InvalidRequest);
            }
        };
        let json = if spoiler { with_spoiler(json) } else { json };
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

    /// M2: `sendMessage` with `inputMessageRichMessage` (TDLib 1.8.67, line
    /// 6084) — sends the rich editor's blocks. The composer clears only
    /// after a `message` response or a surfaced error; a failed send never
    /// reports success (the error is shown, the draft stays). No optimistic
    /// local row — M1's optimistic send is text-only, so a failed rich send
    /// can't strand a fake row.
    pub fn send_rich_snapshot(
        &mut self,
        chat_id: ChatId,
        blocks: &[RichBlock],
        reply_to: Option<SendReply>,
        options: &SendOptions,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        // Phase 2.3: channel posting is admin-gated, same as `send_snapshot`.
        let can_post = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.can_post());
        if !can_post {
            return Err(ConnectSendError::InvalidRequest);
        }
        // Parity slice 4: never send into a closed forum topic.
        if self.topic_send_is_closed(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let rich =
            crate::rich::input_rich_message(blocks).ok_or(ConnectSendError::InvalidRequest)?;
        let extra = self
            .session
            .request(RequestPurpose::SendMessage, Some(chat_id));
        let topic_id = self.send_topic(chat_id);
        let json = send_rich_message(extra, chat_id, topic_id, &rich, reply_to, options);
        let json = self.thread_routed(chat_id, json);
        if let Err(err) = self.sender.send_json(&json) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }
}

/// A built `sendMessage` with its media hidden behind a spoiler.
fn with_spoiler(json: String) -> String {
    match serde_json::from_str::<serde_json::Value>(&json) {
        Ok(mut value) => {
            value["input_message_content"]["has_spoiler"] = serde_json::Value::Bool(true);
            value.to_string()
        }
        Err(_) => json,
    }
}
