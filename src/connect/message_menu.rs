//! Connect driver: the message context menu's Report flow, its seen /
//! reacted lists, and the admin moderation actions.
use super::*;
use crate::ids::{ChatId, MessageId, RequestId};
use crate::moderation::ModerationStep;
use crate::state::RequestPurpose;
use crate::telegram::envelope::{ChatKind, ChatPermissions, MessageActions, MessageSender};
use crate::telegram::requests::{
    MessageSenderRef, add_profile_audio, delete_chat_messages_by_sender,
    delete_message_reactions_from_sender, delete_messages, get_installed_sticker_sets,
    get_message_added_reactions, get_message_read_date, get_message_viewers, get_sticker_set,
    report_chat_messages, report_supergroup_spam,
};

/// One page of reactors the menu list asks for (Telegram Desktop loads 50
/// at a time).
pub const ADDED_REACTIONS_PAGE: i32 = 50;

/// What the delete box's admin checkboxes ask for on top of the delete
/// itself (`boxes/moderate_messages_box.cpp`).
pub use crate::moderation::ModerateChoice as ModerationChoice;

impl<S: JsonSender> ConnectDriver<S> {
    /// `setMessageFactCheck` for the fact-check dialog (empty removes it).
    pub fn set_fact_check(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        text: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || message_id.0 <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetMessageFactCheck, Some(chat_id));
        let json =
            crate::telegram::requests::set_message_fact_check(extra, chat_id, message_id, text);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// The menu's "Save for Notifications": `addSavedNotificationSound`
    /// with the song or voice message's file. The saved-sound list
    /// refetches once TDLib confirms.
    pub fn save_notification_tone(
        &mut self,
        file_id: crate::ids::FileId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || file_id.0 <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::AddSavedNotificationSound, None);
        let json = crate::telegram::requests::add_saved_notification_sound(extra, file_id);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Send one step of the Report flow. The first call (empty
    /// `option_id`) opens the flow; later calls echo the option the user
    /// picked, or the details text. `chosen` is `(option text, current
    /// title, current options)` so the dialog can go back one level.
    pub fn report_messages(
        &mut self,
        chat_id: ChatId,
        message_ids: &[MessageId],
        option_id: &str,
        text: &str,
        chosen: Option<(String, String, Vec<crate::telegram::envelope::ReportOption>)>,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || message_ids.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.chats.contains_key(&chat_id.0) {
            return Err(ConnectSendError::InvalidRequest);
        }
        if option_id.is_empty() && text.is_empty() {
            self.session
                .begin_message_report(chat_id, message_ids.to_vec());
        } else {
            self.session.message_report_sending(chosen);
        }
        let extra = self
            .session
            .request(RequestPurpose::ReportMessages, Some(chat_id));
        let json = report_chat_messages(extra, chat_id, option_id, message_ids, text);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session
                    .fail_message_report(chat_id, "Could not send the report.".into());
                Err(err)
            }
        }
    }

    /// Ask who saw / reacted to the message the menu is open on, as far as
    /// `getMessageProperties` allows it. Each list is its own request and
    /// fills in independently.
    pub fn fetch_message_audience(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        actions: MessageActions,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.begin_message_audience(chat_id, message_id);
        let private = matches!(
            self.session.chats.get(&chat_id.0).map(|c| &c.kind),
            Some(ChatKind::Private { .. } | ChatKind::Secret { .. })
        );
        let can_react_list = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|h| h.messages.get(&message_id.0))
            .and_then(|m| m.interaction_info.as_ref())
            .and_then(|info| info.reactions.as_ref())
            .is_some_and(|r| r.can_get_added_reactions && !r.reactions.is_empty());
        if actions.can_get_viewers && !private {
            let extra = self.session.request(
                RequestPurpose::GetMessageViewers {
                    chat_id,
                    message_id,
                },
                Some(chat_id),
            );
            self.send_audience(extra, &get_message_viewers(extra, chat_id, message_id))?;
            self.session.audience_viewers_loading(chat_id, message_id);
        }
        if actions.can_get_read_date && private {
            let extra = self.session.request(
                RequestPurpose::GetMessageReadDate {
                    chat_id,
                    message_id,
                },
                Some(chat_id),
            );
            self.send_audience(extra, &get_message_read_date(extra, chat_id, message_id))?;
            self.session.audience_read_date_loading(chat_id, message_id);
        }
        if can_react_list {
            let extra = self.session.request(
                RequestPurpose::GetMessageAddedReactions {
                    chat_id,
                    message_id,
                    filter: 0,
                    append: false,
                },
                Some(chat_id),
            );
            self.send_audience(
                extra,
                &get_message_added_reactions(
                    extra,
                    chat_id,
                    message_id,
                    None,
                    "",
                    ADDED_REACTIONS_PAGE,
                ),
            )?;
            self.session.audience_reactions_loading(chat_id, message_id);
        }
        Ok(())
    }

    /// A "who reacted" tab: its first page when it was never asked, or
    /// (`more`) the next page of what it lists. `reaction` `None` is the
    /// "All" tab.
    pub fn fetch_reactors_tab(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        reaction: Option<&crate::telegram::envelope::ReactionType>,
        more: bool,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let filter = reaction.map_or(0, crate::state::reaction_filter_key);
        let Some(audience) = self
            .session
            .message_audience
            .as_ref()
            .filter(|a| a.chat_id == chat_id && a.message_id == message_id)
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let slot = if filter == 0 {
            Some(&audience.reactions)
        } else {
            audience.filtered.get(&filter)
        };
        let offset = if more {
            match slot.and_then(|slot| slot.ready()) {
                Some(page)
                    if !page.next_offset.is_empty() && !audience.more_loading.contains(&filter) =>
                {
                    page.next_offset.clone()
                }
                _ => return Ok(()),
            }
        } else if slot.is_some_and(|slot| {
            !matches!(
                slot,
                crate::state::Audience::NotAsked | crate::state::Audience::Failed
            )
        }) {
            return Ok(());
        } else {
            String::new()
        };
        let extra = self.session.request(
            RequestPurpose::GetMessageAddedReactions {
                chat_id,
                message_id,
                filter,
                append: more,
            },
            Some(chat_id),
        );
        self.send_audience(
            extra,
            &get_message_added_reactions(
                extra,
                chat_id,
                message_id,
                reaction,
                &offset,
                ADDED_REACTIONS_PAGE,
            ),
        )?;
        if more {
            self.session
                .audience_tab_loading_more(chat_id, message_id, filter);
        } else {
            self.session
                .audience_tab_loading(chat_id, message_id, filter);
        }
        Ok(())
    }

    fn send_audience(&mut self, extra: RequestId, json: &str) -> Result<(), ConnectSendError> {
        self.sender.send_json(json).inspect_err(|_| {
            self.session.requests.take(extra);
        })
    }

    /// The admin checkboxes of the delete box: report the message as spam,
    /// delete everything the sender wrote, drop their reaction, ban or
    /// restrict them. Each is its own request, planned by
    /// [`crate::moderation::plan_moderation`] and gated on what the group
    /// allows (the creator or an admin with the right); nothing here
    /// deletes the message itself.
    pub fn moderate_message(
        &mut self,
        chat_id: ChatId,
        message_ids: &[MessageId],
        user_id: i64,
        choice: ModerationChoice,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supergroup_id = match self.session.chats.get(&chat_id.0).map(|c| &c.kind) {
            Some(ChatKind::Supergroup { supergroup_id, .. }) => *supergroup_id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        for step in crate::moderation::plan_moderation(choice) {
            match step {
                ModerationStep::ReportSpam => {
                    let extra = self
                        .session
                        .request(RequestPurpose::ReportSupergroupSpam, Some(chat_id));
                    let json = report_supergroup_spam(extra, supergroup_id, message_ids);
                    self.send_audience(extra, &json)?;
                }
                ModerationStep::DeleteAllFromSender => {
                    let extra = self
                        .session
                        .request(RequestPurpose::DeleteChatMessagesBySender, Some(chat_id));
                    let json = delete_chat_messages_by_sender(extra, chat_id, user_id);
                    self.send_audience(extra, &json)?;
                }
                ModerationStep::DeleteReactionsFromSender => {
                    if let Some(message_id) = message_ids.first() {
                        self.delete_message_reactions_from(
                            chat_id,
                            *message_id,
                            MessageSender::User { user_id },
                        )?;
                    }
                }
                // Reports and deletes name the user's messages, so the ban
                // goes last; both are gated on `can_restrict_members`.
                ModerationStep::Ban => {
                    self.ban_chat_member(chat_id, user_id, 0)?;
                }
                ModerationStep::Restrict => {
                    // View-only: the member stays and can read, nothing else.
                    self.restrict_chat_member(chat_id, user_id, 0, &ChatPermissions::default())?;
                }
            }
        }
        Ok(())
    }

    /// An admin removes one member's reactions from a message
    /// (`deleteMessageReactionsFromSender`). Allowed only when
    /// `messageProperties.can_delete_reactions` said so for this message.
    pub fn delete_message_reactions_from(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        sender: MessageSender,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let allowed = self
            .session
            .message_menu_actions
            .is_some_and(|(c, m, a)| c == chat_id && m == message_id && a.can_delete_reactions);
        if !allowed {
            return Ok(None);
        }
        let user_id = match sender {
            MessageSender::User { user_id } => user_id,
            MessageSender::Chat { .. } => 0,
        };
        let extra = self.session.request(
            RequestPurpose::DeleteMessageReactionsFromSender {
                message_id: message_id.0,
                user_id,
            },
            Some(chat_id),
        );
        let sender = match sender {
            MessageSender::User { user_id } => MessageSenderRef::User(user_id),
            MessageSender::Chat { chat_id } => MessageSenderRef::Chat(chat_id),
        };
        let json =
            delete_message_reactions_from_sender(extra, chat_id, message_id, &sender.to_value());
        self.send_audience(extra, &json)?;
        Ok(Some(extra))
    }

    /// "Save to... Profile": add a song to the profile's music.
    pub fn save_audio_to_profile(
        &mut self,
        file_id: crate::ids::FileId,
        duration: i32,
        title: &str,
        performer: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || file_id.0 <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::AddProfileAudio, None);
        let json = add_profile_audio(extra, file_id.0, duration, title, performer);
        self.send_audience(extra, &json)?;
        Ok(extra)
    }

    /// "Cancel Upload": TDLib stops sending a message that is still being
    /// sent when it is deleted. Only a pending media message qualifies.
    pub fn cancel_upload(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let uploading = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|h| h.messages.get(&message_id.0))
            .is_some_and(|m| m.pending && crate::message_menu::media_target(&m.content).is_some());
        if !uploading {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::CancelUpload, Some(chat_id));
        let json = delete_messages(extra, chat_id, &[message_id], false);
        self.send_audience(extra, &json)?;
        Ok(extra)
    }

    /// Open the sticker set a message's sticker belongs to
    /// (`getStickerSet`); the answer lands in `Session::sticker_set_view`.
    pub fn view_sticker_set(&mut self, set_id: i64) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || set_id == 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.sticker_set_view = Some(crate::state::StickerSetView {
            set_id,
            stage: crate::state::StickerSetViewStage::Loading,
            files_requested: false,
        });
        let extra = self
            .session
            .request(RequestPurpose::ViewStickerSet { set_id }, None);
        match self.sender.send_json(&get_sticker_set(extra, set_id)) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.fail_sticker_set_view(set_id);
                Err(err)
            }
        }
    }

    /// Download the sticker previews of the open set view, once.
    pub fn ensure_sticker_set_view_files(&mut self) -> Result<(), ConnectSendError> {
        let Some(view) = self.session.sticker_set_view.as_mut() else {
            return Ok(());
        };
        let crate::state::StickerSetViewStage::Ready { stickers, .. } = &view.stage else {
            return Ok(());
        };
        if view.files_requested {
            return Ok(());
        }
        view.files_requested = true;
        let files: Vec<crate::ids::FileId> = stickers
            .iter()
            .filter_map(|s| s.display_file_id())
            .take(60)
            .collect();
        self.ensure_media_files(&files)
    }

    /// What a sticker message's menu needs to word its sticker items:
    /// whether the set is installed and whether the sticker is a favorite.
    pub fn fetch_sticker_menu_facts(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.stickers.installed_loaded
            && !self
                .session
                .requests
                .has_purpose(RequestPurpose::GetInstalledStickerSets)
        {
            let extra = self
                .session
                .request(RequestPurpose::GetInstalledStickerSets, None);
            self.send_audience(extra, &get_installed_sticker_sets(extra))?;
        }
        self.fetch_editor_stickers()
    }
}
