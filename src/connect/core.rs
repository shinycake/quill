//! Connect driver core: construction, the update pump, and lifecycle.
use super::*;
use crate::composer::DraftSaveClock;
use crate::credentials::TelegramCredentials;
use crate::ids::{ChatId, MessageId, RequestId};
use crate::state::{ComposerLinkPreview, InstantViewPage, RequestPurpose, Session, ShutdownPhase};
use crate::telegram::client::OwnedEnvelope;
use crate::telegram::envelope::{
    AuthorizationState, EnvelopePayload, MessageContent, RichMessageContent, UsernameCheckResult,
};
use crate::telegram::requests::{
    close_request, get_authorization_state, load_archive_chats, load_chats, log_out,
};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

/// TDLib's media cache folders inside the database directory (TDLib
/// `FileType` directories for stickers, thumbnails, profile photos and
/// wallpapers).
const TDLIB_DATABASE_MEDIA_DIRS: [&str; 4] =
    ["stickers", "thumbnails", "profile_photos", "wallpapers"];
use std::sync::{Arc, Mutex};

/// Status note for a rich AI answer. Create / fix / rewrite must not share
/// one label — only create actually created the draft.
fn ai_rich_draft_note(purpose: RequestPurpose) -> &'static str {
    match purpose {
        RequestPurpose::FixRichMessageWithAi => "AI fixed the draft",
        RequestPurpose::ComposeRichMessageWithAi => "AI rewrote the draft",
        RequestPurpose::CreateRichMessageWithAi => "AI created the draft",
        _ => "AI updated the draft",
    }
}

impl<S: JsonSender> ConnectDriver<S> {
    pub fn new(
        session: Session,
        sender: S,
        credentials: TelegramCredentials,
        prepared: PreparedConnect,
    ) -> Self {
        Self {
            session,
            sender,
            call_engine: None,
            signaling_outbox: Arc::new(Mutex::new(VecDeque::new())),
            transport_outbox: Arc::new(Mutex::new(VecDeque::new())),
            video_state_outbox: Arc::new(Mutex::new(VecDeque::new())),
            screen_state_outbox: Arc::new(Mutex::new(VecDeque::new())),
            video_frame_slots: Arc::new(Mutex::new(HashMap::new())),
            group_video_frame_slots: Arc::new(Mutex::new(HashMap::new())),
            group_camera_state: HashMap::new(),
            selected_camera: None,
            call_devices_cache: Vec::new(),
            selected_devices: (None, None),
            call_connect_params: None,
            reconnect_attempts: 0,
            credentials,
            paths: prepared.paths,
            database_key: prepared.database_key,
            parameters_sent: false,
            search_debounce_token: 0,
            pending_typed_search: None,
            chat_search_debounce_token: 0,
            pending_typed_chat_search: None,
            outgoing_typing: None,
            outgoing_voice: None,
            draft_clock: DraftSaveClock::idle(),
            draft_save_token: 0,
            pending_draft: None,
        }
    }

    pub fn parameters_sent(&self) -> bool {
        self.parameters_sent
    }

    pub fn tdlib_files(&self) -> &Path {
        &self.paths.tdlib_files
    }

    /// Every folder TDLib downloads displayable media into. Besides
    /// `files_directory`, TDLib keeps stickers, thumbnails, profile photos
    /// and wallpapers under the database directory, as part of its cache.
    pub fn tdlib_media_roots(&self) -> Vec<PathBuf> {
        let mut roots = vec![self.paths.tdlib_files.clone()];
        roots.extend(
            TDLIB_DATABASE_MEDIA_DIRS
                .iter()
                .map(|dir| self.paths.tdlib_database.join(dir)),
        );
        roots
    }

    /// Kick the JSON client so authorization updates start flowing.
    pub fn kickoff(&mut self) -> Result<RequestId, ConnectSendError> {
        let extra = self
            .session
            .request(RequestPurpose::GetAuthorizationState, None);
        self.sender.send_json(&get_authorization_state(extra))?;
        Ok(extra)
    }

