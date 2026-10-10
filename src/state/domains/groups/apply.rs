//! Applies TDLib updates and answers for groups and channels: members, admin rights, invite links, join requests, boosts, communities.
use crate::state::*;
use crate::telegram::envelope::GroupsPayload;

impl Session {
    /// Applies one groups payload; called by
    /// [`Session::apply_payload`].
    pub(crate) fn apply_groups_payload(
        &mut self,
        payload: GroupsPayload,
        pending: Option<&PendingRequest>,
        extra: Option<RequestId>,
        seq: u64,
    ) {
        match payload {
            GroupsPayload::SupergroupFullInfo {
                description,
                member_count,
                linked_chat_id,
                slow_mode_delay,
                slow_mode_delay_expires_in,
                my_boost_count,
                unrestrict_boost_count,
                can_get_statistics,
                has_aggressive_anti_spam_enabled,
                can_toggle_aggressive_anti_spam,
                can_set_sticker_set,
                sticker_set_id,
                custom_emoji_sticker_set_id,
                admin,
            } => {
                let full_info_group = pending
                    .filter(|p| p.purpose == RequestPurpose::GetSupergroupFullInfo)
                    .and_then(|p| p.supergroup_id);
                self.apply_supergroup_full_info(
                    description,
                    member_count,
                    linked_chat_id,
                    slow_mode_delay,
                    slow_mode_delay_expires_in,
                    my_boost_count,
                    unrestrict_boost_count,
                    can_get_statistics,
                    has_aggressive_anti_spam_enabled,
                    can_toggle_aggressive_anti_spam,
                    can_set_sticker_set,
                    sticker_set_id,
                    custom_emoji_sticker_set_id,
                    pending,
                    extra,
                    seq,
                );
                if let Some(supergroup_id) = full_info_group {
                    self.merge_full_admin(supergroup_id, admin);
                }
            }
            // Parity slice: `updateSupergroupFullInfo` — the update carries
            // its own id, so it applies whenever it arrives (no pending
            // correlation).
            GroupsPayload::UpdateSupergroupFullInfo {
                supergroup_id,
                description,
                member_count,
                linked_chat_id,
                slow_mode_delay,
                slow_mode_delay_expires_in,
                my_boost_count,
                unrestrict_boost_count,
                can_get_statistics,
                has_aggressive_anti_spam_enabled,
                can_toggle_aggressive_anti_spam,
                can_set_sticker_set,
                sticker_set_id,
                custom_emoji_sticker_set_id,
                admin,
            } => {
                self.apply_update_supergroup_full_info(
                    supergroup_id,
                    description,
                    member_count,
                    linked_chat_id,
                    slow_mode_delay,
                    slow_mode_delay_expires_in,
                    my_boost_count,
                    unrestrict_boost_count,
                    can_get_statistics,
                    has_aggressive_anti_spam_enabled,
                    can_toggle_aggressive_anti_spam,
                    can_set_sticker_set,
                    sticker_set_id,
                    custom_emoji_sticker_set_id,
                    pending,
                    extra,
                    seq,
                );
                self.merge_full_admin(supergroup_id, admin);
            }
            // Slice (communities backend core): `communityId` (schema 1.8.67,
            // line 2264) is the `createCommunity` response — the driver
            // chains it into `getCommunityFullInfo`; nothing to reduce.
            GroupsPayload::CommunityId { .. } => {}
            // Slice (communities backend core): `updateCommunity` (schema
            // 1.8.67, line 10726) — create-on-first-sight, like chat
            // ingestion; the update carries the full object.
            GroupsPayload::UpdateCommunity { community } => {
                self.communities.insert(community.id, community);
            }
            // Slice (communities backend core): `updateCommunityFullInfo`
            // (schema 1.8.67, line 10753) — carries its own `community_id`,
            // so it applies whenever it arrives (no pending correlation).
            // The full pack replaces the cache.
            GroupsPayload::UpdateCommunityFullInfo {
                community_id,
                full_info,
            } => {
                self.community_full_infos.insert(community_id, full_info);
            }
            // TDLib 1.8.68: the direct `getCommunityFullInfo` answer has
            // no community id; the pending request carries it.
            GroupsPayload::CommunityFullInfo { full_info } => {
                if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::GetCommunityFullInfo
                    && let Some(community_id) = pending.community_id
                {
                    self.community_full_infos.insert(community_id, full_info);
                }
            }
            // Slice G2: welcome-message pack (`updateChatWelcomeMessages`,
            // schema 1.8.67, line 10649) — the full pack replaces the
            // cache; the welcome dialog renders it.
            GroupsPayload::UpdateChatWelcomeMessages { chat_id, messages } => {
                self.welcome_messages.insert(chat_id, messages);
                self.welcome_message_fetches
                    .insert(chat_id, WelcomeMessagesFetch::Loaded);
            }
            // Slice G2: `updateChatHasWelcomeMessages` (schema 1.8.67,
            // line 10600).
            GroupsPayload::UpdateChatHasWelcomeMessages {
                chat_id,
                has_welcome_messages,
            } => {
                self.chat_has_welcome_messages
                    .insert(chat_id, has_welcome_messages);
            }
            // Slice G2: `getChatBoostStatus` answer (schema 1.8.67, line
            // 13917) — correlated via the pending request's `chat_id`.
            GroupsPayload::ChatBoostStatus { level, boost_count } => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.chat_boost_status
                        .insert(chat_id.0, (level, boost_count));
                }
            }
            // Slice G2: `chatBoostSlots` (schema 1.8.67, line 6968) — the
            // `getAvailableChatBoostSlots` answer. Stashed per chat so the
            // driver's `boostChat` chain can consume it (see
            // `maybe_continue_boost` in connect.rs).
            GroupsPayload::ChatBoostSlots { slots } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetBoostSlotsForBoost)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.boost_slots_by_chat.insert(chat_id.0, slots);
                }
                // Slice G2: `boostChat` answers `chatBoostSlots` as well
                // (schema 1.8.67, line 13922) — drop the cached status so
                // the dialog refetches it.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::BoostChat)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.chat_boost_status.remove(&chat_id.0);
                }
            }
            // Phase D2: `getChatStatistics` answer — the response carries
            // no chat id, so it is correlated via the pending request's
            // `chat_id`.
            GroupsPayload::ChatStatistics { statistics } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatStatistics)
                    && let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                {
                    self.chat_statistics
                        .insert(chat_id.0, ChatStatisticsFetch::Loaded(Box::new(statistics)));
                }
            }
            // Phase D3a: `createChatInviteLink` / `editChatInviteLink`
            // answer — the created/updated link, correlated via the
            // pending request's `chat_id`. Upserts into the cached list;
            // a genuinely new link (create purpose, not already present)
            // also bumps `total_count`.
            GroupsPayload::ChatInviteLink { link } => {
                self.apply_chat_invite_link(link, pending, extra, seq)
            }
            // Phase D3a: `getChatInviteLinks` / `revokeChatInviteLink`
            // answer — replaces the cached list.
            GroupsPayload::ChatInviteLinks { total_count, links } => {
                if let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                {
                    match pending.purpose {
                        RequestPurpose::GetChatInviteLinks => {
                            self.invite_links.insert(
                                chat_id.0,
                                InviteLinkFetch::Loaded(InviteLinkList { total_count, links }),
                            );
                        }
                        // B8: the revoke answer holds only the revoked link
                        // (and the replacement primary), not the whole list.
                        RequestPurpose::RevokeChatInviteLink => {
                            self.apply_revoke_answer(chat_id.0, links);
                        }
                        RequestPurpose::GetAdminChatInviteLinks { revoked } => {
                            if let Some(state) = self.admin_invite_links.get_mut(&chat_id.0) {
                                let loaded =
                                    InviteLinkFetch::Loaded(InviteLinkList { total_count, links });
                                if revoked && state.revoked_request == Some(pending.id) {
                                    state.revoked = loaded;
                                    state.revoked_request = None;
                                } else if !revoked && state.active_request == Some(pending.id) {
                                    state.active = loaded;
                                    state.active_request = None;
                                }
                            }
                        }
                        RequestPurpose::GetRevokedChatInviteLinks => {
                            self.revoked_invite_links.insert(
                                chat_id.0,
                                InviteLinkFetch::Loaded(InviteLinkList { total_count, links }),
                            );
                        }
                        _ => {}
                    }
                }
            }
            // `getChatBoosts` answer; stale pages (the tab changed) drop.
            GroupsPayload::FoundChatBoosts {
                total_count,
                boosts,
                next_offset,
            } => {
                if let Some(pending) = pending
                    && let RequestPurpose::GetChatBoosts { append } = pending.purpose
                    && let Some(chat_id) = pending.chat_id
                    && let Some(state) = self.chat_boost_lists.get_mut(&chat_id.0)
                    && state.request == Some(pending.id)
                {
                    if !append {
                        state.boosts.clear();
                    }
                    state.boosts.extend(boosts);
                    state.total_count = total_count;
                    state.next_offset = next_offset;
                    state.loading = false;
                    state.error = None;
                    state.request = None;
                }
            }
            // `getChatBoostLink` answer.
            GroupsPayload::ChatBoostLink { link, is_public } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatBoostLink)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.chat_boost_links.insert(chat_id.0, (link, is_public));
                }
            }
            // B8: `getChatInviteLinkCounts` answer.
            GroupsPayload::ChatInviteLinkCounts { counts } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatInviteLinkCounts)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.invite_link_counts
                        .insert(chat_id.0, InviteLinkCountsFetch::Loaded(counts));
                }
            }
            // B8: `getChatInviteLinkMembers` answer; stale replies (the
            // details were opened for another link meanwhile) are dropped.
            GroupsPayload::ChatInviteLinkMembers {
                total_count,
                members,
            } => {
                if let Some(pending) = pending
                    && let RequestPurpose::Groups(GroupsPurpose::GetChatInviteLinkMembers {
                        append,
                    }) = pending.purpose
                    && let Some(chat_id) = pending.chat_id
                    && let Some(state) = self.invite_link_members.get_mut(&chat_id.0)
                    && state.request == Some(pending.id)
                {
                    if !append {
                        state.members.clear();
                    }
                    state.members.extend(members);
                    state.total_count = total_count;
                    state.loading = false;
                    state.error = None;
                    state.request = None;
                }
            }
            // Phase D3a: `getChatJoinRequests` answer.
            GroupsPayload::ChatJoinRequests {
                total_count,
                requests,
            } => {
                if let Some(pending) = pending
                    && let RequestPurpose::GetLinkJoinRequests { append } = pending.purpose
                    && let Some(chat_id) = pending.chat_id
                    && let Some(state) = self.link_join_requests.get_mut(&chat_id.0)
                    && state.request == Some(pending.id)
                {
                    if !append {
                        state.requests.clear();
                    }
                    state.requests.extend(requests);
                    state.total_count = total_count;
                    state.loading = false;
                    state.error = None;
                    state.request = None;
                } else if let Some(pending) = pending
                    && matches!(
                        pending.purpose,
                        RequestPurpose::GetChatJoinRequests
                            | RequestPurpose::GetMoreChatJoinRequests
                    )
                    && let Some(chat_id) = pending.chat_id
                    && self
                        .join_request_latest
                        .get(&chat_id.0)
                        .is_none_or(|latest| *latest == pending.id)
                {
                    self.join_request_latest.remove(&chat_id.0);
                    let mut all = Vec::new();
                    if pending.purpose == RequestPurpose::GetMoreChatJoinRequests
                        && let Some(JoinRequestFetch::Loaded(old)) =
                            self.join_requests.get(&chat_id.0)
                    {
                        all = old.requests.clone();
                    }
                    all.extend(requests);
                    self.join_requests.insert(
                        chat_id.0,
                        JoinRequestFetch::Loaded(JoinRequestList {
                            total_count,
                            requests: all,
                        }),
                    );
                }
            }
            // Phase D3b: `getChatAdministrators` answer — replaces the
            // cached admin list.
            GroupsPayload::ChatAdministrators { administrators } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatAdministrators)
                    && let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                {
                    self.admin_lists
                        .insert(chat_id.0, AdminListFetch::Loaded(administrators));
                }
            }
            // Phase D3b / slice G1: `getSupergroupMembers` answer —
            // replaces the cached page for this (chat, filter).
            GroupsPayload::SupergroupMembers {
                members,
                total_count,
            } => {
                if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::SearchMentionMembers
                {
                    self.apply_mention_members(pending.id, &members);
                } else if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::SearchFromMembers
                {
                    self.apply_from_members(pending.id, &members);
                } else if let Some(RequestPurpose::Groups(GroupsPurpose::GetSupergroupMembers {
                    filter,
                })) = pending.map(|p| p.purpose)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.supergroup_members.insert(
                        (chat_id.0, filter),
                        SupergroupMembersFetch::Loaded {
                            members,
                            total_count,
                        },
                    );
                }
            }
            // Slice G1: `createNewBasicGroupChat` answer
            // (`createdBasicGroupChat`, schema 1.8.67, line 3644). The new
            // chat itself arrives as `updateNewChat`; nothing to cache.
            GroupsPayload::CreatedBasicGroupChat { chat_id: _ } => {}
            // Slice G1: `addChatMembers` answer (`failedToAddMembers`,
            // schema 1.8.67, line 3640). Added members arrive as
            // `updateChatMember`; the failure count drives the notice in
            // the add-members dialog.
            // Slice G1: `getBasicGroupFullInfo` answer — replaces the
            // cached basic-group member list.
            GroupsPayload::BasicGroupFullInfo { members } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetBasicGroupFullInfo)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    let total_count = members.len() as i32;
                    self.basic_group_members.insert(
                        chat_id.0,
                        SupergroupMembersFetch::Loaded {
                            members,
                            total_count,
                        },
                    );
                }
            }
            GroupsPayload::FailedToAddMembers { failed_count } => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    match pending.map(|p| p.purpose) {
                        // Slice G1: the bulk `addChatMembers` answer is a
                        // single response — it replaces the count.
                        Some(RequestPurpose::AddChatMembers) => {
                            self.add_members_failed.insert(chat_id.0, failed_count);
                        }
                        // Slice G1 fix-up: basic groups send one
                        // `addChatMember` per user and each answers
                        // `failedToAddMembers` — accumulate, or the last
                        // response would overwrite the earlier ones.
                        Some(RequestPurpose::AddChatMember) => {
                            *self.add_members_failed.entry(chat_id.0).or_insert(0) += failed_count;
                        }
                        _ => {}
                    }
                }
            }
            // Phase D3c: `getChatEventLog` answer — a first page (cursor
            // 0) replaces the cache; an older page appends, deduped by
            // event id, keeping reverse-chronological order (decreasing
            // event id, schema 1.8.67 line 15252). A full page sets
            // `has_more`; a short page exhausts the log.
            GroupsPayload::ChatEvents { events } => {
                if let Some(pending) = pending
                    && let RequestPurpose::Groups(GroupsPurpose::GetChatEventLog { from_event_id }) =
                        pending.purpose
                    && let Some(chat_id) = pending.chat_id
                {
                    let has_more = events.len() as i32 >= CHAT_EVENT_LOG_PAGE_SIZE;
                    let mut merged = match (from_event_id, self.event_logs.get(&chat_id.0)) {
                        (0, _) => events,
                        (_, Some(ChatEventLogFetch::Loaded(page))) => {
                            let mut merged = page.events.clone();
                            crate::state::paging::append_new_by_id(&mut merged, events, |event| {
                                event.id
                            });
                            merged
                        }
                        _ => events,
                    };
                    merged.sort_by_key(|event| std::cmp::Reverse(event.id));
                    self.event_logs.insert(
                        chat_id.0,
                        ChatEventLogFetch::Loaded(ChatEventLogPage {
                            events: merged,
                            has_more,
                        }),
                    );
                }
            }
            // Phase D3a: `updateNewChatJoinRequest` (schema 1.8.67, line
            // 11210) — a new join request arrived. Prepend it to the cached
            // list when one is loaded; otherwise the next fetch picks it up.
            // The total is bumped: the update announces a genuinely new
            // pending request.
            GroupsPayload::UpdateNewChatJoinRequest {
                chat_id, request, ..
            } => {
                if let Some(JoinRequestFetch::Loaded(mut list)) =
                    self.join_requests.get(&chat_id).cloned()
                    && !list
                        .requests
                        .iter()
                        .any(|existing| existing.user_id == request.user_id)
                {
                    if self
                        .join_request_queries
                        .get(&chat_id)
                        .is_none_or(|q| q.is_empty())
                    {
                        list.requests.insert(0, request);
                    }
                    list.total_count = list.total_count.saturating_add(1);
                    self.join_requests
                        .insert(chat_id, JoinRequestFetch::Loaded(list));
                }
            }
            // Phase D3a: `updateChatPendingJoinRequests` (schema 1.8.67,
            // line 10555) — the badge count. The full list still needs
            // `getChatJoinRequests`.
            GroupsPayload::UpdateChatPendingJoinRequests {
                chat_id,
                total_count,
                user_ids,
            } => {
                self.pending_join_request_counts
                    .insert(chat_id, total_count);
                // Batch 8: the (up to three) newest requesters back the
                // requests bar's avatars.
                if user_ids.is_empty() {
                    self.pending_join_request_users.remove(&chat_id);
                } else {
                    self.pending_join_request_users.insert(chat_id, user_ids);
                }
            }
            // Phase 5.1: `supergroup.is_forum` via `updateSupergroup` (an
            // update — applies whenever it arrives) or the `getSupergroup`
            // response (gated on the pending purpose). Parity slice: the
            // first active username is cached alongside, for the
            // channel/supergroup header.
            GroupsPayload::UpdateBasicGroup {
                basic_group_id,
                member_count,
                status,
                can_restrict_members,
                can_promote_members,
                can_manage_tags,
                can_change_info,
                is_active,
            } => {
                self.basic_group_member_counts
                    .insert(basic_group_id, member_count);
                self.basic_group_status.insert(basic_group_id, status);
                self.basic_group_own.insert(
                    basic_group_id,
                    BasicGroupOwn {
                        status,
                        can_restrict_members,
                        can_promote_members,
                        can_manage_tags,
                    },
                );
                self.basic_group_change_info_right
                    .insert(basic_group_id, can_change_info.unwrap_or(false));
                self.basic_group_active.insert(basic_group_id, is_active);
            }
            GroupsPayload::UpdateSupergroup {
                supergroup_id,
                verification,
                member_count,
                is_forum,
                has_forum_tabs,
                has_automatic_translation,
                username,
                usernames,
                status,
                can_restrict_members,
                can_invite_users,
                can_promote_members,
                can_manage_tags,
                can_manage_topics,
                can_change_info,
                can_send_welcome_messages,
                join_by_request,
                is_broadcast_group,
                sign_messages,
                show_message_sender,
                join_to_send_messages,
            } => {
                self.set_supergroup_usernames(supergroup_id, usernames);
                self.supergroup_join_to_send
                    .insert(supergroup_id, join_to_send_messages);
                if member_count > 0 {
                    self.supergroup_member_counts
                        .insert(supergroup_id, member_count);
                }
                self.supergroup_verification
                    .insert(supergroup_id, verification);
                self.set_supergroup_forum_tabs(supergroup_id, has_forum_tabs);
                self.set_supergroup_auto_translate(supergroup_id, has_automatic_translation);
                self.apply_update_supergroup(
                    supergroup_id,
                    is_forum,
                    username,
                    status,
                    can_restrict_members,
                    can_invite_users,
                    can_promote_members,
                    can_manage_tags,
                    can_manage_topics,
                    can_change_info,
                    can_send_welcome_messages,
                    join_by_request,
                    is_broadcast_group,
                    sign_messages,
                    show_message_sender,
                    pending,
                    extra,
                    seq,
                );
            }
            GroupsPayload::Supergroup {
                supergroup_id,
                is_forum,
                has_forum_tabs,
                has_automatic_translation,
                username,
                status,
                can_restrict_members,
                can_invite_users,
                can_promote_members,
                can_manage_tags,
                can_manage_topics,
                can_change_info,
                can_send_welcome_messages,
                join_by_request,
                is_broadcast_group,
                sign_messages,
                show_message_sender,
                join_to_send_messages,
            } => {
                self.supergroup_join_to_send
                    .insert(supergroup_id, join_to_send_messages);
                self.set_supergroup_forum_tabs(supergroup_id, has_forum_tabs);
                self.set_supergroup_auto_translate(supergroup_id, has_automatic_translation);
                self.apply_supergroup(
                    supergroup_id,
                    is_forum,
                    username,
                    status,
                    can_restrict_members,
                    can_invite_users,
                    can_promote_members,
                    can_manage_tags,
                    can_manage_topics,
                    can_change_info,
                    can_send_welcome_messages,
                    join_by_request,
                    is_broadcast_group,
                    sign_messages,
                    show_message_sender,
                    pending,
                    extra,
                    seq,
                );
            }
            GroupsPayload::CanTransferOwnershipResult { result } => {
                if pending.is_some_and(|p| p.purpose == RequestPurpose::CanTransferOwnership) {
                    self.accept_can_transfer_ownership(result);
                }
            }
            GroupsPayload::ChatMember { member } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatMember)
                    && let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                {
                    self.accept_own_chat_member(chat_id, member.clone());
                }
                // Phase D3b: `getChatMember` for one administrator's rights
                // (edit dialog). Only an administrator status carries a
                // rights block worth caching.
                if let Some(RequestPurpose::Groups(GroupsPurpose::GetAdminRights { user_id })) =
                    pending.map(|p| p.purpose)
                    && let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                    && let Some(rights) = member.admin_rights
                {
                    self.admin_rights
                        .insert((chat_id.0, user_id), AdminRightsFetch::Loaded(rights));
                }
            }
            GroupsPayload::UpdateChatMember { chat_id, member } => {
                // Phase D3b: any membership change may alter the admin
                // list — drop the cached list so the info panel refetches
                // instead of showing stale data. `accept_own_chat_member`
                // also refreshes the viewer's own rights below.
                self.admin_lists.remove(&chat_id.0);
                // Phase D3b: per-admin rights for the changed member are
                // stale too (e.g. after an edit-rights save) — drop them
                // so the editor refetches instead of showing old rights.
                if let MessageSender::User { user_id } = member.member_id {
                    self.admin_rights.remove(&(chat_id.0, user_id));
                }
                // Slice G1 fix-up: membership changes also stale the
                // member-list caches (e.g. our own add, or someone else
                // joining). Drop both so the dialog refetches instead of
                // showing the pre-change list; `member_list_stale` tells
                // the UI an open dialog needs a refetch.
                self.basic_group_members.remove(&chat_id.0);
                self.supergroup_members
                    .retain(|(id, _), _| *id != chat_id.0);
                if !self.member_list_stale.contains(&chat_id.0) {
                    self.member_list_stale.push(chat_id.0);
                }
                self.accept_own_chat_member(chat_id, member);
            }
            GroupsPayload::JoinChatResult(result) => {
                // `parity:platform-deep-links`: `joinChatByInviteLink`
                // answer for a deep-link invite. Success opens the chat;
                // the other variants surface as an honest note.
                if let Some(RequestPurpose::Chats(ChatsPurpose::DeepLinkJoin { generation })) =
                    pending.map(|p| p.purpose)
                    && let Some(DeepLinkState::ResolvingChat {
                        action,
                        generation: slot,
                    }) = self.deep_link.clone()
                    && slot == generation
                {
                    self.deep_link = Some(match result {
                        ChatJoinResult::Success { chat_id } => {
                            DeepLinkState::ChatReady { chat_id, action }
                        }
                        ChatJoinResult::RequestSent => DeepLinkState::ShowText(
                            "Join request sent — the admins need to approve it.".into(),
                        ),
                        ChatJoinResult::GuardBotApprovalRequired => DeepLinkState::ShowText(
                            "This invite needs a guard bot's approval first.".into(),
                        ),
                        ChatJoinResult::Declined => {
                            DeepLinkState::ShowText("The invite was declined.".into())
                        }
                    });
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::JoinChat)
                    && let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                {
                    self.accept_join_chat_result(chat_id, result);
                }
            }
            // Checked invite preview; confirmation is a separate driver action.
            GroupsPayload::ChatInviteLinkInfo {
                title,
                member_count,
                creates_join_request,
                is_channel,
            } => {
                if let Some(RequestPurpose::Chats(ChatsPurpose::DeepLinkCheckInvite { generation })) =
                    pending.map(|p| p.purpose)
                    && let Some(DeepLinkState::ResolvingChat {
                        action: DeepLinkAction::JoinInvite { hash },
                        generation: slot,
                    }) = self.deep_link.clone()
                    && slot == generation
                {
                    self.deep_link = Some(DeepLinkState::InvitePreview {
                        hash,
                        title,
                        member_count,
                        creates_join_request,
                        is_channel,
                        generation,
                    });
                }
            }
        }
    }
}
