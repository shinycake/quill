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
        // Pins over the limit open the Premium limit box (tdesktop `PinsLimitBox`).
        let pins_box = matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::ToggleChatIsPinned | RequestPurpose::SetPinnedChats)
        ) && self.note_premium_limit(err, pending);
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
            // The box that sent the request gets the limit box in its own
            // window; its own answer (`DialogAnswers`) ends its wait.
            self.chat_list.folder_limit_hit = Some((kind, pending.map(|p| p.id)));
        }
        // Share Folder: a failed fetch ends its stage in the error the box
        // draws with a Retry, and recommended folders say so in the toast.
        // (The box's own requests answer through `DialogAnswers`.)
        match pending.map(|p| (p.purpose, p.folder_id)) {
            Some((RequestPurpose::GetChatFolderInviteLinks, Some(folder_id))) => {
                self.chat_list
                    .folder_shares
                    .entry(folder_id)
                    .or_default()
                    .links
                    .fail(error_reason(err));
            }
            Some((RequestPurpose::GetChatsForFolderInviteLink, Some(folder_id))) => {
                self.chat_list
                    .folder_shares
                    .entry(folder_id)
                    .or_default()
                    .chats
                    .fail(error_reason(err));
            }
            Some((RequestPurpose::DeleteChatFolderInviteLink, Some(folder_id))) => {
                // The link left the list at send time: what the box holds is
                // wrong now, so it shows the error with a Retry that asks
                // the list again.
                self.chat_list
                    .folder_shares
                    .entry(folder_id)
                    .or_default()
                    .links
                    .fail_dropping_data(error_reason(err));
            }
            Some((RequestPurpose::GetRecommendedChatFolders, _)) => {
                self.chats_state.chat_action_error = Some(error_reason(err));
            }
            _ => {}
        }
        // The Add to folder box ends in the error with a Retry.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatListsToAddChat)
            && let Some(chat_id) = pending.and_then(|p| p.chat_id)
        {
            self.chat_list
                .chat_lists_for_add
                .entry(chat_id.0)
                .or_default()
                .fail(error_reason(err));
        }
        match pending.map(|p| p.purpose) {
            // Slice CL1: refused chat-list actions surface in the
            // status note (the UI drains `chat_action_error`) —
            // the optimistic state was already rolled back above.
            // Only the numeric code is shown; TDLib's message is
            // never stored.
            Some(RequestPurpose::ToggleChatIsPinned) if !pins_box => {
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
            // tdesktop deletes the history whether or not leaving worked.
            Some(RequestPurpose::LeaveBeforeDelete) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.chat_list.delete_after_leave.push(chat_id);
                }
            }
            Some(RequestPurpose::RemoveChatFromList) => {
                self.chats_state.chat_action_error =
                    Some(format!("could not delete the chat (error {})", err.code));
            }
            // The removals waiting for this spec are dropped: asking again
            // on every ingest would loop at round-trip speed.
            Some(RequestPurpose::GetChatFolder) => {
                if let Some(folder_id) = pending.and_then(|p| p.folder_id) {
                    self.chat_list.folder_spec_errors.insert(
                        folder_id,
                        call_request_error_line(
                            err,
                            crate::lang::tr(crate::lang::keys::Q_NOTES_FAILED_FOLDER),
                        ),
                    );
                    let queue = &mut self.chat_list.folder_remove_queue;
                    let before = queue.len();
                    queue.retain(|(_, folder)| *folder != folder_id);
                    if queue.len() != before {
                        self.chats_state.chat_action_error = Some(format!(
                            "{} (error {})",
                            crate::lang::tr(
                                crate::lang::keys::Q_CHATLIST_COULD_NOT_REMOVE_FROM_FOLDER
                            ),
                            err.code
                        ));
                    }
                }
            }
            // The bar stays: the offer is still open and can be tried again.
            Some(RequestPurpose::ProcessChatFolderNewChats) => {
                self.chats_state.chat_action_error = Some(format!(
                    "{} (error {})",
                    crate::lang::tr(crate::lang::keys::Q_NOTES_FAILED_PROCESS_NEW_CHATS),
                    err.code
                ));
            }
            // Asked, so the poll waits out the update period instead of
            // asking again on every tick.
            Some(RequestPurpose::GetChatFolderNewChats) => {
                if let Some(folder_id) = pending.and_then(|p| p.folder_id) {
                    self.chat_list
                        .folder_new_chats_asked
                        .insert(folder_id, std::time::Instant::now());
                }
            }
            Some(RequestPurpose::HidePromotion) => {
                self.chats_state.chat_action_error = Some(format!(
                    "{} (error {})",
                    crate::lang::tr(crate::lang::keys::Q_CHAT_COULD_NOT_HIDE_THE_ANNOUNCEMENT),
                    err.code
                ));
            }
            // Slice CL2: refused chat-list actions surface in the
            // status note like the CL1 ones — a refusal is never
            // shown as success.
            Some(RequestPurpose::SetPinnedChats) if !pins_box => {
                self.chats_state.chat_action_error = Some(format!(
                    "could not reorder pinned chats (error {})",
                    err.code
                ));
            }
            Some(RequestPurpose::ReadChatInbox) => {
                self.chats_state.chat_action_error = Some(format!(
                    "could not mark the chat as read (error {})",
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
                self.chat_list.archive_mute.note_reload_failed();
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
        // A 404 ends a list. Any other failure (a timeout, a 500) waits
        // and asks again, or paging stopped until the next reconnect.
        let now = std::time::Instant::now();
        let list = &mut self.chat_list;
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::LoadChats) if err.code == 404 => {
                list.chats_exhausted = true;
                list.chats_retry = None;
            }
            Some(RequestPurpose::LoadChats) => {
                list.chats_retry = Some(ChatListState::next_page_retry(list.chats_retry, now));
            }
            Some(RequestPurpose::LoadArchiveChats) if err.code == 404 => {
                list.archive_chats_exhausted = true;
                list.archive_retry = None;
            }
            Some(RequestPurpose::LoadArchiveChats) => {
                list.archive_retry = Some(ChatListState::next_page_retry(list.archive_retry, now));
            }
            _ => {}
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
            self.chat_list
                .chat_preview_fetch
                .entry(chat_id)
                .or_default()
                .fail_dropping_data(call_request_error_line(err, "Could not load preview"));
        }
    }
}
