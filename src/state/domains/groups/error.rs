//! Failed requests for groups and channels: members, admin rights, invite links, join requests, boosts, communities.
use crate::state::*;

impl Session {
    /// Statistics panels of this supergroup's chats that waited for its
    /// full info end in the error the panel draws.
    fn fail_statistics_waiting_for(&mut self, supergroup_id: i64) {
        let chats: Vec<i64> = self
            .chats
            .values()
            .filter(|chat| {
                matches!(chat.kind, ChatKind::Supergroup { supergroup_id: id, .. } if id == supergroup_id)
            })
            .map(|chat| chat.id.0)
            .collect();
        for chat in chats {
            if let Some(fetch @ ChatStatisticsFetch::WaitingForInfo { .. }) =
                self.groups.chat_statistics.get_mut(&chat)
            {
                *fetch = ChatStatisticsFetch::Failed(
                    crate::lang::tr(crate::lang::keys::Q_MONEY_STATISTICS_INFO_FAILED).into(),
                );
            }
        }
    }

    /// Reacts to a failed groups request; called by
    /// [`Session::apply_error`] after the shared handling.
    pub(crate) fn apply_groups_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        if let Some(p) = pending
            && p.purpose == RequestPurpose::GetChatMember
            && let Some(chat_id) = p.chat_id
        {
            self.adopt_supergroup_status_for_chat(chat_id);
            // Not a member (or no access): there is no personal
            // restriction to find, and the probe must not repeat.
            if let Some(chat) = self.chats.get_mut(&chat_id.0) {
                chat.my_rights_fetched = true;
            }
        }
        // Too many communities or public links open the limit box (tdesktop
        // `ChannelsLimitBox`, `PublicLinksLimitBox`).
        if matches!(
            pending.map(|p| p.purpose),
            Some(
                RequestPurpose::JoinChat
                    | RequestPurpose::SetSupergroupUsername
                    | RequestPurpose::Groups(GroupsPurpose::CreateSupergroupChannel { .. })
            )
        ) {
            self.note_premium_limit(err, pending);
        }
        self.record_group_info_edit_error(err, pending);
        self.note_community_full_info_failed(pending);
        self.apply_group_type_error(err, pending);
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::CanTransferOwnership) => {
                self.groups.ownership.check_in_flight = false;
                self.groups.ownership.check_error = Some(format!(
                    "Could not check whether you can transfer ownership: {}",
                    error_reason(err)
                ));
            }
            Some(RequestPurpose::Groups(GroupsPurpose::TransferChatOwnership { .. })) => {
                self.fail_ownership_transfer(err);
            }
            Some(RequestPurpose::Groups(GroupsPurpose::AnswerSharingRequest { .. })) => {
                self.chats_state.chat_action_error = Some(call_request_error_line(
                    err,
                    crate::lang::tr(crate::lang::keys::Q_SHARING_ANSWER_FAILED),
                ));
            }
            Some(RequestPurpose::GetChatOwnerAfterLeaving) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.fail_owner_lookup(chat_id.0, err);
                }
            }
            // tdesktop toasts a refused create; the box stays, and the
            // photo waiting for the community goes.
            Some(RequestPurpose::CreateCommunity) => {
                self.communities.new_community_photo = None;
                self.communities
                    .outcomes
                    .push_back(crate::state::CommunityOutcome::CreateFailed {
                        reason: error_reason(err),
                    });
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
                self.groups.community_error = Some(format!("{action}: {}", error_reason(err)));
            }
            // Switching ads off needs a boost level; say so for the usual
            // refusal (TDLib's message text is not kept, see `TdError`).
            Some(RequestPurpose::ToggleSupergroupCanHaveSponsoredMessages) => {
                self.chats_state.chat_action_error = Some(if err.code == 400 {
                    "Could not switch off ads: the channel needs a higher boost level".to_string()
                } else {
                    call_request_error_line(err, "Could not change ads for the channel")
                });
            }
            // The unrestrict-boosts value was never applied; drop it so the
            // slider shows the stored one again.
            Some(RequestPurpose::SetSupergroupUnrestrictBoostCount) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.groups.unrestrict_boost_pending.remove(&chat_id.0);
                }
                self.chats_state.chat_action_error = Some(call_request_error_line(
                    err,
                    "Could not change the boosts needed to bypass restrictions",
                ));
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
                self.chats_state.chat_action_error = Some(format!(
                    "could not change the group setting (error {})",
                    err.code
                ));
            }
            Some(
                RequestPurpose::SetSupergroupStickerSet
                | RequestPurpose::SetSupergroupCustomEmojiStickerSet,
            ) => {
                self.file_appearance_answer(pending, Err(error_reason(err)));
                self.chats_state.chat_action_error = Some(call_request_error_line(
                    err,
                    "Could not change the group's sticker pack",
                ));
            }
            // A statistics panel waiting for this supergroup's full info
            // cannot learn whether the chat shows statistics: fail it.
            Some(RequestPurpose::GetSupergroupFullInfo) => {
                if let Some(supergroup_id) = pending.and_then(|p| p.supergroup_id) {
                    self.fail_statistics_waiting_for(supergroup_id);
                }
            }
            // Phase D2: a failed `getChatStatistics` lands in the
            // fetch state so the statistics panel shows an honest
            // error instead of spinning forever.
            Some(RequestPurpose::GetChatStatistics) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.groups.chat_statistics.insert(
                        chat_id.0,
                        ChatStatisticsFetch::Failed(call_request_error_line(
                            err,
                            "Could not load statistics",
                        )),
                    );
                }
            }
            Some(RequestPurpose::GetStatisticalGraph) => {
                if let Some(extra) = pending.map(|p| p.id) {
                    self.groups.stats.fail_graph(
                        extra,
                        call_request_error_line(err, "Could not load the chart"),
                    );
                }
            }
            Some(RequestPurpose::Groups(GroupsPurpose::GetMessageStatistics {
                chat_id,
                message_id,
            })) => self.groups.stats.fail_message_statistics(
                chat_id,
                message_id,
                call_request_error_line(err, "Could not load statistics"),
            ),
            Some(RequestPurpose::Groups(GroupsPurpose::GetMessagePublicForwards {
                chat_id,
                message_id,
            })) => self.groups.stats.fail_message_forwards(
                chat_id,
                message_id,
                call_request_error_line(err, "Could not load public shares"),
            ),
            Some(RequestPurpose::Groups(GroupsPurpose::GetStatsPost {
                chat_id,
                message_id,
            })) => self.groups.stats.post_missing(chat_id, message_id),
            // Phase D3a: failed invite-link / join-request requests
            // land in the fetch state so the panel shows an honest
            // error instead of spinning forever.
            // A list that already holds an answer (a newer request landed
            // first) is not failed by an older request's error.
            Some(RequestPurpose::GetChatInviteLinks) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id)
                    && !matches!(
                        self.groups.invite_links.get(&chat_id.0),
                        Some(InviteLinkFetch::Loaded(_))
                    )
                {
                    self.groups.invite_links.insert(
                        chat_id.0,
                        InviteLinkFetch::Failed(call_request_error_line(
                            err,
                            "Could not load invite links",
                        )),
                    );
                }
            }
            Some(RequestPurpose::CreateChatInviteLink) => {
                // Slice G1 fix-up: a failed mutation must not wipe
                // the previously loaded list — surface the error in
                // the status note and keep the last good data.
                self.groups.invite_link_error =
                    Some(call_request_error_line(err, "Could not create invite link"));
            }
            Some(RequestPurpose::EditChatInviteLink) => {
                self.groups.invite_link_error =
                    Some(call_request_error_line(err, "Could not edit invite link"));
            }
            Some(RequestPurpose::RevokeChatInviteLink) => {
                self.groups.invite_link_error =
                    Some(call_request_error_line(err, "Could not revoke invite link"));
            }
            // Slice G1: failed primary-link replacement — keep the
            // last good list, surface the error in the note.
            Some(RequestPurpose::ReplacePrimaryChatInviteLink) => {
                self.groups.invite_link_error = Some(call_request_error_line(
                    err,
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
                    self.groups.supergroup_is_broadcast.remove(&supergroup_id);
                }
            }
            // Only the request the list waits for fails it, and it is
            // settled: an older search's error must not cover a newer
            // list, and a stale `latest` would drop every later answer.
            Some(RequestPurpose::GetChatJoinRequests) => {
                if let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                    && self
                        .groups
                        .join_request_latest
                        .get(&chat_id.0)
                        .is_none_or(|latest| *latest == pending.id)
                {
                    self.groups.join_requests.insert(
                        chat_id.0,
                        JoinRequestFetch::Failed(call_request_error_line(
                            err,
                            "Could not load join requests",
                        )),
                    );
                }
            }
            // B8: failures of the new link/request admin calls keep the
            // last good data and surface the error line.
            Some(RequestPurpose::GetMoreChatJoinRequests) => {
                // A page that was superseded by a newer search says nothing:
                // its list is gone.
                let current = pending.is_some_and(|pending| {
                    pending.chat_id.is_some_and(|chat_id| {
                        self.groups
                            .join_request_latest
                            .get(&chat_id.0)
                            .is_none_or(|latest| *latest == pending.id)
                    })
                });
                if current {
                    self.groups.invite_link_error = Some(call_request_error_line(
                        err,
                        crate::lang::tr(
                            crate::lang::keys::Q_INVITE_LINK_COULD_NOT_LOAD_MORE_JOIN_REQUESTS,
                        ),
                    ));
                }
            }
            Some(RequestPurpose::Groups(GroupsPurpose::ProcessAllChatJoinRequests { .. })) => {
                self.groups.invite_link_error = Some(call_request_error_line(
                    err,
                    "Could not process join requests",
                ));
            }
            Some(RequestPurpose::GetRevokedChatInviteLinks) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id)
                    && !matches!(
                        self.groups.revoked_invite_links.get(&chat_id.0),
                        Some(InviteLinkFetch::Loaded(_))
                    )
                {
                    self.groups.revoked_invite_links.insert(
                        chat_id.0,
                        InviteLinkFetch::Failed(call_request_error_line(
                            err,
                            "Could not load revoked links",
                        )),
                    );
                }
            }
            Some(RequestPurpose::GetChatInviteLinkCounts) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id)
                    && !matches!(
                        self.groups.invite_link_counts.get(&chat_id.0),
                        Some(InviteLinkCountsFetch::Loaded(_))
                    )
                {
                    self.groups.invite_link_counts.insert(
                        chat_id.0,
                        InviteLinkCountsFetch::Failed(call_request_error_line(
                            err,
                            "Could not load link counts",
                        )),
                    );
                }
            }
            Some(RequestPurpose::Groups(GroupsPurpose::GetChatInviteLinkMembers { .. })) => {
                if let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                    && let Some(state) = self.groups.invite_pages.members_mut(chat_id.0)
                    && state.request == Some(pending.id)
                {
                    state.loading = false;
                    state.request = None;
                    state.error = Some(call_request_error_line(err, "Could not load members"));
                }
            }
            Some(RequestPurpose::GetAdminChatInviteLinks { revoked }) => {
                if let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                    && let Some(state) = self.groups.invite_pages.admin_links_mut(chat_id.0)
                {
                    let failed = InviteLinkFetch::Failed(call_request_error_line(
                        err,
                        "Could not load invite links",
                    ));
                    if revoked && state.revoked_request == Some(pending.id) {
                        state.revoked = failed;
                        state.revoked_request = None;
                    } else if !revoked && state.active_request == Some(pending.id) {
                        state.active = failed;
                        state.active_request = None;
                    }
                }
            }
            Some(RequestPurpose::GetLinkJoinRequests { .. }) => {
                if let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                    && let Some(state) = self.groups.invite_pages.join_requests_mut(chat_id.0)
                    && state.request == Some(pending.id)
                {
                    state.request = None;
                    state.error =
                        Some(call_request_error_line(err, "Could not load join requests"));
                }
            }
            Some(RequestPurpose::Groups(GroupsPurpose::ProcessLinkJoinRequests { .. })) => {
                self.groups.invite_link_error = Some(call_request_error_line(
                    err,
                    "Could not process join requests",
                ));
            }
            Some(RequestPurpose::GetChatBoosts { .. }) => {
                if let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                    && let Some(state) = self.groups.chat_boost_lists.get_mut(&chat_id.0)
                    && state.waits_for(pending.id)
                {
                    state.stage =
                        BoostsStage::Failed(call_request_error_line(err, "Could not load boosts"));
                }
            }
            Some(RequestPurpose::GetChatBoostLink) => {
                self.chats_state.chat_action_error =
                    Some(call_request_error_line(err, "Could not get the boost link"));
            }
            Some(
                RequestPurpose::ToggleSupergroupUsername
                | RequestPurpose::ReorderSupergroupUsernames,
            ) => {
                self.chats_state.chat_action_error = Some(call_request_error_line(
                    err,
                    "Could not change the usernames",
                ));
            }
            Some(RequestPurpose::DeleteRevokedChatInviteLink) => {
                if let Some(pending) = pending {
                    self.groups.revoked_link_deletions.remove(&pending.id);
                }
                self.groups.invite_link_error =
                    Some(call_request_error_line(err, "Could not delete invite link"));
            }
            Some(RequestPurpose::DeleteAllRevokedChatInviteLinks) => {
                self.groups.invite_link_error = Some(call_request_error_line(
                    err,
                    "Could not delete revoked links",
                ));
            }
            Some(RequestPurpose::Groups(GroupsPurpose::ProcessChatJoinRequest { user_id })) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.fail_join_request(err, chat_id, user_id);
                }
            }
            // Phase D3b: failed admin-management requests land in
            // the fetch state so the panel shows an honest error
            // instead of spinning forever.
            Some(RequestPurpose::GetChatAdministrators) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.groups.admin_lists.insert(
                        chat_id.0,
                        AdminListFetch::Failed(call_request_error_line(
                            err,
                            "Could not load administrators",
                        )),
                    );
                }
            }
            Some(RequestPurpose::Groups(GroupsPurpose::SetChatMemberStatus { .. })) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    let line = call_request_error_line(err, "Could not update member status");
                    // The member-management dialog and the Administrators
                    // page both show it from here; the loaded administrators
                    // list stays as it was (a refused change is not a
                    // failed load).
                    self.groups.member_action_error.insert(chat_id.0, line);
                }
            }
            // Slice G1: failed `setChatMemberTag` (custom title)
            // surfaces in `member_action_error`, which the
            // member-management dialog and the Administrators page
            // show; the loaded list stays.
            Some(RequestPurpose::Groups(GroupsPurpose::SetChatMemberTag { .. })) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    let line = call_request_error_line(err, "Could not set custom title");
                    self.groups.member_action_error.insert(chat_id.0, line);
                }
            }
            Some(RequestPurpose::GetBasicGroupFullInfo) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.groups.basic_group_members.insert(
                        chat_id.0,
                        SupergroupMembersFetch::Failed(call_request_error_line(
                            err,
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
                    *self.groups.add_members_failed.entry(chat_id.0).or_insert(0) += 1;
                }
            }
            Some(RequestPurpose::Groups(GroupsPurpose::GetSupergroupMembersMore {
                filter,
                ..
            })) => {
                // A failed page of the list shown can be asked for again.
                if let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                    && self.groups.take_member_more((chat_id.0, filter), pending)
                {
                    self.groups.member_more_failed.insert((chat_id.0, filter));
                }
            }
            Some(RequestPurpose::Groups(GroupsPurpose::GetSupergroupMembers { filter })) => {
                if let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                    && self
                        .groups
                        .is_latest_member_request((chat_id.0, filter), pending)
                {
                    self.groups.supergroup_members.insert(
                        (chat_id.0, filter),
                        SupergroupMembersFetch::Failed(call_request_error_line(
                            err,
                            "Could not load members",
                        )),
                    );
                }
            }
            // Phase D3c: a failed first page lands in the fetch
            // state so the panel shows an honest error instead of
            // spinning forever. A failed "load more" keeps the
            // already-loaded page so the button stays retryable.
            // A failed poll changes nothing shown; the next tick asks again.
            Some(RequestPurpose::Groups(GroupsPurpose::PollChatEventLog)) => {
                if let Some(pending) = pending {
                    self.take_event_log_wait(pending);
                }
            }
            Some(RequestPurpose::Groups(GroupsPurpose::GetChatEventLog { from_event_id })) => {
                // Only the page the log waits for; one asked under an
                // earlier query or filter changes nothing.
                if let Some(chat_id) = pending
                    .filter(|pending| self.take_event_log_wait(pending))
                    .and_then(|p| p.chat_id)
                    && (from_event_id == 0
                        || !matches!(
                            self.groups.event_logs.get(&chat_id.0),
                            Some(ChatEventLogFetch::Loaded(_))
                        ))
                {
                    self.groups.event_logs.insert(
                        chat_id.0,
                        ChatEventLogFetch::Failed(call_request_error_line(
                            err,
                            "Could not load recent actions",
                        )),
                    );
                }
            }
            Some(RequestPurpose::Groups(GroupsPurpose::GetAdminRights { user_id })) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.groups.admin_rights.insert(
                        (chat_id.0, user_id),
                        AdminRightsFetch::Failed(call_request_error_line(
                            err,
                            "Could not load admin rights",
                        )),
                    );
                }
            }
            _ => {}
        }
        // Slice G2: failed welcome-message pack fetch — mark it so
        // the dialog shows an error, not a spinner.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::LoadChatWelcomeMessages)
            && let Some(chat_id) = pending.and_then(|p| p.chat_id)
        {
            self.groups.welcome_message_fetches.insert(
                chat_id.0,
                WelcomeMessagesFetch::Failed(call_request_error_line(
                    err,
                    "Could not load welcome messages",
                )),
            );
        }
        // Slice G2: the slots half of a boost failed — the chain
        // cannot continue; drop the intent.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetBoostSlotsForBoost) {
            self.groups.boost_intent = None;
        }
        // A failed slots read or `boostChat` returns the boost box to its
        // counters with the reason (tdesktop shows the error as a toast).
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::GetBoostSlotsForBoost | RequestPurpose::BoostChat)
        ) && let Some(chat_id) = pending.and_then(|p| p.chat_id)
            && let Some(state) = self
                .groups
                .boost_box
                .as_mut()
                .filter(|state| state.chat_id == chat_id.0)
        {
            state.phase = crate::boost_box::BoostPhase::Main;
            state.reassigning = None;
            state.note = Some(call_request_error_line(err, "Could not boost"));
        }
    }
}
