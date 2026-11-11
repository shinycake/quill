//! Error routing: maps TDLib errors onto per-purpose handlers.
use super::request_kind::RequestKind;
use super::*;

impl Session {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_error(
        &mut self,
        err: TdError,
        pending: Option<&PendingRequest>,
        extra: Option<RequestId>,
        seq: u64,
    ) {
        if let Some(pending) = pending {
            self.requests.record_failure(pending.id, error_reason(&err));
        }
        self.settle_own_reaction(extra, true);
        self.fail_own_full_info(&err, pending);
        if let Some(id) = extra {
            self.users_state.saves.failed(id, error_reason(&err));
        }
        if let Some(slot) = extra.and_then(|id| self.groups.new_chats.type_saves.get_mut(&id)) {
            *slot = TypeSave::Failed(super::request_errors::error_reason(&err));
        }
        if matches!(
            pending.map(|p| p.purpose),
            Some(
                RequestPurpose::SendMessage
                    | RequestPurpose::SendMessageAlbum
                    | RequestPurpose::ForwardMessages
                    | RequestPurpose::SendInlineQueryResult
                    | RequestPurpose::ResendMessages
            )
        ) && let Some(notice) = err.send_permission_notice()
        {
            self.messages.send_permission_error.push(notice.into());
        }
        // Q1: a rate-limited user action says so (tdesktop's
        // `lng_flood_error`); background lookups were already retried by
        // the driver and stay quiet.
        if let Some(notice) = err.flood_notice()
            && pending.is_some_and(|p| p.purpose.kind() == RequestKind::UserAction)
        {
            self.messages.flood_notice.push(notice);
        }
        // Slice G1: roll back optimistic mutations the server
        // rejected — the pre-request value rides on
        // `PendingRequest::rollback`.
        match pending.and_then(|p| p.rollback.clone()) {
            Some(RequestRollback::ChatPermissions {
                previous,
                previous_can_send,
                ..
            }) => {
                // A save still out after this one decides what the box
                // shows (its rollback holds the same values, or this
                // chain's last confirmed ones).
                if let Some(chat_id) = pending.and_then(|p| p.chat_id)
                    && self
                        .requests
                        .pending_extra_for(RequestPurpose::SetChatPermissions, Some(chat_id))
                        .is_none()
                    && let Some(chat) = self.chats.get_mut(&chat_id.0)
                {
                    chat.permissions = previous;
                    chat.can_send_basic_messages = previous_can_send;
                }
            }
            // Slice CL2: restore the pre-reorder `order` values
            // the server refused, then rebuild the list order.
            Some(RequestRollback::ChatPinOrder { previous, archived }) => {
                for (chat_id, order) in previous {
                    if let Some(chat) = self.chats.get_mut(&chat_id) {
                        if archived {
                            chat.archive_order = order;
                        } else {
                            chat.order = order;
                        }
                    }
                }
                self.rebuild_main_order();
            }
            // Slice CL2: drop the refused archive-settings flip;
            // the panel re-fetches the truth on next open.
            Some(RequestRollback::ArchiveChatListSettings { previous }) => {
                self.chat_list.archive_chat_list_settings = previous;
            }
            Some(
                rollback @ (RequestRollback::JoinByRequest { .. }
                | RequestRollback::SupergroupUsername { .. }
                | RequestRollback::DefaultReaction { .. }
                | RequestRollback::AvailableReactions { .. }
                | RequestRollback::SignMessages { .. }
                | RequestRollback::ChatIsTranslatable { .. }
                | RequestRollback::AutoTranslate { .. }
                | RequestRollback::AntiSpam { .. }
                | RequestRollback::GroupToggle { .. }
                | RequestRollback::ProtectedContent { .. }
                | RequestRollback::ChatPin { .. }
                | RequestRollback::ChatMarkedAsUnread { .. }
                | RequestRollback::ForumTopicMute { .. }
                | RequestRollback::PollVote { .. }
                | RequestRollback::BusinessBotBar { .. }
                | RequestRollback::ChatLinkDelete { .. }
                | RequestRollback::FolderOrder { .. }
                | RequestRollback::FolderTags { .. }
                | RequestRollback::RecentSearchRemoved { .. }
                | RequestRollback::TopChatRemoved { .. }
                | RequestRollback::TopChatsDisabled { .. }),
            ) => self.restore_optimistic(rollback),
            None => {}
        }
        // Everything else belongs to the request's domain
        // (`src/state/domains/<domain>/error.rs`).
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::Auth(_)) => self.apply_auth_error(&err, pending, extra, seq),
            Some(RequestPurpose::Bots(_)) => self.apply_bots_error(&err, pending, extra, seq),
            Some(RequestPurpose::Calls(_)) => self.apply_calls_error(&err, pending, extra, seq),
            Some(RequestPurpose::ChatList(_)) => {
                self.apply_chat_list_error(&err, pending, extra, seq)
            }
            Some(RequestPurpose::Chats(_)) => self.apply_chats_error(&err, pending, extra, seq),
            Some(RequestPurpose::Communities(_)) => {
                self.apply_communities_error(&err, pending, extra, seq)
            }
            Some(
                RequestPurpose::Groups(_)
                | RequestPurpose::GetChatBoosts { .. }
                | RequestPurpose::GetAdminChatInviteLinks { .. }
                | RequestPurpose::GetLinkJoinRequests { .. },
            ) => self.apply_groups_error(&err, pending, extra, seq),
            Some(RequestPurpose::Media(_) | RequestPurpose::GetSharedMedia { .. }) => {
                self.apply_media_error(&err, pending, extra, seq)
            }
            Some(RequestPurpose::Messages(_) | RequestPurpose::GetRepliedMessage { .. }) => {
                self.apply_messages_error(&err, pending, extra, seq)
            }
            Some(RequestPurpose::Payments(_)) => {
                self.apply_payments_error(&err, pending, extra, seq)
            }
            Some(RequestPurpose::Search(_)) => self.apply_search_error(&err, pending, extra, seq),
            Some(RequestPurpose::Settings(_)) => {
                self.apply_settings_error(&err, pending, extra, seq)
            }
            Some(RequestPurpose::Stickers(_)) => {
                self.apply_stickers_error(&err, pending, extra, seq)
            }
            Some(RequestPurpose::Stories(_)) => self.apply_stories_error(&err, pending, extra, seq),
            Some(
                RequestPurpose::Threads(_)
                | RequestPurpose::GetMessageThread { .. }
                | RequestPurpose::GetMessageThreadHistory { .. }
                | RequestPurpose::GetSavedMessagesTopicHistory { .. }
                | RequestPurpose::GetSavedMessagesTags { .. }
                | RequestPurpose::SearchSavedMessages { .. },
            ) => self.apply_threads_error(&err, pending, extra, seq),
            Some(RequestPurpose::Users(_) | RequestPurpose::GetProfileChats(_)) => {
                self.apply_users_error(&err, pending, extra, seq)
            }
            Some(RequestPurpose::Other) | None => {}
        }
        let download_id = pending
            .filter(|p| p.purpose == RequestPurpose::DownloadFile)
            .and_then(|p| p.file_id)
            .or_else(|| extra.and_then(|id| self.media.download_extras.get(&id.0).copied()));
        if let Some(file_id) = download_id {
            // A user download lands in the Failed section; an automatic
            // one is not retried per ingest.
            self.media.downloads.end(file_id);
            self.unstick_download(file_id);
        }
        if let Some(pending) = pending
            && is_auth_submit(pending.purpose)
        {
            self.auth_state.last_auth_error = Some(AuthRequestError {
                purpose: pending.purpose,
                class: err.class,
                flood_wait_secs: err.flood_wait_secs,
            });
        }
    }
}
