//! Payload handlers: chats, supergroups, invite links.
use super::*;

impl Session {
    /// Batch 8: store (or drop) a chat's action bar.
    pub fn set_chat_action_bar(&mut self, chat_id: i64, bar: Option<ChatActionBar>) {
        match bar {
            Some(bar) => {
                self.chat_action_bars.insert(chat_id, bar);
            }
            None => {
                self.chat_action_bars.remove(&chat_id);
            }
        }
    }

    /// Batch 8: the chat's current action bar, if any.
    pub fn chat_action_bar(&self, chat_id: ChatId) -> Option<&ChatActionBar> {
        self.chat_action_bars.get(&chat_id.0)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_update_new_chat(
        &mut self,
        chat_id: ChatId,
        title: String,
        kind: ChatKind,
        unread_count: i32,
        last_read_inbox_message_id: MessageId,
        last_read_outbox_message_id: MessageId,
        notification_settings: ChatNotificationSettings,
        draft: Option<ChatDraft>,
        photo: Option<ParsedFile>,
        can_send_basic_messages: bool,
        permissions: Option<ChatPermissions>,
        can_be_deleted_for_all_users: bool,
        can_be_deleted_only_for_self: bool,
        is_marked_as_unread: bool,
        message_auto_delete_time: i32,
        video_chat: Option<ParsedVideoChat>,
        has_welcome_messages: bool,
        unread_mention_count: i32,
        unread_reaction_count: i32,
        can_be_reported: bool,
        blocked: bool,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        // Parity slice: keep the chat photo (`chatPhotoInfo.small`)
        // file id so the chat list can render avatars. The file
        // object is remembered first (separate borrow) so the
        // driver can download it.
        let photo_file_id = photo.as_ref().map(|file| file.id.0);
        if let Some(file) = &photo {
            self.remember_files(std::slice::from_ref(file));
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::ResolveGifSearchBot) {
            self.gifs.search_bot_user_id = match &kind {
                ChatKind::Private { user_id }
                    if user_id.0 > 0
                        && !self.users.get(&user_id.0).is_some_and(|user| !user.is_bot) =>
                {
                    Some(user_id.0)
                }
                _ => None,
            };
            if self.gifs.search_bot_user_id.is_none() {
                self.gifs.search_failed = true;
                self.gifs.search_loading = false;
            }
        }
        // Bots slice: `searchPublicChat` answer for `@botname`
        // resolution. A bot username yields `ChatKind::Private`
        // with the bot's user id; any other kind means the
        // username is not a bot. The user object may already
        // have arrived via `updateUser` — re-scan the cache for
        // the inline capability (`None` = unknown; the query
        // attempt itself is the capability check). The generation
        // guard drops stale answers (a newer username is already
        // resolving).
        if let Some(p) = pending.as_ref()
            && let RequestPurpose::ResolveInlineBot { generation } = p.purpose
            && let Some(InlineBotResolve::Resolving {
                username,
                generation: slot_generation,
            }) = self.inline_bot_resolve.as_ref()
            && *slot_generation == generation
        {
            match &kind {
                ChatKind::Private { user_id } => {
                    // Any public username resolves to a private
                    // chat — confirm it is actually a bot. The
                    // user object may already have arrived via
                    // `updateUser`; `is_inline` stays `None` when
                    // unknown (the query attempt itself is then
                    // the capability check).
                    match self.users.get(&user_id.0) {
                        Some(user) if !user.is_bot => {
                            self.inline_bot_resolve = Some(InlineBotResolve::Failed {
                                username: username.clone(),
                                reason: format!("@{username} is not a bot"),
                            });
                        }
                        user => {
                            let is_inline = user.map(|u| u.is_inline);
                            self.inline_bot_resolve = Some(InlineBotResolve::Resolved {
                                username: username.clone(),
                                user_id: user_id.0,
                                is_inline,
                            });
                        }
                    }
                }
                _ => {
                    self.inline_bot_resolve = Some(InlineBotResolve::Failed {
                        username: username.clone(),
                        reason: format!("@{username} is not a bot"),
                    });
                }
            }
        }
        // `parity:platform-deep-links`: the `searchPublicChat` /
        // `createPrivateChat` answer for a deep-link follow-up. Any chat
        // object the server returns is the link's destination (the UI
        // consumes `ChatReady` once to open it). Generation-guarded like
        // the bot-resolve slot above.
        if let Some(p) = pending.as_ref()
            && let RequestPurpose::DeepLinkResolve { generation } = p.purpose
            && let Some(DeepLinkState::ResolvingChat {
                action,
                generation: slot_generation,
            }) = self.deep_link.as_ref()
            && *slot_generation == generation
        {
            self.deep_link = Some(DeepLinkState::ChatReady {
                chat_id,
                action: action.clone(),
            });
        }
        let chat = self
            .chats
            .entry(chat_id.0)
            .or_insert_with(|| placeholder_chat(chat_id));
        chat.title = title;
        chat.kind = kind;
        chat.unread_count = unread_count;
        chat.last_read_inbox_message_id = last_read_inbox_message_id;
        chat.last_read_outbox_message_id = last_read_outbox_message_id;
        chat.notification_settings = notification_settings;
        let old_photo_file_id = std::mem::replace(&mut chat.photo_file_id, photo_file_id);
        chat.can_send_basic_messages = can_send_basic_messages;
        // Slice G1: full default permissions block for the editor.
        chat.permissions = permissions;
        chat.can_be_deleted_for_all_users = can_be_deleted_for_all_users;
        // Slice CL1: clear-history gate + marked-as-unread flag.
        chat.can_be_deleted_only_for_self = can_be_deleted_only_for_self;
        chat.is_marked_as_unread = is_marked_as_unread;
        // Slice CL3: mention / reaction badge counts, report gate,
        // block-list state.
        chat.unread_mention_count = unread_mention_count;
        chat.unread_reaction_count = unread_reaction_count;
        chat.can_be_reported = can_be_reported;
        chat.blocked = blocked;
        // Phase B4: chat-level auto-delete / self-destruct timer
        // (`chat.message_auto_delete_time`, schema 1.8.67, lines
        // 3616 / 3627).
        chat.message_auto_delete_time = message_auto_delete_time;
        // Phase C3a: the chat's active video chat (`videoChat`,
        // schema 1.8.67, lines 3576 / 3579).
        chat.video_chat = video_chat.map(|v| VideoChatInfo {
            group_call_id: v.group_call_id,
            has_participants: v.has_participants,
        });
        // Slice G2: `chat.has_welcome_messages` (schema 1.8.67,
        // line 3627).
        self.chat_has_welcome_messages
            .insert(chat_id.0, has_welcome_messages);
        // Phase B1: secret chats — `updateSecretChat` arrives before
        // `updateNewChat` (schema 1.8.67, line 10740), so a state
        // may already be recorded; otherwise the driver fetches it
        // via `getSecretChat` (an offline method).
        if let ChatKind::Secret { secret_chat_id, .. } = &chat.kind {
            let secret_chat_id = *secret_chat_id;
            match self.secret_chat_states.get(&secret_chat_id) {
                Some(secret_chat) => chat.secret_state = Some(secret_chat.state.clone()),
                None if !self.secret_chat_fetch_queue.contains(&secret_chat_id) => {
                    self.secret_chat_fetch_queue.push(secret_chat_id);
                }
                None => {}
            }
        }
        if !self.draft_dirty.contains(&chat_id.0) {
            chat.draft = draft;
        }
        self.replace_avatar(old_photo_file_id, photo_file_id);
        // `updateSupergroup` precedes `updateNewChat`: adopt its status now.
        self.adopt_supergroup_status_for_chat(chat_id);
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_update_supergroup(
        &mut self,
        supergroup_id: i64,
        is_forum: bool,
        username: String,
        status: ChannelMemberStatus,
        can_restrict_members: Option<bool>,
        can_invite_users: Option<bool>,
        can_promote_members: Option<bool>,
        can_manage_tags: Option<bool>,
        can_manage_topics: Option<bool>,
        can_change_info: Option<bool>,
        can_send_welcome_messages: Option<bool>,
        join_by_request: bool,
        is_broadcast_group: bool,
        sign_messages: bool,
        show_message_sender: bool,
        _pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        self.set_supergroup_forum(supergroup_id, is_forum);
        self.set_supergroup_username(supergroup_id, username);
        // Phase A1: own member status drives the slow-mode bypass.
        self.supergroup_member_status.insert(supergroup_id, status);
        self.adopt_supergroup_status(supergroup_id);
        // Phase A1: `can_restrict_members` gates the slow-mode
        // admin control; absent = unknown → treated as lacking.
        self.supergroup_restrict_right
            .insert(supergroup_id, can_restrict_members.unwrap_or(false));
        // Phase D3a: `can_invite_users` gates invite-link /
        // join-request management; absent = unknown → lacking.
        self.supergroup_invite_right
            .insert(supergroup_id, can_invite_users.unwrap_or(false));
        // Phase D3b: `can_promote_members` gates admin
        // management; absent = unknown → lacking.
        self.supergroup_promote_right
            .insert(supergroup_id, can_promote_members.unwrap_or(false));
        // Slice G1: `can_manage_tags` gates custom-title
        // changes for other members.
        self.supergroup_manage_tags_right
            .insert(supergroup_id, can_manage_tags.unwrap_or(false));
        // Slice G2: forum-topic / sign-messages / welcome-message
        // rights; absent = unknown → treated as lacking.
        self.supergroup_manage_topics_right
            .insert(supergroup_id, can_manage_topics.unwrap_or(false));
        self.supergroup_change_info_right
            .insert(supergroup_id, can_change_info.unwrap_or(false));
        self.supergroup_send_welcome_right
            .insert(supergroup_id, can_send_welcome_messages.unwrap_or(false));
        // Slice G1: `supergroup.join_by_request` (schema 1.8.67,
        // lines 2733/2746) drives the "Approve new members"
        // toggle; `supergroup.is_broadcast_group` (lines
        // 2736/2746) drives the broadcast-group toggle.
        self.supergroup_join_by_request
            .insert(supergroup_id, join_by_request);
        self.supergroup_is_broadcast
            .insert(supergroup_id, is_broadcast_group);
        // Slice G2: `supergroup.sign_messages` /
        // `show_message_sender` (schema 1.8.67, lines 2731/2746).
        self.supergroup_sign_messages
            .insert(supergroup_id, sign_messages);
        self.supergroup_show_message_sender
            .insert(supergroup_id, show_message_sender);
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_supergroup(
        &mut self,
        supergroup_id: i64,
        is_forum: bool,
        username: String,
        status: ChannelMemberStatus,
        can_restrict_members: Option<bool>,
        can_invite_users: Option<bool>,
        can_promote_members: Option<bool>,
        can_manage_tags: Option<bool>,
        can_manage_topics: Option<bool>,
        can_change_info: Option<bool>,
        can_send_welcome_messages: Option<bool>,
        join_by_request: bool,
        is_broadcast_group: bool,
        sign_messages: bool,
        show_message_sender: bool,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetSupergroup) {
            self.set_supergroup_forum(supergroup_id, is_forum);
            self.set_supergroup_username(supergroup_id, username);
            // Phase A1: own member status drives the slow-mode bypass.
            self.supergroup_member_status.insert(supergroup_id, status);
            self.adopt_supergroup_status(supergroup_id);
            self.supergroup_restrict_right
                .insert(supergroup_id, can_restrict_members.unwrap_or(false));
            // Phase D3a: `can_invite_users` gates invite-link /
            // join-request management.
            self.supergroup_invite_right
                .insert(supergroup_id, can_invite_users.unwrap_or(false));
            // Phase D3b: `can_promote_members` gates admin management.
            self.supergroup_promote_right
                .insert(supergroup_id, can_promote_members.unwrap_or(false));
            // Slice G1: `can_manage_tags` gates custom-title
            // changes for other members.
            self.supergroup_manage_tags_right
                .insert(supergroup_id, can_manage_tags.unwrap_or(false));
            // Slice G2: forum-topic / sign-messages /
            // welcome-message rights.
            self.supergroup_manage_topics_right
                .insert(supergroup_id, can_manage_topics.unwrap_or(false));
            self.supergroup_change_info_right
                .insert(supergroup_id, can_change_info.unwrap_or(false));
            self.supergroup_send_welcome_right
                .insert(supergroup_id, can_send_welcome_messages.unwrap_or(false));
            // Slice G1: join-by-request + broadcast flags (schema
            // 1.8.67, lines 2733/2736/2746).
            self.supergroup_join_by_request
                .insert(supergroup_id, join_by_request);
            self.supergroup_is_broadcast
                .insert(supergroup_id, is_broadcast_group);
            // Slice G2: sign/show flags (schema 1.8.67, lines
            // 2731/2746).
            self.supergroup_sign_messages
                .insert(supergroup_id, sign_messages);
            self.supergroup_show_message_sender
                .insert(supergroup_id, show_message_sender);
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_chat_invite_link(
        &mut self,
        link: ParsedChatInviteLink,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        if matches!(
            pending.map(|p| p.purpose),
            Some(
                RequestPurpose::CreateChatInviteLink
                    | RequestPurpose::EditChatInviteLink
                    | RequestPurpose::ReplacePrimaryChatInviteLink,
            )
        ) && let Some(pending) = pending
            && let Some(chat_id) = pending.chat_id
        {
            let is_create = pending.purpose == RequestPurpose::CreateChatInviteLink;
            match self.invite_links.entry(chat_id.0) {
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    match entry.get_mut() {
                        InviteLinkFetch::Loaded(list) => {
                            if pending.purpose == RequestPurpose::ReplacePrimaryChatInviteLink {
                                // Slice G1: the old primary was
                                // revoked server-side; drop it so
                                // the list shows only the new one.
                                list.links
                                    .retain(|e| !e.is_primary || e.invite_link == link.invite_link);
                            }
                            if let Some(existing) = list
                                .links
                                .iter_mut()
                                .find(|e| e.invite_link == link.invite_link)
                            {
                                *existing = link;
                            } else {
                                list.links.push(link);
                                if is_create {
                                    list.total_count = list.total_count.saturating_add(1);
                                }
                            }
                        }
                        fetch => {
                            *fetch = InviteLinkFetch::Loaded(InviteLinkList {
                                total_count: 1,
                                links: vec![link],
                            });
                        }
                    }
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(InviteLinkFetch::Loaded(InviteLinkList {
                        total_count: 1,
                        links: vec![link],
                    }));
                }
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_supergroup_full_info(
        &mut self,
        description: String,
        member_count: i32,
        linked_chat_id: i64,
        slow_mode_delay: i32,
        slow_mode_delay_expires_in: f64,
        my_boost_count: i32,
        unrestrict_boost_count: i32,
        can_get_statistics: bool,
        has_aggressive_anti_spam_enabled: bool,
        can_toggle_aggressive_anti_spam: bool,
        can_set_sticker_set: bool,
        sticker_set_id: i64,
        custom_emoji_sticker_set_id: i64,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        // Phase 6: `getSupergroupFullInfo` answer — the response
        // carries no id, so it is correlated via the pending
        // request's `supergroup_id`.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetSupergroupFullInfo)
            && let Some(pending) = pending
            && let Some(supergroup_id) = pending.supergroup_id
        {
            self.supergroup_full_infos.insert(
                supergroup_id,
                SupergroupFullInfoData {
                    description,
                    member_count,
                    linked_chat_id,
                    slow_mode_delay,
                    slow_mode_delay_expires_in,
                    my_boost_count,
                    unrestrict_boost_count,
                    // Phase A1: timestamp the arrival — the schema
                    // (1.8.67, line 2759) warns no update fires
                    // when only the expiry changes, so the gate
                    // decays it locally against this stamp.
                    fetched_at_ms: unix_ms_now(),
                    can_get_statistics,
                    can_set_sticker_set,
                    sticker_set_id,
                    custom_emoji_sticker_set_id,
                },
            );
            // Slice G2: anti-spam state for the manage-dialog
            // toggle.
            self.supergroup_anti_spam_enabled
                .insert(supergroup_id, has_aggressive_anti_spam_enabled);
            self.supergroup_can_toggle_anti_spam
                .insert(supergroup_id, can_toggle_aggressive_anti_spam);
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_update_supergroup_full_info(
        &mut self,
        supergroup_id: i64,
        description: String,
        member_count: i32,
        linked_chat_id: i64,
        slow_mode_delay: i32,
        slow_mode_delay_expires_in: f64,
        my_boost_count: i32,
        unrestrict_boost_count: i32,
        can_get_statistics: bool,
        has_aggressive_anti_spam_enabled: bool,
        can_toggle_aggressive_anti_spam: bool,
        can_set_sticker_set: bool,
        sticker_set_id: i64,
        custom_emoji_sticker_set_id: i64,
        _pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        self.supergroup_full_infos.insert(
            supergroup_id,
            SupergroupFullInfoData {
                description,
                member_count,
                linked_chat_id,
                slow_mode_delay,
                slow_mode_delay_expires_in,
                my_boost_count,
                unrestrict_boost_count,
                fetched_at_ms: unix_ms_now(),
                can_get_statistics,
                can_set_sticker_set,
                sticker_set_id,
                custom_emoji_sticker_set_id,
            },
        );
        // Slice G2: anti-spam state for the manage-dialog toggle.
        self.supergroup_anti_spam_enabled
            .insert(supergroup_id, has_aggressive_anti_spam_enabled);
        self.supergroup_can_toggle_anti_spam
            .insert(supergroup_id, can_toggle_aggressive_anti_spam);
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_story_album(
        &mut self,
        album: ParsedStoryAlbum,
        pending: Option<&PendingRequest>,
    ) {
        // Phase 9.7: `createStoryAlbum` / `setStoryAlbumName` /
        // `addStoryAlbumStories` / `removeStoryAlbumStories` /
        // `reorderStoryAlbumStories` — each returns the changed
        // album; upsert it into the chat's list and mark the op
        // succeeded. Album-story mutations also invalidate the
        // opened album's cached id list so the next open refetches.
        if let Some(purpose) = pending.map(|p| p.purpose)
            && matches!(
                purpose,
                RequestPurpose::CreateStoryAlbum
                    | RequestPurpose::SetStoryAlbumName
                    | RequestPurpose::AddStoryAlbumStories
                    | RequestPurpose::RemoveStoryAlbumStories
                    | RequestPurpose::ReorderStoryAlbumStories
            )
            && let Some(chat_id) = pending.and_then(|p| p.chat_id)
        {
            let albums = self.story_albums.entry(chat_id.0).or_default();
            if let Some(existing) = albums.iter_mut().find(|a| a.id == album.id) {
                *existing = album.clone();
            } else {
                albums.push(album.clone());
            }
            if !matches!(purpose, RequestPurpose::SetStoryAlbumName) {
                self.story_album_stories.remove(&(chat_id.0, album.id));
            }
            self.succeed_story_page_op(purpose);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_chats(&mut self, chat_ids: Vec<ChatId>, pending: Option<&PendingRequest>) {
        // Parity slice: `getChatFolderChatsToLeave` response for the
        // delete-confirm dialog (correlated via folder_id). Runs
        // before the search branch below consumes `chat_ids`.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatFolderChatsToLeave)
            && let Some(folder_id) = pending.and_then(|p| p.folder_id)
        {
            self.folder_chats_to_leave
                .insert(folder_id, chat_ids.iter().map(|id| id.0).collect());
        }
        // B10: profile panel lists (groups in common, similar channels,
        // personal channel candidates).
        if let Some(pending) = pending
            && matches!(pending.purpose, RequestPurpose::GetProfileChats(_))
        {
            self.apply_profile_chats(&chat_ids, pending);
        }
        // Phase 9.5: `getChatsToPostStories` answer — the
        // composer's "post as" picker options. Runs before the
        // search branch below consumes `chat_ids`.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatsToPostStories) {
            self.story_post_as_chats = chat_ids.iter().map(|id| id.0).collect();
        }
        // Parity slice: `getChatNotificationSettingsExceptions`
        // answer — the scope is correlated via `pending.scope`
        // (the `chats` response carries no scope field).
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetChatNotificationSettingsExceptions)
            && let Some(scope) = pending.and_then(|p| p.scope)
        {
            self.notification_exceptions
                .insert(scope, chat_ids.iter().map(|id| id.0).collect());
            self.notification_exceptions_loading.remove(&scope);
        }
        if let Some(pending) = pending
            && matches!(
                pending.purpose,
                RequestPurpose::SearchShareChats | RequestPurpose::SearchShareChatsOnServer
            )
        {
            self.share_search.accept(pending.id, &chat_ids);
        }
        // `getTopChats`: not tied to a query generation, only to the strip.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::GetTopChats) {
            if !self.search.top_chats_disabled {
                self.search.top_chats = chat_ids;
            }
            return;
        }
        if self.search.matches_generation(pending) {
            match pending.map(|p| p.purpose) {
                Some(RequestPurpose::SearchChats | RequestPurpose::SearchRecentlyFoundChats) => {
                    self.search.accept_chats(chat_ids, false);
                }
                Some(RequestPurpose::SearchChatsOnServer) => {
                    self.search.accept_server_chats(chat_ids);
                }
                Some(RequestPurpose::SearchPublicChats) => {
                    self.search.accept_public_chats(chat_ids, false);
                }
                _ => {}
            }
        }
    }
}
