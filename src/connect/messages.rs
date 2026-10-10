//! Connect driver: the message send pipeline.
use super::*;
use crate::composer::{
    AttachmentKind, ComposerEdit, ComposerEditKind, ComposerScheduling, ComposerSnapshot,
    DeleteConfirm, EditMediaKind, ForwardDraft, LinkPreviewChoice, SendOptions,
};
use crate::ids::{ChatId, MessageId, RequestId};
use crate::rich::RichBlock;
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

impl<S: JsonSender> ConnectDriver<S> {
    pub fn start_account_export(
        &mut self,
        folder: &std::path::Path,
        media: bool,
    ) -> std::io::Result<()> {
        if !self.chats_path_active() || self.session.account_export.is_some() {
            return Err(std::io::Error::other(
                "Account export is unavailable or already running",
            ));
        }
        self.session.account_export = Some(crate::account_export::AccountExport::start(
            folder,
            self.tdlib_files().to_path_buf(),
            media,
        )?);
        Ok(())
    }
    pub fn pump_account_export(&mut self) {
        if let Some(mut request) = self
            .session
            .account_export
            .as_ref()
            .and_then(|export| export.next_request())
        {
            let extra = self.session.request(RequestPurpose::ExportAccount, None);
            request["@extra"] = serde_json::json!({"quill_account_export":extra.as_extra()});
            if let Some(export) = self.session.account_export.as_mut() {
                export.pending = Some(extra);
            }
            if self.sender.send_json(&request.to_string()).is_err() {
                self.session.requests.take(extra);
                if let Some(export) = self.session.account_export.as_mut() {
                    export.fail_send(extra);
                }
            }
        }
    }

    /// `parity:platform-chat-export` — start exporting a chat's history to
    /// a JSON file. Refuses while another export is running.
    pub fn start_chat_export(
        &mut self,
        chat_id: ChatId,
        chat_title: String,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.chat_export.is_some() {
            return Err(ConnectSendError::InvalidRequest);
        }
        // Protected chats can't be saved or forwarded, so they can't be
        // exported either (tdesktop `PeerData::canExportChatHistory`
        // requires `allowsForwarding()`).
        if self.session.chat_has_protected_content(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.chat_export = Some(crate::chat_export::ChatExportState::new(
            chat_id, chat_title,
        ));
        self.send_export_page()
    }

    /// `parity:platform-chat-export` — drive the export forward: send the
    /// next page while history remains, write the file when paging is
    /// done. Called from the app's live poll loop; no-op without an export.
    pub fn pump_chat_export(&mut self) {
        let done_paging = self
            .session
            .chat_export
            .as_ref()
            .is_some_and(|e| e.done_paging && !e.settled());
        if done_paging && let Some(export) = self.session.chat_export.as_mut() {
            let dir = crate::chat_export::default_export_dir();
            match crate::chat_export::write_export(export, &dir) {
                Ok(path) => export.finished_path = Some(path),
                Err(err) => export.failed = Some(format!("could not write export file: {err}")),
            }
            return;
        }
        let need_page = self
            .session
            .chat_export
            .as_ref()
            .is_some_and(|e| !e.in_flight && !e.done_paging && !e.settled());
        if need_page
            && self.send_export_page().is_err()
            && let Some(export) = self.session.chat_export.as_mut()
        {
            export.failed = Some("failed to send TDLib request".into());
        }
    }

    /// Send one export `getChatHistory` page for the active export.
    fn send_export_page(&mut self) -> Result<(), ConnectSendError> {
        let (chat_id, from) = {
            let export = self
                .session
                .chat_export
                .as_ref()
                .ok_or(ConnectSendError::InvalidRequest)?;
            (export.chat_id, export.page_from())
        };
        let extra = self
            .session
            .request(RequestPurpose::ExportChatHistory, Some(chat_id));
        if let Err(err) = self.sender.send_json(&get_chat_history(
            extra,
            chat_id,
            from,
            0,
            crate::chat_export::EXPORT_PAGE_LIMIT,
            false,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        if let Some(export) = self.session.chat_export.as_mut() {
            export.in_flight = true;
        }
        Ok(())
    }
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

    /// `sendMessage` + `inputMessageAnimation` / `inputAnimation` / `inputFileId`.
    pub fn send_animation(
        &mut self,
        chat_id: ChatId,
        animation: AnimationSend,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported || animation.file_id.0 == 0 || self.topic_send_is_closed(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SendMessage, Some(chat_id));
        let topic_id = self.send_topic(chat_id);
        let animation = AnimationSend {
            topic_id,
            ..animation
        };
        let json = send_animation(extra, chat_id, animation);
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

    /// `sendMessage` + `inputMessageSticker` / `inputFileId` (Unigram compose).
    pub fn send_sticker(
        &mut self,
        chat_id: ChatId,
        sticker: StickerSend<'_>,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.sticker_requires_premium(sticker.file_id) && !self.session.my_is_premium() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported || sticker.file_id.0 <= 0 || self.topic_send_is_closed(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SendMessage, Some(chat_id));
        let topic_id = self.send_topic(chat_id);
        let sticker = StickerSend {
            topic_id,
            ..sticker
        };
        let json = send_sticker(extra, chat_id, sticker);
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

    /// M2: `getFullRichMessage` (TDLib 1.8.67, line 11554) for a
    /// partially-received rich message (`is_full == false`). The reducer
    /// replaces the history row's blocks with the full ones on success; a
    /// failed fetch leaves the partial blocks in place (honest).
    pub fn fetch_full_rich_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::GetFullRichMessage {
                chat_id,
                message_id,
            },
            Some(chat_id),
        );
        let json = get_full_rich_message(extra, chat_id, message_id);
        if let Err(err) = self.sender.send_json(&json) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Slice msg-richtext-ai-tools: shared guard for the AI composer
    /// methods. The schema forbids `fixTextWithAi` in secret chats; the
    /// other four get the same refusal — sending draft text to a
    /// server-side AI model would break secret-chat privacy.
    fn ai_compose_guard(&self, chat_id: ChatId) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let is_secret = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }));
        if is_secret {
            return Err(ConnectSendError::InvalidRequest);
        }
        Ok(())
    }

