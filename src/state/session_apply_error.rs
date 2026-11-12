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
            // A link problem is worded as the Edit box words it.
            let problem = (pending.map(|p| p.purpose)
                == Some(RequestPurpose::SetSupergroupUsername))
            .then(|| crate::group_type::username_save_problem(err.server_type.as_deref()))
            .flatten();
            *slot = TypeSave::Failed(match problem {
                Some(problem) => crate::lang::tr(problem.text()).to_string(),
                None => super::request_errors::error_reason(&err),
            });
        }
        if let Some(pending) = pending
            && pending.purpose == RequestPurpose::GetPaidReactionSenders
            && let Some(chat) = pending.chat_id
        {
            self.payments.paid.senders_failed(chat.0, Some(pending.id));
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
        if let Some(rollback) = pending.and_then(|p| p.rollback.clone()) {
            self.restore_optimistic(rollback, pending.and_then(|p| p.chat_id));
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
            .and_then(|p| p.file_id())
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
            self.auth_state.submit.settle(extra);
            self.auth_state.last_auth_error = Some(AuthRequestError {
                purpose: pending.purpose,
                class: err.class,
                flood_wait_secs: err.flood_wait_secs,
            });
            if pending.purpose == RequestPurpose::RequestAuthenticationPasswordRecovery
                && extra.is_some_and(|extra| {
                    self.auth_state.password_recovery == PasswordRecovery::Requested(extra)
                })
            {
                self.cancel_password_recovery();
            }
        }
    }
}
