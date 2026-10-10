//! Error routing: maps TDLib errors onto per-purpose handlers.
use super::*;
use crate::folder_limits::{FolderOp, limit_kind_for_error};

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
            self.send_permission_error = Some(notice.into());
        }
        // Q1: a rate-limited user action says so (tdesktop's
        // `lng_flood_error`); background lookups were already retried by
        // the driver and stay quiet.
        if let Some(notice) = err.flood_notice()
            && pending.is_some_and(|p| is_user_action(p.purpose))
        {
            self.flood_notice = Some(notice);
        }
        if let Some(p) = pending
            && p.purpose == RequestPurpose::GetChatMember
            && let Some(chat_id) = p.chat_id
        {
            self.adopt_supergroup_status_for_chat(chat_id);
        }
        if let Some(RequestPurpose::GetRepliedMessage {
            chat_id,
            message_id,
        }) = pending.map(|p| p.purpose)
        {
            self.reject_replied_message(chat_id, message_id);
        }
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
            self.folder_limit_hit = Some(kind);
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
                self.folder_share_error = Some(error_reason(&err));
            }
            Some(
                RequestPurpose::GetInstalledBackgrounds
                | RequestPurpose::SetDefaultBackground
                | RequestPurpose::DeleteDefaultBackground
                | RequestPurpose::RemoveInstalledBackground
                | RequestPurpose::SetDefaultBackgroundLocal
                | RequestPurpose::SearchBackground
                | RequestPurpose::SetChatBackground
                | RequestPurpose::DeleteChatBackground
                | RequestPurpose::SetChatTheme,
            ) => {
                self.background_error = Some(error_reason(&err));
            }
            Some(
                RequestPurpose::CheckChatFolderInviteLink
                | RequestPurpose::AddChatFolderByInviteLink,
            ) => {
                self.folder_invite_error = Some(error_reason(&err));
            }
            _ => {}
        }
        // Phase 9.3: a `postStory` / `canPostStory` error — the
        // composer shows it instead of spinning forever.
        match pending.map(|p| p.purpose) {
            // Slice S4: Data & Storage request failures surface
            // on the screen (the S3 pattern) — never as toasts.
            Some(RequestPurpose::SetAutoDownloadSettings { .. }) => {
                self.data_storage_error = Some(format!(
                    "Couldn't save auto-download settings: {}",
                    error_reason(&err)
                ));
            }
            Some(RequestPurpose::OptimizeStorage) => {
                self.storage_clearing = false;
                self.data_storage_error =
                    Some(format!("Couldn't clear the cache: {}", error_reason(&err)));
            }
            Some(RequestPurpose::SetStorageOption) => {
                self.data_storage_error = Some(format!(
                    "Couldn't save the storage limits: {}",
                    error_reason(&err)
                ));
            }
            // Batch 4: `setOption("online")` is fire-and-forget; the next
            // presence check sends it again.
            Some(RequestPurpose::SetOnline) => {}
            // A place without a tile keeps the coordinate card.
            Some(RequestPurpose::GetMapThumbnailFile) => {
                if let Some(pending) = pending {
                    self.map_thumbs.failed(pending.id);
                }
            }
            Some(RequestPurpose::ReviewUnconfirmedSession { confirmed }) => {
                self.finish_login_review(
                    confirmed,
                    Some(sessions_error_line("review the new login", &err)),
                );
            }
            Some(RequestPurpose::AcceptTermsOfService) => {
                self.notices.terms_in_flight = false;
                self.notices.terms_error =
                    Some(sessions_error_line("accept the terms of service", &err));
            }
            Some(RequestPurpose::GetAutoDownloadSettingsPresets) => {
                self.auto_download_presets_loading = false;
                self.data_storage_error = Some(format!(
                    "Couldn't load auto-download settings: {}",
                    error_reason(&err)
                ));
            }
            Some(RequestPurpose::PostStory) => {
                self.story_post.outcome =
                    StoryPostOutcome::Failed(format!("Posting failed: {}", error_reason(&err)));
            }
            Some(RequestPurpose::CheckCanPostStory) => {
                // S14: a chat-level story restriction surfaces the
                // TGX-verbatim notice; anything else keeps the
                // generic eligibility failure.
                self.story_post.check_error = Some(
                    crate::story_restriction::notice_for_error_class(err.class)
                        .map(str::to_string)
                        .unwrap_or_else(|| {
                            format!("Eligibility check failed: {}", error_reason(&err))
                        }),
                );
            }
            // Phase 9.5: a `getStoryInteractions` / `reportStory` /
            // `activateStoryStealthMode` error — the viewer panel /
            // report flow / stealth button shows it instead of
            // spinning forever.
            Some(RequestPurpose::GetStoryInteractions) => {
                if let Some(pending) = pending {
                    self.fail_story_viewers(
                        pending,
                        format!("Could not load viewers: {}", error_reason(&err)),
                    );
                }
            }
            Some(
                purpose @ (RequestPurpose::GetStoryStatistics
                | RequestPurpose::GetStoryPublicForwards),
            ) => {
                if let Some(pending) = pending {
                    self.fail_story_insights(
                        pending,
                        purpose,
                        format!("Could not load statistics: {}", error_reason(&err)),
                    );
                }
            }
            Some(RequestPurpose::SearchPublicStories) => {
                self.fail_story_search(format!("Could not search stories: {}", error_reason(&err)));
            }
            // The message menu's Report flow and audience lists.
            Some(RequestPurpose::ReportMessages) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.fail_message_report(
                        chat_id,
                        format!("Reporting failed: {}", error_reason(&err)),
                    );
                }
            }
            Some(
                purpose @ (RequestPurpose::GetMessageViewers { .. }
                | RequestPurpose::GetMessageReadDate { .. }
                | RequestPurpose::GetMessageAddedReactions { .. }),
            ) => self.fail_audience(purpose),
            Some(RequestPurpose::ViewStickerSet { set_id }) => self.fail_sticker_set_view(set_id),
            Some(RequestPurpose::SetChatMessageSender) => {
                self.message_action_note = Some(format!(
                    "could not change the sender: {}",
                    error_reason(&err)
                ));
            }
            Some(RequestPurpose::AddProfileAudio) => {
                self.message_action_note = Some(format!(
                    "could not save to your profile: {}",
                    error_reason(&err)
                ));
            }
            Some(RequestPurpose::DeleteChatMessagesBySender) => {
                self.message_action_note = Some(format!(
                    "could not delete the messages: {}",
                    error_reason(&err)
                ));
            }
            Some(RequestPurpose::ReportSupergroupSpam) => {
                self.message_action_note =
                    Some(format!("could not report the spam: {}", error_reason(&err)));
            }
            Some(RequestPurpose::DeleteMessageReactionsFromSender { .. }) => {
                self.message_action_note = Some(format!(
                    "could not delete the reaction: {}",
                    error_reason(&err)
                ));
            }
            Some(RequestPurpose::CanTransferOwnership) => {
                self.ownership.check_in_flight = false;
                self.ownership.check_error = Some(format!(
                    "Could not check whether you can transfer ownership: {}",
                    error_reason(&err)
                ));
            }
            Some(RequestPurpose::TransferChatOwnership { .. }) => {
                self.fail_ownership_transfer(&err);
            }
            Some(RequestPurpose::GetChatOwnerAfterLeaving) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.fail_owner_lookup(chat_id.0, &err);
                }
            }
            Some(RequestPurpose::ReportStory) => {
                if let Some(pending) = pending {
                    self.fail_story_report(
                        pending,
                        format!("Reporting failed: {}", error_reason(&err)),
                    );
                }
            }
            Some(RequestPurpose::ActivateStoryStealthMode) => {
                self.story_stealth_error =
                    Some(format!("Stealth mode failed: {}", error_reason(&err)));
            }
            // Phase 9.5: a posted-story management call failed —
            // clear the spinner and surface the sanitized error.
            Some(
                RequestPurpose::EditStory
                | RequestPurpose::EditStoryCover
                | RequestPurpose::SetStoryPrivacySettings,
            ) => {
                self.story_manage.pending = false;
                self.story_manage.error =
                    Some(format!("Story update failed: {}", error_reason(&err)));
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
                self.profile_edit_error =
                    Some(format!("Profile update failed: {}", error_reason(&err)));
            }
            // Phase 9.5 (review fix-up): `getChatsToPostStories`
            // failed — surface a transient error so the "Post as"
            // picker doesn't silently show only "Myself".
            Some(RequestPurpose::GetChatsToPostStories) => {
                self.story_post.check_error = Some(format!(
                    "Could not load \"Post as\" chats: {}",
                    error_reason(&err)
                ));
            }
            // Slice P1: a payment request failed — surface the
            // reason in the checkout dialog instead of spinning
            // forever. The receipt fetch has its own error field:
            // the checkout dialog may be closed, so `payment_note`
            // (rendered only there) would stay invisible.
            Some(
                RequestPurpose::GetPaymentForm
                | RequestPurpose::ValidateOrderInfo
                | RequestPurpose::SendPaymentForm,
            ) => {
                self.payment_form_loading = false;
                self.payment_sending = false;
                self.payment_note = Some(format!("Payment failed: {}", error_reason(&err)));
            }
            Some(RequestPurpose::GetPaymentReceipt) => {
                self.payment_receipt_error =
                    Some(format!("Receipt failed: {}", error_reason(&err)));
            }
            Some(RequestPurpose::GetMarketplaceGift | RequestPurpose::SendMarketplaceGift) => {
                if let Some(gift) = self.marketplace_gift.as_mut() {
                    gift.loading = false;
                    gift.sending = false;
                    gift.note = Some(format!("Gift request failed: {}", error_reason(&err)));
                }
            }
            Some(RequestPurpose::GetGiftTextLimit) => {
                self.gift_text_length_max = None;
            }
            // Slice `parity:bots-payment-recurring`: a subscriptions
            // request failed — surface the reason in the dialog
            // instead of spinning forever; a failed mutation also
            // releases the disabled buttons.
            Some(RequestPurpose::GetStarTransactions { .. }) => {
                self.hub.tx_loading = false;
                self.hub.tx_error = Some(format!(
                    "Couldn't load transactions: {}",
                    error_reason(&err)
                ));
            }
            Some(RequestPurpose::GetReceivedGifts { .. }) => {
                self.hub.gifts_loading = false;
                self.hub.gifts_error = Some(format!("Couldn't load gifts: {}", error_reason(&err)));
            }
            Some(RequestPurpose::ToggleGiftSaved { .. } | RequestPurpose::SellGift) => {
                self.hub.gift_mutating = false;
                self.hub.gift_convert_confirm = None;
                self.hub.gifts_error =
                    Some(format!("Couldn't update the gift: {}", error_reason(&err)));
            }
            Some(RequestPurpose::GetPremiumFeatures) => {
                self.hub.premium_loading = false;
                self.hub.premium_error = Some(format!(
                    "Couldn't load Premium features: {}",
                    error_reason(&err)
                ));
            }
            Some(RequestPurpose::GetStarSubscriptions { .. }) => {
                self.star_subscriptions_loading = false;
                self.star_subscriptions_error = Some(format!(
                    "Couldn't load subscriptions: {}",
                    error_reason(&err)
                ));
            }
            Some(RequestPurpose::EditStarSubscription | RequestPurpose::ReuseStarSubscription) => {
                self.star_subscriptions_mutating = false;
                self.star_subscriptions_error = Some(format!(
                    "Couldn't update the subscription: {}",
                    error_reason(&err)
                ));
            }
            // kit Phase 9: a failed first `getChatHistory` must not
            // leave the message list without a history entry — the
            // skeleton shimmer would run forever. Create the entry
            // so the UI settles into the empty state.
            Some(RequestPurpose::GetHistory | RequestPurpose::GetHistoryAround) => {
                if let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                    && !self.take_stale_history_request(pending)
                {
                    // R5: the empty window shows "Couldn't load messages ·
                    // Retry" instead of a skeleton that never settles.
                    self.histories.entry(chat_id.0).or_default().load_failed = true;
                }
            }
            // `parity:platform-chat-export` — a failed export page must not
            // strand the export with `in_flight` set: mark it failed (and
            // clear `in_flight`) so the UI surfaces the error and the state
            // can be cleared and retried.
            Some(RequestPurpose::ExportChatHistory) => {
                if let Some(export) = self.chat_export.as_mut()
                    && pending.and_then(|p| p.chat_id) == Some(export.chat_id)
                {
                    export.failed = Some(error_reason(&err).to_string());
                    export.in_flight = false;
                }
            }
            // Phase 9.7: a story-page mutation error — the page's
            // status line shows it instead of spinning forever.
            Some(
                purpose @ (RequestPurpose::GetChatStoryAlbums
                | RequestPurpose::GetStoryAlbumStories
                | RequestPurpose::CreateStoryAlbum
                | RequestPurpose::ReorderStoryAlbums
                | RequestPurpose::DeleteStoryAlbum
                | RequestPurpose::SetStoryAlbumName
                | RequestPurpose::AddStoryAlbumStories
                | RequestPurpose::RemoveStoryAlbumStories
                | RequestPurpose::ReorderStoryAlbumStories
                | RequestPurpose::GetChatArchivedStories
                | RequestPurpose::GetChatPostedToChatPageStories
                | RequestPurpose::SetChatPinnedStories
                | RequestPurpose::GetCloseFriends
                | RequestPurpose::SetCloseFriends
                | RequestPurpose::SetChatActiveStoriesList
                | RequestPurpose::ToggleStoryIsPostedToChatPage),
            ) => {
                if purpose == RequestPurpose::SetCloseFriends {
                    self.close_friends_pending = None;
                }
                self.fail_story_page_op(purpose, error_reason(&err).to_string());
            }
            // TDLib 1.8.68 community management: surface refusals
            // ("Have not enough rights", a missing community) in the
            // status note.
            Some(
                purpose @ (RequestPurpose::SetCommunityName
                | RequestPurpose::SetCommunityPhoto
                | RequestPurpose::SetCommunityPermissions
                | RequestPurpose::DeleteCommunity),
            ) => {
                let action = match purpose {
                    RequestPurpose::SetCommunityName => "Could not rename the community",
                    RequestPurpose::SetCommunityPhoto => "Could not change the community photo",
                    RequestPurpose::SetCommunityPermissions => {
                        "Could not change the community permissions"
                    }
                    _ => "Could not delete the community",
                };
                self.community_error = Some(format!("{action}: {}", error_reason(&err)));
            }
            _ => {}
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
                    self.supergroup_join_by_request.insert(supergroup_id, flag);
                }
                None => {
                    self.supergroup_join_by_request.remove(&supergroup_id);
                }
            },
            Some(RequestRollback::SupergroupUsername {
                supergroup_id,
                previous,
            }) => match previous {
                Some(username) => {
                    self.supergroup_usernames.insert(supergroup_id, username);
                }
                None => {
                    self.supergroup_usernames.remove(&supergroup_id);
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
                        self.supergroup_sign_messages.insert(supergroup_id, flag);
                    }
                    None => {
                        self.supergroup_sign_messages.remove(&supergroup_id);
                    }
                }
                match previous_show {
                    Some(flag) => {
                        self.supergroup_show_message_sender
                            .insert(supergroup_id, flag);
                    }
                    None => {
                        self.supergroup_show_message_sender.remove(&supergroup_id);
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
                    self.supergroup_anti_spam_enabled
                        .insert(supergroup_id, flag);
                }
                None => {
                    self.supergroup_anti_spam_enabled.remove(&supergroup_id);
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
                    self.chat_available_reactions.insert(chat_id, setting);
                }
                None => {
                    self.chat_available_reactions.remove(&chat_id);
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
                self.archive_chat_list_settings = previous;
            }
            None => {}
        }
        // Phase C1: a failed call request surfaces on the call
        // overlay (shown and cleared by the UI). A failed
        // `createCall` also drops the half-tracked outgoing call.
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::CreateCall { .. }) => {
                // Note: the tracked outgoing call is *not*
                // cleared here — a failed `createCall` never
                // produced a `callId`, so any tracked call came
                // from elsewhere and must survive. The driver
                // also refuses a second `createCall` while one
                // is active.
                self.call_error = Some(call_request_error_line(&err, "Could not start the call"));
            }
            Some(RequestPurpose::AcceptCall) => {
                self.call_error = Some(call_request_error_line(&err, "Could not answer the call"));
            }
            Some(RequestPurpose::DiscardCall) => {
                self.call_error = Some(call_request_error_line(&err, "Could not hang up the call"));
            }
            Some(RequestPurpose::SendCallRating) => {
                self.call_error = Some(call_request_error_line(&err, "Could not send the rating"));
            }
            Some(RequestPurpose::SendCallDebugInformation) => {
                if let Some(summary) = self.call_summary.as_mut() {
                    summary.debug_information_sent = false;
                    summary.debug_information_error = Some(call_request_error_line(
                        &err,
                        "Could not upload diagnostics",
                    ));
                }
            }
            // Phase C2i: call history / privacy / log failures
            // surface on the Recent-calls tab (the UI reads the
            // flags), not the call overlay.
            Some(RequestPurpose::SearchCallMessages) => {
                self.recent_calls_loading = false;
                self.recent_calls_error = true;
            }
            Some(RequestPurpose::GetCallPrivacyRules { .. }) => {
                self.privacy_roundtrip_done();
                self.call_privacy_error = true;
            }
            // Phase C2i: a failed `setUserPrivacySettingRules`
            // clears the optimistic value (the next fetch
            // restores the truth) and flags the error.
            Some(RequestPurpose::SetCallPrivacyRules { setting }) => {
                match setting {
                    CallPrivacySetting::AllowCalls => self.call_privacy_allow_calls = None,
                    CallPrivacySetting::PeerToPeer => self.call_privacy_p2p = None,
                }
                self.privacy_roundtrip_done();
                self.call_privacy_error = true;
            }
            // Slice S3: privacy-screen request failures surface
            // on the Privacy screen (the UI reads the state),
            // never as toasts.
            Some(RequestPurpose::GetPrivacyRules { key }) => {
                self.privacy.insert(key, PrivacyKeyState::Failed);
            }
            Some(RequestPurpose::SetPrivacyRules { key }) => {
                self.privacy.insert(key, PrivacyKeyState::Failed);
            }
            Some(RequestPurpose::GetReadDatePrivacy) => {
                self.read_date_loading = false;
                self.read_date_error = true;
            }
            Some(RequestPurpose::SetReadDatePrivacy) => {
                self.read_date_loading = false;
                self.read_date_error = true;
                // The optimistic value is dropped; the next
                // fetch restores the truth.
                self.read_date_show = None;
            }
            Some(RequestPurpose::GetBlockedSenders { .. }) => {
                self.blocked_loading = false;
                self.blocked_error = true;
            }
            Some(RequestPurpose::SetSenderBlockList { .. }) => {
                self.blocked_error = true;
                // Drop the optimistic list; the next fetch
                // restores the truth.
                self.blocked_senders = None;
            }
            Some(RequestPurpose::SendCallLog) => {
                if let Some(summary) = self.call_summary.as_mut() {
                    summary.log_sent = false;
                    summary.log_error = Some(call_request_error_line(
                        &err,
                        "Could not upload the call log",
                    ));
                }
            }
            // Slice CL1: refused chat-list actions surface in the
            // status note (the UI drains `chat_action_error`) —
            // the optimistic state was already rolled back above.
            // Only the numeric code is shown; TDLib's message is
            // never stored.
            Some(RequestPurpose::ToggleChatIsPinned) => {
                self.chat_action_error =
                    Some(format!("could not pin the chat (error {})", err.code));
            }
            Some(RequestPurpose::ToggleChatIsMarkedAsUnread) => {
                self.chat_action_error =
                    Some(format!("could not change read state (error {})", err.code));
            }
            Some(RequestPurpose::DeleteChatHistory) => {
                self.chat_action_error =
                    Some(format!("could not clear history (error {})", err.code));
            }
            // Slice B2: refused `sendBotStartMessage` / `getBotSimilarBots` —
            // a refusal is never shown as success.
            Some(RequestPurpose::SendBotStartMessage) => {
                self.chat_action_error =
                    Some(format!("could not start the bot (error {})", err.code));
            }
            Some(RequestPurpose::GetBotSimilarBots) => {
                self.chat_action_error =
                    Some(format!("could not load similar bots (error {})", err.code));
            }
            // B10: profile panel fetches keep the reason for a Retry row;
            // refused edits surface as a toast.
            Some(RequestPurpose::GetProfileChats(_) | RequestPurpose::GetUserProfilePhotos) => {
                if let Some(pending) = pending {
                    self.fail_profile_fetch(pending, error_reason(&err));
                }
            }
            Some(
                RequestPurpose::SetBirthdate
                | RequestPurpose::SetPersonalChat
                | RequestPurpose::SetUserNote
                | RequestPurpose::SetUserPersonalPhoto,
            ) => {
                self.chat_action_error =
                    Some(format!("could not save the change (error {})", err.code));
            }
            Some(RequestPurpose::ReportChatPhoto) => {
                self.chat_action_error =
                    Some(format!("could not send the report (error {})", err.code));
            }
            // B7: refused group admin changes were rolled back above; say
            // so instead of showing the old value as if nothing happened.
            Some(
                RequestPurpose::ToggleSupergroupIsForum
                | RequestPurpose::ToggleSupergroupIsAllHistoryAvailable
                | RequestPurpose::ToggleSupergroupJoinToSendMessages
                | RequestPurpose::ToggleSupergroupHasHiddenMembers
                | RequestPurpose::ToggleChatHasProtectedContent
                | RequestPurpose::SetChatAvailableReactions
                | RequestPurpose::SetChatDiscussionGroup
                | RequestPurpose::UpgradeBasicGroup,
            ) => {
                self.chat_action_error = Some(format!(
                    "could not change the group setting (error {})",
                    err.code
                ));
            }
            Some(RequestPurpose::RemoveChatFromList) => {
                self.chat_action_error =
                    Some(format!("could not delete the chat (error {})", err.code));
            }
            // Slice CL2: refused chat-list actions surface in the
            // status note like the CL1 ones — a refusal is never
            // shown as success.
            Some(RequestPurpose::SetPinnedChats) => {
                self.chat_action_error = Some(format!(
                    "could not reorder pinned chats (error {})",
                    err.code
                ));
            }
            Some(
                RequestPurpose::SetEmojiStatus
                | RequestPurpose::ClearRecentEmojiStatuses
                | RequestPurpose::GetRecentEmojiStatuses
                | RequestPurpose::GetThemedEmojiStatuses
                | RequestPurpose::GetDefaultEmojiStatuses
                | RequestPurpose::GetCustomEmojiStickers,
            ) => {
                self.emoji.status_note = Some(call_request_error_line(
                    &err,
                    "Could not update emoji statuses. Retry the action",
                ));
            }
            Some(
                RequestPurpose::SetSupergroupStickerSet
                | RequestPurpose::SetSupergroupCustomEmojiStickerSet,
            ) => {
                self.chat_action_error = Some(call_request_error_line(
                    &err,
                    "Could not change the group's sticker pack",
                ));
            }
            Some(RequestPurpose::ReadChatList) => {
                self.chat_action_error = Some(format!(
                    "could not mark all chats as read (error {})",
                    err.code
                ));
            }
            Some(RequestPurpose::RemoveRecentlyFoundChat) => {
                self.chat_action_error = Some(format!(
                    "could not remove the recent search (error {})",
                    err.code
                ));
            }
            Some(RequestPurpose::RemoveTopChat | RequestPurpose::SetTopChatsDisabled) => {
                self.chat_action_error = Some(format!(
                    "could not update frequent contacts (error {})",
                    err.code
                ));
            }
            Some(RequestPurpose::ClearRecentlyFoundChats) => {
                self.chat_action_error = Some(format!(
                    "could not clear recent searches (error {})",
                    err.code
                ));
            }
            // Slice CL3: refused report / block surfaces in the
            // status note — never shown as success.
            Some(RequestPurpose::ReportChat) => {
                self.chat_action_error =
                    Some(format!("could not report the chat (error {})", err.code));
            }
            Some(RequestPurpose::RemoveChatActionBar) => {
                self.chat_action_error =
                    Some(format!("could not hide the bar (error {})", err.code));
            }
            Some(RequestPurpose::SharePhoneNumber) => {
                self.chat_action_error = Some(format!(
                    "could not share your phone number (error {})",
                    err.code
                ));
            }
            Some(RequestPurpose::ShareWithBot) => {
                self.chat_action_error = Some(format!(
                    "the bot could not receive what you shared (error {})",
                    err.code
                ));
            }
            Some(RequestPurpose::SetMessageSenderBlockList { .. }) => {
                self.chat_action_error = Some(format!(
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
                self.contacts_notice =
                    Some(format!("could not delete {what} (error {})", err.code));
            }
            Some(RequestPurpose::ImportContacts) => {
                self.contacts_notice =
                    Some(format!("could not import contacts (error {})", err.code));
            }
            Some(RequestPurpose::ClearImportedContacts) => {
                self.contacts_notice = Some(format!(
                    "could not delete synced contacts (error {})",
                    err.code
                ));
            }
            // Slice payments: a refused `deleteSavedOrderInfo` /
            // `deleteSavedCredentials` surfaces in the status note (the
            // UI drains `chat_action_error`); the optimistic state was
            // never changed, so nothing to roll back.
            Some(RequestPurpose::DeleteSavedOrderInfo | RequestPurpose::DeleteSavedCredentials) => {
                self.chat_action_error = Some(format!(
                    "could not clear saved payment info (error {})",
                    err.code
                ));
            }
            Some(RequestPurpose::GetArchiveChatListSettings) => {
                self.archive_settings_loading = false;
                self.chat_action_error = Some(format!(
                    "could not load archive settings (error {})",
                    err.code
                ));
            }
            Some(RequestPurpose::SetArchiveChatListSettings) => {
                self.chat_action_error = Some(format!(
                    "could not save archive settings (error {})",
                    err.code
                ));
            }
            Some(RequestPurpose::CreatePrivateChat) => {
                self.chat_action_error = Some(format!(
                    "could not open Saved Messages (error {})",
                    err.code
                ));
            }
            // Phase C3a: group-call request failures surface on
            // the group-call overlay (shown and cleared by the
            // UI). A failed `joinVideoChat` leaves any tracked
            // call in place — `updateGroupCall` is the source
            // of truth for join state.
            Some(RequestPurpose::CreateVideoChat { .. }) => {
                self.group_call_error = Some(call_request_error_line(
                    &err,
                    "Could not start the voice chat",
                ));
            }
            Some(RequestPurpose::JoinVideoChat { .. }) => {
                // Phase C2f: a failed rejoin re-arms
                // `reconnecting` so the driver's auto-rejoin
                // retries (max 3 attempts, the C2d discipline);
                // a plain initial-join failure just reports.
                // `rejoin_attempts > 0` marks the failed join
                // as a rejoin (only `rejoin_group_call`
                // increments the counter).
                let rejoin_attempt = self
                    .active_group_call
                    .as_ref()
                    .map(|call| call.rejoin_attempts)
                    .unwrap_or(0);
                if rejoin_attempt > 0 {
                    if let Some(call) = self.active_group_call.as_mut() {
                        // Keep the banner + manual Rejoin
                        // available even after exhaustion.
                        call.reconnecting = true;
                        if call.rejoin_attempts >= 3 {
                            self.group_call_error =
                                Some("Reconnect attempts exhausted.".to_string());
                        }
                    }
                } else {
                    self.group_call_error = Some(call_request_error_line(
                        &err,
                        "Could not join the voice chat",
                    ));
                }
            }
            // Phase C2g: a failed screen-sharing handshake must
            // not leave the call stuck "sharing" — clear the
            // pending/active flags and surface an honest error.
            Some(RequestPurpose::StartGroupCallScreenSharing { group_call_id }) => {
                if let Some(tracked) = self.active_group_call.as_mut()
                    && tracked.id == group_call_id
                {
                    tracked.screen_share_pending = false;
                    tracked.screen_sharing = false;
                    tracked.screen_share_answer.clear();
                }
                self.group_call_error = Some(call_request_error_line(
                    &err,
                    "Could not start screen sharing",
                ));
            }
            Some(RequestPurpose::EndGroupCallScreenSharing { group_call_id }) => {
                if let Some(tracked) = self.active_group_call.as_mut()
                    && tracked.id == group_call_id
                {
                    tracked.screen_share_pending = false;
                    tracked.screen_sharing = false;
                    tracked.screen_share_answer.clear();
                }
                self.group_call_error = Some(call_request_error_line(
                    &err,
                    "Could not stop screen sharing",
                ));
            }
            Some(
                RequestPurpose::LeaveGroupCall { .. }
                | RequestPurpose::EndGroupCall { .. }
                | RequestPurpose::GetGroupCall { .. }
                | RequestPurpose::LoadGroupCallParticipants { .. }
                | RequestPurpose::GetVideoChatInviteLink { .. }
                | RequestPurpose::SetVideoChatDefaultParticipant { .. }
                | RequestPurpose::SetVideoChatTitle { .. }
                | RequestPurpose::RevokeVideoChatInviteLink { .. }
                | RequestPurpose::StartGroupCallRecording { .. }
                | RequestPurpose::EndGroupCallRecording { .. }
                | RequestPurpose::StartScheduledVideoChat { .. }
                | RequestPurpose::ToggleVideoChatEnabledStartNotification { .. }
                | RequestPurpose::GetVideoChatRtmpUrl { .. }
                | RequestPurpose::ReplaceVideoChatRtmpUrl { .. }
                | RequestPurpose::SendGroupCallMessage { .. }
                | RequestPurpose::ToggleGroupCallAreMessagesAllowed { .. }
                | RequestPurpose::ToggleGroupCallVideo { .. }
                | RequestPurpose::ToggleGroupCallParticipantMute { .. }
                | RequestPurpose::ToggleGroupCallParticipantHand { .. }
                | RequestPurpose::ToggleVideoChatMuteNew { .. }
                | RequestPurpose::InviteGroupCallParticipant { .. }
                | RequestPurpose::BanGroupCallParticipants { .. }
                | RequestPurpose::SetGroupCallParticipantVolumeLevel { .. }
                | RequestPurpose::JoinGroupCallInvitation
                | RequestPurpose::DeclineGroupCallInvitation { .. },
            ) => {
                self.group_call_error =
                    Some(call_request_error_line(&err, "Voice chat request failed"));
            }
            // The "join as" list is optional: a failure just leaves the
            // picker out and the join goes ahead as yourself.
            Some(RequestPurpose::GetVideoChatAvailableParticipants { .. }) => {}
            // Phase D2: a failed `getChatStatistics` lands in the
            // fetch state so the statistics panel shows an honest
            // error instead of spinning forever.
            Some(RequestPurpose::GetChatStatistics) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.chat_statistics.insert(
                        chat_id.0,
                        ChatStatisticsFetch::Failed(call_request_error_line(
                            &err,
                            "Could not load statistics",
                        )),
                    );
                }
            }
            // Phase D3a: failed invite-link / join-request requests
            // land in the fetch state so the panel shows an honest
            // error instead of spinning forever.
            Some(RequestPurpose::GetChatInviteLinks) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.invite_links.insert(
                        chat_id.0,
                        InviteLinkFetch::Failed(call_request_error_line(
                            &err,
                            "Could not load invite links",
                        )),
                    );
                }
            }
            Some(RequestPurpose::CreateChatInviteLink) => {
                // Slice G1 fix-up: a failed mutation must not wipe
                // the previously loaded list — surface the error in
                // the status note and keep the last good data.
                self.invite_link_error = Some(call_request_error_line(
                    &err,
                    "Could not create invite link",
                ));
            }
            Some(RequestPurpose::EditChatInviteLink) => {
                self.invite_link_error =
                    Some(call_request_error_line(&err, "Could not edit invite link"));
            }
            Some(RequestPurpose::RevokeChatInviteLink) => {
                self.invite_link_error = Some(call_request_error_line(
                    &err,
                    "Could not revoke invite link",
                ));
            }
            // Slice G1: failed primary-link replacement — keep the
            // last good list, surface the error in the note.
            Some(RequestPurpose::ReplacePrimaryChatInviteLink) => {
                self.invite_link_error = Some(call_request_error_line(
                    &err,
                    "Could not replace primary invite link",
                ));
            }
            // Slice G1: roll back the optimistic broadcast-group
            // upgrade so the panel doesn't lie.
            Some(RequestPurpose::ToggleBroadcastGroup) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id)
                    && let Some(chat) = self.chats.get(&chat_id.0)
                    && let ChatKind::Supergroup { supergroup_id, .. } = chat.kind
                {
                    self.supergroup_is_broadcast.remove(&supergroup_id);
                }
            }
            Some(RequestPurpose::GetChatJoinRequests) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.join_requests.insert(
                        chat_id.0,
                        JoinRequestFetch::Failed(call_request_error_line(
                            &err,
                            "Could not load join requests",
                        )),
                    );
                }
            }
            // B8: failures of the new link/request admin calls keep the
            // last good data and surface the error line.
            Some(RequestPurpose::GetMoreChatJoinRequests) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.join_request_latest.remove(&chat_id.0);
                }
                self.invite_link_error = Some(call_request_error_line(
                    &err,
                    "Could not load more join requests",
                ));
            }
            Some(RequestPurpose::ProcessAllChatJoinRequests { .. }) => {
                self.invite_link_error = Some(call_request_error_line(
                    &err,
                    "Could not process join requests",
                ));
            }
            Some(RequestPurpose::GetRevokedChatInviteLinks) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.revoked_invite_links.insert(
                        chat_id.0,
                        InviteLinkFetch::Failed(call_request_error_line(
                            &err,
                            "Could not load revoked links",
                        )),
                    );
                }
            }
            Some(RequestPurpose::GetChatInviteLinkCounts) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.invite_link_counts.insert(
                        chat_id.0,
                        InviteLinkCountsFetch::Failed(call_request_error_line(
                            &err,
                            "Could not load link counts",
                        )),
                    );
                }
            }
            Some(RequestPurpose::GetChatInviteLinkMembers { .. }) => {
                if let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                    && let Some(state) = self.invite_link_members.get_mut(&chat_id.0)
                    && state.request == Some(pending.id)
                {
                    state.loading = false;
                    state.request = None;
                    state.error = Some(call_request_error_line(&err, "Could not load members"));
                }
            }
            Some(RequestPurpose::DeleteRevokedChatInviteLink) => {
                if let Some(pending) = pending {
                    self.revoked_link_deletions.remove(&pending.id);
                }
                self.invite_link_error = Some(call_request_error_line(
                    &err,
                    "Could not delete invite link",
                ));
            }
            Some(RequestPurpose::DeleteAllRevokedChatInviteLinks) => {
                self.invite_link_error = Some(call_request_error_line(
                    &err,
                    "Could not delete revoked links",
                ));
            }
            Some(RequestPurpose::ProcessChatJoinRequest { .. }) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.join_requests.insert(
                        chat_id.0,
                        JoinRequestFetch::Failed(call_request_error_line(
                            &err,
                            "Could not process join request",
                        )),
                    );
                }
            }
            // Phase D3b: failed admin-management requests land in
            // the fetch state so the panel shows an honest error
            // instead of spinning forever.
            Some(RequestPurpose::GetChatAdministrators) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.admin_lists.insert(
                        chat_id.0,
                        AdminListFetch::Failed(call_request_error_line(
                            &err,
                            "Could not load administrators",
                        )),
                    );
                }
            }
            Some(RequestPurpose::SetChatMemberStatus { .. }) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    let line = call_request_error_line(&err, "Could not update member status");
                    // Slice G1: the member-management dialog reads
                    // the member-list fetch states, not
                    // `admin_lists`, so the failure is also parked
                    // where the action was taken.
                    self.member_action_error.insert(chat_id.0, line.clone());
                    self.admin_lists
                        .insert(chat_id.0, AdminListFetch::Failed(line));
                }
            }
            // Slice G1: failed `setChatMemberTag` (custom title)
            // surfaces as an admin-list error so the info panel
            // shows it, and in `member_action_error` so the
            // member-management dialog (which launched the
            // action) shows it too.
            Some(RequestPurpose::SetChatMemberTag { .. }) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    let line = call_request_error_line(&err, "Could not set custom title");
                    self.member_action_error.insert(chat_id.0, line.clone());
                    self.admin_lists
                        .insert(chat_id.0, AdminListFetch::Failed(line));
                }
            }
            Some(RequestPurpose::GetBasicGroupFullInfo) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.basic_group_members.insert(
                        chat_id.0,
                        SupergroupMembersFetch::Failed(call_request_error_line(
                            &err,
                            "Could not load members",
                        )),
                    );
                }
            }
            // Slice G1: a basic-group `addChatMember` answers per
            // user with `failedToAddMembers` (schema 1.8.67, line
            // 13578) — but a request-level TDLib error has no
            // such body. Count per-user errors in the same slot
            // the dialog already renders so partial adds stay
            // honest.
            Some(RequestPurpose::AddChatMembers | RequestPurpose::AddChatMember) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    *self.add_members_failed.entry(chat_id.0).or_insert(0) += 1;
                }
            }
            Some(RequestPurpose::GetSupergroupMembers { filter }) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.supergroup_members.insert(
                        (chat_id.0, filter),
                        SupergroupMembersFetch::Failed(call_request_error_line(
                            &err,
                            "Could not load members",
                        )),
                    );
                }
            }
            // B15: a failed `getPollVoteStatistics` lands in the fetch
            // state so the dialog shows an honest error.
            Some(RequestPurpose::GetPollVoteStatistics {
                chat_id,
                message_id,
            }) => {
                self.poll_stats.insert(
                    (chat_id.0, message_id.0),
                    PollStatsFetch::Failed(call_request_error_line(
                        &err,
                        "Could not load poll stats",
                    )),
                );
            }
            // B15: poll option / checklist mutations surface their
            // failure in the status note (tdesktop shows a toast:
            // `lng_polls_add_option_error`).
            Some(RequestPurpose::AddPollOption) => {
                self.message_action_note = Some(if err.code == 400 {
                    "Could not add the option. Please try again.".to_string()
                } else {
                    call_request_error_line(&err, "Could not add the option")
                });
            }
            Some(RequestPurpose::MarkChecklistTasks) => {
                self.message_action_note = Some(call_request_error_line(
                    &err,
                    "Could not update the checklist",
                ));
            }
            Some(RequestPurpose::AddChecklistTasks) => {
                self.message_action_note =
                    Some(call_request_error_line(&err, "Could not add the tasks"));
            }
            // B4: a failed `getPollVoters` first page lands in the
            // fetch state so the dialog shows an honest error; a
            // failed "load more" keeps the loaded page retryable.
            Some(RequestPurpose::GetPollVoters {
                chat_id,
                message_id,
                option_id,
                offset,
            }) => {
                if offset == 0
                    || !matches!(
                        self.poll_voters.get(&(chat_id.0, message_id.0, option_id)),
                        Some(PollVotersFetch::Loaded { .. })
                    )
                {
                    self.poll_voters.insert(
                        (chat_id.0, message_id.0, option_id),
                        PollVotersFetch::Failed(call_request_error_line(
                            &err,
                            "Could not load voters",
                        )),
                    );
                }
            }
            // Bots slice: a failed first page lands in the slot so
            // the picker shows an honest error; a failed "load
            // more" keeps the loaded page retryable.
            Some(RequestPurpose::GetInlineQueryResults {
                chat_id,
                bot_user_id,
                first_page,
            }) => {
                let failed_first_page = first_page
                    && matches!(
                        &self.inline_query,
                        Some(slot)
                            if slot.chat_id == chat_id
                                && slot.bot_user_id == bot_user_id
                    );
                if failed_first_page && let Some(slot) = self.inline_query.as_mut() {
                    slot.fetch = InlineQueryFetch::Failed(call_request_error_line(
                        &err,
                        "Could not load inline results",
                    ));
                }
            }
            // Bots slice: a failed `@botname` lookup lands in the
            // resolve slot so the composer shows an honest hint;
            // stale failures (a newer username is already being
            // resolved) are ignored via the generation guard.
            Some(RequestPurpose::ResolveInlineBot { generation }) => {
                let stale = !matches!(
                    &self.inline_bot_resolve,
                    Some(InlineBotResolve::Resolving {
                        generation: slot_generation,
                        ..
                    }) if *slot_generation == generation
                );
                if !stale {
                    let username = match &self.inline_bot_resolve {
                        Some(InlineBotResolve::Resolving { username, .. }) => username.clone(),
                        _ => String::new(),
                    };
                    self.inline_bot_resolve = Some(InlineBotResolve::Failed {
                        reason: format!("could not find @{username} (error {})", err.code),
                        username,
                    });
                }
            }
            // `parity:platform-deep-links`: a failed deep-link request
            // surfaces TDLib's error as a dialog; stale failures (a newer
            // flow is already in flight) are ignored via the generation
            // guard.
            Some(
                RequestPurpose::DeepLinkInfo { generation }
                | RequestPurpose::DeepLinkInternalType { generation }
                | RequestPurpose::DeepLinkResolve { generation }
                | RequestPurpose::DeepLinkJoin { generation }
                | RequestPurpose::DeepLinkCheckInvite { generation },
            ) => {
                let stale = !matches!(
                    &self.deep_link,
                    Some(
                        DeepLinkState::ResolvingInfo {
                            generation: slot
                        }
                        | DeepLinkState::ResolvingChat {
                            generation: slot,
                            ..
                        }
                    ) if *slot == generation
                );
                if !stale {
                    let text = deep_link_error_text(self.deep_link.as_ref(), err.code);
                    self.deep_link = Some(DeepLinkState::ShowText(text));
                }
            }
            // Phase D3c: a failed first page lands in the fetch
            // state so the panel shows an honest error instead of
            // spinning forever. A failed "load more" keeps the
            // already-loaded page so the button stays retryable.
            Some(RequestPurpose::GetChatEventLog { from_event_id }) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id)
                    && (from_event_id == 0
                        || !matches!(
                            self.event_logs.get(&chat_id.0),
                            Some(ChatEventLogFetch::Loaded(_))
                        ))
                {
                    self.event_logs.insert(
                        chat_id.0,
                        ChatEventLogFetch::Failed(call_request_error_line(
                            &err,
                            "Could not load recent actions",
                        )),
                    );
                }
            }
            Some(RequestPurpose::GetAdminRights { user_id }) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.admin_rights.insert(
                        (chat_id.0, user_id),
                        AdminRightsFetch::Failed(call_request_error_line(
                            &err,
                            "Could not load admin rights",
                        )),
                    );
                }
            }
            // Phase S2: a failed `getStorageStatistics` clears the
            // in-flight flag so the overlay shows "No storage data
            // yet." instead of spinning forever.
            Some(RequestPurpose::GetStorageStatistics) => {
                self.storage_stats_loading = false;
            }
            // Slice A2: a failed 2FA management request clears the
            // in-flight flag and parks the honest, classified
            // error line on the overlay — never a fake success,
            // never an optimistic state change.
            Some(RequestPurpose::PasswordStateOp { op }) => {
                self.password_state_loading = false;
                self.password_op_error = Some(password_op_error_line(op, &err));
            }
            // Slice A3: a failed sessions fetch or terminate
            // clears the in-flight flags and parks the honest,
            // classified error line on the overlay — never a fake
            // success, never an optimistic list change.
            // A failed stale-refetch also clears `sessions_stale`
            // so the next ingest does not retry the fetch and
            // flood state worsens; retry is user-driven via the
            // Refresh button. The old cache stays visible.
            Some(RequestPurpose::ConfirmDeviceLogin) => {
                self.device_login_result = Some(crate::auth::DeviceLoginResult::Failed);
                self.sessions_mutating = false;
                self.sessions_error = Some(sessions_error_line("link the device", &err));
            }
            Some(
                purpose @ (RequestPurpose::GetProxies
                | RequestPurpose::MutateProxy
                | RequestPurpose::PingProxy { .. }
                | RequestPurpose::SetPreferIpv6 { .. }),
            ) => self.apply_proxy_error(purpose, &err),
            Some(
                purpose @ (RequestPurpose::GetNewChatPrivacy
                | RequestPurpose::SetNewChatPrivacy { .. }
                | RequestPurpose::SetGiftSettings
                | RequestPurpose::SetInactiveSessionTtl
                | RequestPurpose::SetSensitiveContent
                | RequestPurpose::SetContactJoinedNotifications
                | RequestPurpose::GetNetworkStatistics
                | RequestPurpose::ResetNetworkStatistics
                | RequestPurpose::CheckRememberedPassword),
            ) => self.apply_privacy_data_error(purpose, &err),
            Some(RequestPurpose::GetActiveSessions) => {
                self.sessions_loading = false;
                self.sessions_stale = false;
                self.sessions_error = Some(sessions_error_line("load the sessions list", &err));
            }
            Some(
                RequestPurpose::TerminateSession { .. } | RequestPurpose::TerminateAllOtherSessions,
            ) => {
                self.sessions_mutating = false;
                self.sessions_error = Some(sessions_error_line("terminate the session", &err));
            }
            // Slice A4: a refused session toggle surfaces an honest
            // classified error and leaves the list untouched (the
            // toggled value is never applied optimistically).
            Some(
                RequestPurpose::ToggleSessionSecretChats { .. }
                | RequestPurpose::ToggleSessionCalls { .. },
            ) => {
                self.sessions_mutating = false;
                self.sessions_error = Some(sessions_error_line("change the session setting", &err));
            }
            // Slice A7: a refused account-lifecycle op (TTL fetch
            // or set, account deletion) clears the in-flight
            // flags and parks the honest, classified error line —
            // never a fake success, never an optimistic change.
            // `sessions_error_line` is reused: it is a pure
            // (action, error-class) formatter, not session-bound.
            Some(RequestPurpose::GetDefaultAutoDelete) => {
                self.default_auto_delete_busy = false;
                self.default_auto_delete_error = Some(sessions_error_line(
                    "load the default auto-delete timer",
                    &err,
                ));
            }
            Some(RequestPurpose::SetDefaultAutoDelete { .. }) => {
                self.default_auto_delete_busy = false;
                self.default_auto_delete_error = Some(sessions_error_line(
                    "change the default auto-delete timer",
                    &err,
                ));
            }
            Some(RequestPurpose::GetAccountTtl) => {
                self.account_ttl_loading = false;
                self.account_error = Some(sessions_error_line(
                    "load the account inactivity timer",
                    &err,
                ));
            }
            Some(RequestPurpose::SetAccountTtl { .. } | RequestPurpose::DeleteAccount) => {
                self.account_mutating = false;
                self.account_error = Some(sessions_error_line("update the account", &err));
            }
            // Slice A8: a refused change-number op (code send /
            // resend, code check) clears the in-flight flags and
            // parks the honest, classified error line — never a
            // fake success. The pending number and timeout stay
            // put: a failed send never reached the server
            // (transport error), and a refused resend does not
            // abort the existing verification, so the pending
            // code is still valid — the user can retry or resend.
            // A failed check likewise keeps the pending number.
            Some(RequestPurpose::SendPhoneNumberCode | RequestPurpose::ResendPhoneNumberCode) => {
                self.change_number_loading = false;
                self.change_number_error =
                    Some(sessions_error_line("send the verification code", &err));
            }
            Some(RequestPurpose::CheckPhoneNumberCode) => {
                self.change_number_checking = false;
                self.change_number_error =
                    Some(sessions_error_line("check the verification code", &err));
            }
            // Slice A4: a failed websites fetch or disconnect
            // clears the in-flight flags and parks the honest,
            // classified error line on the overlay.
            // A failed stale-refetch also clears `websites_stale`
            // (mirroring A3's sessions arm): otherwise the next
            // ingest retries the fetch and flood state worsens;
            // retry is user-driven via the Refresh button. The old
            // cache stays visible.
            Some(RequestPurpose::GetConnectedWebsites) => {
                self.connected_websites_loading = false;
                self.websites_stale = false;
                self.websites_error = Some(sessions_error_line("load the websites list", &err));
            }
            Some(
                RequestPurpose::DisconnectWebsite { .. } | RequestPurpose::DisconnectAllWebsites,
            ) => {
                self.websites_mutating = false;
                self.websites_error = Some(sessions_error_line("disconnect the website", &err));
            }
            // A failed translation shows "Translate failed." where the text
            // would have gone, in the box and in the translated bubble alike.
            Some(RequestPurpose::TranslateJob { job }) => {
                self.finish_translation(job, Translation::Failed(error_reason(&err)));
            }
            // M1 fix-up: a failed `resendMessages` surfaces in the
            // status note instead of vanishing into `_ => {}` —
            // the menu item says "retrying send…" and the user
            // deserves an answer either way.
            Some(RequestPurpose::EditMessageSchedulingState { scheduling, .. }) => {
                let action = if scheduling == ComposerScheduling::None {
                    "Could not send the message now"
                } else {
                    "Could not reschedule the message"
                };
                self.resend_error = Some(call_request_error_line(&err, action));
            }
            Some(RequestPurpose::ResendMessages) => {
                self.resend_error = Some(call_request_error_line(&err, "Could not retry the send"));
            }
            // Saved Messages: a 404 from `loadSavedMessagesTopics` says all
            // sublists were loaded; it is not a failure.
            Some(RequestPurpose::LoadSavedMessagesTopics) => {
                if err.code == 404 {
                    self.saved.topics_exhausted = true;
                } else {
                    self.chat_action_error =
                        Some(call_request_error_line(&err, "Could not load saved chats"));
                }
            }
            Some(RequestPurpose::GetForumTopicLink) => {
                self.message_link_error = Some(call_request_error_line(
                    &err,
                    "Could not get the topic link",
                ));
            }
            Some(
                RequestPurpose::ToggleChatViewAsTopics
                | RequestPurpose::SetPinnedForumTopics
                | RequestPurpose::ReadAllForumTopicMentions { .. }
                | RequestPurpose::ReadAllForumTopicReactions { .. }
                | RequestPurpose::UnpinAllForumTopicMessages { .. }
                | RequestPurpose::DeleteSavedMessagesTopicHistory { .. }
                | RequestPurpose::ToggleSavedMessagesTopicPinned { .. }
                | RequestPurpose::SetSavedMessagesTagLabel
                | RequestPurpose::GetSavedMessagesTags { .. }
                | RequestPurpose::GetSavedMessagesTopicHistory { .. }
                | RequestPurpose::SearchSavedMessages { .. },
            ) => {
                self.chat_action_error = Some(call_request_error_line(
                    &err,
                    "Could not complete that action",
                ));
            }
            // M1 fix-up: a failed "Share link" surfaces in the
            // status note instead of silently doing nothing.
            Some(
                RequestPurpose::GetMessageLink | RequestPurpose::GetMessageLinkProperties { .. },
            ) => {
                self.message_link_error =
                    Some(call_request_error_line(&err, "Could not get message link"));
            }
            // MED2 fix-up: a refused `recognizeSpeech` surfaces in
            // the status note instead of vanishing into `_ => {}` —
            // the row says "transcription requested" and the user
            // deserves an answer either way.
            Some(RequestPurpose::RecognizeSpeech) => {
                self.recognize_speech_error = Some(call_request_error_line(
                    &err,
                    "Could not transcribe this message",
                ));
            }
            // Slice msg-richtext-ai-tools: a failed AI request surfaces
            // in the status note instead of vanishing into `_ => {}` —
            // the button said "AI working…" and the user deserves an
            // answer either way. `AICOMPOSE_FLOOD_PREMIUM` (classified in
            // `parse_error`) gets the documented plain-language line.
            Some(
                RequestPurpose::FixTextWithAi
                | RequestPurpose::ComposeTextWithAi
                | RequestPurpose::ComposeRichMessageWithAi
                | RequestPurpose::CreateRichMessageWithAi
                | RequestPurpose::FixRichMessageWithAi,
            ) => {
                self.ai_error = Some(match err.class {
                    ErrorClass::AiComposeFloodPremium => {
                        "AI limit reached — Telegram Premium is required for more requests"
                            .to_string()
                    }
                    _ => format!("AI tools failed: {}", error_reason(&err)),
                });
            }
            _ => {}
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::LoadChats) && err.code == 404 {
            self.chats_exhausted = true;
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::LoadArchiveChats) && err.code == 404 {
            self.archive_chats_exhausted = true;
        }
        // Parity slice: folder `loadChats` paging ends the same way
        // as the main list — a 404 marks that folder exhausted.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::LoadFolderChats)
            && err.code == 404
            && let Some(folder_id) = pending.and_then(|p| p.folder_id)
        {
            self.folder_chats_exhausted.insert(folder_id);
        }
        // Phase 6: a failed `getContacts` surfaces a retry in the
        // contacts tab instead of a stuck spinner.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetContacts) {
            self.contacts_error = true;
        }
        // Parity slice: a failed `getScopeNotificationSettings` must
        // not leave the scope in `scope_settings_loading` — otherwise
        // `maybe_fetch_scope_notification_settings` skips it on every
        // later ingest and every "Defaults for all chats…" open.
        // Dropping it here means the next fetch retries.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetScopeNotificationSettings)
            && let Some(scope) = pending.and_then(|p| p.scope)
        {
            self.scope_settings_loading.remove(&scope);
        }
        // Parity slice: a failed
        // `getChatNotificationSettingsExceptions` must not leave the
        // scope in `notification_exceptions_loading` — otherwise every
        // later dialog open skips the fetch and the exceptions stay
        // unfetchable. Dropping it here means the next open retries.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatNotificationSettingsExceptions)
            && let Some(scope) = pending.and_then(|p| p.scope)
        {
            self.notification_exceptions_loading.remove(&scope);
        }
        // Phase 3.3: `getCommands` failed — on a user session the
        // method is annotated "for bots only" (schema 1.8.67 line
        // 14953), so the error is permanent. Record an empty set
        // so the fetch is never retried; the `/` menu falls back
        // to the bot's `botInfo` commands.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetCommands)
            && let Some(chat_id) = pending.and_then(|p| p.chat_id)
            && let Some(user_id) = self.bot_user_id_for_chat(chat_id)
        {
            self.bot_commands.entry(user_id).or_default();
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::ViewMessages)
            && let Some(chat_id) = pending.and_then(|p| p.chat_id)
        {
            self.abort_viewing(chat_id);
        }
        if self.search.matches_generation(pending) {
            match pending.map(|p| p.purpose) {
                Some(RequestPurpose::SearchChats | RequestPurpose::SearchRecentlyFoundChats) => {
                    self.search.accept_chats(Vec::new(), true);
                }
                Some(
                    RequestPurpose::SearchMessages
                    | RequestPurpose::SearchPublicPosts
                    | RequestPurpose::SearchPublicMessagesByTag,
                ) => {
                    self.search.accept_messages(Vec::new(), true);
                }
                // The supplement failing changes nothing the user sees.
                Some(RequestPurpose::SearchChatsOnServer) => {}
                Some(RequestPurpose::SearchPublicChats) => {
                    self.search.accept_public_chats(Vec::new(), true);
                }
                _ => {}
            }
        }
        if self.chat_search.matches_generation(pending)
            && pending.map(|p| p.purpose) == Some(RequestPurpose::SearchChatMessagesMore)
        {
            self.chat_search.loading_more = false;
            self.chat_search.next_from_message_id = MessageId(0);
        }
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::GetChatMessageByDate) => self.fail_date_jump(err.code == 404),
            Some(RequestPurpose::GetChatMessageCalendar { .. }) => {
                self.fail_message_calendar(pending)
            }
            Some(RequestPurpose::SearchFromMembers) => {
                if let Some(picker) = self.chat_search.from_picker.as_mut() {
                    picker.request = None;
                }
            }
            _ => {}
        }
        if self.chat_search.matches_generation(pending)
            && pending.map(|p| p.purpose) == Some(RequestPurpose::SearchChatMessages)
        {
            self.chat_search
                .accept_hits(Vec::new(), 0, MessageId(0), true);
        }
        // Slice media-shared-gallery: failed gallery-tab fetch — the
        // tab shows the failed state with Retry, never the spinner
        // or the empty state.
        if let Some(RequestPurpose::GetSharedMedia { tab, generation }) = pending.map(|p| p.purpose)
            && let Some(chat_id) = pending.and_then(|p| p.chat_id)
        {
            self.shared_media.fail(
                chat_id,
                tab,
                generation,
                call_request_error_line(&err, "Could not load shared media"),
            );
        }
        if let Some(RequestPurpose::GetSharedMediaMore { tab, generation }) =
            pending.map(|p| p.purpose)
            && let Some(chat_id) = pending.and_then(|p| p.chat_id)
        {
            self.shared_media.fail_more(chat_id, tab, generation);
        }
        if let Some(pending) = pending
            && pending.purpose == RequestPurpose::GetHistoryNewer
        {
            self.fail_history_newer(pending);
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetHistoryAround)
            && let Some(message_id) = pending.and_then(|p| p.around_message_id)
        {
            self.finish_history_around(pending, message_id, true, seq);
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::ForwardMessages)
            && let Some(pending) = pending
        {
            self.finish_forward(pending, &[], true);
        }
        // Slice G2: failed welcome-message pack fetch — mark it so
        // the dialog shows an error, not a spinner.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::LoadChatWelcomeMessages)
            && let Some(chat_id) = pending.and_then(|p| p.chat_id)
        {
            self.welcome_message_fetches.insert(
                chat_id.0,
                WelcomeMessagesFetch::Failed(call_request_error_line(
                    &err,
                    "Could not load welcome messages",
                )),
            );
        }
        // A failed thread request marks the open thread view.
        self.fail_thread(
            pending,
            call_request_error_line(&err, "Could not load comments"),
        );
        // Slice CL: failed preview-history fetch — mark the peek
        // preview so it shows an error instead of a spinner.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatPreview)
            && let Some(chat_id) = pending.and_then(|p| p.chat_id)
        {
            self.chat_preview_fetch = Some(PreviewHistoryFetch {
                chat_id,
                messages: Vec::new(),
                failed: Some(call_request_error_line(&err, "Could not load preview")),
            });
        }
        // Slice G2: the slots half of a boost failed — the chain
        // cannot continue; drop the intent.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetBoostSlotsForBoost) {
            self.boost_intent = None;
        }
        // Slice G2: `boostChat` failed — the status is refetched on
        // success only, so nothing to roll back.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetInstalledStickerSets) {
            self.stickers.loading_sets = false;
            self.stickers.failed = true;
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetStickerSet) {
            self.stickers.loading_set = false;
            self.stickers.failed = true;
        }
        if let Some(RequestPurpose::LoadLibrarySet { set_id }) = pending.map(|p| p.purpose) {
            self.fail_library_set(set_id);
        }
        if let Some(RequestPurpose::ManageStickerSet { set_id, .. }) = pending.map(|p| p.purpose) {
            self.finish_sticker_batch_item(set_id, false);
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::ChangeEmojiSet) {
            self.emoji.mutation_failed = true;
            self.emoji.mutating_set = None;
        }
        let emoji_purpose = pending.map(|p| p.purpose);
        if emoji_purpose == Some(RequestPurpose::SetEmojiStatus) {
            self.emoji.pending_status_emoji = None;
        }
        if emoji_purpose == Some(RequestPurpose::GetEmojiSet)
            || emoji_purpose == Some(RequestPurpose::ChangeEmojiSet)
            || (emoji_purpose == Some(RequestPurpose::GetInstalledEmojiSets)
                && self.emoji.tab == crate::emoji::EmojiSetTab::Installed)
            || (emoji_purpose == Some(RequestPurpose::GetTrendingEmojiSets)
                && self.emoji.tab == crate::emoji::EmojiSetTab::Trending)
            || (emoji_purpose == Some(RequestPurpose::SearchEmojiSets)
                && self.emoji.tab == crate::emoji::EmojiSetTab::Search)
        {
            self.emoji.failed = true;
        }
        if let Some(request) = pending
            && let RequestPurpose::StopPendingMessage { topic_id, draft_id } = request.purpose
            && let Some(chat_id) = request.chat_id
            && let Some(draft) = self.pending_bot_messages.get_mut(&(chat_id.0, topic_id))
            && draft.draft_id == draft_id
        {
            draft.stop_failed = true;
        }
        if matches!(
            pending.map(|p| p.purpose),
            Some(
                RequestPurpose::ManageStickerSet { .. }
                    | RequestPurpose::ReorderInstalledStickerSets
                    | RequestPurpose::SearchStickers
                    | RequestPurpose::SearchStickerSets
                    | RequestPurpose::GetFavoriteStickers
                    | RequestPurpose::GetRecentStickers
                    | RequestPurpose::GetArchivedStickerSets
                    | RequestPurpose::GetTrendingStickerSets
                    | RequestPurpose::ClearRecentStickers
                    | RequestPurpose::AddFavoriteSticker
                    | RequestPurpose::RemoveFavoriteSticker
            )
        ) {
            self.stickers.failed = true;
        }
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::ResolveGifSearchBot | RequestPurpose::GetGifSearchResults { .. })
        ) {
            self.gifs.search_failed = true;
            self.gifs.search_loading = false;
        }
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::AddSavedAnimation | RequestPurpose::RemoveSavedAnimation)
        ) {
            self.gifs.failed = true;
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetSavedAnimations) {
            self.gifs.loading = false;
            self.gifs.loaded = true;
            self.gifs.failed = true;
            self.gifs.stale = false;
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::ReportChatSponsoredMessage) {
            // A TDLib error dismisses the option picker; no outcome is shown.
            self.sponsored_report = None;
            self.sponsored_report_target = None;
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetGameHighScores) {
            // Slice bots-games: a refused `getGameHighScores` closes the
            // scores panel (it never spins forever) and surfaces as a
            // status note via the callback-answer channel.
            if let Some(pending) = pending
                && let (Some(chat_id), Some(message_id)) =
                    (pending.chat_id, pending.around_message_id)
            {
                self.game_scores.remove(&(chat_id.0, message_id.0));
            }
            self.last_callback_answer = Some(CallbackQueryAnswer {
                text: "couldn't load the scores".to_string(),
                show_alert: false,
                url: String::new(),
            });
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetCallbackQueryAnswer) {
            // TDLib returns error 502 when the bot misses the query
            // timeout: surface it as an answer note (no TDLib text is
            // echoed) so the press gets visible feedback.
            self.last_callback_answer = Some(CallbackQueryAnswer {
                text: "bot did not answer".to_string(),
                show_alert: false,
                url: String::new(),
            });
        }
        // B1: a refused password-protected callback surfaces
        // honestly — 400 is the wrong-password case, anything else
        // is the generic bot-timeout note.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetCallbackQueryAnswerWithPassword) {
            let text = if err.code == 400 {
                "wrong 2-step verification password"
            } else {
                "bot did not answer"
            };
            self.last_callback_answer = Some(CallbackQueryAnswer {
                text: text.to_string(),
                show_alert: false,
                url: String::new(),
            });
        }
        // B1: a refused `getLoginUrlInfo` / `getLoginUrl` degrades
        // the login button to an ordinary URL button (schema 1.8.67
        // doc on `getLoginUrl`).
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::GetLoginUrlInfo) | Some(RequestPurpose::GetLoginUrl)
        ) {
            let fallback_url = self
                .login_url_request
                .take()
                .map(|request| request.raw_url)
                .unwrap_or_default();
            self.last_login_url_info = Some(LoginUrlInfo::Failed { fallback_url });
        }
        let download_id = pending
            .filter(|p| p.purpose == RequestPurpose::DownloadFile)
            .and_then(|p| p.file_id)
            .or_else(|| extra.and_then(|id| self.download_extras.get(&id.0).copied()));
        if let Some(file_id) = download_id {
            // MED3 review: only user-initiated downloads enter the
            // Failed section; automatic downloads never started by
            // the user must not show rows here.
            if self.user_downloads.contains(&file_id) {
                self.failed_downloads.insert(file_id);
            } else {
                // A refused automatic download is not retried per ingest.
                self.stalled_auto_downloads.insert(file_id);
            }
            self.unstick_download(file_id);
        }
        if let Some(pending) = pending
            && is_auth_submit(pending.purpose)
        {
            self.last_auth_error = Some(AuthRequestError {
                purpose: pending.purpose,
                class: err.class,
                flood_wait_secs: err.flood_wait_secs,
            });
        }
    }
}

