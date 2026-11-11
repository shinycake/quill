//! Failed requests for users, contacts, profiles and secret chats.
use crate::state::*;

impl Session {
    /// Reacts to a failed users request; called by
    /// [`Session::apply_error`] after the shared handling.
    pub(crate) fn apply_users_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
        extra: Option<RequestId>,
        _seq: u64,
    ) {
        // The failed check is over: its text no longer reads "Checking…".
        // A superseded check (the person typed on) is not the current one:
        // its failure says nothing about the text now being checked.
        let current_check = extra.is_some()
            && self
                .users_state
                .username_check_pending
                .as_ref()
                .map(|(id, _)| *id)
                == extra;
        if current_check {
            self.users_state.username_check_pending = None;
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::CheckUsername) && !current_check {
            return;
        }
        match pending.map(|p| p.purpose) {
            // A5: profile-edit failures surface in the
            // edit-profile dialog. The message is classified by
            // `error_reason`, never the raw TDLib text.
            Some(
                RequestPurpose::SetName
                | RequestPurpose::SetBio
                | RequestPurpose::SetUsername
                | RequestPurpose::CheckUsername
                | RequestPurpose::ReorderActiveUsernames
                | RequestPurpose::ToggleUsernameIsActive
                | RequestPurpose::SetProfilePhoto
                | RequestPurpose::DeleteProfilePhoto,
            ) => {
                self.users_state.profile_edit_error =
                    Some(format!("Profile update failed: {}", error_reason(err)));
            }
            Some(
                RequestPurpose::SetProfileAccentColor
                | RequestPurpose::SetAccentColor
                | RequestPurpose::SetUpgradedGiftColors,
            ) => {
                self.users_state.peer_color_error =
                    Some(format!("Profile update failed: {}", error_reason(err)));
            }
            Some(RequestPurpose::GetUserFullInfo) => {
                let user = pending.and_then(|p| {
                    p.user_id.or_else(|| {
                        p.chat_id
                            .and_then(|chat_id| self.private_chat_user_id(chat_id))
                    })
                });
                if let Some(user) = user {
                    self.users_state
                        .user_full_infos
                        .fail(user, error_reason(err));
                }
            }
            // One filter's count failed: drop the chat's entry so the next
            // open asks for all of them again, not a zero for good.
            Some(RequestPurpose::Users(UsersPurpose::GetChatMessageCount { .. })) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.media.chat_media_counts.remove(&chat_id.0);
                }
            }
            Some(RequestPurpose::GetProfilePhotoEmojiIcons) => {
                self.users_state.profile_photo_emoji_icons_failed = true;
            }
            Some(RequestPurpose::GetChatPhotoEmojiIcons) => {
                self.users_state.chat_photo_emoji_icons_failed = true;
            }
            Some(RequestPurpose::GetProfileShareThemeBackdrops) => {
                self.users_state.peer_qr.backdrops.fail();
            }
            Some(RequestPurpose::Users(UsersPurpose::GetShareThemeGifts {
                owner, serial, ..
            })) => {
                let gifts = &mut self.users_state.peer_qr.gifts;
                if gifts.is_current(owner, serial) {
                    gifts.fail();
                }
            }
            Some(RequestPurpose::Users(UsersPurpose::SetProfileShareTheme { ticket })) => {
                self.users_state
                    .peer_qr
                    .fail_save(ticket, error_reason(err));
            }
            Some(RequestPurpose::Users(UsersPurpose::GetUserLink { free })) => {
                self.users_state.peer_qr.fail_link(free, error_reason(err));
            }
            Some(RequestPurpose::GetBackgroundEmojiIcons) => {
                self.users_state.background_emoji_icons_failed = true;
            }
            // B10: profile panel fetches keep the reason for a Retry row;
            // refused edits surface as a toast.
            Some(
                RequestPurpose::GetProfileChats(_)
                | RequestPurpose::GetUserProfilePhotos
                | RequestPurpose::GetChatPhotoHistory,
            ) => {
                if let Some(pending) = pending {
                    self.fail_profile_fetch(pending, error_reason(err));
                }
            }
            Some(
                RequestPurpose::SetBirthdate
                | RequestPurpose::SetMainProfileTab
                | RequestPurpose::SetPersonalChat
                | RequestPurpose::SetUserNote
                | RequestPurpose::SetUserPersonalPhoto,
            ) => {
                self.chats_state
                    .chat_action_error
                    .push(format!("could not save the change (error {})", err.code));
            }
            Some(RequestPurpose::ReportChatPhoto) => {
                self.chats_state
                    .chat_action_error
                    .push(format!("could not send the report (error {})", err.code));
            }
            Some(RequestPurpose::SharePhoneNumber) => {
                self.chats_state.chat_action_error.push(format!(
                    "could not share your phone number (error {})",
                    err.code
                ));
            }
            Some(RequestPurpose::Users(UsersPurpose::SetMessageSenderBlockList { .. })) => {
                self.chats_state.chat_action_error.push(format!(
                    "could not change the block state (error {})",
                    err.code
                ));
            }
            Some(RequestPurpose::BlockSenderFromReplies) => {
                self.chats_state.chat_action_error.push(
                    crate::lang::tr(crate::lang::keys::Q_CHAT_COULDNT_BLOCK_THE_SENDER_TRY_AGAIN)
                        .to_string(),
                );
            }
            // Slice A6: contacts mutations — the notice surfaces
            // in the contacts settings section.
            Some(RequestPurpose::RemoveContact) => {
                let what = if pending.and_then(|p| p.user_id).is_some() {
                    "the contact"
                } else {
                    "synced contacts"
                };
                self.users_state.contacts_notice =
                    Some(format!("could not delete {what} (error {})", err.code));
            }
            Some(RequestPurpose::ImportContacts) => {
                self.users_state.contacts_notice =
                    Some(format!("could not import contacts (error {})", err.code));
            }
            Some(RequestPurpose::AddContactByPhone) => {
                let id = pending.map(|p| p.id);
                if id.is_some() && self.users_state.new_contact_request == id {
                    self.users_state.new_contact_request = None;
                    self.users_state.new_contact_outcome =
                        pending.map(|p| (p.id, NewContactOutcome::Failed(err.code)));
                }
            }
            Some(RequestPurpose::ClearImportedContacts) => {
                self.users_state.contacts_notice = Some(format!(
                    "could not delete synced contacts (error {})",
                    err.code
                ));
            }
            _ => {}
        }
        // Phase 6: a failed `getContacts` surfaces a retry in the
        // contacts tab instead of a stuck spinner.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetContacts) {
            self.users_state.contacts_error = true;
        }
    }
}
