//! Failed requests for groups and channels: members, admin rights, invite links, join requests, boosts, communities.
use crate::state::*;

impl Session {
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
            Some(RequestPurpose::GetChatOwnerAfterLeaving) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.fail_owner_lookup(chat_id.0, err);
                }
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
                self.chats_state.chat_action_error = Some(call_request_error_line(
                    err,
                    "Could not change the group's sticker pack",
                ));
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
            // Phase D3a: failed invite-link / join-request requests
            // land in the fetch state so the panel shows an honest
            // error instead of spinning forever.
            Some(RequestPurpose::GetChatInviteLinks) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
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
            Some(RequestPurpose::GetChatJoinRequests) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
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
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.groups.join_request_latest.remove(&chat_id.0);
                }
                self.groups.invite_link_error = Some(call_request_error_line(
                    err,
                    "Could not load more join requests",
                ));
            }
            Some(RequestPurpose::Groups(GroupsPurpose::ProcessAllChatJoinRequests { .. })) => {
                self.groups.invite_link_error = Some(call_request_error_line(
                    err,
                    "Could not process join requests",
                ));
            }
            Some(RequestPurpose::GetRevokedChatInviteLinks) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
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
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
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
                    && let Some(state) = self.groups.invite_link_members.get_mut(&chat_id.0)
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
                    && let Some(state) = self.groups.admin_invite_links.get_mut(&chat_id.0)
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
                    && let Some(state) = self.groups.link_join_requests.get_mut(&chat_id.0)
                    && state.request == Some(pending.id)
                {
                    state.loading = false;
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
                    && state.request == Some(pending.id)
                {
                    state.loading = false;
                    state.request = None;
                    state.error = Some(call_request_error_line(err, "Could not load boosts"));
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
            Some(RequestPurpose::Groups(GroupsPurpose::ProcessChatJoinRequest { .. })) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.groups.join_requests.insert(
                        chat_id.0,
                        JoinRequestFetch::Failed(call_request_error_line(
                            err,
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
                    // Slice G1: the member-management dialog reads
                    // the member-list fetch states, not
                    // `admin_lists`, so the failure is also parked
                    // where the action was taken.
                    self.groups
                        .member_action_error
                        .insert(chat_id.0, line.clone());
                    self.groups
                        .admin_lists
                        .insert(chat_id.0, AdminListFetch::Failed(line));
                }
            }
            // Slice G1: failed `setChatMemberTag` (custom title)
            // surfaces as an admin-list error so the info panel
            // shows it, and in `member_action_error` so the
            // member-management dialog (which launched the
            // action) shows it too.
            Some(RequestPurpose::Groups(GroupsPurpose::SetChatMemberTag { .. })) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    let line = call_request_error_line(err, "Could not set custom title");
                    self.groups
                        .member_action_error
                        .insert(chat_id.0, line.clone());
                    self.groups
                        .admin_lists
                        .insert(chat_id.0, AdminListFetch::Failed(line));
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
            Some(RequestPurpose::Groups(GroupsPurpose::GetSupergroupMembers { filter })) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
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
            Some(RequestPurpose::Groups(GroupsPurpose::GetChatEventLog { from_event_id })) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id)
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
    }
}
