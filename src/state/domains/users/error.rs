//! Failed requests for users, contacts, profiles and secret chats.
use crate::state::*;

impl Session {
    /// Reacts to a failed users request; called by
    /// [`Session::apply_error`] after the shared handling.
    pub(crate) fn apply_users_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::AddProfileAudio) => {
                self.messages.message_action_note = Some(format!(
                    "could not save to your profile: {}",
                    error_reason(err)
                ));
            }
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
                | RequestPurpose::DeleteProfilePhoto
                | RequestPurpose::SetProfileAccentColor,
            ) => {
                self.users_state.profile_edit_error =
                    Some(format!("Profile update failed: {}", error_reason(err)));
            }
            // B10: profile panel fetches keep the reason for a Retry row;
            // refused edits surface as a toast.
            Some(RequestPurpose::GetProfileChats(_) | RequestPurpose::GetUserProfilePhotos) => {
                if let Some(pending) = pending {
                    self.fail_profile_fetch(pending, error_reason(err));
                }
            }
            Some(
                RequestPurpose::SetBirthdate
                | RequestPurpose::SetPersonalChat
                | RequestPurpose::SetUserNote
                | RequestPurpose::SetUserPersonalPhoto,
            ) => {
                self.chats_state.chat_action_error =
                    Some(format!("could not save the change (error {})", err.code));
            }
            Some(RequestPurpose::ReportChatPhoto) => {
                self.chats_state.chat_action_error =
                    Some(format!("could not send the report (error {})", err.code));
            }
            Some(RequestPurpose::SharePhoneNumber) => {
                self.chats_state.chat_action_error = Some(format!(
                    "could not share your phone number (error {})",
                    err.code
                ));
            }
            Some(RequestPurpose::Users(UsersPurpose::SetMessageSenderBlockList { .. })) => {
                self.chats_state.chat_action_error = Some(format!(
                    "could not change the block state (error {})",
                    err.code
                ));
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
