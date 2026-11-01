//! Connect driver core: construction, the update pump, and lifecycle.
use super::composer::ai_rich_draft_note;
use super::*;
use crate::composer::DraftSaveClock;
use crate::credentials::TelegramCredentials;
use crate::ids::{ChatId, MessageId, RequestId};
use crate::state::{ComposerLinkPreview, InstantViewPage, RequestPurpose, Session, ShutdownPhase};
use crate::state::{GroupsPurpose, MessagesPurpose, StickersPurpose, ThreadsPurpose};
use crate::telegram::client::OwnedEnvelope;
use crate::telegram::envelope::{
    AiComposePayload, BotsPayload, CallsPayload, ChatsPayload, CommonPayload, GroupsPayload,
    MediaPayload, MessagesPayload, SearchPayload, SettingsPayload, StickersPayload, UsersPayload,
};
use crate::telegram::envelope::{
    AuthorizationState, EnvelopePayload, MessageContent, RichMessageContent, UsernameCheckResult,
};
use crate::telegram::requests::{
    close_request, get_authorization_state, load_archive_chats, load_chats, log_out,
};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

pub(crate) mod batch;
mod lifecycle;

use std::sync::{Arc, Mutex};

impl<S: JsonSender> ConnectDriver<S> {
    pub fn ingest(&mut self, owned: OwnedEnvelope) -> Result<(), ConnectSendError> {
        // `parity:data-proxy-web`: a carrier's bytes go to its page.
        let Some(owned) = self.absorb_carrier_data(owned) else {
            return Ok(());
        };
        // Q1: a rate-limited read is re-sent later; the reducer never sees
        // the 429 and the request stays pending.
        // Stream segments: the feeder answers the engine before the reducer
        // (and the flood retry, which would delay a live segment) sees it.
        self.note_server_time(&owned.envelope);
        if self.absorb_stream_answer(&owned.envelope, std::time::Instant::now()) {
            return Ok(());
        }
        if self.absorb_flood(&owned.envelope, std::time::Instant::now()) {
            return Ok(());
        }
        // A conference join's answer renames its engine instance before
        // the reducer tracks the call under that id.
        self.note_conference_answer(&owned.envelope);
        let was_ready = matches!(self.session.auth, AuthorizationState::Ready);
        let active_call_before = self.session.calls.active_call.as_ref().map(|call| call.id);
        let group_calls_before = self.session.calls.group_call_ids();
        let bridge_signaling = match &owned.envelope.payload {
            EnvelopePayload::Calls(CallsPayload::UpdateNewCallSignalingData { call_id, data }) => {
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
        let main_was_exhausted = self.session.chat_list.chats_exhausted;
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
        let archive_mute_suggested = matches!(
            &owned.envelope.payload,
            EnvelopePayload::Settings(SettingsPayload::UpdateSuggestedActions { added, .. })
                if added.iter().any(|action| action == crate::archive_mute::ACTION)
        );
        let view_purpose = owned
            .envelope
            .extra
            .and_then(|id| self.session.requests.purpose(id));
        // A message landing in a chat nobody shows changes no open view,
        // unless a shown chat's reply strip shows it.
        let shown_message = match &owned.envelope.payload {
            EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(message)) => {
                self.session.chat_is_shown(message.chat_id)
            }
            EnvelopePayload::Messages(MessagesPayload::UpdateMessageContent {
                chat_id,
                message_id,
                ..
            }) => {
                self.session.chat_is_shown(*chat_id)
                    || self
                        .session
                        .shows_cross_chat_reply_to(*chat_id, *message_id)
            }
            _ => false,
        };
        let view_after = match &owned.envelope.payload {
            EnvelopePayload::Messages(MessagesPayload::Messages(_)) => true,
            EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(_)) => shown_message,
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
                    RequestPurpose::Stickers(StickersPurpose::ManageStickerSet { .. })
                        | RequestPurpose::ReorderInstalledStickerSets
                )
            );
        let installed_stickers_answer = view_purpose
            == Some(RequestPurpose::GetInstalledStickerSets)
            && matches!(
                owned.envelope.payload,
                EnvelopePayload::Stickers(StickersPayload::StickerSets { .. })
            );
        let archive_catalog_changed = sticker_set_changed
            && matches!(
                view_purpose,
                Some(RequestPurpose::Stickers(
                    StickersPurpose::ManageStickerSet { .. }
                ))
            );
        let emoji_trending_answer = view_purpose == Some(RequestPurpose::GetTrendingEmojiSets)
            && matches!(
                owned.envelope.payload,
                EnvelopePayload::Stickers(StickersPayload::TrendingStickerSets { .. })
            )
            && self.session.stickers.emoji.open
            && self.session.stickers.emoji.tab == crate::emoji::EmojiSetTab::Trending;
        let emoji_catalog_changed = (matches!(owned.envelope.payload, EnvelopePayload::Ok)
            && view_purpose == Some(RequestPurpose::ChangeEmojiSet))
            || matches!(
                owned.envelope.payload,
                EnvelopePayload::Stickers(StickersPayload::UpdateInstalledStickerSets {
                    is_regular: false,
                    is_mask: false,
                    ..
                })
            );
        let gif_saved_changed = matches!(
            owned.envelope.payload,
            EnvelopePayload::Stickers(StickersPayload::UpdateSavedAnimations { .. })
        );
        let gif_mutation_ok = matches!(owned.envelope.payload, EnvelopePayload::Ok)
            && matches!(
                view_purpose,
                Some(RequestPurpose::AddSavedAnimation | RequestPurpose::RemoveSavedAnimation)
            );
        let gif_bot_changed = match &owned.envelope.payload {
            EnvelopePayload::Common(CommonPayload::UpdateOption { name, value })
                if name == "animation_search_bot_username" =>
            {
                match value {
                    crate::telegram::envelope::OptionValue::String(name) => {
                        *name != self.session.stickers.gifs.search_bot_username
                    }
                    _ => !self.session.stickers.gifs.search_bot_username.is_empty(),
                }
            }
            _ => false,
        };
        let recent_cleared = matches!(owned.envelope.payload, EnvelopePayload::Ok)
            && view_purpose == Some(RequestPurpose::ClearRecentStickers);
        let trending_answer = matches!(
            owned.envelope.payload,
            EnvelopePayload::Stickers(StickersPayload::TrendingStickerSets { .. })
        ) && view_purpose == Some(RequestPurpose::GetTrendingStickerSets);
        let thumbs_after = shown_message
            || matches!(
                owned.envelope.payload,
                EnvelopePayload::Messages(MessagesPayload::Messages(_))
                | EnvelopePayload::Media(MediaPayload::UpdateFile(_))
                | EnvelopePayload::Media(MediaPayload::File(_))
                // The results panel's images: nothing else asks for them,
                // so without this they waited for an unrelated update.
                | EnvelopePayload::Bots(BotsPayload::InlineQueryResults(_))
            );
        let emoji_neutral = batch::adds_no_custom_emoji(&owned.envelope.payload)
            && !self.stops_waiting_emoji_file(&owned.envelope.payload);
        // Only the first page jumps to its first hit; "older" pages append.
        let chat_search_hits = matches!(
            owned.envelope.payload,
            EnvelopePayload::Search(SearchPayload::FoundChatMessages { .. })
        ) && view_purpose != Some(RequestPurpose::SearchChatMessagesMore);
        // M2: capture the `getFullRichMessage` answer before `apply`
        // takes the pending request; the full blocks replace the
        // partial message's blocks in history after apply.
        let full_rich_answer: Option<(ChatId, MessageId, RichMessageContent)> =
            match &owned.envelope.payload {
                EnvelopePayload::Messages(MessagesPayload::RichMessage { rich }) => owned
                    .envelope
                    .extra
                    .and_then(|id| self.session.requests.purpose(id))
                    .and_then(|purpose| match purpose {
                        RequestPurpose::Messages(MessagesPurpose::GetFullRichMessage {
                            chat_id,
                            message_id,
                        }) => Some((chat_id, message_id, rich.clone())),
                        _ => None,
                    }),
                _ => None,
            };
        // M1: capture the `getMessageLink` answer before `apply` takes the
        // pending request; the UI drains `Session::message_link_result`
        // into the clipboard.
        let message_link_answer: Option<(String, bool)> = match &owned.envelope.payload {
            EnvelopePayload::Messages(MessagesPayload::MessageLink { link, is_public }) => owned
                .envelope
                .extra
                .and_then(|id| self.session.requests.purpose(id))
                .is_some_and(|purpose| {
                    matches!(
                        purpose,
                        RequestPurpose::GetMessageLink | RequestPurpose::GetForumTopicLink
                    )
                })
                .then(|| (link.clone(), *is_public)),
            _ => None,
        };
        // Slice msg-richtext-ai-tools: capture AI text answers
        // (`fixTextWithAi` → `fixedText`, `composeTextWithAi` →
        // `formattedText`) before `apply` takes the pending request. The
        // chat id rides `PendingRequest::chat_id` (the purposes are unit
        // variants); the UI drains `Session::ai_composer_text` into the
        // composer draft.
        let ai_text_answer: Option<(ChatId, String)> = match &owned.envelope.payload {
            EnvelopePayload::Messages(MessagesPayload::AiCompose(
                AiComposePayload::FixedText { text, .. },
            ))
            | EnvelopePayload::Common(CommonPayload::FormattedText { text, .. }) => owned
                .envelope
                .extra
                .and_then(|id| self.session.requests.get(id))
                .filter(|pending| {
                    matches!(
                        pending.purpose,
                        RequestPurpose::FixTextWithAi | RequestPurpose::ComposeTextWithAi
                    )
                })
                .and_then(|pending| pending.chat_id.map(|chat_id| (chat_id, text.clone()))),
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
                EnvelopePayload::Messages(MessagesPayload::RichMessage { rich }) => owned
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
            EnvelopePayload::Messages(MessagesPayload::MessageProperties(actions)) => owned
                .envelope
                .extra
                .and_then(|id| self.session.requests.purpose(id))
                .and_then(|purpose| match purpose {
                    RequestPurpose::Messages(MessagesPurpose::GetMessageLinkProperties {
                        chat_id,
                        message_id,
                    }) => Some((chat_id, message_id, actions.can_get_link)),
                    _ => None,
                }),
            _ => None,
        };
        // The open message menu's properties arrived: chain the seen /
        // reacted lookups they allow (`getMessageViewers`, ...).
        let audience_gate: Option<(ChatId, MessageId, crate::telegram::envelope::MessageActions)> =
            match &owned.envelope.payload {
                EnvelopePayload::Messages(MessagesPayload::MessageProperties(actions)) => owned
                    .envelope
                    .extra
                    .and_then(|id| self.session.requests.purpose(id))
                    .and_then(|purpose| match purpose {
                        RequestPurpose::Messages(MessagesPurpose::GetMessageMenuActions {
                            chat_id,
                            message_id,
                        }) => Some((chat_id, message_id, *actions)),
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
                EnvelopePayload::Users(UsersPayload::CheckChatUsernameResult(result)) => owned
                    .envelope
                    .extra
                    .and_then(|id| self.session.requests.purpose(id))
                    .and_then(|purpose| {
                        self.session
                            .users_state
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
                EnvelopePayload::Messages(MessagesPayload::WebPageInstantView { rich }) => owned
                    .envelope
                    .extra
                    .and_then(|id| {
                        (self.session.requests.purpose(id)
                            == Some(RequestPurpose::GetWebPageInstantView))
                        .then_some(id)
                    })
                    .and_then(|id| {
                        let messages = &mut self.session.messages;
                        messages.instant_view_agent_requests.remove(&id);
                        messages
                            .instant_view_urls
                            .remove(&id)
                            .map(|url| (url, rich.clone()))
                    }),
                _ => None,
            };
        // MED4: a failed `getWebPageInstantView` (TDLib 404 = the page has
        // no Instant View) falls back to the browser like TGX — the URL
        // is stashed for the UI drain, never rendered as a reader.
        let instant_view_fallback: Option<(String, bool)> = match &owned.envelope.payload {
            EnvelopePayload::Error(_) => owned.envelope.extra.and_then(|id| {
                (self.session.requests.purpose(id) == Some(RequestPurpose::GetWebPageInstantView))
                    .then_some(id)
                    .and_then(|id| {
                        let messages = &mut self.session.messages;
                        let from_agent = messages.instant_view_agent_requests.remove(&id);
                        Some((messages.instant_view_urls.remove(&id)?, from_agent))
                    })
            }),
            _ => None,
        };
        // MED4b: capture the `getLinkPreview` answer before `apply` takes
        // the pending request; the composer chip reads
        // `Session::composer_preview`. A late answer for a superseded URL
        // is dropped (the chip only cares about the latest request).
        let link_preview_answer: Option<ComposerLinkPreview> = match &owned.envelope.payload {
            EnvelopePayload::Messages(MessagesPayload::LinkPreview { preview }) => owned
                .envelope
                .extra
                .and_then(|id| {
                    (self.session.requests.purpose(id) == Some(RequestPurpose::GetLinkPreview))
                        .then_some(id)
                })
                .and_then(|id| {
                    self.session
                        .messages
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
                    .and_then(|id| self.session.messages.composer_preview_urls.remove(&id))
            }),
            _ => None,
        };
        // Slice CL2: our `createPrivateChat` answer — the bare `chat`
        // parses as `UpdateNewChat`; the `@extra` tells it apart from a
        // genuine `updateNewChat`. Captured before `apply` takes the
        // pending request; opened through the normal `select_chat`
        // flow (openChat + history) after apply inserts the chat.
        let created_chat: Option<ChatId> = match &owned.envelope.payload {
            EnvelopePayload::Chats(ChatsPayload::UpdateNewChat { chat_id, .. }) => owned
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
                | RequestPurpose::Threads(ThreadsPurpose::EditForumTopic { .. })
                | RequestPurpose::Threads(ThreadsPurpose::ToggleForumTopicClosed { .. })
                | RequestPurpose::Threads(ThreadsPurpose::ToggleForumTopicPinned { .. })
                | RequestPurpose::Threads(ThreadsPurpose::DeleteForumTopic { .. })
                | RequestPurpose::ToggleGeneralForumTopicHidden
                | RequestPurpose::AddChatWelcomeMessage
                | RequestPurpose::Groups(GroupsPurpose::EditChatWelcomeMessage { .. })
                | RequestPurpose::Groups(GroupsPurpose::DeleteChatWelcomeMessage { .. })
                | RequestPurpose::BoostChat => {
                    pending.chat_id.map(|chat_id| (pending.purpose, chat_id))
                }
                _ => None,
            });
        // Slice (communities backend core): the `createCommunity` answer
        // is `communityId`; `updateCommunity` for the new community is
        // guaranteed to have arrived first, so the id chains straight
        // into `getCommunityFullInfo` after apply.
        let created_community_id: Option<i64> = match &owned.envelope.payload {
            EnvelopePayload::Groups(GroupsPayload::CommunityId { id }) => owned
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
        // Batch 6: a confirmed `optimizeStorage` drops the cached storage
        // stats (see `Session::apply`) — capture before apply so the
        // post-apply refetch shows the post-clear numbers. A dropped
        // cache is the success signal: on a TDLib error the cache stays
        // and nothing refetches.
        let cleared_download_cache = matches!(
            &owned.envelope.payload,
            EnvelopePayload::Settings(SettingsPayload::StorageStatistics { .. })
        ) && owned.envelope.extra.is_some_and(|id| {
            self.session.requests.purpose(id) == Some(RequestPurpose::OptimizeStorage)
        });
        let used_emoji: Vec<_> = match &owned.envelope.payload {
            EnvelopePayload::Messages(MessagesPayload::UpdateMessageSendSucceeded {
                message,
                ..
            }) if message.is_outgoing => {
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
            EnvelopePayload::Messages(MessagesPayload::UpdateMessageIsPinned {
                chat_id,
                is_pinned: true,
                ..
            }) => Some(*chat_id),
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
        let recent_packs = self.session.settings.media_prefs.recent_emoji_packs.clone();
        let recent_emoji = self
            .session
            .settings
            .media_prefs
            .recent_custom_emoji_ids
            .clone();
        let topic_chat = Self::possible_topic_chat(&owned.envelope.payload);
        let forum_supergroup = Self::possible_forum_supergroup(&owned.envelope.payload);
        let topic_refresh = Self::possible_topic_refresh(&owned.envelope.payload);
        let previous_seq = self.session.last_seq;
        // B7: the step waiting for this answer (basic group upgrade,
        // history before a discussion link), taken before `apply`.
        let admin_followup = self.capture_admin_followup(&owned);
        let edit_after_upgrade = self.capture_edit_after_upgrade(&owned);
        let new_chat_created = self.capture_new_chat(&owned);
        let paid_prices_change = Self::changes_paid_prices(&owned.envelope.payload);
        let send_outcome = Self::send_outcome(&owned.envelope.payload);
        self.session.apply(owned);
        self.run_deferred_caption(send_outcome);
        // A saved open that missed its message loads the default window
        // on this tick, whatever the `?` returns below do.
        self.load_default_after_saved_open();
        if paid_prices_change {
            self.sync_paid_guard();
        }
        if let Some((followup, ok)) = admin_followup {
            self.run_admin_followup(followup, ok);
        }
        if let Some((edit, ok)) = edit_after_upgrade {
            self.run_edit_after_upgrade(edit, ok);
        }
        if let Some(extra) = new_chat_created {
            self.run_new_chat_created(extra);
        }
        self.maybe_preload_topics(topic_chat, forum_supergroup);
        self.maybe_refresh_forum_topic(topic_refresh);
        if self.session.last_seq != previous_seq {
            self.session.remember_emoji_pack_usage(&used_emoji);
        }
        if recent_packs != self.session.settings.media_prefs.recent_emoji_packs
            || recent_emoji != self.session.settings.media_prefs.recent_custom_emoji_ids
        {
            self.save_media_prefs();
        }
        // Slice S4: persist per-network settings seeded from
        // `getAutoDownloadSettingsPresets` (the reducer cannot touch the
        // filesystem, so it marks them dirty instead).
        if self.session.settings.data_storage_dirty {
            self.session.settings.data_storage_dirty = false;
            let _ = self.save_data_storage_prefs();
        }
        if cleared_download_cache {
            let _ = self.maybe_fetch_storage_statistics();
        }
        // Batch 6: a finished reset / cancel / login-email step left the
        // cached password state out of date.
        if std::mem::take(&mut self.session.auth_state.twofa_flow.refetch) {
            let _ = self.refresh_password_state();
        }
        if let Some(chat_id) = unpinned_all {
            self.session
                .messages
                .pinned_messages
                .insert(chat_id.0, Vec::new());
        }
        if let Some(chat_id) = pins_changed.filter(|chat| self.session.open_chat == Some(*chat)) {
            let _ = self.fetch_pinned_messages(chat_id);
        }
        self.pump_call_engine(active_call_before, bridge_signaling)?;
        self.pump_group_call_transport(group_calls_before)?;
        self.pump_call_media(std::time::Instant::now());
        self.pump_call_audio()?;
        self.maybe_send_parameters()?;
        self.maybe_probe_channel_membership()?;
        // Slice G2: chain `boostChat` once the slots answer arrives.
        self.maybe_continue_boost()?;
        // A channel's boost level moved: refresh the status shown for it.
        self.refresh_moved_boost_statuses();
        // Slice G2: refetch caches the state dropped after a confirmed
        // mutation. A dropped cache is the success signal — on a TDLib
        // error the cache stays and nothing refetches.
        if let Some((purpose, chat_id)) = mutation_refetch {
            match purpose {
                RequestPurpose::CreateForumTopic
                | RequestPurpose::Threads(ThreadsPurpose::EditForumTopic { .. })
                | RequestPurpose::Threads(ThreadsPurpose::ToggleForumTopicClosed { .. })
                | RequestPurpose::Threads(ThreadsPurpose::ToggleForumTopicPinned { .. })
                | RequestPurpose::Threads(ThreadsPurpose::DeleteForumTopic { .. })
                | RequestPurpose::ToggleGeneralForumTopicHidden
                    if !self.session.threads.forum_topics.contains_key(&chat_id.0) =>
                {
                    let _ = self.refresh_forum_topics(chat_id);
                }
                RequestPurpose::AddChatWelcomeMessage
                | RequestPurpose::Groups(GroupsPurpose::EditChatWelcomeMessage { .. })
                | RequestPurpose::Groups(GroupsPurpose::DeleteChatWelcomeMessage { .. })
                    if !self
                        .session
                        .groups
                        .welcome_messages
                        .contains_key(&chat_id.0) =>
                {
                    let _ = self.load_chat_welcome_messages(chat_id);
                }
                RequestPurpose::BoostChat
                    if !self
                        .session
                        .groups
                        .chat_boost_status
                        .contains_key(&chat_id.0) =>
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
        // Settings > Ask a Question (the support account) or a contact
        // just created by phone: open (or create) the private chat.
        if let Some(user_id) = self.session.users_state.user_chat_to_open.take() {
            let _ = self.create_private_chat_for(user_id);
        }
        // Slice (communities backend core): resolve a just-created
        // community id into its full-info pack. `let _` on purpose: the
        // community is already in the model; a failed send must not
        // fail the ingest.
        if let Some(community_id) = created_community_id {
            let _ = self.get_community_full_info(community_id);
        }
        // Slice (communities backend core): refetch the full-info pack
        // the state dropped after a confirmed `setCommunityName`.
        if let Some(community_id) = community_mutation_refetch
            && !self
                .session
                .groups
                .community_full_infos
                .contains_key(&community_id)
        {
            let _ = self.get_community_full_info(community_id);
        }
        let became_ready = !was_ready && matches!(self.session.auth, AuthorizationState::Ready);
        if became_ready {
            self.preload_known_forums();
            self.check_export_leftovers();
        }
        if became_ready || load_chats_ok {
            self.maybe_load_main_chats()?;
        }
        if load_archive_ok || (!main_was_exhausted && self.session.chat_list.chats_exhausted) {
            self.maybe_load_archive_chats()?;
        }
        if let Some(folder_id) = folder_load_ok {
            self.maybe_load_folder_chats(folder_id)?;
        }
        // Parity slice: queued remove-from-folder edits go out once the
        // `getChatFolder` spec arrives.
        self.maybe_finish_folder_removals()?;
        self.maybe_finish_leave_deletes()?;
        if became_ready {
            // Phase 9.1: the story tray needs `updateChatActiveStories`
            // updates; one `loadActiveStories(storyListMain)` per Ready.
            self.session.stories.reset_active_stories_paging();
            self.maybe_load_active_stories()?;
            // Parity slice: saved notification sounds (picker + custom-sound
            // playback) and per-scope default settings (`use_default_*`
            // fallback), once per Ready.
            let _ = self.maybe_fetch_notification_sounds();
            let _ = self.maybe_fetch_scope_notification_settings();
            // Quick reply shortcuts, so the first `/` already lists them
            // (tdesktop preloads them with the dialogs).
            let _ = self.ensure_quick_reply_shortcuts();
            let _ = self.maybe_seed_storage_limits();
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
        // Passkeys: refetch after a removal, resolve the software icons.
        self.after_passkeys_ingest();
        // "Hide new chats?" waits for a fresh copy of the setting, as
        // tdesktop reloads the global privacy settings on the suggestion.
        if archive_mute_suggested {
            let _ = self.reload_archive_chat_list_settings();
        }
        // `parity:proxy-settings`: a proxy mutation marks the list stale.
        let _ = self.refresh_proxies_if_stale();
        // Slice `parity:bots-payment-recurring`: an
        // `editStarSubscription` / `reuseStarSubscription` `ok` marks the
        // subscriptions list stale in the reducer; same pattern.
        let _ = self.refresh_star_subscriptions_if_stale();
        let _ = self.refresh_received_gifts_if_stale();
        let _ = self.refresh_gift_collections_if_stale();
        // "Send via Blockchain": the temporary password is in, ask for the URL.
        let _ = self.continue_gift_export();
        if view_after {
            self.sync_shown_views()?;
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
        if let Some(message_id) = self.session.messages.unread_jump.take() {
            let _ = self.jump_to_chat_search_message(message_id);
        }
        // Jump to date: `getChatMessageByDate` resolved (or 404'd) a target.
        if let Some((message_id, mode)) = self.session.search.date_jump.take() {
            let _ = self.jump_to_message_with(message_id, mode);
        }
        // M1: stash the `getMessageLink` answer for the UI clipboard drain.
        if let Some((link, is_public)) = message_link_answer {
            self.session.messages.message_link_result = Some(link);
            self.session.messages.message_link_public = is_public;
        }
        // Slice msg-richtext-ai-tools: stash AI answers for the composer
        // drain. A late answer for a chat the user has since left is
        // dropped by the UI (chat-id check), never applied blindly.
        if let Some((chat_id, text)) = ai_text_answer {
            self.session.messages.ai_composer_text = Some((chat_id, text));
        }
        if let Some((chat_id, rich, note)) = ai_rich_answer {
            self.session.messages.ai_composer_blocks = Some((chat_id, rich, note));
        }
        // A5: stash the `checkChatUsername` verdict for the
        // edit-profile dialog.
        if let Some((username, result)) = username_check_answer {
            self.session.users_state.username_check = Some((username, result));
        }
        // MED4: stash the `getWebPageInstantView` answer (success →
        // IV reader; error → browser fallback) for the UI drains.
        if let Some((url, rich)) = instant_view_answer {
            self.session.messages.instant_view = Some(InstantViewPage { url, rich });
        }
        if let Some((url, from_agent)) = instant_view_fallback {
            self.session.messages.instant_view_fallback_url = Some(url);
            self.session.messages.instant_view_fallback_from_agent = from_agent;
        }
        // MED4b: stash the `getLinkPreview` answer for the composer
        // chip; a 404 becomes "no link info" (`Some(None)`); answers for
        // superseded URLs are dropped.
        if let Some(answer) = link_preview_answer {
            let current = self.session.messages.composer_preview.as_ref();
            if current.is_none_or(|p| p.url == answer.url) {
                self.session.messages.composer_preview = Some(answer);
            }
        }
        if let Some(url) = link_preview_failed
            && self
                .session
                .messages
                .composer_preview
                .as_ref()
                .is_some_and(|p| p.url == url && p.preview.is_none())
        {
            self.session.messages.composer_preview = Some(ComposerLinkPreview {
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
                self.session.messages.message_link_error =
                    Some("message link not available for this message".into());
            }
        }
        if let Some((chat_id, message_id, actions)) = audience_gate {
            let _ = self.fetch_message_audience(chat_id, message_id, actions);
        }
        if emoji_trending_answer {
            self.mark_emoji_packs_viewed()?;
        }
        if emoji_catalog_changed {
            self.refresh_emoji_pack_catalog()?;
        }
        self.refresh_open_mask_lists()?;
        self.apply_resolved_enable_sharing()?;
        self.maybe_resolve_emoji_status_choices()?;
        if !emoji_neutral {
            self.sync_open_chat_custom_emoji()?;
        }
        if gif_bot_changed {
            self.cancel_gif_search_requests();
            self.session.stickers.gifs.search_results.clear();
            self.session.stickers.gifs.search_next_offset.clear();
            self.session.stickers.gifs.search_loading = self.session.stickers.gifs.search_mode;
        }
        if self.chats_path_active()
            && self.session.stickers.gifs.open
            && self.session.stickers.gifs.search_mode
            && self.session.stickers.gifs.search_loading
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
            && self.session.stickers.stickers.suggest_waiting_for_sets
            && let Some(emoji) = self.session.stickers.stickers.suggest_for.take()
        {
            self.update_sticker_suggestions(&emoji)?;
        }
        if thumbs_after
            || self.session.stickers.stickers.open
            || self.session.stickers.gifs.open
            || self.session.stickers.emoji.open
            || !self.session.stickers.stickers.suggestions.is_empty()
        {
            self.sync_shown_media()?;
        }
        // Parity slice: chat-list avatars download on every ingest; each
        // photo is requested at most once (in-flight / completed dedupe).
        self.maybe_download_chat_list_photos()?;
        self.maybe_download_profile_thumbs()?;
        let _ = self.refresh_stale_playlists();
        self.pump_sticker_manager();
        let _ = self.maybe_download_attach_bot_icons();
        let _ = self.pump_location_picker();
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
        let _ = self.maybe_offer_pending_call_link();
        let _ = self.maybe_fetch_join_as();
        // stories-live-play: the story viewer's pending "Join live"
        // fires `join_video_chat` once the `getGroupCall` answer has
        // created the unjoined tracker.
        let _ = self.maybe_join_live_story();
        // Phase C2f: a dropped group call (`need_rejoin`) auto-rejoins
        // with the C2d attempt discipline (max 3).
        let _ = self.maybe_auto_rejoin_group_call();
        // A conference made from a 1:1 call ends that call with its link
        // and invites the people picked; the other side follows the link.
        let _ = self.maybe_continue_conference();
        if archive_catalog_changed {
            drop(
                self.session
                    .requests
                    .take_purpose(RequestPurpose::GetArchivedStickerSets),
            );
            if self.session.stickers.stickers.open
                && self.session.stickers.stickers.tab == crate::state::StickerTab::Archived
            {
                drop(
                    self.session
                        .requests
                        .take_purpose(RequestPurpose::GetStickerSet),
                );
                self.session.stickers.stickers.loading_set = false;
                self.fetch_archived_stickers(false)?;
            }
        }
        if sticker_set_changed {
            drop(
                self.session
                    .requests
                    .take_purpose(RequestPurpose::GetInstalledStickerSets),
            );
            if self.session.stickers.stickers.open {
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
            if self.session.stickers.stickers.open {
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
        self.refresh_stale_panels()?;
        self.open_attached_sticker_set()?;
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
            &self.session.settings.language_prefs.system_language_code,
            &self.session.settings.device_prefs.custom_device_model,
        );
        // Contains api_hash — do not log `json`.
        let json = params.to_json(extra);
        self.sender.send_json(&json)?;
        self.parameters_sent = true;
        self.send_language_options()
    }
}