/// tdesktop's wording for a failed link (`lng_username_not_found`,
/// `lng_group_invite_bad_link`); other failures keep the error code.
pub(crate) fn deep_link_error_text(flow: Option<&DeepLinkState>, code: i32) -> String {
    let not_found = matches!(code, 400 | 404);
    match flow {
        Some(DeepLinkState::ResolvingChat {
            action: DeepLinkAction::OpenUsername { domain, .. },
            ..
        }) if not_found => format!("The username \"{domain}\" is not occupied by anyone."),
        Some(DeepLinkState::ResolvingChat {
            action: DeepLinkAction::OpenPublicChatDraft { domain, .. },
            ..
        }) if not_found => format!("The username \"{domain}\" is not occupied by anyone."),
        Some(DeepLinkState::ResolvingChat {
            action: DeepLinkAction::UserPhone { phone, .. },
            ..
        }) if not_found => format!("The phone number +{phone} is not on Telegram yet."),
        Some(DeepLinkState::ResolvingChat {
            action: DeepLinkAction::StickerSet { .. },
            ..
        }) if not_found => "This sticker set doesn't exist.".to_string(),
        Some(DeepLinkState::ResolvingChat {
            action: DeepLinkAction::MessageLink { .. },
            ..
        }) if not_found => "This message link is broken or the chat is not available.".to_string(),
        Some(DeepLinkState::ResolvingChat {
            action: DeepLinkAction::BoostLink { .. },
            ..
        }) if not_found => "This boost link is broken.".to_string(),
        Some(DeepLinkState::ResolvingChat {
            action: DeepLinkAction::JoinInvite { .. },
            ..
        }) if not_found => "This invite link is broken or has expired.".to_string(),
        Some(DeepLinkState::ResolvingInfo { .. }) if not_found => {
            "This link isn't supported by Quill.".to_string()
        }
        _ => format!("Couldn't open the link (error {code})."),
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
