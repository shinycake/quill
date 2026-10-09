//! OK routing: maps TDLib acknowledgements onto per-purpose handlers.
use super::*;

impl Session {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_ok(
        &mut self,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        if let Some(pending) = pending
            && let RequestPurpose::StopPendingMessage { topic_id, draft_id } = pending.purpose
            && let Some(chat_id) = pending.chat_id
        {
            self.finish_pending_bot_stop(chat_id, topic_id, draft_id);
        }
        self.apply_proxy_ok(pending);
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::EditMessageSchedulingState {
                message_id,
                scheduling,
            }) => self.finish_scheduling_edit(message_id, scheduling),
            Some(RequestPurpose::AddProfileAudio) => {
                self.message_action_note = Some("saved to your profile".into());
            }
            Some(RequestPurpose::DeleteChatMessagesBySender) => {
                self.message_action_note = Some("messages deleted".into());
            }
            Some(RequestPurpose::ReportSupergroupSpam) => {
                self.message_action_note = Some("spam reported".into());
            }
            // Forum extras and Saved Messages sublists (batch B16).
            Some(RequestPurpose::DeleteSavedMessagesTopicHistory { topic_id }) => {
                self.remove_saved_topic(topic_id);
            }
            Some(RequestPurpose::ReadAllForumTopicMentions { forum_topic_id }) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.clear_topic_marks(chat_id, forum_topic_id, true);
                }
            }
            Some(RequestPurpose::ReadAllForumTopicReactions { forum_topic_id }) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.clear_topic_marks(chat_id, forum_topic_id, false);
                }
            }
            Some(RequestPurpose::DeleteMessageReactionsFromSender {
                message_id,
                user_id,
            }) => {
                self.message_action_note = Some("reaction deleted".into());
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.drop_reactor_from_audience(chat_id, MessageId(message_id), user_id);
                }
            }
            Some(RequestPurpose::TransferChatOwnership { user_id }) => {
                self.finish_ownership_transfer(
                    user_id,
                    pending.and_then(|p| p.chat_id).map(|c| c.0),
                );
            }
            _ => {}
        }
        // Slice A3: a `terminateSession` /
        // `terminateAllOtherSessions` succeeded — keep the old
        // cache visible and mark it stale so the driver refetches
        // the authoritative answer on this same ingest (the
        // `saved_sounds_stale` pattern). No optimistic deletion:
        // the terminated row stays until the server-confirmed
        // list replaces it.
        if matches!(
            pending.map(|p| p.purpose),
            Some(
                RequestPurpose::TerminateSession { .. } | RequestPurpose::TerminateAllOtherSessions
            )
        ) {
            self.sessions_stale = true;
            self.sessions_mutating = false;
            self.sessions_error = None;
        }
        // Slice A4: a `toggleSessionCanAcceptSecretChats` /
        // `toggleSessionCanAcceptCalls` succeeded — same stale
        // pattern: the toggled value comes back in the
        // authoritative refetch, never from an optimistic flip.
        if matches!(
            pending.map(|p| p.purpose),
            Some(
                RequestPurpose::ToggleSessionSecretChats { .. }
                    | RequestPurpose::ToggleSessionCalls { .. }
            )
        ) {
            self.sessions_stale = true;
            self.sessions_mutating = false;
            self.sessions_error = None;
        }
        if let Some(RequestPurpose::SetDefaultAutoDelete { seconds }) = pending.map(|p| p.purpose) {
            self.default_auto_delete_secs = Some(seconds);
            self.default_auto_delete_busy = false;
            self.default_auto_delete_error = None;
        }
        // Slice A7: a `setAccountTtl` succeeded — the server
        // confirmed the write of exactly the sent value, so it
        // is stored directly (not an optimistic guess). A
        // `deleteAccount` success needs no local state change:
        // the authoritative teardown arrives via TDLib's own
        // `updateAuthorizationState` → `Closed` (`set_auth`
        // invalidates the account there). Both clear the
        // in-flight flag and any stale error.
        if let Some(RequestPurpose::SetAccountTtl { days }) = pending.map(|p| p.purpose) {
            self.account_ttl_days = Some(days);
            self.account_mutating = false;
            self.account_error = None;
        }
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::DeleteAccount)
        ) {
            self.account_mutating = false;
            self.account_error = None;
        }
        // Slice A8: a `checkPhoneNumberCode` succeeded — the
        // server completed the number change of the number the
        // code was sent to, so the own user's `phone_number` is
        // updated directly (not an optimistic guess — the `ok`
        // confirms it; the authoritative `updateUser` that
        // follows lands the same value idempotently). The
        // code-entry step clears and any stale error goes with
        // it.
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::CheckPhoneNumberCode)
        ) {
            if let Some(target) = self.change_number_phone.clone()
                && let Some(my_id) = self.my_user_id
                && let Some(me) = self.users.get_mut(&my_id)
            {
                me.phone_number = target;
            }
            self.change_number_phone = None;
            self.change_number_timeout = None;
            self.change_number_checking = false;
            self.change_number_error = None;
            // A8: drop any stale in-flight send/resend purposes —
            // a late resend answer must not resurrect the
            // completed flow (re-write phone/timeout or park a
            // phantom error after the number already changed).
            self.requests
                .take_purpose(RequestPurpose::SendPhoneNumberCode);
            self.requests
                .take_purpose(RequestPurpose::ResendPhoneNumberCode);
        }
        // Slice S4: a `setAutoDownloadSettings` succeeded — apply
        // the confirmed sent settings (the `ok` carries none, so
        // they ride the purpose); the error clears and the driver
        // persists on this ingest via `data_storage_dirty`.
        if let Some(RequestPurpose::SetAutoDownloadSettings { network, settings }) =
            pending.map(|p| p.purpose)
        {
            *self.data_storage.for_network_mut(network) = settings;
            self.data_storage.seeded = true;
            self.data_storage_error = None;
            self.data_storage_dirty = true;
        }
        // Batch 4: a `confirmSession` / `terminateSession` for the
        // new-login alert succeeded.
        if let Some(RequestPurpose::ReviewUnconfirmedSession { confirmed }) =
            pending.map(|p| p.purpose)
        {
            self.finish_login_review(confirmed, None);
        }
        // Batch 4: terms accepted.
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::AcceptTermsOfService)
        ) {
            self.notices.terms = None;
            self.notices.terms_in_flight = false;
            self.notices.terms_error = None;
        }
        // Batch 6: a 2FA step answered `ok` (cancel reset, login email
        // code check).
        if let Some(RequestPurpose::PasswordStateOp { op }) = pending.map(|p| p.purpose) {
            self.apply_password_op_ok(op);
        }
        // Batch 6: a storage limit option was accepted; TDLib echoes the
        // value as `updateOption`.
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::SetStorageOption)
        ) {
            self.data_storage_error = None;
        }
        // Slice A4: a `disconnectWebsite` /
        // `disconnectAllWebsites` succeeded — same stale pattern
        // on the websites list.
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::DisconnectWebsite { .. } | RequestPurpose::DisconnectAllWebsites)
        ) {
            self.websites_stale = true;
            self.websites_mutating = false;
            self.websites_error = None;
        }
        // Slice `parity:bots-payment-recurring`: an
        // `editStarSubscription` / `reuseStarSubscription` succeeded —
        // same stale pattern: the old cache stays visible until the
        // server-confirmed refetch replaces it (never optimistic).
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::EditStarSubscription | RequestPurpose::ReuseStarSubscription)
        ) {
            self.star_subscriptions_stale = true;
            self.star_subscriptions_mutating = false;
            self.star_subscriptions_error = None;
        }
        // Slice S8: a sticker-set mutation succeeded — invalidate
        // the affected cache so the next fetch shows the
        // server-confirmed list instead of a stale one.
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::AddFavoriteSticker | RequestPurpose::RemoveFavoriteSticker)
        ) {
            self.stickers.favorites.clear();
        }
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::ClearRecentStickers)
        ) {
            self.stickers.recent.clear();
        }
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::ChangeStickerSet | RequestPurpose::ReorderInstalledStickerSets)
        ) {
            self.invalidate_installed_sticker_sets();
        }
        if let Some(RequestPurpose::ManageStickerSet {
            set_id, installed, ..
        }) = pending.map(|p| p.purpose)
        {
            self.finish_sticker_batch_item(set_id, true);
            for sets in [&mut self.stickers.trending, &mut self.stickers.found_sets] {
                for set in sets.iter_mut().filter(|set| set.id == set_id) {
                    set.is_installed = installed;
                }
            }
            self.stickers.sets.clear();
            self.stickers.installed_loaded = false;
            self.stickers.archived.clear();
            self.stickers.archived_has_more = false;
            self.stickers.archived_next_offset = 0;
            if self.stickers.tab == StickerTab::Archived {
                self.stickers.selected_set_id = None;
                self.stickers.loaded_set_id = None;
                self.stickers.stickers.clear();
            }
        }
        // Slice S10: emoji mutations invalidate emoji caches (see emoji.rs).
        self.invalidate_emoji_caches(pending.map(|p| p.purpose));
        // Slice S9: a saved-GIF mutation (`addSavedAnimation` /
        // `removeSavedAnimation`) succeeded — drop the saved list so
        // the panel refetches the server-confirmed list instead of
        // a stale one.
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::AddSavedAnimation | RequestPurpose::RemoveSavedAnimation)
        ) {
            self.gifs.animations.clear();
            self.gifs.loaded = false;
        }
        // Phase C3a: a successful `leaveGroupCall` /
        // `endGroupCall` drops the tracked call (the `ok`
        // confirms the server side; `updateGroupCall`
        // `!is_active` is the backstop).
        match pending.map(|p| p.purpose) {
            Some(
                RequestPurpose::LeaveGroupCall { group_call_id }
                | RequestPurpose::EndGroupCall { group_call_id },
            ) if self
                .active_group_call
                .as_ref()
                .is_some_and(|c| c.id == group_call_id) =>
            {
                self.leave_group_call_local();
            }
            // Phase C2h: the server confirmed revocation — drop
            // the cached link so the UI stops showing it.
            Some(RequestPurpose::RevokeVideoChatInviteLink { group_call_id })
                if self
                    .active_group_call
                    .as_ref()
                    .is_some_and(|c| c.id == group_call_id) =>
            {
                if let Some(tracked) = self.active_group_call.as_mut() {
                    tracked.invite_link = None;
                }
            }
            // Slice G2: forum-topic mutations confirmed — drop
            // the cached topic list so the UI refetches it.
            Some(
                RequestPurpose::EditForumTopic { .. }
                | RequestPurpose::ToggleForumTopicClosed { .. }
                | RequestPurpose::ToggleForumTopicPinned { .. }
                | RequestPurpose::DeleteForumTopic { .. }
                | RequestPurpose::ToggleGeneralForumTopicHidden,
            ) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.forum_topics.remove(&chat_id.0);
                }
            }
            // Slice G2: welcome-message mutations confirmed —
            // drop the cached pack so the dialog refetches it.
            Some(
                RequestPurpose::AddChatWelcomeMessage
                | RequestPurpose::EditChatWelcomeMessage { .. }
                | RequestPurpose::DeleteChatWelcomeMessage { .. },
            ) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id) {
                    self.welcome_messages.remove(&chat_id.0);
                    self.welcome_message_fetches.remove(&chat_id.0);
                }
            }
            // Slice (communities backend core): a
            // `setCommunityName` succeeded — drop the cached
            // full-info pack so the driver refetches it; the new
            // name itself arrives via `updateCommunity`.
            Some(RequestPurpose::SetCommunityName) => {
                if let Some(community_id) = pending.and_then(|p| p.community_id) {
                    self.community_full_infos.remove(&community_id);
                }
            }
            // Phase 9.5: a posted-story management call landed —
            // clear the spinner; the edited story itself arrives
            // via `updateStory`.
            Some(
                RequestPurpose::EditStory
                | RequestPurpose::EditStoryCover
                | RequestPurpose::SetStoryPrivacySettings,
            ) => {
                self.story_manage.pending = false;
            }
            // Phase 9.7: `reorderStoryAlbums` confirmed — apply
            // the sent album order (correlated via
            // `PendingRequest::story_ids`); albums missing from
            // the order keep their relative order at the end.
            Some(RequestPurpose::ReorderStoryAlbums) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id)
                    && let Some(order) = pending.and_then(|p| p.story_ids.clone())
                    && let Some(albums) = self.story_albums.get_mut(&chat_id.0)
                {
                    let mut reordered = Vec::with_capacity(albums.len());
                    for id in &order {
                        if let Some(pos) = albums.iter().position(|a| a.id == *id) {
                            reordered.push(albums[pos].clone());
                        }
                    }
                    for album in albums.drain(..) {
                        if !reordered.iter().any(|a| a.id == album.id) {
                            reordered.push(album);
                        }
                    }
                    *albums = reordered;
                }
                self.succeed_story_page_op(RequestPurpose::ReorderStoryAlbums);
            }
            // Phase 9.7: `deleteStoryAlbum` confirmed — drop the
            // album (correlated via
            // `PendingRequest::story_album_id`) and its cached
            // story ids.
            Some(RequestPurpose::DeleteStoryAlbum) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id)
                    && let Some(album_id) = pending.and_then(|p| p.story_album_id)
                {
                    if let Some(albums) = self.story_albums.get_mut(&chat_id.0) {
                        albums.retain(|a| a.id != album_id);
                    }
                    self.story_album_stories.remove(&(chat_id.0, album_id));
                }
                self.succeed_story_page_op(RequestPurpose::DeleteStoryAlbum);
            }
            // Phase 9.7: `setChatPinnedStories` confirmed — the
            // sent story ids (correlated via
            // `PendingRequest::story_ids`) are the new pinned
            // list.
            Some(RequestPurpose::SetChatPinnedStories) => {
                if let Some(chat_id) = pending.and_then(|p| p.chat_id)
                    && let Some(ids) = pending.and_then(|p| p.story_ids.clone())
                {
                    self.chat_page_stories
                        .entry(chat_id.0)
                        .or_default()
                        .pinned_story_ids = ids;
                }
                self.succeed_story_page_op(RequestPurpose::SetChatPinnedStories);
            }
            _ => {}
        }
        // Phase C2i: `sendCallLog` confirmed — the log upload for
        // the ended call succeeded.
        if let Some(RequestPurpose::SendCallLog) = pending.map(|p| p.purpose)
            && let Some(summary) = self.call_summary.as_mut()
        {
            summary.log_sent = true;
            summary.log_error = None;
        }
        // Phase C2i: `setUserPrivacySettingRules` confirmed (the
        // new value was applied optimistically at send time).
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::SetCallPrivacyRules { .. })
        ) {
            self.privacy_roundtrip_done();
        }
        // Slice S3: `setReadDatePrivacySettings` confirmed — clear
        // the optimistic spinner (the value itself was set at
        // send time). `setUserPrivacySettingRules` /
        // `setMessageSenderBlockList` are fully optimistic: a
        // confirming `ok` needs no state change; failures are
        // handled in the error arm below.
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::SetReadDatePrivacy)
        ) {
            self.read_date_loading = false;
            self.read_date_error = false;
        }
        // Parity slice: `resetAllNotificationSettings` confirmed — drop the
        // cached scope defaults so the next fetch (or the authoritative
        // `updateScopeNotificationSettings` answers) shows the
        // server-confirmed defaults instead of the stale pre-reset ones.
        // (`notification_exceptions` lives on the sibling
        // settings-notif-exceptions branch; its per-chat updates prune it.)
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::ResetAllNotificationSettings)
        ) {
            self.scope_notification_settings.clear();
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::LoadChats) {
            // A short OK is not exhaustion; 404 is.
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::DeleteChatFolder)
            && let Some(folder_id) = pending.and_then(|p| p.folder_id)
        {
            // Parity slice: `deleteChatFolder` confirmed — drop the
            // tab and any cached spec. `updateChatFolders` stays the
            // source of truth and will confirm.
            self.chat_folders.retain(|f| f.id != folder_id);
            self.folder_specs.remove(&folder_id);
            self.folder_chats_exhausted.remove(&folder_id);
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::ToggleHasSponsoredMessagesEnabled)
            && let Some(chat_id) = pending.and_then(|p| p.chat_id)
        {
            self.accept_sponsored_hidden(chat_id);
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::ViewMessages)
            && let Some(chat_id) = pending.and_then(|p| p.chat_id)
        {
            self.commit_viewed(chat_id);
        }
        // Phase D3b: `setChatMemberStatus` confirmed — the member
        // change itself arrives as `updateChatMember`. Invalidate
        // the cached admin list so the panel refetches instead of
        // showing stale data.
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::SetChatMemberStatus { .. })
        ) && let Some(chat_id) = pending.and_then(|p| p.chat_id)
        {
            self.admin_lists.remove(&chat_id.0);
            // Slice G1: restrict/ban/unban change the member
            // lists too — drop all cached pages for this chat.
            if matches!(
                pending.map(|p| p.purpose),
                Some(RequestPurpose::SetChatMemberStatus {
                    kind: MemberStatusChange::Restrict
                        | MemberStatusChange::Ban
                        | MemberStatusChange::Unban
                        | MemberStatusChange::Remove,
                    ..
                })
            ) {
                self.supergroup_members
                    .retain(|(id, _), _| *id != chat_id.0);
                self.basic_group_members.remove(&chat_id.0);
            }
        }
        // Slice G1: `setChatMemberTag` confirmed — the custom
        // title itself arrives via `updateChatMember`; drop
        // cached member pages so the new tag is refetched
        // instead of showing stale data.
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::SetChatMemberTag { .. })
        ) && let Some(chat_id) = pending.and_then(|p| p.chat_id)
        {
            self.supergroup_members
                .retain(|(id, _), _| *id != chat_id.0);
            self.basic_group_members.remove(&chat_id.0);
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::LeaveChat)
            && let Some(chat_id) = pending.and_then(|p| p.chat_id)
            && let Some(chat) = self.chats.get_mut(&chat_id.0)
        {
            // Optimistic: `updateChatMember` confirms. TDLib errors
            // keep the old status (Error arm below does not touch it).
            chat.set_member_status(ChannelMemberStatus::Left, None);
        }
        // Slice G1: `deleteChat` confirmed — drop the chat locally.
        // The schema (1.8.67, line 11850) deletes the chat for all
        // members and releases the username; no update announces
        // it, so the client removes it itself.
        if pending.map(|p| p.purpose) == Some(RequestPurpose::DeleteChat)
            && let Some(chat_id) = pending.and_then(|p| p.chat_id)
        {
            self.chats.remove(&chat_id.0);
            self.supergroup_members
                .retain(|(id, _), _| *id != chat_id.0);
            self.admin_lists.remove(&chat_id.0);
            self.add_members_failed.remove(&chat_id.0);
        }
        // Phase D3a: `processChatJoinRequest` confirmed — drop the
        // processed request from the cached list. The count is
        // approximate per the schema; `updateChatPendingJoinRequests`
        // is the authoritative badge source.
        if let Some(RequestPurpose::ProcessChatJoinRequest { user_id }) = pending.map(|p| p.purpose)
            && let Some(chat_id) = pending.and_then(|p| p.chat_id)
            && let Some(JoinRequestFetch::Loaded(list)) =
                self.join_requests.get(&chat_id.0).cloned()
        {
            let requests: Vec<ParsedChatJoinRequest> = list
                .requests
                .into_iter()
                .filter(|r| r.user_id != user_id)
                .collect();
            self.join_requests.insert(
                chat_id.0,
                JoinRequestFetch::Loaded(JoinRequestList {
                    total_count: list.total_count.saturating_sub(1),
                    requests,
                }),
            );
        }
        // B10: the own profile photos changed — refetch the gallery.
        if matches!(
            pending.map(|p| p.purpose),
            Some(RequestPurpose::SetProfilePhoto | RequestPurpose::DeleteProfilePhoto)
        ) && let Some(me) = self.my_user_id
        {
            self.user_profile_photos.remove(&me);
        }
        if pending.map(|p| p.purpose) == Some(RequestPurpose::AddContact) {
            // Phase 6: the new contact arrives via `updateUser`
            // (`is_contact` flips); invalidate the list so the
            // contacts tab refetches it.
            self.contacts = None;
            self.contacts_error = false;
        }
        // Slice A6: a contacts mutation landed — never optimistic:
        // the new list arrives via the `getContacts` refetch the
        // tab triggers.
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::RemoveContact) => {
                self.contacts = None;
                self.contacts_error = false;
                // The delete-synced-contacts batch remove shares
                // this purpose but carries no user_id — its notice
                // must not read as a single delete (or overwrite
                // the synced flow's own notice on arrival order).
                self.contacts_notice = Some(if pending.and_then(|p| p.user_id).is_some() {
                    "Contact deleted.".to_string()
                } else {
                    "Synced contacts deleted from the servers.".to_string()
                });
                // Slice A6: the server confirmed the deletion —
                // drop the contact flag on the cached user too so
                // the info panel stops offering "Delete contact"
                // before the refetched list arrives.
                if let Some(user_id) = pending.and_then(|p| p.user_id)
                    && let Some(user) = self.users.get_mut(&user_id)
                {
                    user.is_contact = false;
                }
            }
            Some(RequestPurpose::ImportContacts) => {
                self.contacts = None;
                self.contacts_error = false;
                self.contacts_notice = Some("Contacts imported.".to_string());
            }
            Some(RequestPurpose::ClearImportedContacts) => {
                self.contacts = None;
                self.contacts_error = false;
                self.contacts_notice =
                    Some("Synced contacts deleted from the servers.".to_string());
            }
            _ => {}
        }
        // Slice A6: a user-scoped `setMessageSenderBlockList`
        // succeeded — the `ok` carries no state, but the request
        // we just confirmed does, so the cached
        // `UserFullInfoData.blocked` is updated authoritatively
        // (never flipped optimistically). Chat-scoped (CL3)
        // requests carry no user_id and keep flowing through
        // `updateChatBlockList`.
        if let Some(p) = pending
            && let RequestPurpose::SetMessageSenderBlockList { block } = p.purpose
            && let Some(user_id) = p.user_id
            && let Some(info) = self.user_full_infos.get_mut(&user_id)
        {
            info.blocked = block;
        }
        if pending.is_some_and(|p| is_auth_submit(p.purpose)) {
            self.last_auth_error = None;
        }
    }
}
