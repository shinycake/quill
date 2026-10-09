//! Payload dispatcher: routes every envelope payload to its handler.
use super::*;

impl Session {
    pub fn apply(&mut self, owned: OwnedEnvelope) {
        self.revision = self.revision.wrapping_add(1);
        if owned.seq <= self.last_seq && self.last_seq != 0 {
            self.diagnostics.record(Diagnostic {
                category: "reducer",
                type_name: Some(owned.envelope.type_name.clone()),
                extra: owned.envelope.extra.map(|id| id.0),
                seq: Some(owned.seq),
                note: "out-of-order-ignored",
            });
            return;
        }
        self.expire_pending_bot_messages(unix_ms_now());
        self.last_seq = owned.seq;
        let extra = owned.envelope.extra;
        let pending = extra.and_then(|id| self.requests.take(id));
        if let Some(pending) = pending.as_ref()
            && pending.account_generation != self.account_generation
        {
            self.diagnostics.record(Diagnostic {
                category: "reducer",
                type_name: Some(owned.envelope.type_name.clone()),
                extra: Some(pending.id.0),
                seq: Some(owned.seq),
                note: "stale-account-generation",
            });
            return;
        }
        self.apply_payload(owned.envelope.payload, pending.as_ref(), extra, owned.seq);
    }

