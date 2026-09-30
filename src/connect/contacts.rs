//! Connect driver: contacts and user info.
use super::*;
use crate::ids::{FileId, RequestId};
use crate::settings::save_contact_prefs;
use crate::state::RequestPurpose;
use crate::telegram::requests::{
    ImportedContact, add_contact, clear_imported_contacts, get_contacts, get_user_full_info,
    import_contacts, remove_contacts, set_message_sender_block_list,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// Phase 6: `getContacts` for the contacts tab. Fires once per list
    /// (deduped by a settled `Session::contacts` + in-flight purpose); a
    /// failed attempt clears its error flag on retry. The `users` response
    /// lands the id list; user objects arrive via `updateUser`.
    pub fn fetch_contacts(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.contacts.is_some()
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetContacts)
        {
            return Ok(None);
        }
        self.session.contacts_error = false;
        let extra = self.session.request(RequestPurpose::GetContacts, None);
        if let Err(err) = self.sender.send_json(&get_contacts(extra)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase 6: user-scoped `getUserFullInfo` for the user info panel
    /// (opened from the contacts list, where there is no chat to resolve
    /// through). Deduped by the bio cache + in-flight user id; the id-less
    /// response correlates via `PendingRequest::user_id`.
    pub fn fetch_user_full_info(
        &mut self,
        user_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.user_full_infos.contains_key(&user_id)
            || self
                .session
                .requests
                .has_purpose_for_user(RequestPurpose::GetUserFullInfo, user_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request_for_user(RequestPurpose::GetUserFullInfo, user_id);
        if let Err(err) = self.sender.send_json(&get_user_full_info(extra, user_id)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase 6: `addContact` from the add-contact dialog. The reducer
    /// invalidates the contacts list on `ok`; the new contact row arrives
    /// via `updateUser`. `share_phone_number` stays `false` — sharing the
    /// user's own number is a privacy decision the dialog does not ask
    /// for (documented in DECISIONS.md).
    pub fn add_contact(
        &mut self,
        user_id: i64,
        phone_number: &str,
        first_name: &str,
        last_name: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request_for_user(RequestPurpose::AddContact, user_id);
        if let Err(err) = self.sender.send_json(&add_contact(
            extra,
            user_id,
            phone_number,
            first_name,
            last_name,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice A6: `removeContacts` (schema 1.8.67, line 14528) for one
    /// contact — the user-panel "Delete contact" confirm (TGX
    /// `TdlibUi.deleteContact` → `RemoveContacts`). The reducer
    /// invalidates the contacts list on `ok`.
    pub fn remove_contact(&mut self, user_id: i64) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request_for_user(RequestPurpose::RemoveContact, user_id);
        if let Err(err) = self.sender.send_json(&remove_contacts(extra, &[user_id])) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice A6: `importContacts` (schema 1.8.67, line 14517) from the
    /// import dialog's parsed vCard contacts. `Ok(None)` = nothing to
    /// import or an import already in flight. The reducer invalidates
    /// the contacts list on `ok`; the `importedContacts` response itself
    /// carries no per-contact user mapping worth keeping.
    pub fn import_contacts(
        &mut self,
        contacts: &[ImportedContact],
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if contacts.is_empty() {
            return Ok(None);
        }
        let purpose = RequestPurpose::ImportContacts;
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, None);
        if let Err(err) = self.sender.send_json(&import_contacts(extra, contacts)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice A6: "Delete synced contacts" (TGX
    /// `TdlibContactManager.deleteContacts`, `SyncContactsDeleteInfo`) —
    /// `clearImportedContacts` (schema 1.8.67, line 14539) wipes the
    /// imported set server-side, then `removeContacts` drops the contact
    /// associations TDLib keeps (schema: `clearImportedContacts` leaves
    /// "contact list remains unchanged"). Both are sent in order without
    /// waiting for the first `ok` (TGX issues them the same way), with
    /// TGX's middle empty `changeImportedContacts` step skipped — no
    /// device address book to sync against, so clear+remove fully
    /// achieves the delete (see DECISIONS.md slice A6). The reducer
    /// invalidates the contacts list on either `ok`.
    pub fn delete_synced_contacts(&mut self) -> Result<usize, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let mut sent = 0;
        let extra = self
            .session
            .request(RequestPurpose::ClearImportedContacts, None);
        if let Err(err) = self.sender.send_json(&clear_imported_contacts(extra)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        sent += 1;
        if let Some(ids) = self.session.contacts.clone()
            && !ids.is_empty()
        {
            let extra = self.session.request(RequestPurpose::RemoveContact, None);
            if let Err(err) = self.sender.send_json(&remove_contacts(extra, &ids)) {
                self.session.requests.take(extra);
                return Err(err);
            }
            sent += 1;
        }
        Ok(sent)
    }

    /// Slice A6: user-scoped `setMessageSenderBlockList` (schema 1.8.67,
    /// line 14492) for the user info panel, where there is no chat to
    /// resolve through (the chat-scoped twin is
    /// `set_chat_user_blocked`, CL3). The new state arrives via
    /// `updateChatBlockList` / `updateUserFullInfo`.
    pub fn set_user_blocked(
        &mut self,
        user_id: i64,
        block: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.my_user_id.is_some_and(|me| me == user_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::SetMessageSenderBlockList { block };
        if self.session.requests.has_purpose_for_user(purpose, user_id) {
            return Ok(None);
        }
        let extra = self.session.request_for_user(purpose, user_id);
        if let Err(err) = self
            .sender
            .send_json(&set_message_sender_block_list(extra, user_id, block))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice A6: persist the contacts preferences edited from the
    /// Contacts tab (same account-scoped dir as the other settings
    /// files).
    pub fn save_contact_prefs(&mut self) -> std::io::Result<()> {
        save_contact_prefs(&self.paths, &self.session.contact_prefs)
    }

    /// Phase 6: download the small profile photo for the user info panel
    /// (thumb priority). No-op when the user has no photo, or the file is
    /// already local / in flight.
    pub fn download_user_photo(
        &mut self,
        user_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        // Prefer the photo from the cached `userFullInfo` (fetched for the
        // panel); fall back to the `user.profile_photo.small` file id.
        let file_id = self
            .session
            .user_full_info(user_id)
            .and_then(|info| info.photo_file_id)
            .map(FileId)
            .unwrap_or_else(|| {
                self.session
                    .user(user_id)
                    .map(|user| FileId(user.photo_small_file_id))
                    .unwrap_or(FileId(0))
            });
        self.download_file(file_id, THUMB_DOWNLOAD_PRIORITY, false)
    }
}
