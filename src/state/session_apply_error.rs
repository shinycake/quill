//! Error routing: maps TDLib errors onto per-purpose handlers.
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
            self.messages.send_permission_error = Some(notice.into());
        }
        // Q1: a rate-limited user action says so (tdesktop's
        // `lng_flood_error`); background lookups were already retried by
        // the driver and stay quiet.
        if let Some(notice) = err.flood_notice()
            && pending.is_some_and(|p| is_user_action(p.purpose))
        {
            self.messages.flood_notice = Some(notice);
        }
        // Slice G1: roll back optimistic mutations the server
        // rejected — the pre-request value rides on
        // `PendingRequest::rollback`.
        match pending.and_then(|p| p.rollback.clone()) {
            Some(RequestRollback::ChatPermissions {
                previous,
                previous_can_send,
            }) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id)
                    && let Some(chat) = self.chats.get_mut(&chat_id.0)
                {
                    chat.permissions = previous;
                    chat.can_send_basic_messages = previous_can_send;
                }
            }
            Some(RequestRollback::JoinByRequest {
                supergroup_id,
                previous,
            }) => match previous {
                Some(flag) => {
                    self.groups
                        .supergroup_join_by_request
                        .insert(supergroup_id, flag);
                }
                None => {
                    self.groups
                        .supergroup_join_by_request
                        .remove(&supergroup_id);
                }
            },
            Some(RequestRollback::SupergroupUsername {
                supergroup_id,
                previous,
            }) => match previous {
                Some(username) => {
                    self.groups
                        .supergroup_usernames
                        .insert(supergroup_id, username);
                }
                None => {
                    self.groups.supergroup_usernames.remove(&supergroup_id);
                }
            },
            // Slice G2: restore the pre-toggle sign/show flags.
            Some(RequestRollback::SignMessages {
                supergroup_id,
                previous_sign,
                previous_show,
            }) => {
                match previous_sign {
                    Some(flag) => {
                        self.groups
                            .supergroup_sign_messages
                            .insert(supergroup_id, flag);
                    }
                    None => {
                        self.groups.supergroup_sign_messages.remove(&supergroup_id);
                    }
                }
                match previous_show {
                    Some(flag) => {
                        self.groups
                            .supergroup_show_message_sender
                            .insert(supergroup_id, flag);
                    }
                    None => {
                        self.groups
                            .supergroup_show_message_sender
                            .remove(&supergroup_id);
                    }
                }
            }
            Some(RequestRollback::ChatIsTranslatable { chat_id, previous }) => {
                self.set_chat_translatable(chat_id, previous);
            }
            Some(RequestRollback::AutoTranslate {
                supergroup_id,
                previous,
            }) => self.set_supergroup_auto_translate(supergroup_id, previous),
            // Slice G2: restore the pre-toggle anti-spam flag.
            Some(RequestRollback::AntiSpam {
                supergroup_id,
                previous,
            }) => match previous {
                Some(flag) => {
                    self.groups
                        .supergroup_anti_spam_enabled
                        .insert(supergroup_id, flag);
                }
                None => {
                    self.groups
                        .supergroup_anti_spam_enabled
                        .remove(&supergroup_id);
                }
            },
            // B7: restore the group admin toggles the server refused.
            Some(RequestRollback::GroupToggle {
                supergroup_id,
                toggle,
                previous,
            }) => self.restore_group_toggle(supergroup_id, toggle, previous),
            Some(RequestRollback::ProtectedContent { chat_id, previous }) => {
                self.set_chat_protected(chat_id, previous);
            }
            Some(RequestRollback::AvailableReactions { chat_id, previous }) => match previous {
                Some(setting) => {
                    self.chats_state
                        .chat_available_reactions
                        .insert(chat_id, setting);
                }
                None => {
                    self.chats_state.chat_available_reactions.remove(&chat_id);
                }
            },
            // Slice CL1: restore the pre-toggle pinned /
            // marked-as-unread flags the server refused.
            Some(RequestRollback::ChatPin { previous, archived }) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id)
                    && let Some(chat) = self.chats.get_mut(&chat_id.0)
                {
                    if archived {
                        chat.archive_is_pinned = previous;
                    } else {
                        chat.is_pinned = previous;
                    }
                }
            }
            Some(RequestRollback::ChatMarkedAsUnread { previous }) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id)
                    && let Some(chat) = self.chats.get_mut(&chat_id.0)
                {
                    chat.is_marked_as_unread = previous;
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
            // MED3 review: only user-initiated downloads enter the
            // Failed section; automatic downloads never started by
            // the user must not show rows here.
            if self.media.user_downloads.contains(&file_id) {
                self.media.failed_downloads.insert(file_id);
            } else {
                // A refused automatic download is not retried per ingest.
                self.media.stalled_auto_downloads.insert(file_id);
            }
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

/// Q1: whether a request is something the user did on purpose (send,
/// edit, join, ...), as opposed to a background lookup, a view/online
/// ping or a login submit (which has its own error line).
fn is_user_action(purpose: RequestPurpose) -> bool {
    if is_auth_submit(purpose) || purpose.is_sweepable() {
        return false;
    }
    let debug = format!("{purpose:?}");
    ![
        "Get",
        "Load",
        "Search",
        "Download",
        "View",
        "Open",
        "Close",
        "SetOnline",
    ]
    .iter()
    .any(|prefix| debug.starts_with(prefix))
}
