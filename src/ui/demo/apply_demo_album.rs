//! Methods moved out of `demo.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    pub(in crate::ui) fn apply_demo_album(
        &mut self,
        text: &str,
        attachments: &[ComposerAttachment],
        reply: Option<&ComposerReplyTo>,
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let Some(chat_id) = session.open_chat else {
            return;
        };
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        let caption = text.trim();
        let album_id = "88001";
        let reply_json = reply
            .filter(|r| r.chat_id == chat_id)
            .map(|r| {
                format!(
                    r#","reply_to":{{"@type":"messageReplyToMessage","chat_id":{},"message_id":{},"quote":null,"checklist_task_id":0,"poll_option_id":""}}"#,
                    r.chat_id.0, r.message_id.0
                )
            })
            .unwrap_or_default();
        for (index, att) in attachments.iter().enumerate() {
            let id = -((index as i64) + 20);
            let item_caption = if index + 1 == attachments.len() {
                caption
            } else {
                ""
            };
            let path = att.path.to_string_lossy();
            let file = demo_file_json(910 + index as i32, &path, true);
            let json = match att.kind {
                AttachmentKind::Photo => format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"media_album_id":"{album_id}","content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":240,"height":160,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":{},"entities":[]}},"has_spoiler":false,"is_secret":false}}}}{reply_json}}}}}"#,
                    chat_id.0,
                    serde_json::to_string(item_caption).unwrap_or_else(|_| "\"\"".into()),
                ),
                AttachmentKind::Video => {
                    let probe = quill::video::probe_local_video(&att.path).unwrap_or(
                        quill::video::VideoProbe {
                            duration: 0,
                            width: 320,
                            height: 180,
                            supports_streaming: false,
                        },
                    );
                    format!(
                        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"media_album_id":"{album_id}","content":{{"@type":"messageVideo","video":{{"@type":"video","duration":{},"width":{},"height":{},"file_name":{},"mime_type":"video/mp4","has_stickers":false,"supports_streaming":{},"minithumbnail":null,"thumbnail":null,"video":{file}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":{},"entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}{reply_json}}}}}"#,
                        chat_id.0,
                        probe.duration,
                        probe.width,
                        probe.height,
                        serde_json::to_string(&att.file_name)
                            .unwrap_or_else(|_| "\"clip.mp4\"".into()),
                        probe.supports_streaming,
                        serde_json::to_string(item_caption).unwrap_or_else(|_| "\"\"".into()),
                    )
                }
                AttachmentKind::Document | AttachmentKind::VideoNote => continue,
            };
            if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &dyn_sink) {
                session.apply(owned);
            }
        }
    }

    pub(in crate::ui) fn apply_demo_outgoing(
        &mut self,
        text: &str,
        attachment: Option<&ComposerAttachment>,
        reply: Option<&ComposerReplyTo>,
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let Some(chat_id) = session.open_chat else {
            return;
        };
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        // After the newest loaded message, so the send lands at the bottom
        // and repeated sends don't replace each other.
        let id = session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.keys().next_back().copied())
            .map_or(1, |last| last.max(0) + 1);
        let caption = text.trim();
        let reply_json = reply
            .filter(|r| r.chat_id == chat_id)
            .map(|r| {
                format!(
                    r#","reply_to":{{"@type":"messageReplyToMessage","chat_id":{},"message_id":{},"quote":null,"checklist_task_id":0,"poll_option_id":""}}"#,
                    r.chat_id.0, r.message_id.0
                )
            })
            .unwrap_or_default();
        let json = match attachment {
            Some(att) if att.kind == AttachmentKind::Photo => {
                let path = att.path.to_string_lossy();
                let file = demo_file_json(900, &path, true);
                format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":240,"height":160,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":{},"entities":[]}},"has_spoiler":false,"is_secret":false}}{reply_json}}}}}"#,
                    chat_id.0,
                    serde_json::to_string(caption).unwrap_or_else(|_| "\"\"".into()),
                )
            }
            Some(att) if att.kind == AttachmentKind::VideoNote => {
                let path = att.path.to_string_lossy();
                let file = demo_file_json(903, &path, true);
                let probe = quill::video::probe_local_video_note(&att.path).unwrap_or(
                    quill::video::VideoNoteProbe {
                        duration: 1,
                        length: 240,
                    },
                );
                let thumb_path = quill::video::write_video_note_thumbnail(&att.path)
                    .map(|thumb| thumb.path.to_string_lossy().into_owned());
                let thumb = thumb_path
                    .as_deref()
                    .map(|path| demo_file_json(904, path, true))
                    .unwrap_or_else(|| "null".into());
                let thumb_obj = if thumb == "null" {
                    "null".to_string()
                } else {
                    format!(
                        r#"{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":240,"height":240,"file":{thumb}}}"#
                    )
                };
                format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messageVideoNote","video_note":{{"@type":"videoNote","duration":{},"waveform":"","length":{},"minithumbnail":null,"thumbnail":{thumb_obj},"speech_recognition_result":null,"video":{file}}},"is_viewed":true,"is_secret":false}}}}{reply_json}}}}}"#,
                    chat_id.0, probe.duration, probe.length,
                )
            }
            Some(att) if att.kind == AttachmentKind::Video => {
                let path = att.path.to_string_lossy();
                let file = demo_file_json(902, &path, true);
                let probe = quill::video::probe_local_video(&att.path).unwrap_or(
                    quill::video::VideoProbe {
                        duration: 0,
                        width: 0,
                        height: 0,
                        supports_streaming: false,
                    },
                );
                format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":{},"width":{},"height":{},"file_name":{},"mime_type":"video/mp4","has_stickers":false,"supports_streaming":{},"minithumbnail":null,"thumbnail":null,"video":{file}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":{},"entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}{reply_json}}}}}"#,
                    chat_id.0,
                    probe.duration,
                    probe.width,
                    probe.height,
                    serde_json::to_string(&att.file_name).unwrap_or_else(|_| "\"clip.mp4\"".into()),
                    probe.supports_streaming,
                    serde_json::to_string(caption).unwrap_or_else(|_| "\"\"".into()),
                )
            }
            Some(att) => {
                let path = att.path.to_string_lossy();
                let file = demo_file_json(901, &path, true);
                format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messageDocument","document":{{"@type":"document","file_name":{},"mime_type":"text/plain","document":{file}}},"caption":{{"@type":"formattedText","text":{},"entities":[]}}}}{reply_json}}}}}"#,
                    chat_id.0,
                    serde_json::to_string(&att.file_name).unwrap_or_else(|_| "\"file\"".into()),
                    serde_json::to_string(caption).unwrap_or_else(|_| "\"\"".into()),
                )
            }
            None => format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{},"entities":[]}}}}{reply_json}}}}}"#,
                chat_id.0,
                serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into()),
            ),
        };
        if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    pub(in crate::ui) fn apply_demo_sticker(
        &mut self,
        chat_id: ChatId,
        emoji: &str,
        file_id: FileId,
        reply: Option<quill::telegram::SendReply>,
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        let id = -(session.view_generation.0 as i64);
        let path = session
            .media
            .files
            .get(&file_id.0)
            .and_then(|file| file.usable_path())
            .unwrap_or("");
        let file = demo_file_json(file_id.0, path, !path.is_empty());
        let reply_json = reply
            .map(|reply| {
                format!(
                    r#","reply_to":{{"@type":"messageReplyToMessage","chat_id":{},"message_id":{},"quote":null,"checklist_task_id":0,"poll_option_id":""}}"#,
                    chat_id.0, reply.message_id.0
                )
            })
            .unwrap_or_default();
        let emoji = serde_json::to_string(emoji).unwrap_or_else(|_| "\"\"".into());
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messageSticker","is_premium":false,"sticker":{{"@type":"sticker","id":"0","set_id":"0","width":512,"height":512,"emoji":{emoji},"format":{{"@type":"stickerFormatWebp"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatPng"}},"width":128,"height":128,"file":{file}}},"sticker":{file}}}}}{reply_json}}}}}"#,
            chat_id.0
        );
        if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    pub(in crate::ui) fn apply_demo_gif(
        &mut self,
        chat_id: ChatId,
        file_id: FileId,
        duration: i32,
        width: i32,
        height: i32,
        reply: Option<quill::telegram::SendReply>,
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        let id = -(session.view_generation.0 as i64);
        let path = session
            .media
            .files
            .get(&file_id.0)
            .and_then(|file| file.usable_path())
            .unwrap_or("");
        let file = demo_file_json(file_id.0, path, !path.is_empty());
        let reply_json = reply
            .map(|reply| {
                format!(
                    r#","reply_to":{{"@type":"messageReplyToMessage","chat_id":{},"message_id":{},"quote":null,"checklist_task_id":0,"poll_option_id":""}}"#,
                    chat_id.0, reply.message_id.0
                )
            })
            .unwrap_or_default();
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messageAnimation","animation":{{"@type":"animation","duration":{duration},"width":{width},"height":{height},"file_name":"gif.mp4","mime_type":"video/mp4","has_stickers":false,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":{width},"height":{height},"file":{file}}},"animation":{file}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}{reply_json}}}}}"#,
            chat_id.0
        );
        if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    pub(in crate::ui) fn apply_demo_edit(&mut self, edit: &ComposerEdit, text: &str) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        match edit.kind {
            quill::composer::ComposerEditKind::Text => {
                let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
                let body = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into());
                let json = format!(
                    r#"{{"@type":"updateMessageContent","chat_id":{},"message_id":{},"new_content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{body},"entities":[]}}}}}}"#,
                    edit.chat_id.0, edit.message_id.0
                );
                if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &dyn_sink) {
                    session.apply(owned);
                }
            }
            quill::composer::ComposerEditKind::Caption => {
                if let Some(message) = session
                    .histories
                    .get_mut(&edit.chat_id.0)
                    .and_then(|history| history.messages.get_mut(&edit.message_id.0))
                {
                    match &mut message.content {
                        MessageContent::Photo(photo) => {
                            photo.caption = text.to_string();
                            photo.show_caption_above_media = edit.caption_above;
                        }
                        MessageContent::Document(doc) => doc.caption = text.to_string(),
                        MessageContent::Text(body) => {
                            body.text = text.to_string();
                            body.entities.clear();
                            body.link_preview = None;
                        }
                        MessageContent::VoiceNote(note) => note.caption = text.to_string(),
                        MessageContent::Animation(animation) => {
                            animation.caption = text.to_string();
                            animation.show_caption_above_media = edit.caption_above;
                        }
                        MessageContent::Video(video) => {
                            video.caption = text.to_string();
                            video.show_caption_above_media = edit.caption_above;
                        }
                        MessageContent::Audio(audio) => audio.caption = text.to_string(),
                        MessageContent::VideoNote(_)
                        | MessageContent::Sticker(_)
                        | MessageContent::Poll(_)
                        | MessageContent::Checklist(_)
                        | MessageContent::Location(_)
                        | MessageContent::Venue(_)
                        | MessageContent::Contact(_)
                        | MessageContent::Dice(_)
                        | MessageContent::GroupCallInvitation { .. }
                        | MessageContent::Call { .. }
                        | MessageContent::ChatTtlChanged { .. }
                        | MessageContent::ScreenshotTaken
                        | MessageContent::Unsupported { .. }
                        // M2: rich messages are not editable through the
                        // legacy text/caption path.
                        | MessageContent::RichMessage(_)
                        // B1: games carry no editable caption.
                        | MessageContent::Game(_)
                        // Slice P1: invoices and payment notices carry no
                        // editable caption.
                        | MessageContent::Invoice(_)
                        | MessageContent::PaymentSuccessful(_)
                        | MessageContent::PaymentReceived(_)
                        // Slice C2k: community service rows carry no
                        // editable caption.
                        | MessageContent::ChatAddedToCommunity { .. }
                        | MessageContent::ChatRemovedFromCommunity
                        // Slice G9: the join-from-community service row
                        // carries no editable caption either.
                        | MessageContent::ChatJoinFromCommunity { .. }
                        | MessageContent::Action(_) => {}
                    }
                }
            }
        }
    }

    pub(in crate::ui) fn apply_demo_delete(&mut self, chat_id: ChatId, message_id: MessageId) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        let json = format!(
            r#"{{"@type":"updateDeleteMessages","chat_id":{},"message_ids":[{}],"is_permanent":true,"from_cache":false}}"#,
            chat_id.0, message_id.0
        );
        if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    /// Demo "send as": apply the `updateChatMessageSender` TDLib would send
    /// after `setChatMessageSender`.
    pub(in crate::ui) fn apply_demo_message_sender(
        &mut self,
        chat_id: ChatId,
        sender: quill::telegram::envelope::MessageSender,
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let sender_json = match sender {
            quill::telegram::envelope::MessageSender::User { user_id } => {
                format!(r#"{{"@type":"messageSenderUser","user_id":{user_id}}}"#)
            }
            quill::telegram::envelope::MessageSender::Chat { chat_id } => {
                format!(r#"{{"@type":"messageSenderChat","chat_id":{chat_id}}}"#)
            }
        };
        let json = format!(
            r#"{{"@type":"updateChatMessageSender","chat_id":{},"message_sender_id":{sender_json}}}"#,
            chat_id.0
        );
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    pub(in crate::ui) fn apply_demo_forward(&mut self, dest: ChatId, draft: &ForwardDraft) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let extra = session.request(RequestPurpose::ForwardMessages, Some(dest));
        session.messages.in_flight_forward = Some(quill::state::ForwardFlight {
            extra,
            dest_chat_id: dest,
            from_chat_id: draft.from_chat_id,
            requested: draft.message_ids.len(),
        });
        let origin_user = session
            .chats
            .get(&draft.from_chat_id.0)
            .and_then(|chat| match chat.kind {
                quill::telegram::envelope::ChatKind::Private { user_id } => Some(user_id.0),
                _ => None,
            })
            .unwrap_or(draft.from_chat_id.0);
        let mut copies = Vec::new();
        for (offset, id) in draft.message_ids.iter().enumerate() {
            let preview = session
                .histories
                .get(&draft.from_chat_id.0)
                .and_then(|history| history.messages.get(&id.0))
                .map(effective_preview)
                .unwrap_or_else(|| "Message".into());
            let body = serde_json::to_string(&preview).unwrap_or_else(|_| "\"\"".into());
            copies.push(format!(
                r#"{{"id":{},"chat_id":{},"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{body},"entities":[]}}}},"forward_info":{{"@type":"messageForwardInfo","origin":{{"@type":"messageOriginUser","sender_user_id":{origin_user}}},"date":1}}}}"#,
                80 + offset as i64,
                dest.0
            ));
        }
        let json = format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":{},"messages":[{}]}}"#,
            extra.0,
            copies.len(),
            copies.join(",")
        );
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    pub(in crate::ui) fn apply_demo_reaction_toggle(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        emoji: &str,
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let current = session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .and_then(|message| message.interaction_info.clone());
        let next = toggle_chosen_emoji_reaction(current.as_ref(), emoji);
        let json = interaction_info_update_json(chat_id, message_id, &next);
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    /// Demo-only vote: resolve the tap with the same `poll_answer_for_tap`
    /// semantics as the live driver, then flip the chosen marks in place
    /// (counts stay as the fixture set them).
    pub(in crate::ui) fn apply_demo_poll_vote(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        option_index: usize,
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let answer = session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .and_then(|message| match &message.content {
                MessageContent::Poll(poll) => Some(poll.poll.clone()),
                _ => None,
            })
            .and_then(|poll| quill::poll::poll_answer_for_tap(&poll, option_index));
        let Some(answer) = answer else {
            return;
        };
        if let Some(history) = session.histories.get_mut(&chat_id.0)
            && let Some(message) = history.messages.get_mut(&message_id.0)
            && let MessageContent::Poll(poll_content) = &mut message.content
        {
            let chosen: HashSet<i32> = answer.into_iter().collect();
            for (index, option) in poll_content.poll.options.iter_mut().enumerate() {
                option.is_chosen = chosen.contains(&(index as i32));
            }
        }
    }

    pub(in crate::ui) fn apply_demo_pin_toggle(&mut self, chat_id: ChatId, message_id: MessageId) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let currently_pinned = session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .is_some_and(|message| message.is_pinned);
        let json = format!(
            r#"{{"@type":"updateMessageIsPinned","chat_id":{},"message_id":{},"is_pinned":{}}}"#,
            chat_id.0, message_id.0, !currently_pinned
        );
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    /// Parity slice: apply arbitrary notification-settings edits to the demo
    /// session (screenshot demos have no TDLib), via the same
    /// `updateChatNotificationSettings` reducer path live updates take.
    pub(in crate::ui) fn apply_demo_notification_settings(
        &mut self,
        chat_id: ChatId,
        edit: impl FnOnce(&mut ChatNotificationSettings),
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let mut current = session
            .chats
            .get(&chat_id.0)
            .map(|chat| chat.notification_settings.clone())
            .unwrap_or_default();
        edit(&mut current);
        let json = format!(
            r#"{{"@type":"updateChatNotificationSettings","chat_id":{},"notification_settings":{}}}"#,
            chat_id.0,
            notification_settings_json(&current)
        );
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &dyn_sink) {
            session.apply(owned);
        }
        cx.notify();
    }

    /// Parity slice: screenshot-demo folder create/edit (no live Telegram —
    /// apply to the demo session directly).
    pub(in crate::ui) fn apply_demo_folder_save(
        &mut self,
        folder_id: Option<i32>,
        spec: ChatFolderSpec,
    ) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        match folder_id {
            Some(id) => {
                if let Some(info) = session
                    .chat_list
                    .chat_folders
                    .iter_mut()
                    .find(|f| f.id == id)
                {
                    info.name = spec.name.clone();
                }
                session.chat_list.folder_specs.insert(id, spec);
            }
            None => {
                let id = session
                    .chat_list
                    .chat_folders
                    .iter()
                    .map(|f| f.id)
                    .max()
                    .unwrap_or(0)
                    + 1;
                session.chat_list.chat_folders.push(ChatFolderInfo {
                    id,
                    name: spec.name.clone(),
                    icon_name: spec.icon_name.clone().unwrap_or_default(),
                    color_id: -1,
                    is_shareable: false,
                    has_my_invite_links: false,
                });
                session.chat_list.folder_specs.insert(id, spec);
            }
        }
    }

    /// Parity slice: screenshot-demo folder delete.
    pub(in crate::ui) fn apply_demo_folder_delete(&mut self, folder_id: i32) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        session.chat_list.chat_folders.retain(|f| f.id != folder_id);
        session.chat_list.folder_specs.remove(&folder_id);
        session.chat_list.folder_chats_to_leave.remove(&folder_id);
        session.chat_list.folder_chats_exhausted.remove(&folder_id);
    }

    /// Parity slice: screenshot-demo folder reorder.
    pub(in crate::ui) fn apply_demo_folder_reorder(&mut self, ids: &[i32]) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let order: HashMap<i32, usize> = ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        session
            .chat_list
            .chat_folders
            .sort_by_key(|f| order.get(&f.id).copied().unwrap_or(usize::MAX));
    }

    pub(in crate::ui) fn apply_demo_archive(&mut self, chat_id: ChatId, archive: bool) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let order = session
            .chats
            .get(&chat_id.0)
            .map(|chat| {
                if archive {
                    chat.order.max(1)
                } else {
                    chat.archive_order.max(1)
                }
            })
            .unwrap_or(1);
        let jsons = if archive {
            vec![
                format!(
                    r#"{{"@type":"updateChatPosition","chat_id":{},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"0","is_pinned":false}}}}"#,
                    chat_id.0
                ),
                format!(
                    r#"{{"@type":"updateChatRemovedFromList","chat_id":{},"chat_list":{{"@type":"chatListMain"}}}}"#,
                    chat_id.0
                ),
                format!(
                    r#"{{"@type":"updateChatPosition","chat_id":{},"position":{{"@type":"chatPosition","list":{{"@type":"chatListArchive"}},"order":"{order}","is_pinned":false}}}}"#,
                    chat_id.0
                ),
                format!(
                    r#"{{"@type":"updateChatAddedToList","chat_id":{},"chat_list":{{"@type":"chatListArchive"}}}}"#,
                    chat_id.0
                ),
            ]
        } else {
            vec![
                format!(
                    r#"{{"@type":"updateChatPosition","chat_id":{},"position":{{"@type":"chatPosition","list":{{"@type":"chatListArchive"}},"order":"0","is_pinned":false}}}}"#,
                    chat_id.0
                ),
                format!(
                    r#"{{"@type":"updateChatRemovedFromList","chat_id":{},"chat_list":{{"@type":"chatListArchive"}}}}"#,
                    chat_id.0
                ),
                format!(
                    r#"{{"@type":"updateChatPosition","chat_id":{},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{order}","is_pinned":false}}}}"#,
                    chat_id.0
                ),
                format!(
                    r#"{{"@type":"updateChatAddedToList","chat_id":{},"chat_list":{{"@type":"chatListMain"}}}}"#,
                    chat_id.0
                ),
            ]
        };
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        if let Some(session) = self.demo_session.as_mut() {
            for json in jsons {
                if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &dyn_sink) {
                    session.apply(owned);
                }
            }
        }
    }

    /// Slice CL1: demo pin/unpin through the real `updateChatPosition`
    /// reducer (screenshot demos have no TDLib).
    pub(in crate::ui) fn apply_demo_pin(&mut self, chat_id: ChatId, pin: bool, archived: bool) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let list = if archived {
            "chatListArchive"
        } else {
            "chatListMain"
        };
        let order = session
            .chats
            .get(&chat_id.0)
            .map(|chat| {
                if archived {
                    chat.archive_order
                } else {
                    chat.order
                }
            })
            .unwrap_or(1)
            .max(1);
        let json = format!(
            r#"{{"@type":"updateChatPosition","chat_id":{},"position":{{"@type":"chatPosition","list":{{"@type":"{list}"}},"order":"{order}","is_pinned":{pin}}}}}"#,
            chat_id.0
        );
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &dyn_sink) {
            session.apply(owned);
        }
    }

    /// Slice CL1: demo mark-as-unread through the real
    /// `updateChatIsMarkedAsUnread` reducer.
    pub(in crate::ui) fn apply_demo_marked_as_unread(&mut self, chat_id: ChatId, marked: bool) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        // Mark-as-read also clears real unread, like TDLib's
        // `updateChatReadInbox` would after `viewMessages`.
        let jsons = if marked {
            vec![format!(
                r#"{{"@type":"updateChatIsMarkedAsUnread","chat_id":{},"is_marked_as_unread":{marked}}}"#,
                chat_id.0
            )]
        } else {
            vec![
                format!(
                    r#"{{"@type":"updateChatIsMarkedAsUnread","chat_id":{},"is_marked_as_unread":false}}"#,
                    chat_id.0
                ),
                format!(
                    r#"{{"@type":"updateChatReadInbox","chat_id":{},"last_read_inbox_message_id":0,"unread_count":0}}"#,
                    chat_id.0
                ),
            ]
        };
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        for json in jsons {
            if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &dyn_sink) {
                session.apply(owned);
            }
        }
    }
}