    pub(crate) fn apply_payload(
        &mut self,
        payload: EnvelopePayload,
        pending: Option<&PendingRequest>,
        extra: Option<RequestId>,
        seq: u64,
    ) {
        match payload {
            EnvelopePayload::AccountExport(value) => {
                if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::ExportAccount
                    && let Some(export) = self.account_export.as_mut()
                {
                    export.reply(pending.id, value);
                }
            }
            EnvelopePayload::UpdateAuthorizationState(state) => self.set_auth(state),
            // MED4: `updateOption` (schema:10926). Only
            // `message_caption_length_max` is consumed (caption edits /
            // media-send captions); every other option parses but is
            // ignored, never an error.
            EnvelopePayload::UpdateOption { name, value } => {
                self.storage_limits.apply_option(&name, &value);
                if name == "disable_top_chats"
                    && let OptionValue::Boolean(off) = &value
                {
                    self.search.top_chats_disabled = *off;
                    if *off {
                        self.search.top_chats.clear();
                        self.search.top_menu = None;
                    }
                }
                if name == "my_id"
                    && let OptionValue::Integer(id) = &value
                    && *id > 0
                {
                    self.my_user_id = Some(*id);
                }
                if name == "prefer_ipv6"
                    && let OptionValue::Boolean(on) = &value
                {
                    self.proxy.prefer_ipv6 = *on;
                }
                if name == "is_premium" {
                    self.premium_option = match &value {
                        OptionValue::Boolean(on) => Some(*on),
                        _ => Some(false),
                    };
                }
                if name == "gift_text_length_max"
                    && let OptionValue::Integer(limit) = &value
                {
                    self.gift_text_length_max = usize::try_from(*limit).ok();
                }
                if name == "pending_text_message_period"
                    && let OptionValue::Integer(period) = &value
                {
                    self.pending_bot_period_secs = u64::try_from(*period).unwrap_or(0);
                }
                if name == "animation_search_bot_username" {
                    let username = match &value {
                        OptionValue::String(name) => name.clone(),
                        _ => String::new(),
                    };
                    if self.gifs.search_bot_username != username {
                        self.gifs.search_bot_username = username;
                        self.gifs.search_bot_user_id = None;
                    }
                }
                if name == "message_caption_length_max"
                    && let OptionValue::Integer(limit) = value
                {
                    self.message_caption_length_max = limit.max(0).min(i64::from(i32::MAX)) as i32;
                }
                // Slice CL1: pin-limit options (schema:13674) for the
                // client-side pin pre-check.
                if (name == "pinned_chat_count_max" || name == "pinned_archived_chat_count_max")
                    && let OptionValue::Integer(limit) = value
                {
                    let limit = limit.max(0).min(i64::from(i32::MAX)) as i32;
                    if name == "pinned_chat_count_max" {
                        self.pinned_chat_count_max = limit;
                    } else {
                        self.pinned_archived_chat_count_max = limit;
                    }
                }
            }
            EnvelopePayload::UpdateConnectionState(state) => self.connection = state,
            EnvelopePayload::UpdateNewChat {
                chat_id,
                title,
                kind,
                unread_count,
                last_read_inbox_message_id,
                last_read_outbox_message_id,
                notification_settings,
                draft,
                photo,
                can_send_basic_messages,
                permissions,
                can_be_deleted_for_all_users,
                can_be_deleted_only_for_self,
                is_marked_as_unread,
                message_auto_delete_time,
                video_chat,
                has_welcome_messages,
                has_protected_content,
                available_reactions,
                has_scheduled_messages,
                message_sender,
                is_translatable,
                reply_markup_message_id,
                unread_mention_count,
                unread_reaction_count,
                unread_poll_vote_count,
                can_be_reported,
                action_bar,
                blocked,
                positions,
                last_message,
            } => {
                self.set_chat_protected(chat_id.0, has_protected_content);
                if let Some(setting) = available_reactions {
                    self.chat_available_reactions.insert(chat_id.0, setting);
                }
                // B7: the answer to `upgradeBasicGroupChatToSupergroupChat`
                // is the new supergroup chat; remember `old -> new`.
                if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::UpgradeBasicGroup
                    && let Some(old) = pending.chat_id
                {
                    self.chat_upgrades.push((old.0, chat_id.0));
                }
                self.set_chat_has_scheduled(chat_id.0, has_scheduled_messages);
                self.set_chat_message_sender(chat_id.0, message_sender);
                self.set_chat_translatable(chat_id.0, is_translatable);
                if reply_markup_message_id.0 > 0 {
                    self.reply_keyboards
                        .markup_message_ids
                        .insert(chat_id.0, reply_markup_message_id.0);
                }
                self.set_chat_action_bar(chat_id.0, action_bar);
                self.apply_update_new_chat(
                    chat_id,
                    title,
                    kind,
                    unread_count,
                    last_read_inbox_message_id,
                    last_read_outbox_message_id,
                    notification_settings,
                    draft,
                    photo,
                    can_send_basic_messages,
                    permissions,
                    can_be_deleted_for_all_users,
                    can_be_deleted_only_for_self,
                    is_marked_as_unread,
                    message_auto_delete_time,
                    video_chat,
                    has_welcome_messages,
                    unread_mention_count,
                    unread_reaction_count,
                    can_be_reported,
                    blocked,
                    pending,
                    extra,
                    seq,
                );
                if let Some(chat) = self.chats.get_mut(&chat_id.0) {
                    chat.unread_poll_vote_count = unread_poll_vote_count;
                }
                // `chat.last_message`: the starting preview. Never replaces
                // a newer one an `updateChatLastMessage` already set.
                if let Some(message) = last_message {
                    let newer_known = self
                        .chats
                        .get(&chat_id.0)
                        .and_then(|chat| chat.last_message)
                        .is_some_and(|known| known.id.0 > message.id.0);
                    if !newer_known {
                        self.remember_files(&message.files);
                        self.set_chat_last_message(chat_id, Some(&message));
                    }
                }
                // `chat.positions`: place the chat in every list it already
                // has a non-zero position in, like Telegram X
                // (`Tdlib.updateNewChat` → `TdlibChatList.onUpdateNewChat`).
                // Additive only — an empty set never evicts a position that
                // arrived first (TDLib reports later changes separately).
                let placed = positions.iter().any(|pos| pos.order != 0);
                for pos in positions.into_iter().filter(|pos| pos.order != 0) {
                    self.apply_position_fields(pos);
                }
                if placed {
                    self.rebuild_main_order();
                }
            }
            // Parity slice: `updateChatPhoto` — swap the cached small
            // photo file id (the chat list re-renders avatars from it).
            EnvelopePayload::UpdateChatPhoto { chat_id, photo } => {
                let photo_file_id = photo.as_ref().map(|file| file.id.0);
                if let Some(file) = &photo {
                    self.remember_files(std::slice::from_ref(file));
                }
                let old_photo_file_id = std::mem::replace(
                    &mut self
                        .chats
                        .entry(chat_id.0)
                        .or_insert_with(|| placeholder_chat(chat_id))
                        .photo_file_id,
                    photo_file_id,
                );
                self.replace_avatar(old_photo_file_id, photo_file_id);
            }
            EnvelopePayload::UpdateChatPermissions {
                chat_id,
                can_send_basic_messages,
                permissions,
            } => {
                let chat = self
                    .chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id));
                chat.can_send_basic_messages = can_send_basic_messages;
                // Slice G1: keep the full default permissions block for
                // the editor.
                chat.permissions = permissions;
            }
            EnvelopePayload::UpdateChatDraftMessage {
                chat_id,
                draft,
                positions,
            } => {
                let chat = self
                    .chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id));
                if !self.draft_dirty.contains(&chat_id.0) {
                    chat.draft = draft;
                }
                self.replace_main_list_from_positions(chat_id, &positions);
                self.rebuild_main_order();
            }
            EnvelopePayload::UpdateUser { user_id, user } => {
                // Phase 6: keep the full user object for the contacts list
                // and info panels.
                let old_photo_file_id = self
                    .users
                    .insert(user_id.0, user.clone())
                    .map(|old| old.photo_small_file_id);
                self.replace_avatar(old_photo_file_id, Some(user.photo_small_file_id));
                if user.is_bot {
                    self.bot_user_ids.insert(user_id.0);
                } else {
                    // No longer a bot: drop any cached bot info so the panel
                    // cannot show stale description/commands (Phase 3.1).
                    self.bot_user_ids.remove(&user_id.0);
                    self.bot_info.remove(&user_id.0);
                }
            }
            EnvelopePayload::UpdateUserStatus { user_id, status } => {
                // Phase 6: live online / last-seen for the contacts list.
                if let Some(user) = self.users.get_mut(&user_id.0) {
                    user.status = status;
                }
            }
            EnvelopePayload::UpdateProfileAccentColors {
                colors,
                available_ids,
            } => {
                // Slice A12: palette + settable ids for the edit-profile
                // accent picker. Replaces wholesale — the update is the
                // full server state.
                self.profile_accent_colors = colors;
                self.available_accent_color_ids = available_ids;
            }
            EnvelopePayload::Users { user_ids } => {
                // Phase 6: `getContacts` answer — only answers to our own
                // fetch are accepted (matched by `@extra`); the user
                // objects themselves arrive via `updateUser`.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetContacts) {
                    self.contacts = Some(user_ids);
                    self.contacts_error = false;
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::GetRecentInlineBots) {
                    self.reply_keyboards.recent_inline_bots = Some(user_ids);
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::GetBotSimilarBots)
                    && let Some(pending) = pending
                    && let Some(bot_user_id) = pending.user_id
                {
                    // Slice B2: `getBotSimilarBots` answer — the user
                    // objects arrive via `updateUser`; ids alone drive the
                    // list (Telegram X `SharedChatsController` similarly
                    // resolves users from its cache).
                    self.similar_bots
                        .insert(bot_user_id, SimilarBotsFetch::Loaded(user_ids));
                }
            }
            EnvelopePayload::SupergroupFullInfo {
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
            EnvelopePayload::UpdateSupergroupFullInfo {
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
            // chains it into `loadCommunityFullInfo`; nothing to reduce.
            EnvelopePayload::CommunityId { .. } => {}
            // Slice (communities backend core): `updateCommunity` (schema
            // 1.8.67, line 10726) — create-on-first-sight, like chat
            // ingestion; the update carries the full object.
            EnvelopePayload::UpdateCommunity { community } => {
                self.communities.insert(community.id, community);
            }
            // Slice (communities backend core): `updateCommunityFullInfo`
            // (schema 1.8.67, line 10753) — carries its own `community_id`,
            // so it applies whenever it arrives (no pending correlation).
            // The full pack replaces the cache.
            EnvelopePayload::UpdateCommunityFullInfo {
                community_id,
                full_info,
            } => {
                self.community_full_infos.insert(community_id, full_info);
            }
            // Slice G2: welcome-message pack (`updateChatWelcomeMessages`,
            // schema 1.8.67, line 10649) — the full pack replaces the
            // cache; the welcome dialog renders it.
            EnvelopePayload::UpdateChatWelcomeMessages { chat_id, messages } => {
                self.welcome_messages.insert(chat_id, messages);
                self.welcome_message_fetches
                    .insert(chat_id, WelcomeMessagesFetch::Loaded);
            }
            // Slice G2: `updateChatHasWelcomeMessages` (schema 1.8.67,
            // line 10600).
            EnvelopePayload::UpdateChatHasWelcomeMessages {
                chat_id,
                has_welcome_messages,
            } => {
                self.chat_has_welcome_messages
                    .insert(chat_id, has_welcome_messages);
            }
            EnvelopePayload::UpdateChatHasProtectedContent {
                chat_id,
                has_protected_content,
            } => self.set_chat_protected(chat_id, has_protected_content),
            // B7: allowed reactions and the active emoji list.
            EnvelopePayload::UpdateChatAvailableReactions {
                chat_id,
                available_reactions,
            } => {
                self.chat_available_reactions
                    .insert(chat_id, available_reactions);
            }
            EnvelopePayload::UpdateActiveEmojiReactions { emojis } => {
                self.active_emoji_reactions = emojis;
            }
            EnvelopePayload::UpdateChatHasScheduledMessages {
                chat_id,
                has_scheduled_messages,
            } => self.set_chat_has_scheduled(chat_id, has_scheduled_messages),
            EnvelopePayload::UpdateChatReplyMarkup {
                chat_id,
                message_id,
                reply_markup,
            } => self.set_chat_reply_keyboard(chat_id, message_id, reply_markup),
            EnvelopePayload::UpdateChatMessageSender {
                chat_id,
                message_sender,
            } => self.set_chat_message_sender(chat_id, message_sender),
            EnvelopePayload::ChatMessageSenders { senders } => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.send_as_options.insert(chat_id.0, senders);
                }
            }
            EnvelopePayload::UpdateChatIsTranslatable {
                chat_id,
                is_translatable,
            } => self.set_chat_translatable(chat_id, is_translatable),
            // Slice G2: `getChatBoostStatus` answer (schema 1.8.67, line
            // 13917) — correlated via the pending request's `chat_id`.
            EnvelopePayload::ChatBoostStatus { level, boost_count } => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.chat_boost_status
                        .insert(chat_id.0, (level, boost_count));
                }
            }
            // Slice G2: `chatBoostSlots` (schema 1.8.67, line 6968) — the
            // `getAvailableChatBoostSlots` answer. Stashed per chat so the
            // driver's `boostChat` chain can consume it (see
            // `maybe_continue_boost` in connect.rs).
            EnvelopePayload::ChatBoostSlots { slots } => {
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
            EnvelopePayload::ChatStatistics { statistics } => {
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
            EnvelopePayload::ChatInviteLink { link } => {
                self.apply_chat_invite_link(link, pending, extra, seq)
            }
            // Phase D3a: `getChatInviteLinks` / `revokeChatInviteLink`
            // answer — replaces the cached list.
            EnvelopePayload::ChatInviteLinks { total_count, links } => {
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(RequestPurpose::GetChatInviteLinks | RequestPurpose::RevokeChatInviteLink)
                ) && let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                {
                    self.invite_links.insert(
                        chat_id.0,
                        InviteLinkFetch::Loaded(InviteLinkList { total_count, links }),
                    );
                }
            }
            // Phase D3a: `getChatJoinRequests` answer.
            EnvelopePayload::ChatJoinRequests {
                total_count,
                requests,
            } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatJoinRequests)
                    && let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                {
                    self.join_requests.insert(
                        chat_id.0,
                        JoinRequestFetch::Loaded(JoinRequestList {
                            total_count,
                            requests,
                        }),
                    );
                }
            }
            // Phase D3b: `getChatAdministrators` answer — replaces the
            // cached admin list.
            EnvelopePayload::ChatAdministrators { administrators } => {
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
            EnvelopePayload::SupergroupMembers {
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
                } else if let Some(RequestPurpose::GetSupergroupMembers { filter }) =
                    pending.map(|p| p.purpose)
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
            // B4: `getPollVoters` answer — a first page (offset 0)
            // replaces the cached list for this (chat, message, option);
            // a later page appends, deduped by sender, keeping server
            // order (the list is per-option; `option_id` is part of the
            // key so switching options refetches).
            EnvelopePayload::PollVoters {
                total_count,
                voters,
            } => self.apply_poll_voters(total_count, voters, pending, extra, seq),
            // Bots slice: `inlineQueryResults` — the `getInlineQueryResults`
            // answer (schema 1.8.67, line 7716). A first page replaces the
            // slot; a later page appends, deduped by result id, keeping
            // the new page's id/offset.
            // ponytail: rapid re-queries can let an older response land on
            // a newer slot — the response never echoes the query text, so
            // the slot keys on (chat, bot) only; the UI slice debounces
            // queries anyway.
            EnvelopePayload::InlineQueryResults(page) => {
                self.apply_inline_query_results(page, pending, extra, seq)
            }
            // Slice G1: `createNewBasicGroupChat` answer
            // (`createdBasicGroupChat`, schema 1.8.67, line 3644). The new
            // chat itself arrives as `updateNewChat`; nothing to cache.
            EnvelopePayload::CreatedBasicGroupChat { chat_id: _ } => {}
            // Slice G1: `addChatMembers` answer (`failedToAddMembers`,
            // schema 1.8.67, line 3640). Added members arrive as
            // `updateChatMember`; the failure count drives the notice in
            // the add-members dialog.
            // Slice G1: `getBasicGroupFullInfo` answer — replaces the
            // cached basic-group member list.
            EnvelopePayload::BasicGroupFullInfo { members } => {
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
            EnvelopePayload::FailedToAddMembers { failed_count } => {
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
            EnvelopePayload::ChatEvents { events } => {
                if let Some(pending) = pending
                    && let RequestPurpose::GetChatEventLog { from_event_id } = pending.purpose
                    && let Some(chat_id) = pending.chat_id
                {
                    let has_more = events.len() as i32 >= CHAT_EVENT_LOG_PAGE_SIZE;
                    let mut merged = match (from_event_id, self.event_logs.get(&chat_id.0)) {
                        (0, _) => events,
                        (_, Some(ChatEventLogFetch::Loaded(page))) => {
                            let mut merged = page.events.clone();
                            for event in events {
                                if !merged.iter().any(|old| old.id == event.id) {
                                    merged.push(event);
                                }
                            }
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
            EnvelopePayload::UpdateNewChatJoinRequest {
                chat_id, request, ..
            } => {
                if let Some(JoinRequestFetch::Loaded(mut list)) =
                    self.join_requests.get(&chat_id).cloned()
                    && !list
                        .requests
                        .iter()
                        .any(|existing| existing.user_id == request.user_id)
                {
                    list.requests.insert(0, request);
                    list.total_count = list.total_count.saturating_add(1);
                    self.join_requests
                        .insert(chat_id, JoinRequestFetch::Loaded(list));
                }
            }
            // Phase D3a: `updateChatPendingJoinRequests` (schema 1.8.67,
            // line 10555) — the badge count. The full list still needs
            // `getChatJoinRequests`.
            EnvelopePayload::UpdateChatPendingJoinRequests {
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
            // Batch 8: `updateChatActionBar` (schema 1.8.67, line 10526).
            EnvelopePayload::UpdateChatActionBar {
                chat_id,
                action_bar,
            } => self.set_chat_action_bar(chat_id.0, action_bar),
            EnvelopePayload::UpdateChatNotificationSettings {
                chat_id,
                notification_settings,
            } => {
                let chat = self
                    .chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id));
                // A placeholder chat (kind unknown — a server id with no
                // local chat, e.g. archived or never opened) has no
                // trustworthy scope: `scope_for_chat_kind` would always
                // guess GroupChats. Prune/invalidate across all scopes
                // instead of the guessed one.
                let kind_unknown = matches!(chat.kind, ChatKind::Unknown);
                let scope = scope_for_chat_kind(&chat.kind);
                let fully_default = notification_settings == ChatNotificationSettings::default();
                chat.notification_settings = notification_settings;
                // Parity slice: the exceptions list for the chat's scope is
                // stale now. A reset to the scope default (e.g. our own
                // "Reset to default") just prunes the chat from the cached
                // list; any other change drops the list so the next dialog
                // open refetches it.
                if fully_default {
                    if kind_unknown {
                        for list in self.notification_exceptions.values_mut() {
                            list.retain(|id| *id != chat_id.0);
                        }
                    } else if let Some(list) = self.notification_exceptions.get_mut(&scope) {
                        list.retain(|id| *id != chat_id.0);
                    }
                } else if kind_unknown {
                    self.notification_exceptions.clear();
                } else {
                    self.notification_exceptions.remove(&scope);
                }
            }
            // Slice CL1: `updateChatIsMarkedAsUnread` (schema 1.8.67,
            // line 10588) — the authoritative marked-as-unread flag; the
            // row shows the unread badge while set.
            EnvelopePayload::UpdateChatIsMarkedAsUnread {
                chat_id,
                is_marked_as_unread,
            } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .is_marked_as_unread = is_marked_as_unread;
            }
            // Phase B4: `updateChatMessageAutoDeleteTime` (schema 1.8.67,
            // line 10549) — keep the chat-level timer fresh. The same
            // change also lands in history as a
            // `messageChatSetMessageAutoDeleteTime` service row.
            EnvelopePayload::UpdateChatMessageAutoDeleteTime {
                chat_id,
                message_auto_delete_time,
            } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .message_auto_delete_time = message_auto_delete_time;
            }
            EnvelopePayload::UpdateChatAction {
                chat_id,
                sender,
                action,
            } => {
                let name = self.sender_first_name(sender);
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .set_sender_action(sender, action, name);
            }
            // Phase B1: secret chat lifecycle (schema 1.8.67, lines
            // 10741 / 2816). `updateSecretChat` may arrive before any
            // `updateNewChat`; `secretChat` is the `getSecretChat` answer.
            // Both funnel into `accept_secret_chat`.
            EnvelopePayload::UpdateSecretChat { secret_chat } => {
                self.accept_secret_chat(&secret_chat);
            }
            EnvelopePayload::SecretChat { secret_chat } => {
                self.accept_secret_chat(&secret_chat);
            }
            // Phase C1: call signaling (schema 1.8.67, lines 10816 /
            // 10862). `updateCall` drives the single-call state machine;
            // signaling data is queued as a diagnostic record and also fed
            // to the engine by the C2b driver bridge; `callId` is the answer
            // that starts tracking the outgoing call.
            EnvelopePayload::UpdateCall { call } => {
                self.accept_call_update(&call);
            }
            EnvelopePayload::UpdateNewCallSignalingData { call_id, data } => {
                self.accept_call_signaling_data(call_id, data);
            }
            EnvelopePayload::CallId { id } => self.apply_call_id(id, pending, extra, seq),
            // Phase C3a: `groupCallId` — the `createVideoChat` answer.
            // Queue a `getGroupCall` fetch so tracking starts even if
            // the `updateGroupCall` is delayed; the update remains the
            // source of truth.
            EnvelopePayload::GroupCallId { id } => {
                if let Some(RequestPurpose::CreateVideoChat { .. }) = pending.map(|p| p.purpose)
                    && !self.group_call_fetch_queue.contains(&id)
                {
                    self.group_call_fetch_queue.push(id);
                }
                self.group_call_error = None;
            }
            // Phase C2f: `groupCallInfo` — the `joinGroupCall`
            // answer to invitation acceptance. Queue a `getGroupCall`
            // fetch so tracking starts even if the `updateGroupCall`
            // is delayed; the update remains the source of truth for
            // `is_joined`. Store the tgcalls join payload like the
            // `joinVideoChat` Text arm does.
            EnvelopePayload::GroupCallInfo {
                group_call_id,
                join_payload,
            } => {
                if let Some(RequestPurpose::JoinGroupCallInvitation) = pending.map(|p| p.purpose) {
                    if !self.group_call_fetch_queue.contains(&group_call_id) {
                        self.group_call_fetch_queue.push(group_call_id);
                    }
                    self.set_group_call_join_payload(group_call_id, join_payload);
                }
                self.group_call_error = None;
            }
            // Phase C3a: group-call signaling (schema 1.8.67, lines
            // 10819 / 10824 / 10830 / 10836 / 10576). `updateGroupCall`
            // drives the tracked-call state; participant updates feed
            // the grid; the verification state feeds the E2E emoji UI;
            // `updateChatVideoChat` refreshes the chat's join affordance.
            // All signaling-only — no media transport until Phase C2.
            EnvelopePayload::UpdateGroupCall { group_call } => {
                self.accept_group_call_update(&group_call);
            }
            EnvelopePayload::UpdateGroupCallParticipant {
                group_call_id,
                participant,
            } => {
                self.accept_group_call_participant_update(group_call_id, &participant);
            }
            EnvelopePayload::UpdateGroupCallParticipants {
                group_call_id,
                participant_user_ids,
            } => {
                self.accept_group_call_participants_update(group_call_id, &participant_user_ids);
            }
            EnvelopePayload::UpdateGroupCallVerificationState {
                group_call_id,
                generation,
                emojis,
            } => {
                self.accept_group_call_verification_state(group_call_id, generation, &emojis);
            }
            EnvelopePayload::UpdateChatVideoChat {
                chat_id,
                video_chat,
            } => {
                self.accept_chat_video_chat(ChatId(chat_id), &video_chat);
            }
            // Phase C2h: in-call chat message updates.
            EnvelopePayload::UpdateNewGroupCallMessage {
                group_call_id,
                message,
            } => {
                self.accept_new_group_call_message(group_call_id, &message);
            }
            EnvelopePayload::UpdateGroupCallMessageSendFailed {
                group_call_id,
                message_id: _,
                error,
            } => {
                if self
                    .active_group_call
                    .as_ref()
                    .is_some_and(|c| c.id == group_call_id)
                {
                    self.group_call_error = Some(call_request_error_line(
                        &error,
                        "Could not send the message",
                    ));
                }
            }
            EnvelopePayload::UpdateGroupCallMessagesDeleted {
                group_call_id,
                message_ids,
            } => {
                self.accept_group_call_messages_deleted(group_call_id, &message_ids);
            }
            // Phase C2h: `rtmpUrl` — the `getVideoChatRtmpUrl` /
            // `replaceVideoChatRtmpUrl` answer. Stored on the tracked
            // call whose chat the request targeted.
            EnvelopePayload::RtmpUrl { url, stream_key } => {
                if let Some(
                    RequestPurpose::GetVideoChatRtmpUrl { chat_id }
                    | RequestPurpose::ReplaceVideoChatRtmpUrl { chat_id },
                ) = pending.map(|p| p.purpose)
                {
                    let call_id = self
                        .chats
                        .get(&chat_id)
                        .and_then(|c| c.video_chat.as_ref())
                        .map(|vc| vc.group_call_id);
                    if let (Some(call_id), Some(tracked)) =
                        (call_id, self.active_group_call.as_mut())
                        && tracked.id == call_id
                    {
                        tracked.rtmp_url = Some(url);
                        tracked.rtmp_stream_key = Some(stream_key);
                    }
                }
            }
            EnvelopePayload::UpdateChatTitle { chat_id, title } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .title = title;
            }
            EnvelopePayload::UpdateUnreadMessageCount {
                list,
                unread_count,
                unread_unmuted_count,
            } => {
                if std::env::var_os("QUILL_TRACE_STATUS").is_some() {
                    eprintln!(
                        "status: unread totals messages list={list:?} unread_count={unread_count} unread_unmuted_count={unread_unmuted_count}"
                    );
                }
                if let Some(totals) = self.unread_totals.list_mut(&list) {
                    totals.messages = Some(UnreadPair {
                        all: unread_count,
                        unmuted: unread_unmuted_count,
                    });
                }
            }
            EnvelopePayload::UpdateUnreadChatCount {
                list,
                unread_count,
                unread_unmuted_count,
                total_count,
                marked_as_unread_count,
                marked_as_unread_unmuted_count,
            } => {
                if std::env::var_os("QUILL_TRACE_STATUS").is_some() {
                    eprintln!(
                        "status: unread totals chats list={list:?} total_count={total_count} unread_count={unread_count} unread_unmuted_count={unread_unmuted_count} marked_as_unread_count={marked_as_unread_count} marked_as_unread_unmuted_count={marked_as_unread_unmuted_count}"
                    );
                }
                if let Some(totals) = self.unread_totals.list_mut(&list) {
                    totals.chats = Some(UnreadPair {
                        all: unread_count,
                        unmuted: unread_unmuted_count,
                    });
                }
            }
            EnvelopePayload::UpdateChatReadInbox {
                chat_id,
                last_read_inbox_message_id,
                unread_count,
            } => {
                let chat = self
                    .chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id));
                chat.last_read_inbox_message_id = last_read_inbox_message_id;
                chat.unread_count = unread_count;
                // Read here or on another device: whatever toast we showed
                // for the chat is stale (tdesktop `clearFromHistory`).
                if unread_count == 0 {
                    self.clear_chat_notifications(chat_id);
                }
            }
            // Slice CL3: mention / reaction badge counts (schema 1.8.67,
            // lines 10567/10570).
            EnvelopePayload::UpdateChatUnreadMentionCount {
                chat_id,
                unread_mention_count,
            } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .unread_mention_count = unread_mention_count;
            }
            EnvelopePayload::UpdateChatUnreadReactionCount {
                chat_id,
                unread_reaction_count,
            } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .unread_reaction_count = unread_reaction_count;
            }
            // B15: poll-vote badge count (schema 1.8.67, lines 10457/10573).
            EnvelopePayload::UpdateChatUnreadPollVoteCount {
                chat_id,
                unread_poll_vote_count,
            } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .unread_poll_vote_count = unread_poll_vote_count;
            }
            // B15: `getPollVoteStatistics` answer — cached per message.
            EnvelopePayload::PollVoteStatistics { graph } => {
                if let Some(RequestPurpose::GetPollVoteStatistics {
                    chat_id,
                    message_id,
                }) = pending.map(|p| p.purpose)
                {
                    self.poll_stats
                        .insert((chat_id.0, message_id.0), PollStatsFetch::Loaded(graph));
                }
            }
            EnvelopePayload::UpdateMessageUnreadReactions {
                chat_id,
                unread_reaction_count,
                newest,
                ..
            } => {
                let previous = self
                    .chats
                    .get(&chat_id.0)
                    .map_or(0, |chat| chat.unread_reaction_count);
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .unread_reaction_count = unread_reaction_count;
                // A grown counter with a visible newest reaction is a new
                // reaction on one of our messages.
                if unread_reaction_count > previous
                    && let Some(reaction) = newest
                {
                    self.queue_reaction_notification(chat_id, &reaction);
                }
            }
            // `updateNotificationGroup` / `updateActiveNotifications`: a
            // group that emptied (read elsewhere, or removed) clears the
            // OS notifications we showed for the chat.
            EnvelopePayload::UpdateNotificationGroup {
                chat_id,
                total_count,
                added_count,
                removed_count,
            } => {
                if total_count == 0 && added_count == 0 && removed_count > 0 {
                    self.clear_chat_notifications(chat_id);
                }
            }
            EnvelopePayload::UpdateActiveNotifications { chat_ids } => {
                // Notifications of a previous launch: remember the chats so
                // a later read clears them too.
                self.shown_notification_chats.extend(chat_ids);
            }
            // Slice CL3: `updateChatBlockList` (schema 1.8.67, line
            // 10594).
            EnvelopePayload::UpdateChatBlockList { chat_id, blocked } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .blocked = blocked;
            }
            // Slice CL3: `reportChat` result — surfaced as a status note
            // via the same drain as `chat_action_error`; a refusal or a
            // "more info required" is never shown as success.
            EnvelopePayload::ReportChatResult(outcome) => {
                match pending.map(|p| p.purpose) {
                    Some(RequestPurpose::ReportChat) => {
                        self.report_chat_outcome = Some(match outcome {
                            ReportChatOutcome::Ok => "chat reported".to_string(),
                            _ => "report needs a reason or messages — the chat list only sends simple spam reports".to_string(),
                        });
                    }
                    // The message menu's Report flow walks the answers.
                    Some(RequestPurpose::ReportMessages) => {
                        if let Some(pending) = pending {
                            self.accept_message_report(pending, outcome);
                        }
                    }
                    _ => {}
                }
            }
            // The message menu's "N Seen" / "Seen at" / "N Reacted" rows.
            EnvelopePayload::MessageViewers(viewers) => {
                if let Some(RequestPurpose::GetMessageViewers {
                    chat_id,
                    message_id,
                }) = pending.map(|p| p.purpose)
                {
                    self.accept_message_viewers(chat_id, message_id, viewers);
                }
            }
            EnvelopePayload::MessageReadDate(date) => {
                if let Some(RequestPurpose::GetMessageReadDate {
                    chat_id,
                    message_id,
                }) = pending.map(|p| p.purpose)
                {
                    self.accept_message_read_date(chat_id, message_id, date);
                }
            }
            EnvelopePayload::AddedReactions(page) => {
                if let Some(RequestPurpose::GetMessageAddedReactions {
                    chat_id,
                    message_id,
                }) = pending.map(|p| p.purpose)
                {
                    self.accept_added_reactions(chat_id, message_id, page);
                }
            }
            EnvelopePayload::UpdateChatReadOutbox {
                chat_id,
                last_read_outbox_message_id,
            } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .last_read_outbox_message_id = last_read_outbox_message_id;
            }
            // `updateChatAddedToList` / `updateChatRemovedFromList` track
            // `chat.chat_lists`, which is *not* list placement: "A chat can
            // have a non-zero position in a chat list even if it doesn't
            // belong to the chat list and have no position in a chat list
            // even if it belongs to the chat list" (schema 1.8.67, line
            // 3595). Rows come only from positions (`updateChatPosition` /
            // the full sets on last-message and draft updates), as in
            // Telegram X, which keeps `chat.chatLists` apart from
            // `chat.positions` (`Tdlib.updateChatAddedToList`).
            EnvelopePayload::UpdateChatAddedToList { .. }
            | EnvelopePayload::UpdateChatRemovedFromList { .. } => {}
            EnvelopePayload::UpdateChatLastMessage {
                chat_id,
                last_message,
                positions,
            } => {
                if let Some(ref message) = last_message {
                    self.remember_files(&message.files);
                }
                // Slice chatlist-list-style: the preview's style inputs
                // (media icon, formatted-text entities) and the 3-line
                // sender name are pure functions of the same content.
                self.set_chat_last_message(chat_id, last_message.as_ref());
                // `positions` is the full set of lists this chat belongs to.
                self.replace_main_list_from_positions(chat_id, &positions);
                self.rebuild_main_order();
            }
            EnvelopePayload::UpdateChatPosition(pos) => {
                self.apply_position_fields(pos);
                self.rebuild_main_order();
            }
            EnvelopePayload::UpdateChatFolders {
                folders,
                are_tags_enabled,
            } => {
                // The update carries the full ordered list — replace.
                self.chat_folders = folders;
                self.are_folder_tags_enabled = are_tags_enabled;
            }
            EnvelopePayload::ChatFolderInfo(info) => {
                // Parity slice: `createChatFolder` / `editChatFolder`
                // response — upsert into the tab list so the UI reflects the
                // change without waiting for `updateChatFolders` (which
                // stays the source of truth).
                match self.chat_folders.iter_mut().find(|f| f.id == info.id) {
                    Some(existing) => *existing = info,
                    None => self.chat_folders.push(info),
                }
            }
            EnvelopePayload::ChatFolder { spec } => {
                // Parity slice: `getChatFolder` response — cache the full
                // spec for the edit dialog prefill / remove-from-folder
                // chain (correlated via `PendingRequest::folder_id`).
                if let Some(folder_id) = pending.and_then(|p| p.folder_id) {
                    self.folder_specs.insert(folder_id, spec);
                }
            }
            EnvelopePayload::ChatLists { lists } => {
                // Parity slice: `getChatListsToAddChat` response — cache per
                // chat for the folder picker (correlated via
                // `PendingRequest::chat_id`).
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.chat_lists_for_add.insert(chat_id.0, lists);
                }
            }
            EnvelopePayload::UpdateChatActiveStories { active_stories } => {
                // Phase 9.1: keep the tray entry only for the main story
                // list; archived / hidden chats drop out of the tray.
                self.upsert_story_tray_entry(active_stories);
            }
            EnvelopePayload::ChatActiveStories { active_stories } => {
                // Phase 9.1: `getChatActiveStories` answer — refresh the
                // tray entry (matched by `@extra` in the UI's fetch guard,
                // but the object itself is authoritative).
                self.upsert_story_tray_entry(active_stories);
            }
            EnvelopePayload::Story { story, files } => {
                // Phase 9.1: `getStory` response or `updateStory` update.
                self.remember_files(&files);
                // Phase 9.3: a `postStory` answer is the pending story —
                // its id is the temporary id the succeeded/failed updates
                // correlate against.
                if pending.is_some_and(|p| p.purpose == RequestPurpose::PostStory) {
                    self.story_post.outcome = StoryPostOutcome::Posting { story_id: story.id };
                }
                self.stories.insert((story.poster_chat_id, story.id), story);
            }
            EnvelopePayload::StoryAlbums { albums } => {
                // Phase 9.7: `getChatStoryAlbums` — honored only for the
                // matching purpose (a stray `storyAlbums` never flips the
                // UI); replaces the chat's album list.
                if pending.is_some_and(|p| p.purpose == RequestPurpose::GetChatStoryAlbums)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.story_albums.insert(chat_id.0, albums);
                    self.clear_story_page_op(RequestPurpose::GetChatStoryAlbums);
                }
            }
            EnvelopePayload::StoryAlbum { album } => {
                self.apply_story_album(album, pending);
            }
            EnvelopePayload::Stories {
                total_count,
                stories,
                pinned_story_ids,
            } => self.apply_stories(total_count, stories, pinned_story_ids, pending, extra, seq),
            EnvelopePayload::CanPostStoryResult { result } => {
                // Phase 9.3: `canPostStory` answer — honored only for the
                // composer's own check (purpose-gated, so a stray result
                // never flips the UI).
                if pending.is_some_and(|p| p.purpose == RequestPurpose::CheckCanPostStory) {
                    self.story_post.eligibility = Some(result);
                    self.story_post.check_error = None;
                }
            }
            EnvelopePayload::UpdateStoryDeleted {
                poster_chat_id,
                story_id,
            } => {
                // Phase 9.2: drop the story from the cache and from the
                // poster's tray entry. The UI closes the viewer when its
                // current story disappears from the cache.
                self.stories.remove(&(poster_chat_id, story_id));
                let empty = if let Some(tray) = self.story_tray.get_mut(&poster_chat_id) {
                    tray.stories.retain(|info| info.story_id != story_id);
                    tray.stories.is_empty()
                } else {
                    false
                };
                if empty {
                    self.story_tray.remove(&poster_chat_id);
                }
            }
            EnvelopePayload::UpdateStoryPostSucceeded {
                story,
                files,
                old_story_id,
            } => {
                // Phase 9.2: a story posted from another client is live —
                // upsert it and refresh the poster's tray row so an own
                // story appears in the tray.
                self.remember_files(&files);
                let poster_chat_id = story.poster_chat_id;
                // Phase 9.3: our own pending post went live — the composer
                // shows "Posted".
                if matches!(
                    self.story_post.outcome,
                    StoryPostOutcome::Posting { story_id } if story_id == old_story_id
                ) {
                    self.story_post.outcome = StoryPostOutcome::Succeeded;
                }
                self.stories.insert((story.poster_chat_id, story.id), story);
                self.story_tray_refresh.insert(poster_chat_id);
            }
            EnvelopePayload::UpdateStoryPostFailed { story, error } => {
                // Phase 9.2: a story failed to post — drop it like a delete
                // (it never went live).
                // Phase 9.3: the failure reaches our own pending post (the
                // `sendStory` blocker was retracted — the constructor is
                // `postStory`, schema `td_api.tl:13715`) and the composer
                // shows it.
                if matches!(
                    self.story_post.outcome,
                    StoryPostOutcome::Posting { story_id } if story_id == story.id
                ) {
                    self.story_post.outcome = StoryPostOutcome::Failed(format!(
                        "Posting failed: {}",
                        error_reason(&error)
                    ));
                }
                self.stories.remove(&(story.poster_chat_id, story.id));
                let empty = if let Some(tray) = self.story_tray.get_mut(&story.poster_chat_id) {
                    tray.stories.retain(|info| info.story_id != story.id);
                    tray.stories.is_empty()
                } else {
                    false
                };
                if empty {
                    self.story_tray.remove(&story.poster_chat_id);
                }
            }
            EnvelopePayload::StoryAvailableReactions {
                reactions,
                recent,
                popular,
                allow_custom_emoji,
            } => {
                if let Some(pending) = pending
                    && let RequestPurpose::GetMessageAvailableReactions { message_id } =
                        pending.purpose
                    && let Some(chat_id) = pending.chat_id
                {
                    self.accept_message_reaction_options(
                        chat_id,
                        MessageId(message_id),
                        reactions,
                        recent,
                        popular,
                        allow_custom_emoji,
                    );
                } else {
                    // Phase 9.2: `getStoryAvailableReactions` answer — the
                    // viewer picker options.
                    self.story_available_reactions = Some(reactions);
                }
            }
            EnvelopePayload::StoryInteractions { interactions } => {
                // Phase 9.5: a `getStoryInteractions` page — honored only
                // for the viewer's own fetch (purpose-gated, and the page
                // is dropped when the viewer moved to another story).
                if pending.is_some_and(|p| p.purpose == RequestPurpose::GetStoryInteractions)
                    && let Some(pending) = pending
                {
                    self.accept_story_interactions(pending, interactions);
                }
            }
            EnvelopePayload::ReportStoryResult(result) => {
                // Phase 9.5: a `reportStory` answer — honored only for the
                // viewer's own report flow.
                if pending.is_some_and(|p| p.purpose == RequestPurpose::ReportStory)
                    && let Some(pending) = pending
                {
                    self.accept_story_report(pending, result);
                }
            }
            EnvelopePayload::UpdateStoryStealthMode {
                active_until_date,
                cooldown_until_date,
            } => {
                // Phase 9.5: stealth-mode state changed (Telegram X keeps
                // the same two timestamps; there is no getter, so updates
                // are the only source).
                self.apply_update_story_stealth_mode(active_until_date, cooldown_until_date);
            }
            EnvelopePayload::UpdatePendingMessage {
                chat_id,
                forum_topic_id,
                draft_id,
                can_stop,
                keep_on_stop,
                content,
                files,
            } => {
                if chat_id.0 != 0
                    && draft_id != 0
                    && forum_topic_id >= 0
                    && matches!(
                        content,
                        MessageContent::Text(_) | MessageContent::RichMessage(_)
                    )
                {
                    let stopped = self
                        .pending_bot_messages
                        .get(&(chat_id.0, forum_topic_id))
                        .is_some_and(|old| old.draft_id == draft_id && old.stopped);
                    self.remember_files(&files);
                    self.pending_bot_messages.insert(
                        (chat_id.0, forum_topic_id),
                        PendingBotMessage {
                            draft_id,
                            can_stop: can_stop && !stopped,
                            keep_on_stop,
                            content,
                            stop_failed: false,
                            stopped,
                            expires_at_ms: unix_ms_now()
                                .saturating_add(self.pending_bot_period_secs.saturating_mul(1000)),
                        },
                    );
                }
            }
            EnvelopePayload::UpdateStopMessageDraft {
                chat_id,
                forum_topic_id,
                draft_id,
            } => {
                self.finish_pending_bot_stop(chat_id, forum_topic_id, draft_id);
            }
            EnvelopePayload::UpdateNewMessage(message) => {
                self.note_forum_topic_message(&message);
                self.apply_update_new_message(message);
            }
            EnvelopePayload::UpdateMessageSendSucceeded {
                message,
                old_message_id,
            } => {
                let chat_id = message.chat_id;
                let topic_id = message.topic_id;
                self.note_last_message_send_state(
                    chat_id,
                    old_message_id,
                    message.id,
                    crate::telegram::envelope::MessageSendState::Sent,
                );
                self.remember_files(&message.files);
                let row = history_message(message, false);
                self.index_poll(&row);
                let history = self.histories.entry(chat_id.0).or_default();
                history.replace_id(old_message_id, row.clone());
                // Parity slice 4: the pending row in the topic's history
                // resolves the same way (the succeeded message carries its
                // topic).
                if let Some(topic_id) = topic_id
                    && let Some(topic_history) =
                        self.topic_histories.get_mut(&(chat_id.0, topic_id))
                {
                    topic_history.replace_id(old_message_id, row.clone());
                }
                if let Some(thread) = self.thread.as_mut()
                    && thread.chat_id == chat_id
                    && thread.history.messages.contains_key(&old_message_id.0)
                {
                    thread.history.replace_id(old_message_id, row);
                }
                self.draft_clears.push(chat_id);
            }
            EnvelopePayload::UpdateMessageSendFailed {
                message,
                old_message_id,
                error,
            } => {
                if let Some(notice) = error.send_permission_notice() {
                    self.send_permission_error = Some(notice.into());
                } else if let Some(notice) = error.flood_notice() {
                    // Q1: the failed row keeps its retry affordance
                    // (`can_retry` comes from TDLib, which marks rate
                    // limits retryable); the text stays in the history.
                    self.flood_notice = Some(notice);
                }
                let chat_id = message.chat_id;
                let topic_id = message.topic_id;
                self.note_last_message_send_state(
                    chat_id,
                    old_message_id,
                    message.id,
                    crate::telegram::envelope::MessageSendState::Failed,
                );
                self.remember_files(&message.files);
                // M1: mark the row failed. The retry affordance is gated
                // separately on `can_retry` (`resendMessages` via
                // `driver.resend_failed_message`) — not every failed send
                // may be retried.
                let mut row = history_message(message, true);
                row.failed = true;
                self.index_poll(&row);
                let history = self.histories.entry(chat_id.0).or_default();
                history.replace_id(old_message_id, row.clone());
                // Parity slice 4: the failed pending row shows in the topic
                // view too.
                if let Some(topic_id) = topic_id
                    && let Some(topic_history) =
                        self.topic_histories.get_mut(&(chat_id.0, topic_id))
                {
                    topic_history.replace_id(old_message_id, row.clone());
                }
                if let Some(thread) = self.thread.as_mut()
                    && thread.chat_id == chat_id
                    && thread.history.messages.contains_key(&old_message_id.0)
                {
                    thread.history.replace_id(old_message_id, row);
                }
            }
            EnvelopePayload::UpdateMessageSendAcknowledged { .. } => {
                // Not success. Keep the pending row until Succeeded/Failed.
            }
            EnvelopePayload::UpdateMessageInteractionInfo {
                chat_id,
                message_id,
                interaction_info,
            } => {
                self.edit_loaded_message(chat_id, message_id, |message| {
                    message.interaction_info = interaction_info.clone();
                });
                self.sync_thread_reply_info(
                    chat_id,
                    message_id,
                    interaction_info
                        .as_ref()
                        .and_then(|info| info.reply_info.as_ref()),
                );
            }
            EnvelopePayload::UpdateMessageIsPinned {
                chat_id,
                message_id,
                is_pinned,
            } => {
                self.edit_loaded_message(chat_id, message_id, |message| {
                    message.is_pinned = is_pinned;
                });
                // An unpin leaves the pinned list at once; a pin is added
                // by the refetch the driver sends for the open chat.
                if !is_pinned && let Some(list) = self.pinned_messages.get_mut(&chat_id.0) {
                    list.retain(|message| message.id != message_id);
                }
            }
            EnvelopePayload::UpdateMessageContentOpened {
                chat_id,
                message_id,
            } => {
                self.edit_loaded_message(chat_id, message_id, |message| {
                    message.content.mark_content_opened();
                });
            }
            EnvelopePayload::UpdateMessageEdited {
                chat_id,
                message_id,
                edit_date,
                reply_markup,
            } => {
                // Phase 3.2: bots edit inline keyboards via `updateMessageEdited`
                // (schema 1.8.67 line 10431) — the new `reply_markup` (possibly
                // None) replaces the message's keyboard.
                // The same update stamps the edit date shown as "edited".
                self.edit_loaded_message(chat_id, message_id, |message| {
                    message.reply_markup = reply_markup.clone();
                    message.extras.edit_date = edit_date;
                });
            }
            EnvelopePayload::UpdatePoll { poll } => {
                // Phase 4.2: `updatePoll` (schema 1.8.67 line 11179) carries
                // only the new `poll` — no chat or message id — so its rows
                // are found through the poll-id index and the poll is
                // replaced in place.
                self.apply_update_poll(poll);
            }
            EnvelopePayload::UpdateMessageContent {
                chat_id,
                message_id,
                content,
                files,
            } => {
                self.apply_update_message_content(chat_id, message_id, content, files);
            }
            EnvelopePayload::UpdateMessageEphemeralContent {
                chat_id,
                message_id,
                ephemeral,
            } => self.apply_update_message_ephemeral_content(
                chat_id, message_id, ephemeral, pending, extra, seq,
            ),
            EnvelopePayload::UpdateDeleteMessages {
                chat_id,
                message_ids,
                is_permanent,
                from_cache,
            } => {
                // `from_cache`: TDLib only dropped its in-memory copy; the
                // messages "can possibly be retrieved again" (schema 1.8.67,
                // line 10699). Telegram X ignores these
                // (`Tdlib.updateMessagesDeleted`); dropping the rows here
                // would punch holes that oldest-first paging never refills.
                let message_ids = if from_cache && !is_permanent {
                    Vec::new()
                } else {
                    message_ids
                };
                if let Some(list) = self.pinned_messages.get_mut(&chat_id.0) {
                    list.retain(|message| !message_ids.contains(&message.id));
                }
                let history = self.histories.entry(chat_id.0).or_default();
                for id in message_ids.iter().copied() {
                    // Permanent or "became inaccessible": either way the
                    // row must not come back from a stale page.
                    history.remove(id, true);
                    if is_permanent
                        && matches!(
                            self.chat_search.jump,
                            ChatSearchJump::Ready { message_id }
                                | ChatSearchJump::Loading { message_id }
                                if message_id == id
                        )
                    {
                        self.chat_search.jump = ChatSearchJump::Missing { message_id: id };
                    }
                }
                // Parity slice 4: the topic view reads only
                // `topic_histories`, so deletions must reach its rows too.
                for ((topic_chat_id, _), topic) in self.topic_histories.iter_mut() {
                    if *topic_chat_id == chat_id.0 {
                        for id in &message_ids {
                            topic.messages.remove(&id.0);
                        }
                    }
                }
                self.thread_remove(chat_id, &message_ids);
            }
            EnvelopePayload::Chats { chat_ids, .. } => {
                self.apply_chats(chat_ids, pending);
            }
            EnvelopePayload::ChatPhotos {
                total_count,
                photos,
            } => {
                if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::GetUserProfilePhotos
                {
                    self.apply_profile_photos(total_count, photos, pending);
                }
            }
            EnvelopePayload::FoundMessages {
                messages,
                next_offset,
                ..
            } => {
                if self.search.matches_generation(pending)
                    && matches!(
                        pending.map(|p| p.purpose),
                        Some(
                            RequestPurpose::SearchMessages
                                | RequestPurpose::SearchPublicMessagesByTag
                        )
                    )
                {
                    for message in &messages {
                        self.remember_files(&message.files);
                    }
                    let hits = messages.iter().map(SearchMessageHit::from_parsed).collect();
                    self.search.accept_messages(hits, false);
                }
                // Phase C2i: `searchCallMessages` pages for the
                // Recent-calls tab. `searchCallMessages` returns call and
                // group-call messages newest-first; the rows render both.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::SearchCallMessages) {
                    for message in &messages {
                        self.remember_files(&message.files);
                    }
                    self.recent_calls.extend(messages);
                    self.recent_calls_offset = next_offset;
                    self.recent_calls_loading = false;
                    self.recent_calls_error = false;
                }
            }
            // `searchPublicPosts` answer: posts of public channels; an
            // exhausted free quota is flagged, never paid for.
            EnvelopePayload::FoundPublicPosts {
                messages,
                are_limits_exceeded,
                ..
            } => {
                if self.search.matches_generation(pending)
                    && pending.map(|p| p.purpose) == Some(RequestPurpose::SearchPublicPosts)
                {
                    for message in &messages {
                        self.remember_files(&message.files);
                    }
                    let hits = messages.iter().map(SearchMessageHit::from_parsed).collect();
                    self.search.public_limits_exceeded = are_limits_exceeded;
                    self.search.accept_messages(hits, false);
                }
            }
            // Phase C2i: `getUserPrivacySettingRules` answer — map the
            // rule list to the simple Everybody / Contacts / Nobody
            // choice (`None` when the account has custom rules the UI
            // cannot represent; the radios then show nothing selected).
            // Slice S3: the Privacy-screen keys keep the full parsed
            // detail (exception lists) instead.
            EnvelopePayload::UserPrivacySettingRules { rules } => {
                if let Some(purpose) = pending.map(|p| p.purpose) {
                    match purpose {
                        RequestPurpose::GetCallPrivacyRules { setting } => {
                            let names: Vec<String> = rules.iter().map(|r| r.name.clone()).collect();
                            let who = PrivacyWho::from_rule_names(&names);
                            match setting {
                                CallPrivacySetting::AllowCalls => {
                                    self.call_privacy_allow_calls = who
                                }
                                CallPrivacySetting::PeerToPeer => self.call_privacy_p2p = who,
                            }
                            self.privacy_roundtrip_done();
                        }
                        RequestPurpose::GetPrivacyRules { key } => {
                            self.privacy.insert(
                                key,
                                PrivacyKeyState::Ready(PrivacyRuleDetail::from_rules(&rules)),
                            );
                        }
                        _ => {}
                    }
                }
            }
            // Slice S3: `updateUserPrivacySettingRules` (:10871) — rules
            // changed on another device; only refresh keys the Privacy
            // screen (or the C2i calls UI) owns.
            EnvelopePayload::UpdateUserPrivacySettingRules { setting, rules } => {
                let detail = PrivacyRuleDetail::from_rules(&rules);
                if let Some(key) = PrivacySettingKey::all()
                    .into_iter()
                    .find(|k| k.td_type() == setting)
                {
                    self.privacy.insert(key, PrivacyKeyState::Ready(detail));
                } else if setting == CallPrivacySetting::AllowCalls.td_type() {
                    let names: Vec<String> = rules.iter().map(|r| r.name.clone()).collect();
                    self.call_privacy_allow_calls = PrivacyWho::from_rule_names(&names);
                } else if setting == CallPrivacySetting::PeerToPeer.td_type() {
                    let names: Vec<String> = rules.iter().map(|r| r.name.clone()).collect();
                    self.call_privacy_p2p = PrivacyWho::from_rule_names(&names);
                }
            }
            // Slice S3: `readDatePrivacySettings` answer.
            EnvelopePayload::ReadDatePrivacySettings { show_read_date } => {
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(RequestPurpose::GetReadDatePrivacy)
                ) {
                    self.read_date_show = Some(show_read_date);
                    self.read_date_loading = false;
                    self.read_date_error = false;
                }
            }
            // Slice S3: `messageSenders` answer — one blocked-senders
            // page. Later pages append; a refetch restarts at 0.
            EnvelopePayload::BlockedMessageSenders {
                total_count,
                sender_ids,
            } => {
                if let Some(RequestPurpose::GetBlockedSenders { offset }) =
                    pending.map(|p| p.purpose)
                {
                    self.blocked_total = total_count;
                    if offset == 0 {
                        self.blocked_senders = Some(sender_ids);
                    } else if let Some(list) = self.blocked_senders.as_mut() {
                        list.extend(sender_ids);
                    }
                    self.blocked_loading = false;
                    self.blocked_error = false;
                }
            }
            EnvelopePayload::Count { count } => {
                if let Some(RequestPurpose::GetChatMessageCount { filter }) =
                    pending.map(|p| p.purpose)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.chat_media_counts
                        .entry(chat_id.0)
                        .or_default()
                        .insert(filter, count);
                }
            }
            EnvelopePayload::MessageCalendar { days, .. } => {
                self.apply_message_calendar(days, pending);
            }
            EnvelopePayload::FoundChatMessages {
                messages,
                total_count,
                next_from_message_id,
            } => self.apply_found_chat_messages(
                messages,
                total_count,
                next_from_message_id,
                pending,
                extra,
                seq,
            ),
            // Phase 5.1: `supergroup.is_forum` via `updateSupergroup` (an
            // update — applies whenever it arrives) or the `getSupergroup`
            // response (gated on the pending purpose). Parity slice: the
            // first active username is cached alongside, for the
            // channel/supergroup header.
            EnvelopePayload::UpdateBasicGroup {
                basic_group_id,
                member_count,
                status,
                can_change_info,
                is_active,
            } => {
                self.basic_group_member_counts
                    .insert(basic_group_id, member_count);
                self.basic_group_status.insert(basic_group_id, status);
                self.basic_group_change_info_right
                    .insert(basic_group_id, can_change_info.unwrap_or(false));
                self.basic_group_active.insert(basic_group_id, is_active);
            }
            EnvelopePayload::UpdateChatOnlineMemberCount {
                chat_id,
                online_member_count,
            } => {
                self.chat_online_counts.insert(chat_id, online_member_count);
            }
            EnvelopePayload::UpdateSupergroup {
                supergroup_id,
                verification,
                member_count,
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
            EnvelopePayload::Supergroup {
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
            // Phase 5.1: `getForumTopics` response — cache the first page
            // against the requesting chat.
            EnvelopePayload::ForumTopics {
                total_count: _,
                topics,
            } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetForumTopics)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    let mut topics = topics;
                    for topic in &mut topics {
                        super::session_subsection_tabs::settle_topic_unread(topic);
                        super::session_subsection_tabs::trace_topic_unread(
                            "getForumTopics",
                            chat_id.0,
                            topic,
                        );
                    }
                    self.forum_topics.insert(chat_id.0, topics);
                }
            }
            // Slice G2: `createForumTopic` answers `forumTopicInfo` —
            // drop the cached topic list so the UI refetches it.
            EnvelopePayload::ForumTopic { chat_id } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::CreateForumTopic) {
                    self.forum_topics.remove(&chat_id);
                }
            }
            EnvelopePayload::UpdateForumTopicInfo(info) => {
                self.apply_update_forum_topic_info(info);
            }
            EnvelopePayload::UpdateForumTopic(update) => {
                self.apply_update_forum_topic(update);
            }
            EnvelopePayload::ForumTopicAnswer(topic) => {
                if let Some(pending) = pending
                    && matches!(pending.purpose, RequestPurpose::GetForumTopic { .. })
                    && let Some(chat_id) = pending.chat_id
                {
                    self.replace_forum_topic(chat_id, topic);
                }
            }
            EnvelopePayload::Messages(messages) => {
                self.apply_messages(messages, pending, extra, seq)
            }
            EnvelopePayload::MessageThreadInfo(info) => {
                self.apply_message_thread_info(*info, pending);
            }
            EnvelopePayload::Message(message) => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatReplyMarkupMessage) {
                    self.set_chat_reply_keyboard(
                        message.chat_id,
                        Some(message.id),
                        message.reply_markup.clone(),
                    );
                    return;
                }
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatMessageByDate) {
                    self.accept_date_message(message.id);
                    return;
                }
                if let Some(RequestPurpose::GetRepliedMessage {
                    chat_id,
                    message_id,
                }) = pending.map(|p| p.purpose)
                {
                    self.accept_replied_message(chat_id, message_id, message);
                    return;
                }
                // M1 fix-up: editing a scheduled send returns the edited
                // `message` with `scheduling_state` set — refresh the
                // scheduled-list entry instead of inserting a phantom row
                // into chat history (which also left the scheduled list
                // showing the stale pre-edit text).
                if pending.map(|p| p.purpose) == Some(RequestPurpose::EditMessage)
                    && message.scheduling_state.is_some()
                {
                    self.remember_files(&message.files);
                    if let Some(slot) = self
                        .scheduled_messages
                        .iter_mut()
                        .find(|m| m.id == message.id)
                    {
                        *slot = message;
                    } else {
                        self.scheduled_messages.push(message);
                    }
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::SendMessage) {
                    self.upsert_message(message, true);
                } else {
                    self.upsert_message(message, false);
                }
            }
            EnvelopePayload::UpdateFile(file) | EnvelopePayload::File(file) => {
                self.upsert_file(file, true);
            }
            EnvelopePayload::UpdateFileDownload {
                file_id,
                is_paused,
                complete_date,
            } => {
                // Slice media-downloads-pause: the list API's pause/completion
                // channel. Pause state is tracked only for user-initiated
                // (listed) downloads; completion mirrors the `updateFile`
                // path (recent list + unstick).
                // The idle file update can precede the list's pause event.
                // A later authoritative list update restores that transfer.
                if (is_paused || complete_date != 0) && self.failed_downloads.remove(&file_id) {
                    self.user_downloads.insert(file_id);
                    self.downloading.insert(file_id);
                }
                if complete_date != 0 {
                    self.record_completed_user_download(file_id);
                    self.unstick_download(file_id);
                } else if self.user_downloads.contains(&file_id) {
                    if is_paused {
                        self.paused_downloads.insert(file_id);
                    } else {
                        self.paused_downloads.remove(&file_id);
                    }
                }
            }
            EnvelopePayload::StickerSets { sets, .. } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetInstalledStickerSets) {
                    self.accept_installed_sticker_sets(sets);
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::GetArchivedStickerSets)
                {
                    self.accept_archived_sticker_sets(sets);
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::SearchStickerSets) {
                    // Slice S8: `searchStickerSets` answers with `stickerSets`.
                    self.accept_found_sticker_sets(sets);
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::GetInstalledEmojiSets)
                {
                    // Slice S10: emoji `stickerSets` land in the emoji panel (see emoji.rs).
                    self.accept_installed_emoji_sets(sets);
                } else if let Some(RequestPurpose::GetArchivedEmojiSets { first_page }) =
                    pending.map(|p| p.purpose)
                {
                    self.accept_archived_emoji_sets(sets, first_page);
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::SearchEmojiSets) {
                    self.accept_found_emoji_sets(sets);
                }
            }
            // Slice S15: `updateInstalledStickerSets` — TDLib's authoritative
            // new order after a usage-driven (sticker send with
            // `update_order_of_installed_sticker_sets`) or manual reorder.
            // Applied in place so an open panel reshuffles live; an empty
            // cache is a no-op and the next fetch arrives ordered.
            EnvelopePayload::UpdateInstalledStickerSets {
                sticker_set_ids,
                is_regular,
            } => {
                self.apply_installed_sticker_set_order(&sticker_set_ids, is_regular);
            }
            // Slice S8: `getTrendingStickerSets` answers with
            // `trendingStickerSets`.
            EnvelopePayload::TrendingStickerSets {
                sets,
                is_premium,
                total_count,
            } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetTrendingStickerSets) {
                    self.stickers.trending_total = total_count.max(0) as usize;
                    self.accept_trending_sticker_sets(sets, is_premium);
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::GetTrendingEmojiSets) {
                    // Slice S10: emoji `trendingStickerSets` land in the emoji panel (see emoji.rs).
                    self.emoji.trending_total = total_count;
                    self.accept_trending_emoji_sets(sets, is_premium);
                }
            }
            // Slice S8: `searchStickers` / `getFavoriteStickers` /
            // `getRecentStickers` answer with bare `stickers`.
            EnvelopePayload::Stickers { stickers, files } => {
                self.remember_files(&files);
                let purpose = pending.map(|p| p.purpose);
                if purpose == Some(RequestPurpose::SearchStickers) {
                    self.accept_found_stickers(stickers);
                } else if purpose == Some(RequestPurpose::SuggestStickers) {
                    // Slice S12: composer suggestions land in their own
                    // slot — never the search UI's `found_stickers`.
                    self.accept_sticker_suggestions(stickers);
                } else if purpose == Some(RequestPurpose::GetFavoriteStickers) {
                    self.accept_favorite_stickers(stickers);
                } else if purpose == Some(RequestPurpose::GetRecentStickers) {
                    self.accept_recent_stickers(stickers);
                } else if purpose == Some(RequestPurpose::GetCustomEmojiStickers) {
                    // Slice S10: bare `stickers` land in the emoji panel (see emoji.rs).
                    self.accept_custom_emoji_stickers(stickers);
                } else if purpose == Some(RequestPurpose::GetStoryCustomEmojiStickers) {
                    // Phase 9.2+: story reaction picker visuals — keyed by
                    // sticker id (= custom emoji id).
                    self.accept_story_custom_emoji_stickers(stickers);
                }
            }
            // Slice S10: emoji payloads — purpose-gated dispatch lives in emoji.rs.
            payload @ (EnvelopePayload::EmojiStatuses { .. }
            | EnvelopePayload::EmojiStatusCustomEmojis { .. }
            | EnvelopePayload::AnimatedEmoji { .. }
            | EnvelopePayload::EmojiKeywords { .. }
            | EnvelopePayload::EmojiCategories { .. }) => {
                self.dispatch_emoji_payload(pending.map(|p| p.purpose), payload);
            }
            EnvelopePayload::UpdateStickerSet {
                id,
                is_custom_emoji,
            } => {
                if is_custom_emoji && id > 0 {
                    self.emoji.outdated_packs.insert(id);
                    if self.emoji.selected_set_id == Some(id) {
                        drop(self.requests.take_purpose(RequestPurpose::GetEmojiSet));
                    }
                }
            }
            EnvelopePayload::StickerSet {
                id,
                stickers,
                files,
                title,
                is_installed,
                ..
            } => {
                if let Some(RequestPurpose::DeepLinkResolve { generation }) =
                    pending.map(|p| p.purpose)
                    && matches!(
                        &self.deep_link,
                        Some(DeepLinkState::ResolvingChat {
                            action: DeepLinkAction::StickerSet { .. },
                            generation: slot,
                        }) if *slot == generation
                    )
                {
                    // `addstickers` / `addemoji` link: hand the set id to the
                    // preview dialog (`getStickerSet` loads its stickers).
                    self.deep_link = Some(DeepLinkState::Ui(
                        crate::deep_link_types::DeepLinkUi::StickerSet { set_id: id },
                    ));
                } else if let Some(RequestPurpose::ViewStickerSet { set_id }) =
                    pending.map(|p| p.purpose)
                {
                    self.remember_files(&files);
                    self.accept_sticker_set_view(set_id, title, is_installed, stickers);
                } else if let Some(RequestPurpose::LoadLibrarySet { set_id }) =
                    pending.map(|p| p.purpose)
                {
                    self.remember_files(&files);
                    self.accept_library_set(set_id, stickers);
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::GetEmojiSet)
                    && self.emoji.selected_set_id == Some(id)
                {
                    self.remember_files(&files);
                    self.emoji.failed = false;
                    self.emoji.preview_title = title;
                    self.emoji
                        .pack_files
                        .insert(id, stickers.iter().map(|item| item.file_id).collect());
                    self.emoji.outdated_packs.remove(&id);
                    self.emoji.preview = stickers;
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::GetStickerSet) {
                    self.remember_files(&files);
                    self.accept_sticker_set(id, stickers);
                }
            }
            EnvelopePayload::Animations { animations, files } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetSavedAnimations) {
                    self.remember_files(&files);
                    self.accept_saved_animations(animations);
                }
            }
            EnvelopePayload::SponsoredMessages {
                messages,
                files,
                messages_between,
            } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatSponsoredMessages)
                    && let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                {
                    if self.open_chat != Some(chat_id) {
                        self.diagnostics.record(Diagnostic {
                            category: "reducer",
                            type_name: Some("sponsoredMessages".into()),
                            extra: Some(pending.id.0),
                            seq: Some(seq),
                            note: "stale-chat-sponsored",
                        });
                        return;
                    }
                    self.accept_sponsored_messages(chat_id, messages, messages_between, &files);
                }
            }
            EnvelopePayload::ReportSponsoredResult(result) => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::ReportChatSponsoredMessage)
                    && let Some(pending) = pending
                {
                    self.accept_sponsored_report(pending, result);
                }
            }
            EnvelopePayload::Me { user_id } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetMe) {
                    self.my_user_id = Some(user_id);
                } else if let Some(RequestPurpose::DeepLinkResolve { generation }) =
                    pending.map(|p| p.purpose)
                    && let Some(DeepLinkState::ResolvingChat {
                        action: DeepLinkAction::UserPhone { draft, .. },
                        generation: slot,
                    }) = self.deep_link.clone()
                    && slot == generation
                {
                    // `searchUserByPhoneNumber` answer: open the private chat.
                    self.deep_link = Some(DeepLinkState::Info {
                        text: String::new(),
                        need_update: false,
                        action: Some(DeepLinkAction::OpenUserDraft { user_id, draft }),
                        generation,
                    });
                }
            }
            EnvelopePayload::InternalLinkType(link) => {
                if let Some(RequestPurpose::DeepLinkInternalType { generation }) =
                    pending.map(|p| p.purpose)
                    && matches!(
                        &self.deep_link,
                        Some(DeepLinkState::ResolvingInfo { generation: slot }) if *slot == generation
                    )
                {
                    use crate::deep_link_types::{LinkRoute, route};
                    let original = std::mem::take(&mut self.deep_link_original);
                    self.deep_link = Some(match route(&link, &original) {
                        LinkRoute::Resolve(action) => DeepLinkState::Info {
                            text: String::new(),
                            need_update: false,
                            action: Some(action),
                            generation,
                        },
                        LinkRoute::Ui(ui) => DeepLinkState::Ui(ui),
                        LinkRoute::Unknown(link) => DeepLinkState::Unknown { link },
                        LinkRoute::Message(text) => DeepLinkState::ShowText(text),
                    });
                }
            }
            EnvelopePayload::MessageLinkInfo {
                chat_id,
                message_id,
                media_timestamp,
                thread_id,
            } => {
                if let Some(RequestPurpose::DeepLinkResolve { generation }) =
                    pending.map(|p| p.purpose)
                    && matches!(
                        &self.deep_link,
                        Some(DeepLinkState::ResolvingChat {
                            action: DeepLinkAction::MessageLink { .. },
                            generation: slot,
                        }) if *slot == generation
                    )
                {
                    self.deep_link = Some(if chat_id == 0 {
                        DeepLinkState::ShowText(
                            "This message is in a chat you can't see. Join it first.".into(),
                        )
                    } else {
                        DeepLinkState::Info {
                            text: String::new(),
                            need_update: false,
                            action: Some(DeepLinkAction::OpenChatById {
                                chat_id,
                                message_id,
                                media_timestamp,
                                thread_id,
                            }),
                            generation,
                        }
                    });
                }
            }
            EnvelopePayload::ChatMember { member } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatMember)
                    && let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                {
                    self.accept_own_chat_member(chat_id, member.clone());
                }
                // Phase D3b: `getChatMember` for one administrator's rights
                // (edit dialog). Only an administrator status carries a
                // rights block worth caching.
                if let Some(RequestPurpose::GetAdminRights { user_id }) = pending.map(|p| p.purpose)
                    && let Some(pending) = pending
                    && let Some(chat_id) = pending.chat_id
                    && let Some(rights) = member.admin_rights
                {
                    self.admin_rights
                        .insert((chat_id.0, user_id), AdminRightsFetch::Loaded(rights));
                }
            }
            EnvelopePayload::UserFullInfo {
                extras,
                bot_info,
                bio,
                photo,
                photo_id,
                blocked,
            } => self.apply_user_full_info(
                bot_info, bio, photo, photo_id, blocked, extras, pending, extra, seq,
            ),
            EnvelopePayload::UpdateUserFullInfo {
                user_id,
                extras,
                bot_info,
                bio,
                photo,
                photo_id,
                blocked,
            } => {
                self.bot_info.insert(user_id.0, bot_info);
                let photo_file_id = photo.map(|file| {
                    let id = file.id.0;
                    self.upsert_file(file, false);
                    id
                });
                self.user_full_infos.insert(
                    user_id.0,
                    UserFullInfoData {
                        bio,
                        photo_file_id,
                        photo_id,
                        blocked,
                        extras,
                    },
                );
            }
            EnvelopePayload::BotCommands {
                bot_user_id,
                commands,
            } => {
                // Phase 3.3: `getCommands` response — cache the global-scope
                // commands for the bot. Only answers to our own fetch are
                // cached (matched by `@extra`); a user session gets an
                // `error` instead of `botCommands` (schema: "for bots
                // only"), recorded as an empty set by the `Error` arm.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetCommands) {
                    self.bot_commands.insert(bot_user_id.0, commands);
                }
            }
            EnvelopePayload::UpdateChatMember { chat_id, member } => {
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
            EnvelopePayload::JoinChatResult(result) => {
                // `parity:platform-deep-links`: `joinChatByInviteLink`
                // answer for a deep-link invite. Success opens the chat;
                // the other variants surface as an honest note.
                if let Some(RequestPurpose::DeepLinkJoin { generation }) =
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
            EnvelopePayload::ChatInviteLinkInfo {
                title,
                member_count,
                creates_join_request,
                is_channel,
            } => {
                if let Some(RequestPurpose::DeepLinkCheckInvite { generation }) =
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
            // `parity:platform-deep-links`: `getDeepLinkInfo` answer. The
            // actionable destination is parsed from the `textEntityTypeTextUrl`
            // entities; the generation guard drops stale answers. The UI
            // consumes `Info` once (follow-up request, or a dialog with
            // `text` when there is no action / an update is required).
            EnvelopePayload::DeepLinkInfo {
                text,
                need_update,
                entities,
            } => {
                if let Some(RequestPurpose::DeepLinkInfo { generation }) =
                    pending.map(|p| p.purpose)
                    && matches!(
                        &self.deep_link,
                        Some(DeepLinkState::ResolvingInfo {
                            generation: slot
                        }) if *slot == generation
                    )
                {
                    let action = crate::connect::parse_deep_link_action(&entities);
                    self.deep_link = Some(DeepLinkState::Info {
                        text,
                        need_update,
                        action,
                        generation,
                    });
                }
            }
            EnvelopePayload::CallbackQueryAnswer(answer) => {
                // `getCallbackQueryAnswer` response: only answers to our own
                // inline-button presses are surfaced (matched by `@extra`).
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(
                        RequestPurpose::GetCallbackQueryAnswer
                            | RequestPurpose::GetCallbackQueryAnswerWithPassword
                            | RequestPurpose::GetCallbackQueryAnswerGame
                    )
                ) {
                    self.last_callback_answer = Some(answer);
                }
            }
            EnvelopePayload::GameHighScores(scores) => {
                // Slice bots-games: `getGameHighScores` answer to our own
                // Scores press (matched by `@extra`). The message id rides
                // `around_message_id` (`request_for_message`); the panel
                // flips from its loading row to the rows.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetGameHighScores)
                    && let Some(pending) = pending
                    && let (Some(chat_id), Some(message_id)) =
                        (pending.chat_id, pending.around_message_id)
                {
                    // Guard: a panel the user closed while the answer was in
                    // flight must stay closed — only fill the loading entry.
                    if self.game_scores.contains_key(&(chat_id.0, message_id.0)) {
                        self.game_scores
                            .insert((chat_id.0, message_id.0), Some(scores));
                    }
                }
            }
            EnvelopePayload::LoginUrlInfo(info) => {
                // B1: `getLoginUrlInfo` response to our own login-button
                // press (matched by `@extra`).
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetLoginUrlInfo) {
                    self.last_login_url_info = Some(info);
                }
            }
            EnvelopePayload::PaymentForm(form) => {
                // Slice P1: `getPaymentForm` answer to our own Buy press
                // (matched by `@extra`). Opens the checkout dialog.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetPaymentForm) {
                    self.payment_form = Some(form);
                    self.payment_form_loading = false;
                    self.payment_validated = None;
                    self.payment_shipping_id = None;
                    self.payment_note = None;
                }
            }
            EnvelopePayload::ValidatedOrderInfo(validated) => {
                // Slice P1: `validateOrderInfo` answer (matched by `@extra`).
                // The first shipping option is pre-selected, like the
                // official clients.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::ValidateOrderInfo) {
                    self.payment_shipping_id =
                        validated.shipping_options.first().map(|o| o.id.clone());
                    self.payment_validated = Some(validated);
                    self.payment_note = None;
                }
            }
            EnvelopePayload::PaymentResult(result) => {
                // Slice P1: `sendPaymentForm` answer (matched by `@extra`).
                if pending.map(|p| p.purpose) == Some(RequestPurpose::SendPaymentForm) {
                    self.payment_sending = false;
                    if result.success {
                        self.payment_note = Some("✅ Payment successful".to_string());
                    } else if !result.verification_url.is_empty() {
                        // Schema: the URL is for additional payment
                        // credentials verification (e.g. 3-D Secure) — the
                        // UI opens it in the OS browser.
                        self.payment_verification_url = Some(result.verification_url);
                    } else {
                        self.payment_note = Some("Payment failed".to_string());
                    }
                }
            }
            EnvelopePayload::MarketplaceGift(quote) => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetMarketplaceGift)
                    && let Some(gift) = self.marketplace_gift.as_mut()
                {
                    gift.loading = false;
                    if let Some(quote) = quote.filter(|q| q.name == gift.requested_name) {
                        gift.price = quote.stars.or(quote.ton);
                        gift.note = gift
                            .price
                            .is_none()
                            .then(|| "This gift is not available for resale.".into());
                        gift.quote = Some(quote);
                    } else {
                        gift.note =
                            Some("Could not verify the returned gift. Load the gift again.".into());
                    }
                }
            }
            EnvelopePayload::GiftTextLimit(limit) => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetGiftTextLimit) {
                    self.gift_text_length_max = usize::try_from(limit).ok();
                }
            }
            EnvelopePayload::GiftPurchaseResult(result) => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::SendMarketplaceGift)
                    && let Some(gift) = self.marketplace_gift.as_mut()
                {
                    gift.sending = false;
                    match result {
                        crate::marketplace::GiftPurchaseResult::Sent(_) => {
                            // The receipt id is intentionally empty for gifts sent to others.
                            gift.completed = true;
                            gift.note = Some("Gift sent successfully.".into());
                            gift.price = None;
                        }
                        crate::marketplace::GiftPurchaseResult::PriceIncreased(price) => {
                            gift.price = price;
                            if price.is_none() {
                                gift.quote = None;
                            }
                            if let (Some(q), Some(price)) = (gift.quote.as_mut(), price) {
                                match price {
                                    crate::marketplace::GiftPrice::Stars(_) => {
                                        q.stars = Some(price)
                                    }
                                    crate::marketplace::GiftPrice::TonCents(_) => {
                                        q.ton = Some(price)
                                    }
                                }
                            }
                            gift.note=Some("The price increased. Review the new amount and confirm again; nothing was purchased.".into());
                        }
                    }
                }
            }
            EnvelopePayload::PaymentReceipt(receipt) => {
                // Slice P1: `getPaymentReceipt` answer (matched by `@extra`).
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetPaymentReceipt) {
                    self.payment_receipt = Some(receipt);
                    self.payment_receipt_open = true;
                }
            }
            EnvelopePayload::StarSubscriptions(subs) => {
                // Slice `parity:bots-payment-recurring`:
                // `getStarSubscriptions` answer to our own fetch (matched
                // by `@extra`). Follow-up pages append; a fresh fetch
                // replaces.
                if let Some(p) = pending
                    && let RequestPurpose::GetStarSubscriptions { append } = p.purpose
                {
                    self.star_subscriptions_loading = false;
                    self.star_subscriptions_error = None;
                    self.star_subscriptions_stale = false;
                    self.star_subscriptions_offset = subs.next_offset.clone();
                    if append && let Some(existing) = self.star_subscriptions.as_mut() {
                        existing.star_amount = subs.star_amount;
                        existing.required_star_count = subs.required_star_count;
                        existing.subscriptions.extend(subs.subscriptions);
                    } else {
                        self.star_subscriptions = Some(subs);
                    }
                }
            }
            EnvelopePayload::UpdateSavedAnimations { .. } => {
                self.gifs.stale = true;
            }
            // Slice S9: `updateAnimationSearchParameters` (schema 1.8.67,
            // line 11064) — server-pushed; store the provider name and the
            // new suggested search emojis for the GIF search surface.
            EnvelopePayload::UpdateAnimationSearchParameters { provider, emojis } => {
                self.gifs.search_provider = provider;
                self.gifs.provider_emojis = emojis;
            }
            EnvelopePayload::NotificationSounds { sounds } => {
                // Parity slice: `getSavedNotificationSounds` answer.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetSavedNotificationSounds) {
                    let files: Vec<ParsedFile> = sounds.iter().map(|s| s.sound.clone()).collect();
                    self.remember_files(&files);
                    // Parity slice: a refetch replaces the saved list, so
                    // evict `sound_file_ids` entries for sounds that are no
                    // longer saved — stale file→sound mappings would
                    // otherwise accumulate forever.
                    let live_ids: HashSet<i64> = sounds.iter().map(|s| s.id).collect();
                    self.sound_file_ids
                        .retain(|_, sound_id| live_ids.contains(sound_id));
                    self.saved_notification_sounds = sounds;
                    self.saved_sounds_loaded = true;
                    self.saved_sounds_stale = false;
                }
            }
            EnvelopePayload::UpdateSavedNotificationSounds { .. } => {
                // The list changed server-side; refetch on the next ingest.
                self.saved_sounds_stale = true;
            }
            EnvelopePayload::StorageStatistics {
                total_size,
                by_file_type,
                by_chat,
            } => {
                // Phase S2: `getStorageStatistics` answer — only our own
                // in-flight request writes the cache (matched by `@extra`).
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetStorageStatistics) {
                    self.storage_stats = Some(StorageStats {
                        total_size,
                        by_file_type,
                        by_chat,
                    });
                    self.storage_stats_loading = false;
                }
                // Batch 6: `optimizeStorage` answers with the statistics
                // of the files it deleted. Drop the usage cache so the
                // driver refetches the post-clear numbers on this same
                // ingest.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::OptimizeStorage) {
                    self.storage_freed = Some(total_size);
                    self.storage_clearing = false;
                    self.storage_stats = None;
                    self.storage_stats_loading = false;
                    self.data_storage_error = None;
                }
            }
            EnvelopePayload::AutoDownloadSettingsPresets { low, medium, high } => {
                // Slice S4: `getAutoDownloadSettingsPresets` answer —
                // only our own in-flight request seeds the local
                // per-network settings (matched by `@extra`), and only
                // when nothing was loaded from disk. The driver persists
                // the seed on the next ingest (`data_storage_dirty`).
                if pending.map(|p| p.purpose)
                    == Some(RequestPurpose::GetAutoDownloadSettingsPresets)
                    && !self.data_storage.seeded
                {
                    self.data_storage.seed_from_presets(low, medium, high);
                    self.data_storage_dirty = true;
                    self.auto_download_presets_loading = false;
                    self.data_storage_error = None;
                }
            }
            EnvelopePayload::UpdateUnconfirmedSession { session, count } => {
                self.apply_unconfirmed_session(session, count);
            }
            EnvelopePayload::UpdateServiceNotification { kind, text } => {
                self.apply_service_notification(kind, text);
            }
            EnvelopePayload::UpdateTermsOfService { terms } => {
                self.notices.terms = Some(terms);
                self.notices.terms_error = None;
            }
            EnvelopePayload::EmailCodeInfo { pattern, .. } => {
                // Batch 6: only our own in-flight 2FA step takes the
                // answer (matched by `@extra`).
                if let Some(RequestPurpose::PasswordStateOp { op }) = pending.map(|p| p.purpose) {
                    self.password_state_loading = false;
                    self.password_op_error = None;
                    self.apply_email_code_info(op, pattern);
                }
            }
            EnvelopePayload::ResetPasswordResult(outcome) => {
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(RequestPurpose::PasswordStateOp {
                        op: PasswordOp::ResetPassword
                    })
                ) {
                    self.password_state_loading = false;
                    self.password_op_error = None;
                    self.apply_reset_password_result(outcome);
                }
            }
            EnvelopePayload::PasswordState { state } => {
                // Slice A2: `passwordState` answer — only our own
                // in-flight `PasswordStateOp` writes the cache (matched by
                // `@extra`). The response is authoritative: it replaces
                // the cached state and clears any stale error. No
                // optimistic mutation ever happens client-side.
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(RequestPurpose::PasswordStateOp { .. })
                ) {
                    self.password_state = Some(state);
                    self.password_state_loading = false;
                    self.password_op_error = None;
                    if let Some(RequestPurpose::PasswordStateOp { op }) = pending.map(|p| p.purpose)
                    {
                        self.apply_password_state_op(op);
                    }
                }
            }
            EnvelopePayload::DeviceLoginResult { result } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::ConfirmDeviceLogin) {
                    self.device_login_result = Some(result);
                    self.sessions_mutating = false;
                    if result != crate::auth::DeviceLoginResult::Failed {
                        self.sessions_stale = true;
                        self.sessions_error = None;
                    } else {
                        self.sessions_error =
                            Some("Telegram returned an invalid device session.".into());
                    }
                }
            }
            EnvelopePayload::AddedProxies { proxies } => {
                self.apply_added_proxies(pending, proxies);
            }
            EnvelopePayload::AddedProxy { .. } => self.apply_added_proxy(pending),
            EnvelopePayload::Seconds { seconds } => self.apply_proxy_ping(pending, seconds),
            EnvelopePayload::Sessions { sessions } => {
                // Slice A3: `getActiveSessions` answer — only our own
                // in-flight request writes the cache (matched by `@extra`).
                // The answer is authoritative: it replaces the list and
                // clears any stale error. No optimistic mutation ever
                // happens client-side.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetActiveSessions) {
                    self.resolve_unconfirmed_entries(&sessions);
                    self.sessions = Some(sessions);
                    self.sessions_loading = false;
                    self.sessions_error = None;
                    self.sessions_stale = false;
                }
            }
            EnvelopePayload::AuthenticationCodeInfo {
                phone_number,
                timeout,
            } => {
                // Slice A8: the `sendPhoneNumberCode` /
                // `resendPhoneNumberCode` answer — only our own in-flight
                // request writes the pending number/timeout (matched by
                // `@extra`); a late answer for a superseded send has no
                // pending entry and is ignored.
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(
                        RequestPurpose::SendPhoneNumberCode | RequestPurpose::ResendPhoneNumberCode
                    )
                ) {
                    self.change_number_phone = Some(phone_number);
                    self.change_number_timeout = Some(timeout);
                    self.change_number_loading = false;
                    self.change_number_error = None;
                }
            }
            EnvelopePayload::MessageAutoDeleteTime { seconds } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetDefaultAutoDelete) {
                    self.default_auto_delete_secs = Some(seconds);
                    self.default_auto_delete_busy = false;
                    self.default_auto_delete_error = None;
                }
            }
            EnvelopePayload::AccountTtl { days } => {
                // Slice A7: `getAccountTtl` answer — only our own
                // in-flight request writes the cache (matched by
                // `@extra`). Authoritative: replaces the cached days and
                // clears any stale error.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetAccountTtl) {
                    self.account_ttl_days = Some(days);
                    self.account_ttl_loading = false;
                    self.account_error = None;
                }
            }
            EnvelopePayload::ConnectedWebsites { websites } => {
                // Slice A4: `getConnectedWebsites` answer — only our own
                // in-flight request writes the cache (matched by `@extra`).
                // The answer is authoritative: it replaces the list and
                // clears any stale error. No optimistic mutation ever
                // happens client-side.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetConnectedWebsites) {
                    self.connected_websites = Some(websites);
                    self.connected_websites_loading = false;
                    self.websites_error = None;
                    self.websites_stale = false;
                }
            }
            EnvelopePayload::ArchiveChatListSettings { settings } => {
                // Slice CL2: `getArchiveChatListSettings` answer — only
                // our own in-flight request writes the cache.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetArchiveChatListSettings) {
                    self.archive_chat_list_settings = Some(settings);
                    self.archive_settings_loading = false;
                }
            }
            EnvelopePayload::ScopeNotificationSettings { settings, .. } => {
                // Parity slice: `getScopeNotificationSettings` answer; the
                // scope is correlated via the pending request (the response
                // carries no scope field).
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetScopeNotificationSettings)
                    && let Some(scope) = pending.and_then(|p| p.scope)
                {
                    self.scope_notification_settings.insert(scope, settings);
                    self.scope_settings_loading.remove(&scope);
                }
            }
            EnvelopePayload::UpdateScopeNotificationSettings { scope, settings } => {
                // Parity slice: scope defaults changed (or our own
                // `setScopeNotificationSettings` was confirmed).
                self.scope_notification_settings.insert(scope, settings);
                self.scope_settings_loading.remove(&scope);
            }
            EnvelopePayload::UpdateReactionNotificationSettings { settings } => {
                // Parity slice: no getter exists — the update stream is the
                // source of truth (it also confirms our own
                // `setReactionNotificationSettings`).
                self.reaction_notification_settings = Some(settings);
            }
            // Phase C2g: `joinVideoChat` returns `text` — the tgcalls
            // join answer, stored on the tracked call and consumed by
            // the driver pump (`ntg_connect`).
            // `startGroupCallScreenSharing` returns `text` — the
            // presentation answer, consumed by the driver pump.
            EnvelopePayload::Text { text } => match pending.map(|p| p.purpose) {
                Some(RequestPurpose::JoinVideoChat { group_call_id }) => {
                    self.set_group_call_join_payload(group_call_id, text);
                }
                Some(RequestPurpose::StartGroupCallScreenSharing { group_call_id }) => {
                    self.set_group_call_screen_share_answer(group_call_id, text);
                }
                _ => {}
            },
            // Phase C3a: `getVideoChatInviteLink` returns `httpUrl`.
            EnvelopePayload::HttpUrl { url } => {
                if let Some(RequestPurpose::GetVideoChatInviteLink { group_call_id }) =
                    pending.map(|p| p.purpose)
                {
                    self.set_group_call_invite_link(group_call_id, url);
                // B1: `getLoginUrl` returns `httpUrl` too (schema 1.8.67,
                // line 7458) — the authorized URL after consent.
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::GetLoginUrl) {
                    self.last_login_url_info = Some(LoginUrlInfo::Open { url });
                }
            }
            // M1: `getMessageLink` returns `messageLink`. The driver
            // stashes the link in `Session::message_link_result` before
            // `apply` takes the pending request; nothing to reduce here.
            EnvelopePayload::MessageLink { .. } => {}
            // A5: `checkChatUsername` answer — the driver stashes the
            // verdict in `Session::username_check` before `apply` takes
            // the pending request; nothing to reduce here.
            EnvelopePayload::CheckChatUsernameResult(_) => {}
            // M2: handled by the driver before `apply` (blocks land in
            // history there); nothing to reduce here.
            EnvelopePayload::RichMessage { .. } => {}
            // Slice msg-richtext-ai-tools: `fixedText` / `formattedText`
            // answers — captured by the driver before `apply` into
            // `Session::ai_composer_text`; nothing to reduce here.
            EnvelopePayload::FormattedText { text, entities }
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(RequestPurpose::TranslateJob { .. })
                ) =>
            {
                if let Some(RequestPurpose::TranslateJob { job }) = pending.map(|p| p.purpose) {
                    self.finish_translation(job, Translation::Done { text, entities });
                }
            }
            EnvelopePayload::FixedText { .. } | EnvelopePayload::FormattedText { .. } => {}
            // MED4: `webPageInstantView` — captured by the driver before
            // `apply` into `Session::instant_view` (success) or
            // `Session::instant_view_fallback_url` (error); nothing to
            // reduce here.
            EnvelopePayload::WebPageInstantView { .. } => {}
            // MED4b: `linkPreview` (`getLinkPreview` answer) — captured
            // by the driver before `apply` into
            // `Session::composer_preview`; nothing to reduce here.
            EnvelopePayload::LinkPreview { .. } => {}
            // M1 fix-up: `getMessageProperties` returns
            // `messageProperties`. The driver gates the chained
            // `getMessageLink` on `can_get_link` before `apply` takes
            // the pending request; nothing to reduce here.
            EnvelopePayload::MessageProperties(actions) => {
                if let Some(RequestPurpose::GetMessageMenuActions {
                    chat_id,
                    message_id,
                }) = pending.map(|p| p.purpose)
                {
                    self.message_menu_actions = Some((chat_id, message_id, actions));
                }
            }
            // Phase C2f: `inviteGroupCallParticipant` answer. A success
            // clears any earlier invite error; the three failure
            // variants surface honestly via `group_call_error` (shown
            // on the group-call overlay).
            EnvelopePayload::InviteGroupCallParticipantResult(result) => {
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(RequestPurpose::InviteGroupCallParticipant { .. })
                ) {
                    self.group_call_error = match result {
                        InviteGroupCallParticipantResult::Success { .. } => None,
                        InviteGroupCallParticipantResult::UserPrivacyRestricted => Some(
                            "Couldn't invite: that user restricts group-call invitations."
                                .to_string(),
                        ),
                        InviteGroupCallParticipantResult::UserAlreadyParticipant => {
                            Some("That user is already in the voice chat.".to_string())
                        }
                        InviteGroupCallParticipantResult::UserWasBanned => {
                            Some("That user was banned from the voice chat.".to_string())
                        }
                    };
                }
            }
            EnvelopePayload::Ok => self.apply_ok(pending, extra, seq),
            // Slice A6: `importedContacts` (schema 1.8.67, line 14517) —
            // the `importContacts` answer. Same invalidate + notice as
            // the `ok` of the other contact mutations; the user ids are
            // not merged into the cache (the tab refetches the
            // authoritative list).
            EnvelopePayload::ImportedContacts { .. } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::ImportContacts) {
                    self.contacts = None;
                    self.contacts_error = false;
                    self.contacts_notice = Some("Contacts imported.".to_string());
                }
            }
            EnvelopePayload::Error(err) => self.apply_error(err, pending, extra, seq),
            EnvelopePayload::Unknown(kind) => {
                self.diagnostics.record(Diagnostic {
                    category: "reducer",
                    type_name: Some(kind.type_name),
                    extra: pending.map(|p| p.id.0),
                    seq: Some(seq),
                    note: "unknown-variant",
                });
            }
        }
    }
}

