//! Failed requests for the chat list: loading, folders, archive and pins.
use crate::folder_limits::{FolderOp, limit_kind_for_error};
use crate::state::*;

impl Session {
    /// Reacts to a failed chat list request; called by
    /// [`Session::apply_error`] after the shared handling.
    pub(crate) fn apply_chat_list_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        // A folder request that hit a limit opens the limit box (tdesktop
        // `ShowImportError` / the `*LimitBox`es) instead of an error line.
        let folder_op = match pending.map(|p| p.purpose) {
            Some(RequestPurpose::CreateChatFolder) => Some(FolderOp::Create),
            Some(RequestPurpose::EditChatFolder) => Some(FolderOp::Edit),
            Some(RequestPurpose::CreateChatFolderInviteLink) => Some(FolderOp::CreateLink),
            Some(RequestPurpose::AddChatFolderByInviteLink) => Some(FolderOp::AddByLink),
            _ => None,
        };
        let limit_kind = folder_op
            .zip(err.limit_hint)
            .map(|(op, hint)| limit_kind_for_error(op, hint));
        if let Some(kind) = limit_kind {
            self.chat_list.folder_limit_hit = Some(kind);
        }
        // Share Folder / recommended folders / "Add folder" by link: the
        // dialog shows the reason instead of spinning.
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::CreateChatFolderInviteLink) if limit_kind.is_some() => {}
            Some(RequestPurpose::AddChatFolderByInviteLink) if limit_kind.is_some() => {}
            Some(
                RequestPurpose::GetChatFolderInviteLinks
                | RequestPurpose::GetChatsForFolderInviteLink
                | RequestPurpose::CreateChatFolderInviteLink
                | RequestPurpose::EditChatFolderInviteLink
                | RequestPurpose::DeleteChatFolderInviteLink
                | RequestPurpose::GetRecommendedChatFolders,
            ) => {
                self.chat_list.folder_share_error = Some(error_reason(err));
            }
            Some(
                RequestPurpose::CheckChatFolderInviteLink
                | RequestPurpose::AddChatFolderByInviteLink,
            ) => {
                self.chat_list.folder_invite_error = Some(error_reason(err));
            }
            // Slice CL1: refused chat-list actions surface in the
            // status note (the UI drains `chat_action_error`) —
            // the optimistic state was already rolled back above.
            // Only the numeric code is shown; TDLib's message is
            // never stored.
            Some(RequestPurpose::ToggleChatIsPinned) => {
                self.chats_state.chat_action_error =
                    Some(format!("could not pin the chat (error {})", err.code));
            }
            Some(RequestPurpose::ToggleChatIsMarkedAsUnread) => {
                self.chats_state.chat_action_error =
                    Some(format!("could not change read state (error {})", err.code));
            }
            Some(RequestPurpose::DeleteChatHistory) => {
                self.chats_state.chat_action_error =
                    Some(format!("could not clear history (error {})", err.code));
            }
            Some(RequestPurpose::RemoveChatFromList) => {
                self.chats_state.chat_action_error =
                    Some(format!("could not delete the chat (error {})", err.code));
            }
            // Slice CL2: refused chat-list actions surface in the
            // status note like the CL1 ones — a refusal is never
            // shown as success.
            Some(RequestPurpose::SetPinnedChats) => {
                self.chats_state.chat_action_error = Some(format!(
                    "could not reorder pinned chats (error {})",
                    err.code
                ));
            }
            Some(RequestPurpose::ReadChatList) => {
                self.chats_state.chat_action_error = Some(format!(
                    "could not mark all chats as read (error {})",
                    err.code
                ));
            }
            Some(RequestPurpose::ClearRecentlyFoundChats) => {
                self.chats_state.chat_action_error = Some(format!(
                    "could not clear recent searches (error {})",
                    err.code
                ));
            }
            Some(RequestPurpose::GetArchiveChatListSettings) => {
                self.chat_list.archive_settings_loading = false;
                self.chats_state.chat_action_error = Some(format!(
                    "could not load archive settings (error {})",
                    err.code
                ));
            }
            Some(RequestPurpose::SetArchiveChatListSettings) => {
                self.chats_state.chat_action_error = Some(format!(
                    "could not save archive settings (error {})",
                    err.code
                ));
            }
            _ => {}
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::LoadChats) && err.code == 404 {
            self.chat_list.chats_exhausted = true;
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::LoadArchiveChats) && err.code == 404 {
            self.chat_list.archive_chats_exhausted = true;
        }
        // Parity slice: folder `loadChats` paging ends the same way
        // as the main list — a 404 marks that folder exhausted.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::LoadFolderChats)
            && err.code == 404
            && let Some(folder_id) = pending.and_then(|p| p.folder_id)
        {
            self.chat_list.folder_chats_exhausted.insert(folder_id);
        }
        // Slice CL: failed preview-history fetch — mark the peek
        // preview so it shows an error instead of a spinner.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatPreview)
            && let Some(chat_id) = pending.and_then(|p| p.chat_id)
        {
            self.chat_list.chat_preview_fetch = Some(PreviewHistoryFetch {
                chat_id,
                messages: Vec::new(),
                failed: Some(call_request_error_line(err, "Could not load preview")),
            });
        }
    }
}
