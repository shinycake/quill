//! Applies TDLib updates and answers for chat-level look and actions: backgrounds, themes, deep links, action bar.
use crate::state::*;
use crate::telegram::envelope::ChatsPayload;

impl Session {
    /// Applies one chats payload; called by
    /// [`Session::apply_payload`].
    pub(crate) fn apply_chats_payload(
        &mut self,
        payload: ChatsPayload,
        pending: Option<&PendingRequest>,
        extra: Option<RequestId>,
        seq: u64,
    ) {
        match payload {
            ChatsPayload::UpdateChatAccentColors { chat_id, accent } => {
                self.chat_accents.insert(chat_id, accent);
            }
            ChatsPayload::UpdateNewChat {
                chat_id,
                title,
                kind,
                accent,
                unread_count,
                last_read_inbox_message_id,
                last_read_outbox_message_id,
                notification_settings,
                draft,
                photo,
                can_send_basic_messages,
                permissions,
                can_be_deleted_for_all_users,
                can_be_deleted_only_for_self,
                is_marked_as_unread,
                message_auto_delete_time,
                video_chat,
                has_welcome_messages,
                has_protected_content,
                available_reactions,
                has_scheduled_messages,
                message_sender,
                is_translatable,
                view_as_topics,
                default_disable_notification,
                background,
                theme_name,
                reply_markup_message_id,
                unread_mention_count,
                unread_reaction_count,
                unread_poll_vote_count,
                can_be_reported,
                action_bar,
                blocked,
                positions,
                last_message,
            } => {
                if let Some(view_as_topics) = view_as_topics {
                    self.set_chat_view_as_topics(chat_id.0, view_as_topics);
                }
                self.chat_accents.insert(chat_id.0, accent);
                if let Some(silent) = default_disable_notification {
                    self.sync.set_default_silent(chat_id.0, silent);
                }
                self.set_chat_background(chat_id.0, background);
                self.set_chat_theme_name(chat_id.0, theme_name);
                self.set_chat_protected(chat_id.0, has_protected_content);
                if let Some(setting) = available_reactions {
                    self.chat_available_reactions.insert(chat_id.0, setting);
                }
                // B7: the answer to `upgradeBasicGroupChatToSupergroupChat`
                // is the new supergroup chat; remember `old -> new`.
                if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::UpgradeBasicGroup
                    && let Some(old) = pending.chat_id
                {
                    self.chat_upgrades.push((old.0, chat_id.0));
                }
                self.set_chat_has_scheduled(chat_id.0, has_scheduled_messages);
                self.set_chat_message_sender(chat_id.0, message_sender);
                self.set_chat_translatable(chat_id.0, is_translatable);
                if reply_markup_message_id.0 > 0 {
                    self.reply_keyboards
                        .markup_message_ids
                        .insert(chat_id.0, reply_markup_message_id.0);
                }
                self.set_chat_action_bar(chat_id.0, action_bar);
                self.apply_update_new_chat(
                    chat_id,
                    title,
                    kind,
                    unread_count,
                    last_read_inbox_message_id,
                    last_read_outbox_message_id,
                    notification_settings,
                    draft,
                    photo,
                    can_send_basic_messages,
                    permissions,
                    can_be_deleted_for_all_users,
                    can_be_deleted_only_for_self,
                    is_marked_as_unread,
                    message_auto_delete_time,
                    video_chat,
                    has_welcome_messages,
                    unread_mention_count,
                    unread_reaction_count,
                    can_be_reported,
                    blocked,
                    pending,
                    extra,
                    seq,
                );
                if let Some(chat) = self.chats.get_mut(&chat_id.0) {
                    chat.unread_poll_vote_count = unread_poll_vote_count;
                }
                // `chat.last_message`: the starting preview. Never replaces
                // a newer one an `updateChatLastMessage` already set.
                if let Some(message) = last_message {
                    let newer_known = self
                        .chats
                        .get(&chat_id.0)
                        .and_then(|chat| chat.last_message)
                        .is_some_and(|known| known.id.0 > message.id.0);
                    if !newer_known {
                        self.remember_files(&message.files);
                        self.set_chat_last_message(chat_id, Some(&message));
                    }
                }
                // `chat.positions`: place the chat in every list it already
                // has a non-zero position in, like Telegram X
                // (`Tdlib.updateNewChat` → `TdlibChatList.onUpdateNewChat`).
                // Additive only — an empty set never evicts a position that
                // arrived first (TDLib reports later changes separately).
                let placed = positions.iter().any(|pos| pos.order != 0);
                for pos in positions.into_iter().filter(|pos| pos.order != 0) {
                    self.apply_position_fields(pos);
                }
                if placed {
                    self.rebuild_main_order();
                }
            }
            // Parity slice: `updateChatPhoto` — swap the cached small
            // photo file id (the chat list re-renders avatars from it).
            ChatsPayload::UpdateChatPhoto { chat_id, photo } => {
                let photo_file_id = photo.as_ref().map(|file| file.id.0);
                if let Some(file) = &photo {
                    self.remember_files(std::slice::from_ref(file));
                }
                let old_photo_file_id = std::mem::replace(
                    &mut self
                        .chats
                        .entry(chat_id.0)
                        .or_insert_with(|| placeholder_chat(chat_id))
                        .photo_file_id,
                    photo_file_id,
                );
                self.replace_avatar(old_photo_file_id, photo_file_id);
            }
            ChatsPayload::UpdateChatPermissions {
                chat_id,
                can_send_basic_messages,
                permissions,
            } => {
                let chat = self
                    .chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id));
                chat.can_send_basic_messages = can_send_basic_messages;
                // Slice G1: keep the full default permissions block for
                // the editor.
                chat.permissions = permissions;
            }
            ChatsPayload::UpdateProfileAccentColors {
                colors,
                available_ids,
            } => {
                // Slice A12: palette + settable ids for the edit-profile
                // accent picker. Replaces wholesale — the update is the
                // full server state.
                self.profile_accent_colors = colors;
                self.available_accent_color_ids = available_ids;
            }
            ChatsPayload::UpdateAccentColors {
                colors,
                available_ids: _,
            } => {
                // Server name-color palette (ids 7+); the renderer reads it
                // through the process-wide table.
                crate::telegram::name_accent::set_palette(&colors);
                self.name_accent_colors = colors;
            }
            ChatsPayload::UpdateChatHasProtectedContent {
                chat_id,
                has_protected_content,
            } => self.set_chat_protected(chat_id, has_protected_content),
            // B7: allowed reactions and the active emoji list.
            ChatsPayload::UpdateChatAvailableReactions {
                chat_id,
                available_reactions,
            } => {
                self.chat_available_reactions
                    .insert(chat_id, available_reactions);
                // An open reaction picker for this chat refetches.
                if self
                    .message_reaction_options
                    .as_ref()
                    .is_some_and(|options| options.chat_id.0 == chat_id)
                {
                    self.reaction_options_stale = true;
                }
            }
            // Batch 8: `updateChatActionBar` (schema 1.8.67, line 10526).
            ChatsPayload::UpdateChatActionBar {
                chat_id,
                action_bar,
            } => self.set_chat_action_bar(chat_id.0, action_bar),
            ChatsPayload::UpdateChatNotificationSettings {
                chat_id,
                notification_settings,
            } => {
                let chat = self
                    .chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id));
                // A placeholder chat (kind unknown — a server id with no
                // local chat, e.g. archived or never opened) has no
                // trustworthy scope: `scope_for_chat_kind` would always
                // guess GroupChats. Prune/invalidate across all scopes
                // instead of the guessed one.
                let kind_unknown = matches!(chat.kind, ChatKind::Unknown);
                let scope = scope_for_chat_kind(&chat.kind);
                let fully_default = notification_settings == ChatNotificationSettings::default();
                chat.notification_settings = notification_settings;
                // Parity slice: the exceptions list for the chat's scope is
                // stale now. A reset to the scope default (e.g. our own
                // "Reset to default") just prunes the chat from the cached
                // list; any other change drops the list so the next dialog
                // open refetches it.
                if fully_default {
                    if kind_unknown {
                        for list in self.settings.notification_exceptions.values_mut() {
                            list.retain(|id| *id != chat_id.0);
                        }
                    } else if let Some(list) = self.settings.notification_exceptions.get_mut(&scope)
                    {
                        list.retain(|id| *id != chat_id.0);
                    }
                } else if kind_unknown {
                    self.settings.notification_exceptions.clear();
                } else {
                    self.settings.notification_exceptions.remove(&scope);
                }
            }
            ChatsPayload::UpdateChatDefaultDisableNotification {
                chat_id,
                default_disable_notification,
            } => self
                .sync
                .set_default_silent(chat_id.0, default_disable_notification),
            // Phase B4: `updateChatMessageAutoDeleteTime` (schema 1.8.67,
            // line 10549) — keep the chat-level timer fresh. The same
            // change also lands in history as a
            // `messageChatSetMessageAutoDeleteTime` service row.
            ChatsPayload::UpdateChatMessageAutoDeleteTime {
                chat_id,
                message_auto_delete_time,
            } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .message_auto_delete_time = message_auto_delete_time;
            }
            ChatsPayload::UpdateChatAction {
                chat_id,
                sender,
                action,
            } => {
                let name = self.sender_first_name(sender);
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .set_sender_action(sender, action, name);
            }
            ChatsPayload::UpdateChatTitle { chat_id, title } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .title = title;
            }
            ChatsPayload::UpdateChatReadInbox {
                chat_id,
                last_read_inbox_message_id,
                unread_count,
            } => {
                let chat = self
                    .chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id));
                chat.last_read_inbox_message_id = last_read_inbox_message_id;
                chat.unread_count = unread_count;
                // Read here or on another device: whatever toast we showed
                // for the chat is stale (tdesktop `clearFromHistory`).
                if unread_count == 0 {
                    self.clear_chat_notifications(chat_id);
                }
            }
            // Slice CL3: mention / reaction badge counts (schema 1.8.67,
            // lines 10567/10570).
            ChatsPayload::UpdateChatUnreadMentionCount {
                chat_id,
                unread_mention_count,
            } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .unread_mention_count = unread_mention_count;
            }
            ChatsPayload::UpdateChatUnreadReactionCount {
                chat_id,
                unread_reaction_count,
            } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .unread_reaction_count = unread_reaction_count;
            }
            // B15: poll-vote badge count (schema 1.8.67, lines 10457/10573).
            ChatsPayload::UpdateChatUnreadPollVoteCount {
                chat_id,
                unread_poll_vote_count,
            } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .unread_poll_vote_count = unread_poll_vote_count;
            }
            // Slice CL3: `updateChatBlockList` (schema 1.8.67, line
            // 10594).
            ChatsPayload::UpdateChatBlockList { chat_id, blocked } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .blocked = blocked;
            }
            // Slice CL3: `reportChat` result — surfaced as a status note
            // via the same drain as `chat_action_error`; a refusal or a
            // "more info required" is never shown as success.
            ChatsPayload::ReportChatResult(outcome) => {
                match pending.map(|p| p.purpose) {
                    Some(RequestPurpose::ReportChat) => {
                        self.report_chat_outcome = Some(match outcome {
                            ReportChatOutcome::Ok => "chat reported".to_string(),
                            _ => "report needs a reason or messages — the chat list only sends simple spam reports".to_string(),
                        });
                    }
                    // The message menu's Report flow walks the answers.
                    Some(RequestPurpose::ReportMessages) => {
                        if let Some(pending) = pending {
                            self.accept_message_report(pending, outcome);
                        }
                    }
                    _ => {}
                }
            }
            ChatsPayload::UpdateChatReadOutbox {
                chat_id,
                last_read_outbox_message_id,
            } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .last_read_outbox_message_id = last_read_outbox_message_id;
            }
            ChatsPayload::Backgrounds(list) => {
                for background in &list {
                    if let Some(file) = &background.file {
                        self.upsert_file(file.clone(), false);
                    }
                }
                self.installed_backgrounds = Some(list);
            }
            ChatsPayload::Background(background) => {
                if let Some(file) = &background.file {
                    self.upsert_file(file.clone(), false);
                }
                match pending.map(|p| p.purpose) {
                    Some(
                        RequestPurpose::SetDefaultBackground
                        | RequestPurpose::SetDefaultBackgroundLocal,
                    ) => {
                        self.default_backgrounds
                            .insert(self.background_set_for_dark, background);
                    }
                    Some(RequestPurpose::SearchBackground) => {
                        self.searched_background = Some(background);
                    }
                    _ => {}
                }
            }
            ChatsPayload::UpdateChatBackground {
                chat_id,
                background,
            } => self.set_chat_background(chat_id.0, background),
            ChatsPayload::UpdateChatTheme {
                chat_id,
                theme_name,
            } => self.set_chat_theme_name(chat_id.0, theme_name),
            ChatsPayload::UpdateEmojiChatThemes(themes) => {
                for theme in &themes {
                    for settings in [&theme.light, &theme.dark] {
                        if let Some(file) =
                            settings.background.as_ref().and_then(|b| b.file.as_ref())
                        {
                            self.upsert_file(file.clone(), false);
                        }
                    }
                }
                self.emoji_chat_themes = themes;
            }
            ChatsPayload::UpdateDefaultBackground {
                for_dark_theme,
                background,
            } => {
                if let Some(file) = &background.file {
                    self.upsert_file(file.clone(), false);
                }
                self.default_backgrounds.insert(for_dark_theme, background);
            }
            ChatsPayload::ChatPhotos {
                total_count,
                photos,
            } => {
                if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::GetUserProfilePhotos
                {
                    self.apply_profile_photos(total_count, photos, pending);
                }
            }
            ChatsPayload::UpdateChatOnlineMemberCount {
                chat_id,
                online_member_count,
            } => {
                self.chat_online_counts.insert(chat_id, online_member_count);
            }
            ChatsPayload::InternalLinkType(link) => {
                if let Some(RequestPurpose::Chats(ChatsPurpose::DeepLinkInternalType {
                    generation,
                })) = pending.map(|p| p.purpose)
                    && matches!(
                        &self.deep_link,
                        Some(DeepLinkState::ResolvingInfo { generation: slot }) if *slot == generation
                    )
                {
                    use crate::deep_link_types::{LinkRoute, route};
                    let original = std::mem::take(&mut self.deep_link_original);
                    self.deep_link = Some(match route(&link, &original) {
                        LinkRoute::Resolve(action) => DeepLinkState::Info {
                            text: String::new(),
                            need_update: false,
                            action: Some(action),
                            generation,
                        },
                        LinkRoute::Ui(ui) => DeepLinkState::Ui(ui),
                        LinkRoute::Unknown(link) => DeepLinkState::Unknown { link },
                        LinkRoute::Message(text) => DeepLinkState::ShowText(text),
                    });
                }
            }
            ChatsPayload::MessageLinkInfo {
                chat_id,
                message_id,
                media_timestamp,
                thread_id,
            } => {
                if let Some(RequestPurpose::Chats(ChatsPurpose::DeepLinkResolve { generation })) =
                    pending.map(|p| p.purpose)
                    && matches!(
                        &self.deep_link,
                        Some(DeepLinkState::ResolvingChat {
                            action: DeepLinkAction::MessageLink { .. },
                            generation: slot,
                        }) if *slot == generation
                    )
                {
                    self.deep_link = Some(if chat_id == 0 {
                        DeepLinkState::ShowText(
                            "This message is in a chat you can't see. Join it first.".into(),
                        )
                    } else {
                        DeepLinkState::Info {
                            text: String::new(),
                            need_update: false,
                            action: Some(DeepLinkAction::OpenChatById {
                                chat_id,
                                message_id,
                                media_timestamp,
                                thread_id,
                            }),
                            generation,
                        }
                    });
                }
            }
            ChatsPayload::ChatBoostLinkInfo { chat_id } => {
                if let Some(RequestPurpose::Chats(ChatsPurpose::DeepLinkResolve { generation })) =
                    pending.map(|p| p.purpose)
                    && matches!(
                        &self.deep_link,
                        Some(DeepLinkState::ResolvingChat {
                            action: DeepLinkAction::BoostLink { .. },
                            generation: slot,
                        }) if *slot == generation
                    )
                {
                    self.deep_link = Some(if chat_id == 0 {
                        DeepLinkState::ShowText("This boost link is broken.".into())
                    } else {
                        DeepLinkState::Info {
                            text: String::new(),
                            need_update: false,
                            action: Some(DeepLinkAction::OpenChannelBoost { chat_id }),
                            generation,
                        }
                    });
                }
            }
            // `parity:platform-deep-links`: `getDeepLinkInfo` answer. The
            // actionable destination is parsed from the `textEntityTypeTextUrl`
            // entities; the generation guard drops stale answers. The UI
            // consumes `Info` once (follow-up request, or a dialog with
            // `text` when there is no action / an update is required).
            ChatsPayload::DeepLinkInfo {
                text,
                need_update,
                entities,
            } => {
                if let Some(RequestPurpose::Chats(ChatsPurpose::DeepLinkInfo { generation })) =
                    pending.map(|p| p.purpose)
                    && matches!(
                        &self.deep_link,
                        Some(DeepLinkState::ResolvingInfo {
                            generation: slot
                        }) if *slot == generation
                    )
                {
                    let action = crate::connect::parse_deep_link_action(&entities);
                    self.deep_link = Some(DeepLinkState::Info {
                        text,
                        need_update,
                        action,
                        generation,
                    });
                }
            }
        }
    }
}