    /// Slice msg-richtext-ai-tools: `fixTextWithAi` (TDLib 1.8.67,
    /// `schema/td_api.tl:12172`) on the composer draft. The `fixedText`
    /// answer replaces the draft.
    pub fn fix_text_with_ai(
        &mut self,
        chat_id: ChatId,
        text: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if text.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.ai_compose_guard(chat_id)?;
        let extra = self
            .session
            .request(RequestPurpose::FixTextWithAi, Some(chat_id));
        let json = fix_text_with_ai(extra, text);
        if let Err(err) = self.sender.send_json(&json) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Slice msg-richtext-ai-tools: `composeTextWithAi` (TDLib 1.8.67,
    /// `schema/td_api.tl:12154`) on the composer draft. Defaults are the
    /// honest no-ops: no translation, keep the current style, no emoji
    /// (the composer has no style/translate picker in this slice).
    pub fn compose_text_with_ai(
        &mut self,
        chat_id: ChatId,
        text: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if text.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.ai_compose_guard(chat_id)?;
        let extra = self
            .session
            .request(RequestPurpose::ComposeTextWithAi, Some(chat_id));
        let json = compose_text_with_ai(extra, text, "", "", false);
        if let Err(err) = self.sender.send_json(&json) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Slice msg-richtext-ai-tools: `composeRichMessageWithAi` (TDLib
    /// 1.8.67, `schema/td_api.tl:12162`) on the composer's parsed blocks.
    /// Defaults: no translation, keep the current style, no custom
    /// prompt, no emoji — same no-picker rationale as
    /// `compose_text_with_ai`.
    pub fn compose_rich_message_with_ai(
        &mut self,
        chat_id: ChatId,
        blocks: &[RichBlock],
    ) -> Result<RequestId, ConnectSendError> {
        let message =
            crate::rich::input_rich_message(blocks).ok_or(ConnectSendError::InvalidRequest)?;
        self.ai_compose_guard(chat_id)?;
        let extra = self
            .session
            .request(RequestPurpose::ComposeRichMessageWithAi, Some(chat_id));
        let json = compose_rich_message_with_ai(extra, &message, "", "", "", false);
        if let Err(err) = self.sender.send_json(&json) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Slice msg-richtext-ai-tools: `createRichMessageWithAi` (TDLib
    /// 1.8.67, `schema/td_api.tl:12168`). The composer text is the
    /// prompt; `language_code` is the user's app language
    /// (`session.language_prefs.system_language_code`, e.g. "en") —
    /// the schema documents no server-side default for it, so a real
    /// code is always sent. `add_emojis` is false — no pickers in
    /// this slice. The `richMessage` answer replaces the draft (the
    /// prompt was the whole draft).
    pub fn create_rich_message_with_ai(
        &mut self,
        chat_id: ChatId,
        prompt: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if prompt.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.ai_compose_guard(chat_id)?;
        let extra = self
            .session
            .request(RequestPurpose::CreateRichMessageWithAi, Some(chat_id));
        let json = create_rich_message_with_ai(
            extra,
            prompt,
            &self.session.language_prefs.system_language_code,
            false,
        );
        if let Err(err) = self.sender.send_json(&json) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Slice msg-richtext-ai-tools: `fixRichMessageWithAi` (TDLib 1.8.67,
    /// `schema/td_api.tl:12176`) on the composer's parsed blocks. The
    /// `richMessage` answer replaces the draft as editor markup.
    pub fn fix_rich_message_with_ai(
        &mut self,
        chat_id: ChatId,
        blocks: &[RichBlock],
    ) -> Result<RequestId, ConnectSendError> {
        let message =
            crate::rich::input_rich_message(blocks).ok_or(ConnectSendError::InvalidRequest)?;
        self.ai_compose_guard(chat_id)?;
        let extra = self
            .session
            .request(RequestPurpose::FixRichMessageWithAi, Some(chat_id));
        let json = fix_rich_message_with_ai(extra, &message);
        if let Err(err) = self.sender.send_json(&json) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// `sendMessageAlbum` for 2–10 local photos and/or videos.
    pub(crate) fn send_album_snapshot(
        &mut self,
        snapshot: &ComposerSnapshot,
    ) -> Result<RequestId, ConnectSendError> {
        let chat_id = snapshot.chat_id();
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported || !snapshot.is_media_album() || self.topic_send_is_closed(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let caption = snapshot.caption();
        let last = snapshot.album.len() - 1;
        // MED4: caption-length gate (same runtime option as single
        // sends — `send_snapshot` returns here before its own check).
        self.check_caption_length(caption)?;
        // Phase B3: same private-chat gate as `send_snapshot` — the timer
        // applies per album item (the schema allows it per
        // `inputMessagePhoto`/`inputMessageVideo`).
        let self_destruct = self
            .session
            .chats
            .get(&chat_id.0)
            .filter(|chat| matches!(chat.kind, ChatKind::Private { .. }))
            .and(snapshot.self_destruct);
        // M1 fix-up: same secret-chat blockquote strip as `send_snapshot`.
        let is_secret = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }));
        let mut contents = Vec::with_capacity(snapshot.album.len());
        for (index, att) in snapshot.album.iter().enumerate() {
            let path = att
                .send_path_str()
                .ok_or(ConnectSendError::InvalidRequest)?;
            let item_caption = if index == last { caption } else { "" };
            let content = match att.kind {
                AttachmentKind::Photo => input_message_photo(
                    &path,
                    item_caption,
                    snapshot.caption_above_media,
                    self_destruct,
                    is_secret,
                ),
                AttachmentKind::Video => {
                    let probe = crate::video::probe_local_video(&att.path)
                        .map_err(|_| ConnectSendError::InvalidRequest)?;
                    input_message_video(
                        &path,
                        &VideoSend {
                            duration: probe.duration,
                            width: probe.width,
                            height: probe.height,
                            supports_streaming: probe.supports_streaming,
                            self_destruct,
                        },
                        item_caption,
                        snapshot.caption_above_media,
                        is_secret,
                    )
                }
                AttachmentKind::Document | AttachmentKind::VideoNote => {
                    return Err(ConnectSendError::InvalidRequest);
                }
            };
            let mut content = content;
            if att.spoiler && matches!(att.kind, AttachmentKind::Photo | AttachmentKind::Video) {
                content["has_spoiler"] = serde_json::Value::Bool(true);
            }
            contents.push(content);
        }
        // Slice G1: quote-carrying reply (`inputTextQuote`).
        let reply_to = snapshot.send_reply();
        let extra = self
            .session
            .request(RequestPurpose::SendMessageAlbum, Some(chat_id));
        let topic_id = self.send_topic(chat_id);
        let json = send_message_album(extra, chat_id, topic_id, reply_to, contents);
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

    /// `sendMessage` + `inputMessageVoiceNote` for a finished local capture.
    pub fn send_voice_note(
        &mut self,
        draft: &VoiceDraft,
        caption: &str,
        reply_to: Option<SendReply>,
        play_once: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat_id) = self.session.open_chat else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported || self.topic_send_is_closed(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let path = crate::local_path::pick_send_path(&draft.path)
            .ok_or(ConnectSendError::InvalidRequest)?;
        let path = path.to_string_lossy().into_owned();
        let extra = self
            .session
            .request(RequestPurpose::SendMessage, Some(chat_id));
        let topic_id = self.send_topic(chat_id);
        // "Play once" is a self-destruct type, which TDLib accepts in
        // private chats only (the same gate as photos and videos).
        let self_destruct = self
            .session
            .chats
            .get(&chat_id.0)
            .filter(|chat| matches!(chat.kind, ChatKind::Private { .. }))
            .and(play_once.then_some(crate::telegram::requests::SelfDestructSend::Immediately));
        let json = send_voice_note(
            extra,
            chat_id,
            VoiceNoteSend {
                path: &path,
                duration: draft.duration_secs,
                waveform_b64: &draft.waveform_b64(),
                caption,
                reply_to,
                topic_id,
                self_destruct,
            },
        );
        let json = self.thread_routed(chat_id, json);
        match self.sender.send_json(&json) {
            Ok(()) => {
                let _ = self.cancel_outgoing_typing();
                let _ = self.sync_voice_recording(false, 0);
                Ok(extra)
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// `sendMessage` + `inputMessageVideoNote` for a finished camera
    /// capture. Mirrors `send_voice_note`; the draft is already squared
    /// and probed by `VideoNoteCapture::finish`.
    pub fn send_recorded_video_note(
        &mut self,
        draft: &crate::video::VideoNoteDraft,
        reply_to: Option<SendReply>,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat_id) = self.session.open_chat else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported || self.topic_send_is_closed(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let path = crate::local_path::pick_send_path(&draft.path)
            .ok_or(ConnectSendError::InvalidRequest)?;
        let path = path.to_string_lossy().into_owned();
        let thumbnail = crate::video::write_video_note_thumbnail(&draft.path).map(|thumb| {
            VideoNoteThumbnailSend {
                path: thumb.path.to_string_lossy().into_owned(),
                width: thumb.width,
                height: thumb.height,
            }
        });
        let extra = self
            .session
            .request(RequestPurpose::SendMessage, Some(chat_id));
        let topic_id = self.send_topic(chat_id);
        let json = send_video_note(
            extra,
            chat_id,
            topic_id,
            &path,
            &VideoNoteSend {
                duration: draft.duration_secs,
                length: draft.length,
                thumbnail,
            },
            reply_to,
        );
        let json = self.thread_routed(chat_id, json);
        match self.sender.send_json(&json) {
            Ok(()) => {
                let _ = self.cancel_outgoing_typing();
                let _ = self.sync_video_note_recording(false, 0);
                Ok(extra)
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// MED2: `recognizeSpeech` for a voice/video note message. Only real
    /// (non-pending) messages qualify; the transcript arrives later via
    /// `updateMessageContent`. A refused request is an error, never a
    /// faked transcript.
    pub fn recognize_speech(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(message) = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if message.pending || message.id.0 <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::RecognizeSpeech, Some(chat_id));
        let json = recognize_speech(extra, chat_id, message_id);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// `openMessageContent` when playback of a voice note starts.
    pub fn open_voice_content(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::OpenMessageContent, Some(chat_id));
        let json = open_message_content(extra, chat_id, message_id);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Save an own-message edit via `editMessageText` or `editMessageCaption`.
    pub fn edit_snapshot(
        &mut self,
        edit: &ComposerEdit,
        text: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&edit.chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        // M1: scheduled sends live in `session.scheduled_messages`
        // (`ParsedMessage`, never pending), not in history
        // (`HistoryMessage`) — same `editMessageText` request, different
        // validation source.
        let owned = if edit.scheduled {
            self.session
                .scheduled_messages
                .iter()
                .find(|m| m.chat_id == edit.chat_id && m.id == edit.message_id)
                .map(|m| (m.chat_id, m.id, m.is_outgoing, false, &m.content))
        } else {
            self.session
                .histories
                .get(&edit.chat_id.0)
                .and_then(|history| history.messages.get(&edit.message_id.0))
                .map(|m| (m.chat_id, m.id, m.is_outgoing, m.pending, &m.content))
        };
        let Some((chat_id, message_id, is_outgoing, pending, content)) = owned else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if ComposerEdit::from_own_content(chat_id, message_id, is_outgoing, pending, content)
            .is_none()
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let caption = text.trim();
        if matches!(edit.kind, ComposerEditKind::Text) && caption.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        // MED4: caption-length gate for caption edits (same runtime
        // option as media sends).
        if matches!(edit.kind, ComposerEditKind::Caption) {
            self.check_caption_length(caption)?;
        }
        // R8: text edits are bounded by `message_text_length_max`
        // (counted after markup parsing, in UTF-16 units).
        if matches!(edit.kind, ComposerEditKind::Text) {
            let limit = self.session.message_text_length_max;
            if crate::text_split::units_over_limit(caption, limit) > 0 {
                return Err(ConnectSendError::TextTooLong { limit });
            }
        }
        // M1 fix-up: secret chats strip `textEntityTypeBlockQuote` from
        // the edited caption too (unsupported in secret chats).
        let strip_blockquote = self
            .session
            .chats
            .get(&edit.chat_id.0)
            .is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }));
        // B5: a replacement file turns the caption edit into
        // `editMessageMedia` (tdesktop `EditCaptionBox` with a prepared
        // list). Validate path and probe video before allocating `@extra`.
        let replacement = match edit.media_edit.replacement.as_ref() {
            Some(_) if !edit.allows_replace() => return Err(ConnectSendError::InvalidRequest),
            other => other,
        };
        let mut replacement_content = None;
        if let Some(rep) = replacement {
            let path = rep
                .send_path_str()
                .ok_or(ConnectSendError::InvalidRequest)?;
            let video = if rep.kind == EditMediaKind::Video {
                let probe = crate::video::probe_local_video(&rep.path)
                    .map_err(|_| ConnectSendError::InvalidRequest)?;
                Some(VideoSend {
                    duration: probe.duration,
                    width: probe.width,
                    height: probe.height,
                    supports_streaming: probe.supports_streaming,
                    self_destruct: None,
                })
            } else {
                None
            };
            replacement_content = Some(
                edit_media_content(
                    rep,
                    &path,
                    caption,
                    edit.caption_above && !strip_blockquote,
                    video.as_ref(),
                    strip_blockquote,
                )
                .ok_or(ConnectSendError::InvalidRequest)?,
            );
        }
        let extra = self
            .session
            .request(RequestPurpose::EditMessage, Some(edit.chat_id));
        let replacement_json = replacement_content
            .map(|content| edit_message_media(extra, edit.chat_id, edit.message_id, content));
        let json = match edit.kind {
            ComposerEditKind::Caption if replacement_json.is_some() => {
                replacement_json.unwrap_or_default()
            }
            ComposerEditKind::Text => edit_message_text(
                extra,
                edit.chat_id,
                edit.message_id,
                caption,
                strip_blockquote,
                // Secret chats never get previews (same rule as sends).
                &LinkPreviewChoice {
                    disabled: edit.link_preview.disabled || strip_blockquote,
                    ..edit.link_preview
                },
            ),
            ComposerEditKind::Caption => edit_message_caption(
                extra,
                edit.chat_id,
                edit.message_id,
                caption,
                // MED4 review nit: secret chats force caption-below on send
                // too (TGX `allowShowCaptionAboveMedia`).
                edit.caption_above && !strip_blockquote,
                strip_blockquote,
            ),
        };
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// After UI confirm (tdesktop `DeleteMessagesBox`), send `deleteMessages`.
    /// `revoke: true` deletes for everyone (own outgoing default), false only
    /// for the current user (schema 1.8.67 line 12282).
    pub fn delete_confirmed(
        &mut self,
        confirm: &DeleteConfirm,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&confirm.chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(message) = self
            .session
            .histories
            .get(&confirm.chat_id.0)
            .and_then(|history| history.messages.get(&confirm.message_id.0))
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        // M1: any sent message may be deleted; the for-everyone toggle is
        // only honored for own outgoing (schema 1.8.67 lines 6228–6229).
        if DeleteConfirm::for_message(
            message.chat_id,
            message.id,
            message.is_outgoing,
            message.pending,
        )
        .is_none()
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        // For everyone only where the UI offered it: TDLib's
        // `can_be_deleted_for_all_users` (or, before that arrives, your own
        // messages). `can_revoke` carries that decision.
        let revoke = confirm.revoke && confirm.can_revoke;
        let extra = self
            .session
            .request(RequestPurpose::DeleteMessages, Some(confirm.chat_id));
        let json = delete_messages(extra, confirm.chat_id, &[confirm.message_id], revoke);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Delete a selection of messages (Telegram Desktop's "Delete N" in
    /// selection mode) with one `deleteMessages`. Every message must be
    /// loaded and deletable; `revoke` (delete for everyone) is only honored
    /// when they're all your own.
    pub fn delete_selected(
        &mut self,
        chat_id: ChatId,
        message_ids: &[MessageId],
        revoke: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || message_ids.is_empty() || message_ids.len() > 100 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        let history = self.session.histories.get(&chat_id.0);
        let mut all_outgoing = true;
        for id in message_ids {
            let Some(message) = history.and_then(|history| history.messages.get(&id.0)) else {
                return Err(ConnectSendError::InvalidRequest);
            };
            if DeleteConfirm::for_message(
                message.chat_id,
                message.id,
                message.is_outgoing,
                message.pending,
            )
            .is_none()
            {
                return Err(ConnectSendError::InvalidRequest);
            }
            all_outgoing &= message.is_outgoing;
        }
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::DeleteMessages, Some(chat_id));
        let json = delete_messages(extra, chat_id, message_ids, revoke && all_outgoing);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Send `forwardMessages` after the dest picker chooses a supported chat.
    /// `send_copy: false` preserves official "Forwarded from" attribution.
    pub fn forward_messages(
        &mut self,
        dest: ChatId,
        draft: &ForwardDraft,
    ) -> Result<RequestId, ConnectSendError> {
        self.forward_messages_with_options(dest, draft, &SendOptions::default())
    }

    /// `forwardMessages` with the share box's silent / scheduled options.
    pub fn forward_messages_with_options(
        &mut self,
        dest: ChatId,
        draft: &ForwardDraft,
        options: &SendOptions,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if draft.is_empty() || draft.message_ids.len() > 100 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let dest_ok = self
            .session
            .chats
            .get(&dest.0)
            .is_some_and(|chat| chat.supported());
        let from_ok = self
            .session
            .chats
            .get(&draft.from_chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !dest_ok || !from_ok {
            return Err(ConnectSendError::InvalidRequest);
        }
        for id in &draft.message_ids {
            let Some(message) = self
                .session
                .histories
                .get(&draft.from_chat_id.0)
                .and_then(|history| history.messages.get(&id.0))
            else {
                return Err(ConnectSendError::InvalidRequest);
            };
            if message.pending || id.0 <= 0 {
                return Err(ConnectSendError::InvalidRequest);
            }
        }
        let extra = self
            .session
            .request(RequestPurpose::ForwardMessages, Some(dest));
        let flight = ForwardFlight {
            extra,
            dest_chat_id: dest,
            from_chat_id: draft.from_chat_id,
            requested: draft.message_ids.len(),
        };
        // Several destinations (share box) are in flight at once: the first
        // takes the main slot, the rest queue behind it.
        let queued = self.session.in_flight_forward.is_some();
        if queued {
            self.session.queued_forward_flights.push(flight);
        } else {
            self.session.in_flight_forward = Some(flight);
        }
        let json = forward_messages_with_options(
            extra,
            dest,
            draft.from_chat_id,
            &draft.message_ids,
            draft.send_copy,
            draft.remove_caption,
            options,
        );
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                if queued {
                    self.session
                        .queued_forward_flights
                        .retain(|f| f.extra != extra);
                } else {
                    self.session.in_flight_forward = None;
                }
                Err(err)
            }
        }
    }

    pub fn delete_scheduled_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        // M1: delete a scheduled send. Scheduled messages live in
        // `session.scheduled_messages`, not in history, so the
        // history-validated `delete_confirmed` can't take them. `revoke`
        // is always false (no for-everyone distinction before sending).
        let known = self
            .session
            .scheduled_messages
            .iter()
            .any(|m| m.chat_id == chat_id && m.id == message_id);
        if !known {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::DeleteMessages, Some(chat_id));
        let json = delete_messages(extra, chat_id, &[message_id], false);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// `editMessageSchedulingState`: reschedule a scheduled message, or send
    /// it now with `ComposerScheduling::None`. Only messages in the loaded
    /// scheduled list qualify.
    pub fn edit_scheduled_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        scheduling: ComposerScheduling,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let known = self
            .session
            .scheduled_messages
            .iter()
            .any(|m| m.chat_id == chat_id && m.id == message_id);
        if !known {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::EditMessageSchedulingState {
                message_id,
                scheduling,
            },
            Some(chat_id),
        );
        let json = edit_message_scheduling_state(extra, chat_id, message_id, scheduling);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// M1: retry a failed send (`resendMessages`, TDLib 1.8.67,
    /// `schema/td_api.tl:12251`; `message.sending_state.can_retry`, schema
    /// line 3038). The driver only retries rows the reducer marked
    /// `failed` **and** retryable — not every failed send may be
    /// retried, and the context menu offers "Retry send" on the same
    /// gate.
    pub fn resend_failed_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let can_retry = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .is_some_and(|message| message.failed && message.can_retry);
        if !can_retry {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::ResendMessages, Some(chat_id));
        let json = resend_messages(extra, chat_id, &[message_id]);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// M1: load a chat's scheduled (pending) sends into
    /// `session.scheduled_messages` (TDLib 1.8.67,
    /// `schema/td_api.tl:12000`).
    pub fn get_chat_scheduled_messages(
        &mut self,
        chat_id: ChatId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetChatScheduledMessages, Some(chat_id));
        let json = get_chat_scheduled_messages(extra, chat_id);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
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