    pub fn ingest(&mut self, owned: OwnedEnvelope) -> Result<(), ConnectSendError> {
        let was_ready = matches!(self.session.auth, AuthorizationState::Ready);
        let active_call_before = self.session.active_call.as_ref().map(|call| call.id);
        let active_group_call_before = self.session.active_group_call.as_ref().map(|call| call.id);
        let bridge_signaling = match &owned.envelope.payload {
            EnvelopePayload::UpdateNewCallSignalingData { call_id, data } => {
                Some((*call_id, data.clone()))
            }
            _ => None,
        };
        // Continue paging only when this envelope completes an in-flight loadChats
        // with ok. Unrelated ingest ticks and non-404 errors must not re-issue.
        let load_chats_ok = matches!(owned.envelope.payload, EnvelopePayload::Ok)
            && owned.envelope.extra.is_some_and(|id| {
                self.session.requests.purpose(id) == Some(RequestPurpose::LoadChats)
            });
        // The archive pages the same way, starting once the main list is
        // exhausted (its 404 lands in this envelope).
        let load_archive_ok = matches!(owned.envelope.payload, EnvelopePayload::Ok)
            && owned.envelope.extra.is_some_and(|id| {
                self.session.requests.purpose(id) == Some(RequestPurpose::LoadArchiveChats)
            });
        let main_was_exhausted = self.session.chats_exhausted;
        // Parity slice: a folder `loadChats` page completing with ok pages
        // on (until a 404 marks the folder exhausted in the reducer).
        // Captured before `apply` takes the pending request.
        let folder_load_ok: Option<i32> = match owned.envelope.payload {
            EnvelopePayload::Ok => owned.envelope.extra.and_then(|id| {
                (self.session.requests.purpose(id) == Some(RequestPurpose::LoadFolderChats))
                    .then(|| self.session.requests.folder_id_for(id))
                    .flatten()
            }),
            _ => None,
        };
        let view_purpose = owned
            .envelope
            .extra
            .and_then(|id| self.session.requests.purpose(id));
        let view_after = match &owned.envelope.payload {
            EnvelopePayload::Messages(_) | EnvelopePayload::UpdateNewMessage(_) => true,
            EnvelopePayload::Ok | EnvelopePayload::Error(_)
                if view_purpose == Some(RequestPurpose::ViewMessages) =>
            {
                true
            }
            _ => false,
        };
        let sticker_mutation_ok = matches!(owned.envelope.payload, EnvelopePayload::Ok)
            && matches!(
                view_purpose,
                Some(RequestPurpose::AddFavoriteSticker | RequestPurpose::RemoveFavoriteSticker)
            );
        let sticker_set_changed = matches!(owned.envelope.payload, EnvelopePayload::Ok)
            && matches!(
                view_purpose,
                Some(
                    RequestPurpose::ManageStickerSet { .. }
                        | RequestPurpose::ReorderInstalledStickerSets
                )
            );
        let installed_stickers_answer = view_purpose
            == Some(RequestPurpose::GetInstalledStickerSets)
            && matches!(owned.envelope.payload, EnvelopePayload::StickerSets { .. });
        let archive_catalog_changed = sticker_set_changed
            && matches!(view_purpose, Some(RequestPurpose::ManageStickerSet { .. }));
        let emoji_trending_answer = view_purpose == Some(RequestPurpose::GetTrendingEmojiSets)
            && matches!(
                owned.envelope.payload,
                EnvelopePayload::TrendingStickerSets { .. }
            )
            && self.session.emoji.open
            && self.session.emoji.tab == crate::emoji::EmojiSetTab::Trending;
        let emoji_catalog_changed = (matches!(owned.envelope.payload, EnvelopePayload::Ok)
            && view_purpose == Some(RequestPurpose::ChangeEmojiSet))
            || matches!(
                owned.envelope.payload,
                EnvelopePayload::UpdateInstalledStickerSets {
                    is_regular: false,
                    ..
                }
            );
        let gif_saved_changed = matches!(
            owned.envelope.payload,
            EnvelopePayload::UpdateSavedAnimations { .. }
        );
        let gif_mutation_ok = matches!(owned.envelope.payload, EnvelopePayload::Ok)
            && matches!(
                view_purpose,
                Some(RequestPurpose::AddSavedAnimation | RequestPurpose::RemoveSavedAnimation)
            );
        let gif_bot_changed = match &owned.envelope.payload {
            EnvelopePayload::UpdateOption { name, value }
                if name == "animation_search_bot_username" =>
            {
                match value {
                    crate::telegram::envelope::OptionValue::String(name) => {
                        *name != self.session.gifs.search_bot_username
                    }
                    _ => !self.session.gifs.search_bot_username.is_empty(),
                }
            }
            _ => false,
        };
        let recent_cleared = matches!(owned.envelope.payload, EnvelopePayload::Ok)
            && view_purpose == Some(RequestPurpose::ClearRecentStickers);
        let trending_answer = matches!(
            owned.envelope.payload,
            EnvelopePayload::TrendingStickerSets { .. }
        ) && view_purpose == Some(RequestPurpose::GetTrendingStickerSets);
        let thumbs_after = matches!(
            owned.envelope.payload,
            EnvelopePayload::Messages(_)
                | EnvelopePayload::UpdateNewMessage(_)
                | EnvelopePayload::UpdateMessageContent { .. }
                | EnvelopePayload::UpdateFile(_)
                | EnvelopePayload::File(_)
        );
        let chat_search_hits = matches!(
            owned.envelope.payload,
            EnvelopePayload::FoundChatMessages { .. }
        );
        // M2: capture the `getFullRichMessage` answer before `apply`
        // takes the pending request; the full blocks replace the
        // partial message's blocks in history after apply.
        let full_rich_answer: Option<(ChatId, MessageId, RichMessageContent)> =
            match &owned.envelope.payload {
                EnvelopePayload::RichMessage { rich } => owned
                    .envelope
                    .extra
                    .and_then(|id| self.session.requests.purpose(id))
                    .and_then(|purpose| match purpose {
                        RequestPurpose::GetFullRichMessage {
                            chat_id,
                            message_id,
                        } => Some((chat_id, message_id, rich.clone())),
                        _ => None,
                    }),
                _ => None,
            };
        // M1: capture the `getMessageLink` answer before `apply` takes the
        // pending request; the UI drains `Session::message_link_result`
        // into the clipboard.
        let message_link_answer: Option<String> = match &owned.envelope.payload {
            EnvelopePayload::MessageLink { link, .. } => owned
                .envelope
                .extra
                .and_then(|id| self.session.requests.purpose(id))
                .is_some_and(|purpose| purpose == RequestPurpose::GetMessageLink)
                .then(|| link.clone()),
            _ => None,
        };
        // Slice msg-richtext-ai-tools: capture AI text answers
        // (`fixTextWithAi` → `fixedText`, `composeTextWithAi` →
        // `formattedText`) before `apply` takes the pending request. The
        // chat id rides `PendingRequest::chat_id` (the purposes are unit
        // variants); the UI drains `Session::ai_composer_text` into the
        // composer draft.
        let ai_text_answer: Option<(ChatId, String)> = match &owned.envelope.payload {
            EnvelopePayload::FixedText { text, .. } | EnvelopePayload::FormattedText { text } => {
                owned
                    .envelope
                    .extra
                    .and_then(|id| self.session.requests.get(id))
                    .filter(|pending| {
                        matches!(
                            pending.purpose,
                            RequestPurpose::FixTextWithAi | RequestPurpose::ComposeTextWithAi
                        )
                    })
                    .and_then(|pending| pending.chat_id.map(|chat_id| (chat_id, text.clone())))
            }
            _ => None,
        };
        // Slice msg-richtext-ai-tools: capture AI rich-message answers
        // (`composeRichMessageWithAi` / `createRichMessageWithAi` /
        // `fixRichMessageWithAi` → `richMessage`) before `apply` takes
        // the pending request. Same drain contract as the text answers.
        // The UI serializes the blocks back to editor markup; the note
        // records which method answered.
        let ai_rich_answer: Option<(ChatId, RichMessageContent, &'static str)> =
            match &owned.envelope.payload {
                EnvelopePayload::RichMessage { rich } => owned
                    .envelope
                    .extra
                    .and_then(|id| self.session.requests.get(id))
                    .filter(|pending| {
                        matches!(
                            pending.purpose,
                            RequestPurpose::ComposeRichMessageWithAi
                                | RequestPurpose::CreateRichMessageWithAi
                                | RequestPurpose::FixRichMessageWithAi
                        )
                    })
                    .and_then(|pending| {
                        let note = ai_rich_draft_note(pending.purpose);
                        pending.chat_id.map(|chat_id| (chat_id, rich.clone(), note))
                    }),
                _ => None,
            };
        // M1 fix-up: capture the `getMessageProperties` answer for the
        // "Share link" gate before `apply` takes the pending request.
        let link_gate: Option<(ChatId, MessageId, bool)> = match &owned.envelope.payload {
            EnvelopePayload::MessageProperties(actions) => owned
                .envelope
                .extra
                .and_then(|id| self.session.requests.purpose(id))
                .and_then(|purpose| match purpose {
                    RequestPurpose::GetMessageLinkProperties {
                        chat_id,
                        message_id,
                    } => Some((chat_id, message_id, actions.can_get_link)),
                    _ => None,
                }),
            _ => None,
        };
        // A5: capture the `checkChatUsername` verdict before `apply`
        // takes the pending request. The verdict is stashed with the
        // in-flight username text so the edit-profile dialog can ignore
        // stale answers for superseded text.
        let username_check_answer: Option<(String, UsernameCheckResult)> =
            match &owned.envelope.payload {
                EnvelopePayload::CheckChatUsernameResult(result) => owned
                    .envelope
                    .extra
                    .and_then(|id| self.session.requests.purpose(id))
                    .and_then(|purpose| {
                        self.session
                            .username_check_pending
                            .clone()
                            .filter(|_| purpose == RequestPurpose::CheckUsername)
                            .map(|username| (username, *result))
                    }),
                _ => None,
            };
        // MED4: capture the `getWebPageInstantView` answer before `apply`
        // takes the pending request; the UI drains
        // `Session::instant_view` into the IV reader. The URL rides
        // `Session::instant_view_urls` (the purpose stays `Copy`).
        let instant_view_answer: Option<(String, RichMessageContent)> =
            match &owned.envelope.payload {
                EnvelopePayload::WebPageInstantView { rich } => owned
                    .envelope
                    .extra
                    .and_then(|id| {
                        (self.session.requests.purpose(id)
                            == Some(RequestPurpose::GetWebPageInstantView))
                        .then_some(id)
                    })
                    .and_then(|id| {
                        self.session
                            .instant_view_urls
                            .remove(&id)
                            .map(|url| (url, rich.clone()))
                    }),
                _ => None,
            };
        // MED4: a failed `getWebPageInstantView` (TDLib 404 = the page has
        // no Instant View) falls back to the browser like TGX — the URL
        // is stashed for the UI drain, never rendered as a reader.
        let instant_view_fallback: Option<String> = match &owned.envelope.payload {
            EnvelopePayload::Error(_) => owned.envelope.extra.and_then(|id| {
                (self.session.requests.purpose(id) == Some(RequestPurpose::GetWebPageInstantView))
                    .then_some(id)
                    .and_then(|id| self.session.instant_view_urls.remove(&id))
            }),
            _ => None,
        };
        // MED4b: capture the `getLinkPreview` answer before `apply` takes
        // the pending request; the composer chip reads
        // `Session::composer_preview`. A late answer for a superseded URL
        // is dropped (the chip only cares about the latest request).
        let link_preview_answer: Option<ComposerLinkPreview> = match &owned.envelope.payload {
            EnvelopePayload::LinkPreview { preview } => owned
                .envelope
                .extra
                .and_then(|id| {
                    (self.session.requests.purpose(id) == Some(RequestPurpose::GetLinkPreview))
                        .then_some(id)
                })
                .and_then(|id| {
                    self.session
                        .composer_preview_urls
                        .remove(&id)
                        .map(|url| ComposerLinkPreview {
                            url,
                            preview: Some(preview.clone()),
                        })
                }),
            _ => None,
        };
        // MED4b: a failed `getLinkPreview` (TDLib 404 = no preview for
        // this URL) is "no link info" (TGX `LinkPreview.isNotFound`) —
        // never a card, never a crash.
        let link_preview_failed: Option<String> = match &owned.envelope.payload {
            EnvelopePayload::Error(_) => owned.envelope.extra.and_then(|id| {
                (self.session.requests.purpose(id) == Some(RequestPurpose::GetLinkPreview))
                    .then_some(id)
                    .and_then(|id| self.session.composer_preview_urls.remove(&id))
            }),
            _ => None,
        };
        // Slice CL2: our `createPrivateChat` answer — the bare `chat`
        // parses as `UpdateNewChat`; the `@extra` tells it apart from a
        // genuine `updateNewChat`. Captured before `apply` takes the
        // pending request; opened through the normal `select_chat`
        // flow (openChat + history) after apply inserts the chat.
        let created_chat: Option<ChatId> = match &owned.envelope.payload {
            EnvelopePayload::UpdateNewChat { chat_id, .. } => owned
                .envelope
                .extra
                .and_then(|id| self.session.requests.purpose(id))
                .and_then(|purpose| {
                    (purpose == RequestPurpose::CreatePrivateChat).then_some(*chat_id)
                }),
            _ => None,
        };
        // Slice G2: capture forum/welcome/boost mutations before `apply`
        // takes the pending request. The state drops the stale cache on
        // confirmed success; the post-apply refetch reloads it now that
        // the server has applied the change (never pre-confirmation).
        let mutation_refetch: Option<(RequestPurpose, ChatId)> = owned
            .envelope
            .extra
            .and_then(|id| self.session.requests.get(id))
            .and_then(|pending| match pending.purpose {
                RequestPurpose::CreateForumTopic
                | RequestPurpose::EditForumTopic { .. }
                | RequestPurpose::ToggleForumTopicClosed { .. }
                | RequestPurpose::ToggleForumTopicPinned { .. }
                | RequestPurpose::DeleteForumTopic { .. }
                | RequestPurpose::ToggleGeneralForumTopicHidden
                | RequestPurpose::AddChatWelcomeMessage
                | RequestPurpose::EditChatWelcomeMessage { .. }
                | RequestPurpose::DeleteChatWelcomeMessage { .. }
                | RequestPurpose::BoostChat => {
                    pending.chat_id.map(|chat_id| (pending.purpose, chat_id))
                }
                _ => None,
            });
        // Slice (communities backend core): the `createCommunity` answer
        // is `communityId`; `updateCommunity` for the new community is
        // guaranteed to have arrived first, so the id chains straight
        // into `loadCommunityFullInfo` after apply.
        let created_community_id: Option<i64> = match &owned.envelope.payload {
            EnvelopePayload::CommunityId { id } => owned
                .envelope
                .extra
                .and_then(|id| self.session.requests.purpose(id))
                .and_then(|purpose| (purpose == RequestPurpose::CreateCommunity).then_some(*id)),
            _ => None,
        };
        // Slice (communities backend core): a confirmed `setCommunityName`
        // drops the cached full-info pack (see `Session::apply`); capture
        // before apply so the post-apply refetch reloads it. A dropped
        // cache is the success signal — on a TDLib error the cache stays
        // and nothing refetches.
        let community_mutation_refetch: Option<i64> = owned
            .envelope
            .extra
            .and_then(|id| self.session.requests.get(id))
            .and_then(|pending| {
                (pending.purpose == RequestPurpose::SetCommunityName)
                    .then_some(pending.community_id)
                    .flatten()
            });
        // Slice S4: a confirmed `removeAllFilesFromDownloads` drops the
        // cached storage stats (see `Session::apply`) — capture before
        // apply so the post-apply refetch shows the post-clear numbers.
        // A dropped cache is the success signal: on a TDLib error the
        // cache stays and nothing refetches.
        let cleared_download_cache = matches!(&owned.envelope.payload, EnvelopePayload::Ok)
            && owned.envelope.extra.is_some_and(|id| {
                self.session.requests.purpose(id)
                    == Some(RequestPurpose::RemoveAllFilesFromDownloads)
            });
        let used_emoji: Vec<_> = match &owned.envelope.payload {
            EnvelopePayload::UpdateMessageSendSucceeded { message, .. } if message.is_outgoing => {
                if let crate::telegram::envelope::MessageContent::Text(text) = &message.content {
                    text.entities
                        .iter()
                        .filter_map(|entity| match entity.kind {
                            crate::text::TextEntityKind::CustomEmoji { custom_emoji_id } => {
                                Some(custom_emoji_id)
                            }
                            _ => None,
                        })
                        .collect()
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        };
        // A pin in the open chat refetches its pinned list (the pinned
        // message may not be loaded); a confirmed unpin-all empties it.
        let pins_changed: Option<ChatId> = match &owned.envelope.payload {
            EnvelopePayload::UpdateMessageIsPinned {
                chat_id,
                is_pinned: true,
                ..
            } => Some(*chat_id),
            _ => None,
        };
        let unpinned_all: Option<ChatId> = match &owned.envelope.payload {
            EnvelopePayload::Ok => owned
                .envelope
                .extra
                .and_then(|id| self.session.requests.get(id))
                .filter(|pending| pending.purpose == RequestPurpose::UnpinAllChatMessages)
                .and_then(|pending| pending.chat_id),
            _ => None,
        };
        let recent_packs = self.session.media_prefs.recent_emoji_packs.clone();
        let recent_emoji = self.session.media_prefs.recent_custom_emoji_ids.clone();
        let topic_chat = Self::possible_topic_chat(&owned.envelope.payload);
        let topic_refresh = Self::possible_topic_refresh(&owned.envelope.payload);
        let previous_seq = self.session.last_seq;
        self.session.apply(owned);
        self.maybe_fetch_bot_topics(topic_chat);
        self.maybe_refresh_forum_topic(topic_refresh);
        if self.session.last_seq != previous_seq {
            self.session.remember_emoji_pack_usage(&used_emoji);
        }
        if (recent_packs != self.session.media_prefs.recent_emoji_packs
            || recent_emoji != self.session.media_prefs.recent_custom_emoji_ids)
            && self.save_media_prefs().is_err()
        {
            self.session.chat_action_error =
                Some("Could not save emoji pack order. Retry in settings.".into());
        }
        // Slice S4: persist per-network settings seeded from
        // `getAutoDownloadSettingsPresets` (the reducer cannot touch the
        // filesystem, so it marks them dirty instead).
        if self.session.data_storage_dirty {
            self.session.data_storage_dirty = false;
            let _ = self.save_data_storage_prefs();
        }
        if cleared_download_cache {
            let _ = self.maybe_fetch_storage_statistics();
        }
        if let Some(chat_id) = unpinned_all {
            self.session.pinned_messages.insert(chat_id.0, Vec::new());
        }
        if let Some(chat_id) = pins_changed.filter(|chat| self.session.open_chat == Some(*chat)) {
            let _ = self.fetch_pinned_messages(chat_id);
        }
        self.pump_call_engine(active_call_before, bridge_signaling)?;
        self.pump_group_call_transport(active_group_call_before)?;
        self.maybe_send_parameters()?;
        self.maybe_probe_channel_membership()?;
        // Slice G2: chain `boostChat` once the slots answer arrives.
        self.maybe_continue_boost()?;
        // Slice G2: refetch caches the state dropped after a confirmed
        // mutation. A dropped cache is the success signal — on a TDLib
        // error the cache stays and nothing refetches.
        if let Some((purpose, chat_id)) = mutation_refetch {
            match purpose {
                RequestPurpose::CreateForumTopic
                | RequestPurpose::EditForumTopic { .. }
                | RequestPurpose::ToggleForumTopicClosed { .. }
                | RequestPurpose::ToggleForumTopicPinned { .. }
                | RequestPurpose::DeleteForumTopic { .. }
                | RequestPurpose::ToggleGeneralForumTopicHidden
                    if !self.session.forum_topics.contains_key(&chat_id.0) =>
                {
                    let _ = self.refresh_forum_topics(chat_id);
                }
                RequestPurpose::AddChatWelcomeMessage
                | RequestPurpose::EditChatWelcomeMessage { .. }
                | RequestPurpose::DeleteChatWelcomeMessage { .. }
                    if !self.session.welcome_messages.contains_key(&chat_id.0) =>
                {
                    let _ = self.load_chat_welcome_messages(chat_id);
                }
                RequestPurpose::BoostChat
                    if !self.session.chat_boost_status.contains_key(&chat_id.0) =>
                {
                    let _ = self.fetch_chat_boost_status(chat_id);
                }
                _ => {}
            }
        }
        // Slice CL2: open the `createPrivateChat` chat (Saved Messages
        // flow) through the normal chat-open path — `openChat` and
        // history load. `let _` on purpose: the chat is already in the
        // model; a failed open must not fail the ingest.
        if let Some(chat_id) = created_chat {
            let _ = self.select_chat(chat_id);
        }
        // Slice (communities backend core): resolve a just-created
        // community id into its full-info pack. `let _` on purpose: the
        // community is already in the model; a failed send must not
        // fail the ingest.
        if let Some(community_id) = created_community_id {
            let _ = self.load_community_full_info(community_id);
        }
        // Slice (communities backend core): refetch the full-info pack
        // the state dropped after a confirmed `setCommunityName`.
        if let Some(community_id) = community_mutation_refetch
            && !self
                .session
                .community_full_infos
                .contains_key(&community_id)
        {
            let _ = self.load_community_full_info(community_id);
        }
        let became_ready = !was_ready && matches!(self.session.auth, AuthorizationState::Ready);
        if became_ready || load_chats_ok {
            self.maybe_load_main_chats()?;
        }
        if load_archive_ok || (!main_was_exhausted && self.session.chats_exhausted) {
            self.maybe_load_archive_chats()?;
        }
        if let Some(folder_id) = folder_load_ok {
            self.maybe_load_folder_chats(folder_id)?;
        }
        // Parity slice: queued remove-from-folder edits go out once the
        // `getChatFolder` spec arrives.
        self.maybe_finish_folder_removals()?;
        if became_ready {
            // Phase 9.1: the story tray needs `updateChatActiveStories`
            // updates; one `loadActiveStories(storyListMain)` per Ready.
            self.maybe_load_active_stories()?;
            // Parity slice: saved notification sounds (picker + custom-sound
            // playback) and per-scope default settings (`use_default_*`
            // fallback), once per Ready.
            let _ = self.maybe_fetch_notification_sounds();
            let _ = self.maybe_fetch_scope_notification_settings();
        }
        // Parity slice: `updateSavedNotificationSounds` may have marked the
        // list stale between ingests.
        let _ = self.refresh_notification_sounds_if_stale();
        // Slice A3: a `terminateSession` / `terminateAllOtherSessions`
        // `ok` marks the sessions list stale in the reducer; refetch the
        // authoritative answer on the same ingest.
        let _ = self.refresh_active_sessions_if_stale();
        // Slice A4: a `disconnectWebsite` / `disconnectAllWebsites` `ok`
        // marks the websites list stale in the reducer; same pattern.
        let _ = self.refresh_connected_websites_if_stale();
        // Slice `parity:bots-payment-recurring`: an
        // `editStarSubscription` / `reuseStarSubscription` `ok` marks the
        // subscriptions list stale in the reducer; same pattern.
        let _ = self.refresh_star_subscriptions_if_stale();
        if view_after {
            self.maybe_view_open_messages()?;
        }
        // M2: full rich blocks replace the partial message's blocks in
        // history. Only `RichMessage` rows are touched — an unrelated
        // response can never clobber a different content kind.
        if let Some((chat_id, message_id, rich)) = full_rich_answer
            && let Some(history) = self.session.histories.get_mut(&chat_id.0)
            && let Some(message) = history.messages.get_mut(&message_id.0)
            && let MessageContent::RichMessage(existing) = &mut message.content
        {
            *existing = rich;
        }
        // Corner "@" / heart buttons: jump to the oldest unread marker the
        // search found. A failed jump (closed chat) just drops it.
        if let Some(message_id) = self.session.unread_jump.take() {
            let _ = self.jump_to_chat_search_message(message_id);
        }
        // M1: stash the `getMessageLink` answer for the UI clipboard drain.
        if let Some(link) = message_link_answer {
            self.session.message_link_result = Some(link);
        }
        // Slice msg-richtext-ai-tools: stash AI answers for the composer
        // drain. A late answer for a chat the user has since left is
        // dropped by the UI (chat-id check), never applied blindly.
        if let Some((chat_id, text)) = ai_text_answer {
            self.session.ai_composer_text = Some((chat_id, text));
        }
        if let Some((chat_id, rich, note)) = ai_rich_answer {
            self.session.ai_composer_blocks = Some((chat_id, rich, note));
        }
        // A5: stash the `checkChatUsername` verdict for the
        // edit-profile dialog.
        if let Some((username, result)) = username_check_answer {
            self.session.username_check = Some((username, result));
        }
        // MED4: stash the `getWebPageInstantView` answer (success →
        // IV reader; error → browser fallback) for the UI drains.
        if let Some((url, rich)) = instant_view_answer {
            self.session.instant_view = Some(InstantViewPage { url, rich });
        }
        if let Some(url) = instant_view_fallback {
            self.session.instant_view_fallback_url = Some(url);
        }
        // MED4b: stash the `getLinkPreview` answer for the composer
        // chip; a 404 becomes "no link info" (`Some(None)`); answers for
        // superseded URLs are dropped.
        if let Some(answer) = link_preview_answer {
            let current = self.session.composer_preview.as_ref();
            if current.is_none_or(|p| p.url == answer.url) {
                self.session.composer_preview = Some(answer);
            }
        }
        if let Some(url) = link_preview_failed
            && self
                .session
                .composer_preview
                .as_ref()
                .is_some_and(|p| p.url == url && p.preview.is_none())
        {
            self.session.composer_preview = Some(ComposerLinkPreview {
                url,
                preview: Some(None),
            });
        }
        // M1 fix-up: "Share link" gate — chain to `getMessageLink` only
        // when `messageProperties.can_get_link` passed; otherwise tell
        // the user instead of silently doing nothing.
        if let Some((chat_id, message_id, can_get_link)) = link_gate {
            if can_get_link {
                let _ = self.send_message_link_request(chat_id, message_id);
            } else {
                self.session.message_link_error =
                    Some("message link not available for this message".into());
            }
        }
        if emoji_trending_answer {
            self.mark_emoji_packs_viewed()?;
        }
        if emoji_catalog_changed {
            self.refresh_emoji_pack_catalog()?;
        }
        self.maybe_resolve_emoji_status_choices()?;
        self.maybe_resolve_message_custom_emoji()?;
        if !self.session.emoji.custom_emoji_stickers.is_empty() {
            let files = self.session.open_chat_custom_emoji_files();
            self.ensure_media_files(&files)?;
        }
        if gif_bot_changed {
            self.cancel_gif_search_requests();
            self.session.gifs.search_results.clear();
            self.session.gifs.search_next_offset.clear();
            self.session.gifs.search_loading = self.session.gifs.search_mode;
        }
        if self.chats_path_active()
            && self.session.gifs.open
            && self.session.gifs.search_mode
            && self.session.gifs.search_loading
        {
            self.maybe_search_gifs(false)?;
        }
        if gif_mutation_ok || gif_saved_changed {
            drop(
                self.session
                    .requests
                    .take_purpose(RequestPurpose::GetSavedAnimations),
            );
        }
        if installed_stickers_answer
            && self.session.stickers.suggest_waiting_for_sets
            && let Some(emoji) = self.session.stickers.suggest_for.take()
        {
            self.update_sticker_suggestions(&emoji)?;
        }
        if view_after {
            let _ = self.maybe_fetch_replied_messages();
        }
        if thumbs_after
            || self.session.stickers.open
            || self.session.gifs.open
            || self.session.emoji.open
            || !self.session.stickers.suggestions.is_empty()
        {
            self.maybe_download_open_thumbs()?;
            self.maybe_download_open_chat_media()?;
        }
        // Parity slice: chat-list avatars download on every ingest; each
        // photo is requested at most once (in-flight / completed dedupe).
        self.maybe_download_chat_list_photos()?;
        // Phase B1: secret chats whose state never arrived via
        // `updateSecretChat` (e.g. loaded from the local DB) resolve it
        // through the offline `getSecretChat`.
        let _ = self.maybe_fetch_secret_chat_states();
        // Phase C1: incoming calls that arrived while another call was
        // active are declined (busy).
        let _ = self.maybe_decline_busy_calls();
        // Swap prompt: the queued post-swap acceptCall fires once the
        // old call's terminal updateCall has cleared active_call.
        let _ = self.maybe_accept_queued_swap();
        // Phase C3a: freshly created voice chats get their full
        // `groupCall` via `getGroupCall`.
        let _ = self.maybe_fetch_group_calls();
        // stories-live-play: the story viewer's pending "Join live"
        // fires `join_video_chat` once the `getGroupCall` answer has
        // created the unjoined tracker.
        let _ = self.maybe_join_live_story();
        // Phase C2f: a dropped group call (`need_rejoin`) auto-rejoins
        // with the C2d attempt discipline (max 3).
        let _ = self.maybe_auto_rejoin_group_call();
        if archive_catalog_changed {
            drop(
                self.session
                    .requests
                    .take_purpose(RequestPurpose::GetArchivedStickerSets),
            );
            if self.session.stickers.open
                && self.session.stickers.tab == crate::state::StickerTab::Archived
            {
                drop(
                    self.session
                        .requests
                        .take_purpose(RequestPurpose::GetStickerSet),
                );
                self.session.stickers.loading_set = false;
                self.fetch_archived_stickers(false)?;
            }
        }
        if sticker_set_changed {
            drop(
                self.session
                    .requests
                    .take_purpose(RequestPurpose::GetInstalledStickerSets),
            );
            if self.session.stickers.open {
                self.refresh_installed_sticker_sets()?;
            }
        }
        if sticker_mutation_ok {
            // An older fetch can answer after the mutation with stale contents.
            drop(
                self.session
                    .requests
                    .take_purpose(RequestPurpose::GetFavoriteStickers),
            );
            if self.session.stickers.open {
                self.sticker_request_favorites()?;
            }
        }
        if recent_cleared {
            drop(
                self.session
                    .requests
                    .take_purpose(RequestPurpose::GetRecentStickers),
            );
        }
        if trending_answer {
            self.mark_trending_stickers_viewed()?;
        }
        self.maybe_load_selected_sticker_set()?;
        self.maybe_refresh_saved_animations()?;
        if chat_search_hits {
            // Unigram ChatSearchViewModel: first hit → LoadMessageSliceAsync.
            self.jump_selected_chat_search_hit()?;
        }
        Ok(())
    }

    fn maybe_send_parameters(&mut self) -> Result<(), ConnectSendError> {
        if self.parameters_sent {
            return Ok(());
        }
        if !matches!(self.session.auth, AuthorizationState::WaitTdlibParameters) {
            return Ok(());
        }
        let extra = self.session.request(RequestPurpose::SetParameters, None);
        let params = build_set_tdlib_parameters(
            &self.credentials,
            &self.paths,
            &self.database_key,
            &self.session.language_prefs.system_language_code,
        );
        // Contains api_hash — do not log `json`.
        let json = params.to_json(extra);
        self.sender.send_json(&json)?;
        self.parameters_sent = true;
        Ok(())
    }

    pub(crate) fn chats_path_active(&self) -> bool {
        matches!(self.session.auth, AuthorizationState::Ready)
            && matches!(self.session.shutdown, ShutdownPhase::Running)
    }

    /// First page on Ready; further pages only when a `loadChats` request returns ok.
    pub fn maybe_load_main_chats(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(None);
        }
        if self.session.chats_exhausted {
            return Ok(None);
        }
        if self.session.requests.has_purpose(RequestPurpose::LoadChats) {
            return Ok(None);
        }
        let extra = self.session.request(RequestPurpose::LoadChats, None);
        self.sender
            .send_json(&load_chats(extra, MAIN_CHAT_LOAD_LIMIT))?;
        Ok(Some(extra))
    }

    /// One `loadChats(chatListArchive)` page unless the archive is
    /// exhausted or a page is in flight. TDLib sends archived chats'
    /// positions only for the loaded part of the list (Telegram X
    /// `TdlibChatList.loadMore`).
    pub fn maybe_load_archive_chats(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() || self.session.archive_chats_exhausted {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::LoadArchiveChats)
        {
            return Ok(None);
        }
        let extra = self.session.request(RequestPurpose::LoadArchiveChats, None);
        if let Err(err) = self
            .sender
            .send_json(&load_archive_chats(extra, MAIN_CHAT_LOAD_LIMIT))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Send `close` (not `logOut`). Callers must keep receiving until Closed.
    pub fn request_close(&mut self) -> Result<RequestId, ConnectSendError> {
        if matches!(
            self.session.auth,
            AuthorizationState::Closed | AuthorizationState::Closing
        ) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.begin_close();
        let extra = self.session.request(RequestPurpose::Close, None);
        self.sender.send_json(&close_request(extra))?;
        Ok(extra)
    }

    /// Slice auth-logout-warning: send `logOut` (not `close`). TDLib
    /// answers `ok`, then drives `Ready → authorizationStateLoggingOut →
    /// authorizationStateClosed` (`Session::set_auth` handles both); the
    /// UI restarts the live connection on Closed so the user lands back
    /// on the login screen. Only valid while authorized.
    pub fn request_logout(&mut self) -> Result<RequestId, ConnectSendError> {
        if !matches!(self.session.auth, AuthorizationState::Ready) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.begin_logout();
        let extra = self.session.request(RequestPurpose::LogOut, None);
        self.sender.send_json(&log_out(extra))?;
        Ok(extra)
    }
}
