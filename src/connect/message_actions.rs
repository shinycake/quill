//! Connect driver: reactions, pins, links, instant view.
use super::*;
use crate::ids::{ChatId, MessageId, RequestId, TopicId};
use crate::settings::InstantViewMode;
use crate::state::{ComposerLinkPreview, RequestPurpose, UnreadJumpKind};
use crate::telegram::requests::{
    add_message_reaction, get_link_preview, get_message_link, get_message_properties,
    get_replied_message, get_web_page_instant_view, pin_chat_message, read_all_chat_markers,
    remove_message_reaction, search_chat_messages, search_messages_filter_json,
    unpin_all_chat_messages, unpin_chat_message,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// Ask TDLib for the replied-to message of every open-chat row whose
    /// original is outside the loaded window (`getRepliedMessage`, like
    /// tdesktop `HistoryItem::requestReply`). The answers land in
    /// `Session::reply_targets` and the strips fill in on the next frame.
    pub fn maybe_fetch_replied_messages(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        for (chat_id, message_id) in self.session.reply_fetch_candidates() {
            let extra = self.session.request(
                RequestPurpose::GetRepliedMessage {
                    chat_id,
                    message_id,
                },
                Some(chat_id),
            );
            if let Err(err) = self
                .sender
                .send_json(&get_replied_message(extra, chat_id, message_id))
            {
                self.session.requests.take(extra);
                return Err(err);
            }
            self.session.reply_targets.insert(
                (chat_id.0, message_id.0),
                crate::state::ReplyTarget::Loading,
            );
        }
        Ok(())
    }

    /// Add an emoji reaction (tdesktop InlineList / Unigram ReactionButton).
    /// `is_big: false`, `update_recent_reactions: true` — official picker click.
    pub fn add_message_reaction(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        emoji: &str,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_message_reaction(chat_id, message_id, emoji, false)
    }

    /// Remove the current user's chosen emoji reaction.
    pub fn remove_message_reaction(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        emoji: &str,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_message_reaction(chat_id, message_id, emoji, true)
    }

    /// Chip / picker toggle: chosen → `removeMessageReaction`, else add.
    pub fn toggle_message_reaction(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        emoji: &str,
    ) -> Result<RequestId, ConnectSendError> {
        let chosen = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .is_some_and(|message| message.chosen_emoji(emoji));
        self.send_message_reaction(chat_id, message_id, emoji, chosen)
    }

    fn send_message_reaction(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        emoji: &str,
        remove: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if emoji.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(message) = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if !message.can_react() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = if remove {
            RequestPurpose::RemoveMessageReaction
        } else {
            RequestPurpose::AddMessageReaction
        };
        let extra = self.session.request(purpose, Some(chat_id));
        let json = if remove {
            remove_message_reaction(extra, chat_id, message_id, emoji)
        } else {
            add_message_reaction(extra, chat_id, message_id, emoji, false, true)
        };
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Pin a history message (tdesktop / Unigram Pin). `silent` maps to
    /// `pinChatMessage.disable_notification` (TDLib 1.8.67 line 13559);
    /// `only_for_self` stays false.
    pub fn pin_chat_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        silent: bool,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_pin_chat_message(chat_id, message_id, false, silent)
    }

    /// Unpin one pinned message (tdesktop PinnedBar cancel / Unpin).
    pub fn unpin_chat_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_pin_chat_message(chat_id, message_id, true, false)
    }

    /// Toggle pin for an already-sent message.
    pub fn toggle_pin_chat_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        let pinned = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .is_some_and(|message| message.is_pinned);
        self.send_pin_chat_message(chat_id, message_id, pinned, false)
    }

    fn send_pin_chat_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        unpin: bool,
        silent: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(message) = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if !message.can_pin() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = if unpin {
            RequestPurpose::UnpinChatMessage
        } else {
            RequestPurpose::PinChatMessage
        };
        let extra = self.session.request(purpose, Some(chat_id));
        let json = if unpin {
            unpin_chat_message(extra, chat_id, message_id)
        } else {
            pin_chat_message(extra, chat_id, message_id, silent, false)
        };
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// M1: unpin every pinned message in a chat (tdesktop pinned-bar menu /
    /// context action; TDLib 1.8.67 `schema/td_api.tl:13565`).
    /// Fetch the chat's pinned messages (newest first) for the pinned bar
    /// — `searchChatMessages` with `searchMessagesFilterPinned`. One
    /// request per chat at a time.
    pub fn fetch_pinned_messages(&mut self, chat_id: ChatId) -> Result<(), ConnectSendError> {
        let purpose = RequestPurpose::GetPinnedMessages;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(());
        }
        let extra = self.session.request(purpose, Some(chat_id));
        let json = search_chat_messages(
            extra,
            chat_id,
            &TopicId::None,
            "",
            MessageId(0),
            0,
            100,
            Some(search_messages_filter_json("searchMessagesFilterPinned")),
        );
        if let Err(err) = self.sender.send_json(&json) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// The corner "@" / heart buttons (tdesktop `CornerButtons`): find the
    /// open chat's oldest unread mention or reaction. TDLib answers newest
    /// first, so the oldest of the page is taken when the answer lands and
    /// `ingest` jumps to it; viewing the row then reads it and TDLib
    /// lowers the chat's counter (`updateChatUnread*Count`).
    pub fn jump_to_unread_marker(&mut self, kind: UnreadJumpKind) -> Result<(), ConnectSendError> {
        let Some(chat_id) = self.session.open_chat else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::JumpToUnread { kind };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(());
        }
        let extra = self.session.request(purpose, Some(chat_id));
        let json = search_chat_messages(
            extra,
            chat_id,
            &TopicId::None,
            "",
            MessageId(0),
            0,
            100,
            Some(search_messages_filter_json(kind.filter_constructor())),
        );
        if let Err(err) = self.sender.send_json(&json) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// Corner button context menu "Mark all as read".
    pub fn read_all_unread_markers(
        &mut self,
        kind: UnreadJumpKind,
    ) -> Result<(), ConnectSendError> {
        let Some(chat_id) = self.session.open_chat else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::ReadAllUnreadMarkers { kind }, Some(chat_id));
        let json = read_all_chat_markers(extra, chat_id, kind == UnreadJumpKind::Reaction);
        if let Err(err) = self.sender.send_json(&json) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    pub fn unpin_all_chat_messages(
        &mut self,
        chat_id: ChatId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::UnpinAllChatMessages, Some(chat_id));
        let json = unpin_all_chat_messages(extra, chat_id);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// M1: share a message link (tdesktop context menu "Copy Message Link").
    /// M1 fix-up: `getMessageLink` is only valid when
    /// `messageProperties.can_get_link` (schema 1.8.67 line 12056), so
    /// this sends `getMessageProperties` first; `ingest` chains to the
    /// actual `getMessageLink` only when the gate passes, and otherwise
    /// stashes `Session::message_link_error` for the UI status note.
    /// The parsed `messageLink.link` lands in
    /// `session.message_link_result` for the UI to copy to the clipboard.
    pub fn get_message_link(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let message = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0));
        let Some(message) = message else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if message.pending || message.id.0 <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::GetMessageLinkProperties {
                chat_id,
                message_id,
            },
            Some(chat_id),
        );
        let json = get_message_properties(extra, chat_id, message_id);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// M1 fix-up: the second half of the "Share link" gate — sent from
    /// `ingest` only after `messageProperties.can_get_link` passed.
    pub(crate) fn send_message_link_request(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetMessageLink, Some(chat_id));
        let json = get_message_link(extra, chat_id, message_id);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// MED4: outcome of an Instant View open attempt (TGX
    /// `TdlibUi.fetchInstantView` + browser fallback).
    pub fn open_instant_view(&mut self, url: &str) -> InstantViewOutcome {
        // Mode Off (or a send failure) means the caller opens the URL in
        // the browser directly — never a fake reader.
        if !matches!(
            self.session.media_prefs.instant_view_mode,
            InstantViewMode::Telegram | InstantViewMode::All
        ) {
            return InstantViewOutcome::Browser;
        }
        let extra = self
            .session
            .request(RequestPurpose::GetWebPageInstantView, None);
        let json = get_web_page_instant_view(extra, url);
        match self.sender.send_json(&json) {
            Ok(()) => {
                self.session
                    .instant_view_urls
                    .insert(extra, url.to_string());
                InstantViewOutcome::Requested
            }
            Err(_) => {
                self.session.requests.take(extra);
                InstantViewOutcome::Browser
            }
        }
    }

    /// MED4b: `getLinkPreview` prefetch for the composer chip (TGX
    /// `LinkPreview.loadLinkPreview`: `GetLinkPreview(FormattedText(url),
    /// null)`). The UI debounces (schema: "Do not call this function too
    /// often"); the answer (or 404) lands in
    /// `Session::composer_preview` for the chip.
    pub fn request_composer_link_preview(&mut self, url: &str) -> Result<(), ConnectSendError> {
        let extra = self.session.request(RequestPurpose::GetLinkPreview, None);
        let json = get_link_preview(extra, url);
        match self.sender.send_json(&json) {
            Ok(()) => {
                self.session
                    .composer_preview_urls
                    .insert(extra, url.to_string());
                // Loading state — the chip shows "Getting link info…"
                // (TGX `LinkPreview.isLoading`).
                self.session.composer_preview = Some(ComposerLinkPreview {
                    url: url.to_string(),
                    preview: None,
                });
                Ok(())
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }
}