impl Session {
    /// Set (or clear) a chat's last-message preview fields: preview text,
    /// its style inputs, the 3-line sender name, and the row's
    /// id/date/direction for the timestamp and receipt.
    /// A send resolved: if it was the chat's last message, the row's
    /// clock becomes a check (or a failed mark) without waiting for the
    /// next `updateChatLastMessage`.
    pub(crate) fn note_last_message_send_state(
        &mut self,
        chat_id: ChatId,
        old_id: MessageId,
        new_id: MessageId,
        state: crate::telegram::envelope::MessageSendState,
    ) {
        if let Some(last) = self
            .chats
            .get_mut(&chat_id.0)
            .and_then(|chat| chat.last_message.as_mut())
            .filter(|last| last.id == old_id || last.id == new_id)
        {
            last.id = new_id;
            last.send_state = state;
        }
    }

    pub(crate) fn set_chat_last_message(
        &mut self,
        chat_id: ChatId,
        message: Option<&ParsedMessage>,
    ) {
        // Service messages preview as their wording ("Dana pinned \"hi\""),
        // computed before the chat is borrowed mutably.
        let service_preview = message.and_then(|message| {
            let content = effective_content(&message.content, message.ephemeral.as_ref());
            self.service_text_for(chat_id, content, message.sender, message.is_outgoing)
                .map(|text| text.plain())
        });
        let chat = self
            .chats
            .entry(chat_id.0)
            .or_insert_with(|| placeholder_chat(chat_id));
        if let Some(message) = message {
            let content = effective_content(&message.content, message.ephemeral.as_ref());
            chat.last_preview = service_preview.unwrap_or_else(|| content.preview());
            chat.last_preview_style = preview_style(content, &chat.last_preview);
            chat.last_preview_thumb = match content {
                MessageContent::Photo(photo) if !photo.is_secret && !photo.has_spoiler => photo
                    .minithumbnail
                    .clone()
                    .filter(|mini| !mini.data.is_empty())
                    .map(std::sync::Arc::new),
                _ => None,
            };
            chat.last_preview_sender = if chat.last_preview_style.service {
                String::new()
            } else {
                preview_sender_name(
                    message.is_outgoing,
                    message.author_signature.as_deref(),
                    &chat.title,
                )
            };
            chat.last_message = Some(ChatLastMessage {
                id: message.id,
                date: message.date,
                is_outgoing: message.is_outgoing,
                sender: message.sender,
                send_state: message.send_state,
            });
        } else {
            chat.last_preview = String::new();
            chat.last_preview_style = ChatPreviewStyle::default();
            chat.last_preview_thumb = None;
            chat.last_preview_sender = String::new();
            chat.last_message = None;
        }
    }
}
