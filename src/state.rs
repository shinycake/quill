use crate::auth::{AuthView, view_for};
use crate::diagnostics::{Diagnostic, DiagnosticSink};
use crate::ids::{
    AccountGeneration, AccountKey, ChatId, FileId, MessageId, RequestId, ViewGeneration,
};
use crate::telegram::client::OwnedEnvelope;
use crate::telegram::envelope::{
    AuthorizationState, ChatAction, ChatKind, ChatList, ChatNotificationSettings,
    ChatPositionUpdate, ConnectionState, EnvelopePayload, ErrorClass, MessageContent,
    MessageForwardInfo, MessageInteractionInfo, MessageOrigin, MessageReaction, MessageReplyTo,
    MessageSender, ParsedFile, ParsedMessage, StickerFormat, StickerItem, StickerSetInfo,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestPurpose {
    GetAuthorizationState,
    SetParameters,
    SetPhoneNumber,
    CheckAuthenticationCode,
    CheckAuthenticationPassword,
    LoadChats,
    GetHistory,
    /// Any `sendMessage` (text / photo / document). Response `message` is pending.
    SendMessage,
    OpenChat,
    CloseChat,
    ViewMessages,
    DownloadFile,
    SearchChats,
    SearchMessages,
    SearchRecentlyFoundChats,
    AddRecentlyFoundChat,
    SearchChatMessages,
    /// `getChatHistory` around a jump target (Unigram `LoadMessageSliceImpl`).
    GetHistoryAround,
    /// `editMessageText` / `editMessageCaption`. Response is `message`.
    EditMessage,
    /// `deleteMessages`. Response is `ok`; rows leave via `updateDeleteMessages`.
    DeleteMessages,
    /// `forwardMessages`. Response is `messages`.
    ForwardMessages,
    /// `addMessageReaction`. Response is `ok`; chips via `updateMessageInteractionInfo`.
    AddMessageReaction,
    /// `removeMessageReaction`. Response is `ok`; chips via `updateMessageInteractionInfo`.
    RemoveMessageReaction,
    /// `pinChatMessage`. Response is `ok`; pin via `updateMessageIsPinned`.
    PinChatMessage,
    /// `unpinChatMessage`. Response is `ok`; pin via `updateMessageIsPinned`.
    UnpinChatMessage,
    /// `setChatNotificationSettings`. Response is `ok`; mute via
    /// `updateChatNotificationSettings`.
    SetChatNotificationSettings,
    /// `addChatToList` (`chatListArchive` or `chatListMain`). Response is `ok`;
    /// list membership via position / added-to-list updates.
    AddChatToList,
    /// `sendChatAction` (`chatActionTyping` / `chatActionCancel`). Response is `ok`.
    SendChatAction,
    /// `getInstalledStickerSets` (`stickerTypeRegular`). Response is `stickerSets`.
    GetInstalledStickerSets,
    /// `getStickerSet`. Response is `stickerSet`.
    GetStickerSet,
    Close,
    LogOut,
    Other,
}

fn is_auth_submit(purpose: RequestPurpose) -> bool {
    matches!(
        purpose,
        RequestPurpose::SetPhoneNumber
            | RequestPurpose::CheckAuthenticationCode
            | RequestPurpose::CheckAuthenticationPassword
    )
}

/// Classified TDLib error for an auth submit. Never includes the native message
/// (codes, passwords, and phone numbers live there).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthRequestError {
    pub purpose: RequestPurpose,
    pub class: ErrorClass,
}

impl AuthRequestError {
    pub fn user_message(self) -> &'static str {
        match (self.purpose, self.class) {
            (RequestPurpose::SetPhoneNumber, ErrorClass::Invalid) => "phone not accepted",
            (RequestPurpose::SetPhoneNumber, ErrorClass::Flood) => {
                "too many phone attempts — wait and try again"
            }
            (RequestPurpose::CheckAuthenticationCode, ErrorClass::Invalid) => "code not accepted",
            (RequestPurpose::CheckAuthenticationCode, ErrorClass::Flood) => {
                "too many code attempts — wait and try again"
            }
            (RequestPurpose::CheckAuthenticationPassword, ErrorClass::Invalid) => {
                "password not accepted"
            }
            (RequestPurpose::CheckAuthenticationPassword, ErrorClass::Flood) => {
                "too many password attempts — wait and try again"
            }
            (_, ErrorClass::Unauthorized) => "session is no longer authorized",
            _ => "Telegram rejected the request",
        }
    }
}

/// Source + dest frozen when `forwardMessages` is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardFlight {
    pub extra: RequestId,
    pub dest_chat_id: ChatId,
    pub from_chat_id: ChatId,
    pub requested: usize,
}

/// Result of `forwardMessages` (`messages` or `error`). Dest title is resolved
/// from the loaded chat list (tdesktop ShareBox success names the peer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardResult {
    pub dest_chat_id: ChatId,
    pub dest_title: String,
    pub from_chat_id: ChatId,
    pub requested: usize,
    pub forwarded_ids: Vec<MessageId>,
}

impl ForwardResult {
    pub fn success_label(&self) -> String {
        let n = self.forwarded_ids.len();
        if n == 0 {
            format!("Could not forward to {}", self.dest_title)
        } else if n == 1 {
            format!("Forwarded to {}", self.dest_title)
        } else {
            format!("Forwarded {n} messages to {}", self.dest_title)
        }
    }
}

#[derive(Debug, Clone)]
pub struct PendingRequest {
    pub id: RequestId,
    pub account_generation: AccountGeneration,
    pub purpose: RequestPurpose,
    pub chat_id: Option<ChatId>,
    pub view_generation: Option<ViewGeneration>,
    pub file_id: Option<i32>,
    pub search_generation: Option<u64>,
    pub around_message_id: Option<MessageId>,
}

#[derive(Debug, Default)]
pub struct RequestRegistry {
    next: u64,
    pending: HashMap<u64, PendingRequest>,
}

impl RequestRegistry {
    pub fn register(
        &mut self,
        account_generation: AccountGeneration,
        purpose: RequestPurpose,
        chat_id: Option<ChatId>,
        view_generation: Option<ViewGeneration>,
    ) -> RequestId {
        self.next += 1;
        let id = RequestId(self.next);
        self.pending.insert(
            id.0,
            PendingRequest {
                id,
                account_generation,
                purpose,
                chat_id,
                view_generation,
                file_id: None,
                search_generation: None,
                around_message_id: None,
            },
        );
        id
    }

    pub fn register_search(
        &mut self,
        account_generation: AccountGeneration,
        purpose: RequestPurpose,
        search_generation: u64,
    ) -> RequestId {
        self.next += 1;
        let id = RequestId(self.next);
        self.pending.insert(
            id.0,
            PendingRequest {
                id,
                account_generation,
                purpose,
                chat_id: None,
                view_generation: None,
                file_id: None,
                search_generation: Some(search_generation),
                around_message_id: None,
            },
        );
        id
    }

    pub fn register_chat_search(
        &mut self,
        account_generation: AccountGeneration,
        purpose: RequestPurpose,
        chat_id: ChatId,
        search_generation: u64,
    ) -> RequestId {
        self.next += 1;
        let id = RequestId(self.next);
        self.pending.insert(
            id.0,
            PendingRequest {
                id,
                account_generation,
                purpose,
                chat_id: Some(chat_id),
                view_generation: None,
                file_id: None,
                search_generation: Some(search_generation),
                around_message_id: None,
            },
        );
        id
    }

    pub fn register_around(
        &mut self,
        account_generation: AccountGeneration,
        chat_id: ChatId,
        view_generation: ViewGeneration,
        around_message_id: MessageId,
    ) -> RequestId {
        self.next += 1;
        let id = RequestId(self.next);
        self.pending.insert(
            id.0,
            PendingRequest {
                id,
                account_generation,
                purpose: RequestPurpose::GetHistoryAround,
                chat_id: Some(chat_id),
                view_generation: Some(view_generation),
                file_id: None,
                search_generation: None,
                around_message_id: Some(around_message_id),
            },
        );
        id
    }

    pub fn register_download(
        &mut self,
        account_generation: AccountGeneration,
        file_id: FileId,
    ) -> RequestId {
        self.next += 1;
        let id = RequestId(self.next);
        self.pending.insert(
            id.0,
            PendingRequest {
                id,
                account_generation,
                purpose: RequestPurpose::DownloadFile,
                chat_id: None,
                view_generation: None,
                file_id: Some(file_id.0),
                search_generation: None,
                around_message_id: None,
            },
        );
        id
    }

    pub fn take(&mut self, id: RequestId) -> Option<PendingRequest> {
        self.pending.remove(&id.0)
    }

    pub fn invalidate_account(&mut self) {
        self.pending.clear();
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn purpose(&self, id: RequestId) -> Option<RequestPurpose> {
        self.pending.get(&id.0).map(|p| p.purpose)
    }

    pub fn has_purpose(&self, purpose: RequestPurpose) -> bool {
        self.pending.values().any(|p| p.purpose == purpose)
    }

    pub fn has_purpose_for_chat(&self, purpose: RequestPurpose, chat_id: ChatId) -> bool {
        self.pending
            .values()
            .any(|p| p.purpose == purpose && p.chat_id == Some(chat_id))
    }

    pub fn has_download(&self, file_id: FileId) -> bool {
        self.pending
            .values()
            .any(|p| p.purpose == RequestPurpose::DownloadFile && p.file_id == Some(file_id.0))
    }
}

/// Outgoing read-receipt state from `last_read_outbox_message_id`.
/// Schema supports this; `0` means nothing outgoing has been read yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboxReceipt {
    /// Not an outgoing server message (incoming or still pending).
    None,
    /// Reached the server; peer has not read past this id.
    Sent,
    /// `last_read_outbox_message_id` is >= this outgoing id.
    Read,
}

/// Badge label for the chat list. `None` when the chat is fully read.
pub fn unread_badge_text(count: i32) -> Option<String> {
    if count <= 0 {
        None
    } else if count > 99 {
        Some("99+".into())
    } else {
        Some(count.to_string())
    }
}

pub fn outgoing_status_label(pending: bool, receipt: OutboxReceipt) -> &'static str {
    if pending {
        "You (sending)"
    } else {
        match receipt {
            OutboxReceipt::Read => "You · read",
            OutboxReceipt::Sent | OutboxReceipt::None => "You · sent",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatSummary {
    pub id: ChatId,
    pub title: String,
    pub kind: ChatKind,
    pub unread_count: i32,
    pub last_read_inbox_message_id: MessageId,
    pub last_read_outbox_message_id: MessageId,
    pub order: i64,
    pub is_pinned: bool,
    pub in_main_list: bool,
    /// `chatListArchive` membership (`updateChatPosition` / add-remove-from-list).
    pub in_archive: bool,
    pub archive_order: i64,
    pub archive_is_pinned: bool,
    /// `chat.notification_settings` / `updateChatNotificationSettings`.
    pub notification_settings: ChatNotificationSettings,
    /// Sidebar preview from `updateChatLastMessage`. Not logged.
    pub last_preview: String,
    /// Senders with an active `chatActionTyping` (`updateChatAction`).
    pub typing_senders: Vec<MessageSender>,
}

impl ChatSummary {
    pub fn supported(&self) -> bool {
        self.kind.is_supported_cloud_chat()
    }

    pub fn is_muted(&self) -> bool {
        self.notification_settings.is_muted()
    }

    pub fn is_peer_typing(&self) -> bool {
        !self.typing_senders.is_empty()
    }

    pub fn set_sender_action(&mut self, sender: MessageSender, action: ChatAction) {
        self.typing_senders.retain(|existing| *existing != sender);
        if action == ChatAction::Typing {
            self.typing_senders.push(sender);
        }
    }

    pub fn sidebar_preview(&self) -> String {
        if let Some(reason) = self.kind.gate_reason() {
            return reason.to_string();
        }
        if self.is_peer_typing() {
            return "typing…".into();
        }
        if !self.last_preview.is_empty() {
            return self.last_preview.clone();
        }
        if self.unread_count > 0 {
            return format!("{} unread", self.unread_count);
        }
        "cloud chat".into()
    }

    pub fn outbox_receipt(&self, message: &HistoryMessage) -> OutboxReceipt {
        if !message.is_outgoing || message.pending {
            return OutboxReceipt::None;
        }
        if self.last_read_outbox_message_id.0 > 0
            && message.id.0 <= self.last_read_outbox_message_id.0
        {
            OutboxReceipt::Read
        } else {
            OutboxReceipt::Sent
        }
    }
}

fn placeholder_chat(chat_id: ChatId) -> ChatSummary {
    ChatSummary {
        id: chat_id,
        title: format!("chat {}", chat_id.0),
        kind: ChatKind::Unknown,
        unread_count: 0,
        last_read_inbox_message_id: MessageId(0),
        last_read_outbox_message_id: MessageId(0),
        order: 0,
        is_pinned: false,
        in_main_list: false,
        in_archive: false,
        archive_order: 0,
        archive_is_pinned: false,
        notification_settings: ChatNotificationSettings::default(),
        last_preview: String::new(),
        typing_senders: Vec::new(),
    }
}

fn preview_from_content(content: &MessageContent) -> String {
    content.preview()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryMessage {
    pub id: MessageId,
    pub chat_id: ChatId,
    pub is_outgoing: bool,
    pub content: MessageContent,
    pub pending: bool,
    pub reply_to: Option<MessageReplyTo>,
    pub forward_info: Option<MessageForwardInfo>,
    pub interaction_info: Option<MessageInteractionInfo>,
    /// Schema `message.is_pinned` / `updateMessageIsPinned`.
    pub is_pinned: bool,
}

impl HistoryMessage {
    pub fn can_react(&self) -> bool {
        !self.pending && self.id.0 > 0
    }

    /// Already-sent messages can be pinned/unpinned (live `messageProperties.can_be_pinned`
    /// stays out — same default as edit/forward/react).
    pub fn can_pin(&self) -> bool {
        !self.pending && self.id.0 > 0
    }

    pub fn emoji_reaction_chips(&self) -> Vec<&MessageReaction> {
        self.interaction_info
            .as_ref()
            .map(MessageInteractionInfo::emoji_chips)
            .unwrap_or_default()
    }

    pub fn chosen_emoji(&self, emoji: &str) -> bool {
        self.interaction_info
            .as_ref()
            .is_some_and(|info| info.chosen_emoji(emoji))
    }
}

#[derive(Debug, Default)]
pub struct HistoryState {
    pub messages: BTreeMap<i64, HistoryMessage>,
    pub tombstones: HashSet<i64>,
    pub loaded_complete: bool,
    pub view_generation: ViewGeneration,
    /// Message ids TDLib has accepted for `viewMessages` this open generation.
    pub viewed: HashSet<i64>,
    /// In-flight `viewMessages` ids. Cleared on send failure or TDLib error so we can retry.
    pub viewing: HashSet<i64>,
}

impl HistoryState {
    pub fn ordered(&self) -> Vec<&HistoryMessage> {
        self.messages.values().collect()
    }

    pub fn oldest_id(&self) -> Option<MessageId> {
        self.messages.keys().next().copied().map(MessageId)
    }

    fn upsert(&mut self, message: HistoryMessage) {
        if self.tombstones.contains(&message.id.0) {
            return;
        }
        self.messages.insert(message.id.0, message);
    }

    fn remove(&mut self, id: MessageId, permanent: bool) {
        self.messages.remove(&id.0);
        if permanent {
            self.tombstones.insert(id.0);
        }
    }

    fn replace_id(&mut self, old: MessageId, new_message: HistoryMessage) {
        self.messages.remove(&old.0);
        self.upsert(new_message);
    }

    pub fn contains(&self, id: MessageId) -> bool {
        self.messages.contains_key(&id.0)
    }

    pub fn is_tombstone(&self, id: MessageId) -> bool {
        self.tombstones.contains(&id.0)
    }

    fn update_content(&mut self, id: MessageId, content: MessageContent) -> bool {
        if let Some(message) = self.messages.get_mut(&id.0) {
            message.content = content;
            true
        } else {
            false
        }
    }

    fn update_interaction_info(
        &mut self,
        id: MessageId,
        interaction_info: Option<MessageInteractionInfo>,
    ) -> bool {
        if let Some(message) = self.messages.get_mut(&id.0) {
            message.interaction_info = interaction_info;
            true
        } else {
            false
        }
    }

    fn update_is_pinned(&mut self, id: MessageId, is_pinned: bool) -> bool {
        if let Some(message) = self.messages.get_mut(&id.0) {
            message.is_pinned = is_pinned;
            true
        } else {
            false
        }
    }

    /// Newest pinned message in loaded history (`getChatPinnedMessage` is newest).
    pub fn newest_pinned(&self) -> Option<&HistoryMessage> {
        self.messages
            .values()
            .rev()
            .find(|message| message.is_pinned)
    }
}

/// Global search (official sidebar field): recents, then `searchChats` + `searchMessages`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchStatus {
    Closed,
    Idle,
    Searching,
    Ready,
    Empty,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMessageHit {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub preview: String,
    pub is_outgoing: bool,
    pub content: MessageContent,
    pub reply_to: Option<MessageReplyTo>,
    pub forward_info: Option<MessageForwardInfo>,
    pub interaction_info: Option<MessageInteractionInfo>,
    pub is_pinned: bool,
}

impl SearchMessageHit {
    fn from_parsed(message: &ParsedMessage) -> Self {
        Self {
            chat_id: message.chat_id,
            message_id: message.id,
            preview: message.content.preview(),
            is_outgoing: message.is_outgoing,
            content: message.content.clone(),
            reply_to: message.reply_to.clone(),
            forward_info: message.forward_info.clone(),
            interaction_info: message.interaction_info.clone(),
            is_pinned: message.is_pinned,
        }
    }

    fn into_history(self) -> HistoryMessage {
        HistoryMessage {
            id: self.message_id,
            chat_id: self.chat_id,
            is_outgoing: self.is_outgoing,
            content: self.content,
            pending: false,
            reply_to: self.reply_to,
            forward_info: self.forward_info,
            interaction_info: self.interaction_info,
            is_pinned: self.is_pinned,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchState {
    pub open: bool,
    pub query: String,
    pub generation: u64,
    pub status: SearchStatus,
    pub chat_ids: Vec<ChatId>,
    pub messages: Vec<SearchMessageHit>,
    /// Empty-query surface: `searchRecentlyFoundChats` (official Recent).
    pub recents: bool,
    chats_done: bool,
    messages_done: bool,
    chats_error: bool,
    messages_error: bool,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            open: false,
            query: String::new(),
            generation: 0,
            status: SearchStatus::Closed,
            chat_ids: Vec::new(),
            messages: Vec::new(),
            recents: false,
            chats_done: false,
            messages_done: false,
            chats_error: false,
            messages_error: false,
        }
    }
}

impl SearchState {
    pub fn open_field(&mut self) {
        if self.open {
            return;
        }
        self.open = true;
        self.query.clear();
        self.recents = true;
        self.status = SearchStatus::Idle;
        self.clear_results();
    }

    pub fn close(&mut self) {
        self.open = false;
        self.query.clear();
        self.recents = false;
        self.status = SearchStatus::Closed;
        self.generation = self.generation.saturating_add(1);
        self.clear_results();
    }

    pub fn clear_query(&mut self) {
        self.query.clear();
        self.recents = true;
        self.generation = self.generation.saturating_add(1);
        self.clear_results();
        self.status = if self.open {
            SearchStatus::Idle
        } else {
            SearchStatus::Closed
        };
    }

    /// Empty search field: wait only for `searchRecentlyFoundChats`.
    pub fn begin_recents(&mut self) -> u64 {
        self.open = true;
        self.query.clear();
        self.generation = self.generation.saturating_add(1);
        self.status = SearchStatus::Searching;
        self.clear_results();
        self.recents = true;
        self.messages_done = true;
        self.generation
    }

    pub fn begin_query(&mut self, query: &str) -> u64 {
        self.open = true;
        self.query = query.to_string();
        self.generation = self.generation.saturating_add(1);
        self.status = SearchStatus::Searching;
        self.clear_results();
        self.recents = false;
        self.generation
    }

    fn clear_results(&mut self) {
        self.chat_ids.clear();
        self.messages.clear();
        self.chats_done = false;
        self.messages_done = false;
        self.chats_error = false;
        self.messages_error = false;
    }

    fn matches_generation(&self, pending: Option<&PendingRequest>) -> bool {
        pending
            .and_then(|p| p.search_generation)
            .is_some_and(|search_gen| search_gen == self.generation)
            && self.open
    }

    pub(crate) fn accept_chats(&mut self, chat_ids: Vec<ChatId>, error: bool) {
        self.chat_ids = chat_ids;
        self.chats_done = true;
        self.chats_error = error;
        self.finish_if_complete();
    }

    pub(crate) fn accept_messages(&mut self, messages: Vec<SearchMessageHit>, error: bool) {
        self.messages = messages;
        self.messages_done = true;
        self.messages_error = error;
        self.finish_if_complete();
    }

    fn finish_if_complete(&mut self) {
        if !(self.chats_done && self.messages_done) {
            return;
        }
        let any = !self.chat_ids.is_empty() || !self.messages.is_empty();
        self.status = if any {
            SearchStatus::Ready
        } else if self.chats_error || self.messages_error {
            SearchStatus::Failed
        } else if self.recents {
            SearchStatus::Idle
        } else {
            SearchStatus::Empty
        };
    }
}

/// Jump-to-message after an in-chat hit (Unigram `LoadMessageSliceAsync`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatSearchJump {
    None,
    /// `getChatHistory` around the target is in flight — not yet loaded.
    Loading {
        message_id: MessageId,
    },
    Ready {
        message_id: MessageId,
    },
    /// Deleted (tombstone) or inaccessible after the around-load returned.
    Missing {
        message_id: MessageId,
    },
}

impl ChatSearchJump {
    pub fn message_id(self) -> Option<MessageId> {
        match self {
            Self::None => None,
            Self::Loading { message_id }
            | Self::Ready { message_id }
            | Self::Missing { message_id } => Some(message_id),
        }
    }

    pub fn is_ready_at(self, id: MessageId) -> bool {
        matches!(self, Self::Ready { message_id } if message_id == id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatSearchJumpNeed {
    AlreadyReady,
    Missing,
    LoadAround,
}

/// In-chat search (tdesktop `searchInChat` / ComposeSearch): `searchChatMessages`.
#[derive(Debug, Clone)]
pub struct ChatSearchState {
    pub open: bool,
    pub chat_id: Option<ChatId>,
    pub query: String,
    pub generation: u64,
    pub status: SearchStatus,
    pub hits: Vec<SearchMessageHit>,
    pub total_count: i32,
    pub next_from_message_id: MessageId,
    pub selected: Option<usize>,
    pub jump: ChatSearchJump,
}

impl Default for ChatSearchState {
    fn default() -> Self {
        Self {
            open: false,
            chat_id: None,
            query: String::new(),
            generation: 0,
            status: SearchStatus::Closed,
            hits: Vec::new(),
            total_count: 0,
            next_from_message_id: MessageId(0),
            selected: None,
            jump: ChatSearchJump::None,
        }
    }
}

impl ChatSearchState {
    pub fn open_for(&mut self, chat_id: ChatId) {
        if self.open && self.chat_id == Some(chat_id) {
            return;
        }
        self.open = true;
        self.chat_id = Some(chat_id);
        self.query.clear();
        self.generation = self.generation.saturating_add(1);
        self.status = SearchStatus::Idle;
        self.clear_results();
    }

    pub fn close(&mut self) {
        self.open = false;
        self.chat_id = None;
        self.query.clear();
        self.status = SearchStatus::Closed;
        self.generation = self.generation.saturating_add(1);
        self.clear_results();
    }

    pub fn begin_query(&mut self, query: &str) -> u64 {
        self.open = true;
        self.query = query.to_string();
        self.generation = self.generation.saturating_add(1);
        self.status = SearchStatus::Searching;
        self.clear_results();
        self.generation
    }

    pub fn clear_query(&mut self) {
        self.query.clear();
        self.generation = self.generation.saturating_add(1);
        self.clear_results();
        self.status = if self.open {
            SearchStatus::Idle
        } else {
            SearchStatus::Closed
        };
    }

    fn clear_results(&mut self) {
        self.hits.clear();
        self.total_count = 0;
        self.next_from_message_id = MessageId(0);
        self.selected = None;
        self.jump = ChatSearchJump::None;
    }

    fn matches_generation(&self, pending: Option<&PendingRequest>) -> bool {
        pending
            .and_then(|p| p.search_generation)
            .is_some_and(|search_gen| search_gen == self.generation)
            && self.open
            && pending.and_then(|p| p.chat_id) == self.chat_id
    }

    pub(crate) fn accept_hits(
        &mut self,
        hits: Vec<SearchMessageHit>,
        total_count: i32,
        next_from_message_id: MessageId,
        error: bool,
    ) {
        self.hits = hits;
        self.total_count = total_count;
        self.next_from_message_id = next_from_message_id;
        self.selected = if self.hits.is_empty() { None } else { Some(0) };
        self.jump = ChatSearchJump::None;
        self.status = if !self.hits.is_empty() {
            SearchStatus::Ready
        } else if error {
            SearchStatus::Failed
        } else {
            SearchStatus::Empty
        };
    }

    pub fn selected_hit(&self) -> Option<&SearchMessageHit> {
        self.selected.and_then(|i| self.hits.get(i))
    }

    /// Newer hit (Unigram `NextExecute`: lower index; results are newest-first).
    pub fn select_newer(&mut self) -> Option<MessageId> {
        let index = self.selected?;
        if index == 0 {
            return None;
        }
        self.selected = Some(index - 1);
        self.hits.get(index - 1).map(|hit| hit.message_id)
    }

    /// Older hit (Unigram `PreviousExecute`: higher index).
    pub fn select_older(&mut self) -> Option<MessageId> {
        let index = self.selected?;
        if index + 1 >= self.hits.len() {
            return None;
        }
        self.selected = Some(index + 1);
        self.hits.get(index + 1).map(|hit| hit.message_id)
    }

    pub fn select_message(&mut self, message_id: MessageId) -> bool {
        let Some(index) = self
            .hits
            .iter()
            .position(|hit| hit.message_id == message_id)
        else {
            return false;
        };
        self.selected = Some(index);
        true
    }

    pub fn position_label(&self) -> String {
        match (self.selected, self.hits.len()) {
            (Some(i), n) if n > 0 => format!("{} of {n}", i + 1),
            _ => String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownPhase {
    Running,
    CloseRequested,
    WaitingClosed,
    Closed,
}

/// Composer sticker panel (Unigram `StickerDrawerViewModel` installed regular sets).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StickerPanel {
    pub open: bool,
    pub sets: Vec<StickerSetInfo>,
    pub selected_set_id: Option<i64>,
    pub stickers: Vec<StickerItem>,
    pub loaded_set_id: Option<i64>,
    pub loading_sets: bool,
    pub loading_set: bool,
    pub failed: bool,
}

impl StickerPanel {
    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn selected_needs_load(&self) -> Option<i64> {
        let id = self.selected_set_id?;
        if self.loading_set || self.loaded_set_id == Some(id) {
            None
        } else {
            Some(id)
        }
    }
}

pub struct Session {
    pub account: AccountKey,
    pub account_generation: AccountGeneration,
    pub auth: AuthorizationState,
    pub auth_view: AuthView,
    pub connection: ConnectionState,
    pub chats: HashMap<i64, ChatSummary>,
    pub main_order: Vec<ChatId>,
    pub archive_order: Vec<ChatId>,
    pub histories: HashMap<i64, HistoryState>,
    pub open_chat: Option<ChatId>,
    pub view_generation: ViewGeneration,
    pub requests: RequestRegistry,
    pub chats_exhausted: bool,
    pub shutdown: ShutdownPhase,
    pub last_seq: u64,
    /// Last classified error for phone / code / password submit. Never a secret.
    pub last_auth_error: Option<AuthRequestError>,
    /// In-flight `forwardMessages` (dest / source / requested count).
    pub in_flight_forward: Option<ForwardFlight>,
    /// Last `forwardMessages` outcome for the dest picker success surface.
    pub last_forward: Option<ForwardResult>,
    /// TDLib `file.id` → latest `file` / `localFile` snapshot.
    pub files: HashMap<i32, ParsedFile>,
    /// `downloadFile` in flight (until completed, undownloadable, idle, or error).
    pub downloading: HashSet<i32>,
    /// `@extra` → `file.id` until the download unsticks (survives `file@extra` consuming pending).
    download_extras: HashMap<u64, i32>,
    pub search: SearchState,
    pub chat_search: ChatSearchState,
    /// Installed regular sticker sets + the loaded `stickerSet` for the picker.
    pub stickers: StickerPanel,
    diagnostics: Arc<dyn DiagnosticSink>,
}

impl Session {
    pub fn new(account: AccountKey, diagnostics: Arc<dyn DiagnosticSink>) -> Self {
        let auth = AuthorizationState::WaitTdlibParameters;
        Self {
            account,
            account_generation: AccountGeneration(1),
            auth_view: view_for(&auth),
            auth,
            connection: ConnectionState::WaitingForNetwork,
            chats: HashMap::new(),
            main_order: Vec::new(),
            archive_order: Vec::new(),
            histories: HashMap::new(),
            open_chat: None,
            view_generation: ViewGeneration(1),
            requests: RequestRegistry::default(),
            chats_exhausted: false,
            shutdown: ShutdownPhase::Running,
            last_seq: 0,
            last_auth_error: None,
            in_flight_forward: None,
            last_forward: None,
            files: HashMap::new(),
            downloading: HashSet::new(),
            download_extras: HashMap::new(),
            search: SearchState::default(),
            chat_search: ChatSearchState::default(),
            stickers: StickerPanel::default(),
            diagnostics,
        }
    }

    pub fn apply(&mut self, owned: OwnedEnvelope) {
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

    fn apply_payload(
        &mut self,
        payload: EnvelopePayload,
        pending: Option<&PendingRequest>,
        extra: Option<RequestId>,
        seq: u64,
    ) {
        match payload {
            EnvelopePayload::UpdateAuthorizationState(state) => self.set_auth(state),
            EnvelopePayload::UpdateConnectionState(state) => self.connection = state,
            EnvelopePayload::UpdateNewChat {
                chat_id,
                title,
                kind,
                unread_count,
                last_read_inbox_message_id,
                last_read_outbox_message_id,
                notification_settings,
            } => {
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
            }
            EnvelopePayload::UpdateChatNotificationSettings {
                chat_id,
                notification_settings,
            } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .notification_settings = notification_settings;
            }
            EnvelopePayload::UpdateChatAction {
                chat_id,
                sender,
                action,
            } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .set_sender_action(sender, action);
            }
            EnvelopePayload::UpdateChatTitle { chat_id, title } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .title = title;
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
            EnvelopePayload::UpdateChatAddedToList { chat_id, list } => {
                let chat = self
                    .chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id));
                match list {
                    ChatList::Main => chat.in_main_list = true,
                    ChatList::Archive => chat.in_archive = true,
                    _ => {}
                }
                self.rebuild_main_order();
            }
            EnvelopePayload::UpdateChatRemovedFromList { chat_id, list } => {
                if let Some(chat) = self.chats.get_mut(&chat_id.0) {
                    match list {
                        ChatList::Main => chat.in_main_list = false,
                        ChatList::Archive => chat.in_archive = false,
                        _ => {}
                    }
                    self.rebuild_main_order();
                }
            }
            EnvelopePayload::UpdateChatLastMessage {
                chat_id,
                last_message,
                positions,
            } => {
                if let Some(ref message) = last_message {
                    self.remember_files(&message.files);
                }
                let chat = self
                    .chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id));
                chat.last_preview = last_message
                    .as_ref()
                    .map(|message| preview_from_content(&message.content))
                    .unwrap_or_default();
                // `positions` is the full set of lists this chat belongs to.
                self.replace_main_list_from_positions(chat_id, &positions);
                self.rebuild_main_order();
            }
            EnvelopePayload::UpdateChatPosition(pos) => {
                self.apply_position_fields(pos);
                self.rebuild_main_order();
            }
            EnvelopePayload::UpdateNewMessage(message) => {
                self.upsert_message(message, false);
            }
            EnvelopePayload::UpdateMessageSendSucceeded {
                message,
                old_message_id,
            } => {
                self.remember_files(&message.files);
                let history = self.histories.entry(message.chat_id.0).or_default();
                history.replace_id(old_message_id, history_message(message, false));
            }
            EnvelopePayload::UpdateMessageSendFailed {
                message,
                old_message_id,
                ..
            } => {
                self.remember_files(&message.files);
                let history = self.histories.entry(message.chat_id.0).or_default();
                history.replace_id(old_message_id, history_message(message, true));
            }
            EnvelopePayload::UpdateMessageSendAcknowledged { .. } => {
                // Not success. Keep the pending row until Succeeded/Failed.
            }
            EnvelopePayload::UpdateMessageInteractionInfo {
                chat_id,
                message_id,
                interaction_info,
            } => {
                if let Some(history) = self.histories.get_mut(&chat_id.0) {
                    history.update_interaction_info(message_id, interaction_info);
                }
            }
            EnvelopePayload::UpdateMessageIsPinned {
                chat_id,
                message_id,
                is_pinned,
            } => {
                if let Some(history) = self.histories.get_mut(&chat_id.0) {
                    history.update_is_pinned(message_id, is_pinned);
                }
            }
            EnvelopePayload::UpdateMessageContent {
                chat_id,
                message_id,
                content,
                files,
            } => {
                self.remember_files(&files);
                let preview = content.preview();
                let updated = self
                    .histories
                    .get_mut(&chat_id.0)
                    .is_some_and(|history| history.update_content(message_id, content));
                if updated {
                    let is_last = self
                        .histories
                        .get(&chat_id.0)
                        .and_then(|history| history.messages.keys().next_back().copied())
                        == Some(message_id.0);
                    if is_last && let Some(chat) = self.chats.get_mut(&chat_id.0) {
                        chat.last_preview = preview;
                    }
                }
            }
            EnvelopePayload::UpdateDeleteMessages {
                chat_id,
                message_ids,
                is_permanent,
                from_cache,
            } => {
                let history = self.histories.entry(chat_id.0).or_default();
                for id in message_ids {
                    if is_permanent {
                        history.remove(id, true);
                    } else if from_cache {
                        history.remove(id, false);
                    } else {
                        history.remove(id, true);
                    }
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
            }
            EnvelopePayload::Chats { chat_ids, .. } => {
                if self.search.matches_generation(pending)
                    && matches!(
                        pending.map(|p| p.purpose),
                        Some(
                            RequestPurpose::SearchChats | RequestPurpose::SearchRecentlyFoundChats
                        )
                    )
                {
                    self.search.accept_chats(chat_ids, false);
                }
            }
            EnvelopePayload::FoundMessages { messages, .. } => {
                if self.search.matches_generation(pending)
                    && pending.map(|p| p.purpose) == Some(RequestPurpose::SearchMessages)
                {
                    for message in &messages {
                        self.remember_files(&message.files);
                    }
                    let hits = messages.iter().map(SearchMessageHit::from_parsed).collect();
                    self.search.accept_messages(hits, false);
                }
            }
            EnvelopePayload::FoundChatMessages {
                messages,
                total_count,
                next_from_message_id,
            } => {
                if self.chat_search.matches_generation(pending)
                    && pending.map(|p| p.purpose) == Some(RequestPurpose::SearchChatMessages)
                {
                    for message in &messages {
                        self.remember_files(&message.files);
                    }
                    let hits = messages.iter().map(SearchMessageHit::from_parsed).collect();
                    self.chat_search
                        .accept_hits(hits, total_count, next_from_message_id, false);
                }
            }
            EnvelopePayload::Messages(messages) => {
                if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::ForwardMessages
                {
                    self.finish_forward(pending, &messages, false);
                    return;
                }
                if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::GetHistoryAround
                {
                    self.apply_history_around(pending, &messages, seq);
                    return;
                }
                if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::GetHistory
                {
                    if pending.view_generation != Some(self.view_generation) {
                        self.diagnostics.record(Diagnostic {
                            category: "reducer",
                            type_name: Some("messages".into()),
                            extra: Some(pending.id.0),
                            seq: Some(seq),
                            note: "stale-view-generation",
                        });
                        return;
                    }
                    if let Some(chat_id) = pending.chat_id {
                        if self.open_chat != Some(chat_id) {
                            self.diagnostics.record(Diagnostic {
                                category: "reducer",
                                type_name: Some("messages".into()),
                                extra: Some(pending.id.0),
                                seq: Some(seq),
                                note: "stale-chat-history",
                            });
                            return;
                        }
                        if messages.is_empty() {
                            self.histories.entry(chat_id.0).or_default().loaded_complete = true;
                        }
                        for message in messages {
                            self.upsert_message(message, false);
                        }
                    }
                }
            }
            EnvelopePayload::Message(message) => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::SendMessage) {
                    self.upsert_message(message, true);
                } else {
                    self.upsert_message(message, false);
                }
            }
            EnvelopePayload::UpdateFile(file) | EnvelopePayload::File(file) => {
                self.upsert_file(file, true);
            }
            EnvelopePayload::StickerSets { sets, .. } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetInstalledStickerSets) {
                    self.accept_installed_sticker_sets(sets);
                }
            }
            EnvelopePayload::StickerSet {
                id,
                stickers,
                files,
                ..
            } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetStickerSet) {
                    self.remember_files(&files);
                    self.accept_sticker_set(id, stickers);
                }
            }
            EnvelopePayload::Ok => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::LoadChats) {
                    // A short OK is not exhaustion; 404 is.
                }
                if pending.map(|p| p.purpose) == Some(RequestPurpose::ViewMessages)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.commit_viewed(chat_id);
                }
                if pending.is_some_and(|p| is_auth_submit(p.purpose)) {
                    self.last_auth_error = None;
                }
            }
            EnvelopePayload::Error(err) => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::LoadChats) && err.code == 404
                {
                    self.chats_exhausted = true;
                }
                if pending.map(|p| p.purpose) == Some(RequestPurpose::ViewMessages)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.abort_viewing(chat_id);
                }
                if self.search.matches_generation(pending) {
                    match pending.map(|p| p.purpose) {
                        Some(
                            RequestPurpose::SearchChats | RequestPurpose::SearchRecentlyFoundChats,
                        ) => {
                            self.search.accept_chats(Vec::new(), true);
                        }
                        Some(RequestPurpose::SearchMessages) => {
                            self.search.accept_messages(Vec::new(), true);
                        }
                        _ => {}
                    }
                }
                if self.chat_search.matches_generation(pending)
                    && pending.map(|p| p.purpose) == Some(RequestPurpose::SearchChatMessages)
                {
                    self.chat_search
                        .accept_hits(Vec::new(), 0, MessageId(0), true);
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
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetInstalledStickerSets) {
                    self.stickers.loading_sets = false;
                    self.stickers.failed = true;
                }
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetStickerSet) {
                    self.stickers.loading_set = false;
                    self.stickers.failed = true;
                }
                let download_id = pending
                    .filter(|p| p.purpose == RequestPurpose::DownloadFile)
                    .and_then(|p| p.file_id)
                    .or_else(|| extra.and_then(|id| self.download_extras.get(&id.0).copied()));
                if let Some(file_id) = download_id {
                    self.unstick_download(file_id);
                }
                if let Some(pending) = pending
                    && is_auth_submit(pending.purpose)
                {
                    self.last_auth_error = Some(AuthRequestError {
                        purpose: pending.purpose,
                        class: err.class,
                    });
                }
            }
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

    fn set_auth(&mut self, state: AuthorizationState) {
        if matches!(state, AuthorizationState::Closed) {
            self.shutdown = ShutdownPhase::Closed;
            self.requests.invalidate_account();
            self.account_generation.bump();
            self.files.clear();
            self.downloading.clear();
            self.download_extras.clear();
            self.search.close();
            self.chat_search.close();
            self.in_flight_forward = None;
            self.last_forward = None;
        }
        if matches!(state, AuthorizationState::LoggingOut) {
            self.requests.invalidate_account();
            self.search.close();
            self.chat_search.close();
            self.in_flight_forward = None;
        }
        if matches!(state, AuthorizationState::Closing) {
            self.shutdown = ShutdownPhase::WaitingClosed;
        }
        self.auth = state;
        self.auth_view = view_for(&self.auth);
        self.last_auth_error = None;
    }

    fn apply_position_fields(&mut self, pos: ChatPositionUpdate) {
        // A position on one list is not an eviction from the other.
        // A single `updateChatPosition` updates only that list. A full
        // `updateChatLastMessage` positions set replaces both memberships.
        let chat = self
            .chats
            .entry(pos.chat_id.0)
            .or_insert_with(|| placeholder_chat(pos.chat_id));
        match pos.list {
            ChatList::Main => {
                if pos.order == 0 {
                    chat.in_main_list = false;
                } else {
                    chat.order = pos.order;
                    chat.is_pinned = pos.is_pinned;
                    chat.in_main_list = true;
                }
            }
            ChatList::Archive => {
                if pos.order == 0 {
                    chat.in_archive = false;
                } else {
                    chat.archive_order = pos.order;
                    chat.archive_is_pinned = pos.is_pinned;
                    chat.in_archive = true;
                }
            }
            ChatList::Folder(_) | ChatList::Unknown => {}
        }
    }

    fn replace_main_list_from_positions(
        &mut self,
        chat_id: ChatId,
        positions: &[ChatPositionUpdate],
    ) {
        match positions
            .iter()
            .find(|pos| pos.list == ChatList::Main)
            .cloned()
        {
            Some(pos) => self.apply_position_fields(pos),
            None => {
                if let Some(chat) = self.chats.get_mut(&chat_id.0) {
                    chat.in_main_list = false;
                }
            }
        }
        match positions
            .iter()
            .find(|pos| pos.list == ChatList::Archive)
            .cloned()
        {
            Some(pos) => self.apply_position_fields(pos),
            None => {
                if let Some(chat) = self.chats.get_mut(&chat_id.0) {
                    chat.in_archive = false;
                }
            }
        }
    }

    fn upsert_message(&mut self, message: ParsedMessage, pending: bool) {
        self.remember_files(&message.files);
        let history = self.histories.entry(message.chat_id.0).or_default();
        history.upsert(history_message(message, pending));
    }

    fn remember_files(&mut self, files: &[ParsedFile]) {
        for file in files {
            // Nested message files can still be idle while a download is in flight.
            self.upsert_file(file.clone(), false);
        }
    }

    fn upsert_file(&mut self, file: ParsedFile, from_file_update: bool) {
        let idle_incomplete = file.local.is_idle_incomplete();
        if file.local.is_downloading_completed
            || !file.local.can_be_downloaded
            || (from_file_update && idle_incomplete)
        {
            self.unstick_download(file.id.0);
        }
        self.files.insert(file.id.0, file);
    }

    fn unstick_download(&mut self, file_id: i32) {
        self.downloading.remove(&file_id);
        self.download_extras.retain(|_, id| *id != file_id);
    }

    pub fn file(&self, id: FileId) -> Option<&ParsedFile> {
        self.files.get(&id.0)
    }

    pub fn should_download(&self, file_id: FileId) -> bool {
        if file_id.0 == 0 {
            return false;
        }
        if self.downloading.contains(&file_id.0) || self.requests.has_download(file_id) {
            return false;
        }
        match self.files.get(&file_id.0) {
            Some(file) => file.needs_download(),
            None => true,
        }
    }

    pub fn begin_download(&mut self, file_id: FileId) {
        if file_id.0 != 0 {
            self.downloading.insert(file_id.0);
        }
    }

    pub fn abort_download(&mut self, file_id: FileId) {
        self.unstick_download(file_id.0);
    }

    /// Photo thumbs in the open chat that are not secret/spoiler and still need a download.
    pub fn thumb_file_ids_to_download(&self) -> Vec<FileId> {
        let Some(chat_id) = self.open_chat else {
            return Vec::new();
        };
        let Some(history) = self.histories.get(&chat_id.0) else {
            return Vec::new();
        };
        let mut ids = Vec::new();
        for message in history.messages.values() {
            match &message.content {
                MessageContent::Photo(photo) => {
                    if photo.is_secret || photo.has_spoiler {
                        continue;
                    }
                    if let Some(size) = photo.thumb_size()
                        && self.should_download(size.file_id)
                    {
                        ids.push(size.file_id);
                    }
                }
                MessageContent::Sticker(sticker) => {
                    if let Some(file_id) = sticker.display_file_id()
                        && self.should_download(file_id)
                    {
                        ids.push(file_id);
                    }
                }
                _ => {}
            }
        }
        if self.stickers.open {
            for sticker in &self.stickers.stickers {
                let file_id = sticker.thumb_file_id.filter(|id| id.0 != 0).or_else(|| {
                    (sticker.format == StickerFormat::Webp && sticker.file_id.0 != 0)
                        .then_some(sticker.file_id)
                });
                if let Some(file_id) = file_id
                    && self.should_download(file_id)
                {
                    ids.push(file_id);
                }
            }
        }
        ids.sort_by_key(|id| id.0);
        ids.dedup();
        ids
    }

    pub fn accept_installed_sticker_sets(&mut self, sets: Vec<StickerSetInfo>) {
        self.stickers.loading_sets = false;
        self.stickers.failed = false;
        self.stickers.sets = sets;
        let still_selected = self
            .stickers
            .selected_set_id
            .is_some_and(|id| self.stickers.sets.iter().any(|set| set.id == id));
        if !still_selected {
            self.stickers.selected_set_id = self.stickers.sets.first().map(|set| set.id);
            self.stickers.loaded_set_id = None;
            self.stickers.stickers.clear();
        }
    }

    pub fn select_sticker_set(&mut self, set_id: i64) {
        if self.stickers.selected_set_id == Some(set_id) {
            return;
        }
        self.stickers.selected_set_id = Some(set_id);
        self.stickers.loaded_set_id = None;
        self.stickers.stickers.clear();
        self.stickers.loading_set = false;
        self.stickers.failed = false;
    }

    pub fn mark_sticker_set_loading(&mut self) {
        self.stickers.loading_set = true;
        self.stickers.failed = false;
    }

    pub fn accept_sticker_set(&mut self, id: i64, stickers: Vec<StickerItem>) {
        self.stickers.loading_set = false;
        if self
            .stickers
            .selected_set_id
            .is_some_and(|selected| selected != id)
        {
            return;
        }
        self.stickers.failed = false;
        self.stickers.loaded_set_id = Some(id);
        self.stickers.stickers = stickers;
    }

    pub fn request_download(&mut self, file_id: FileId) -> RequestId {
        let extra = self
            .requests
            .register_download(self.account_generation, file_id);
        self.download_extras.insert(extra.0, file_id.0);
        extra
    }

    fn rebuild_main_order(&mut self) {
        let mut rows: Vec<ChatSummary> = self
            .chats
            .values()
            .filter(|c| c.in_main_list)
            .cloned()
            .collect();
        rows.sort_by(|a, b| b.order.cmp(&a.order).then(b.id.0.cmp(&a.id.0)));
        self.main_order = rows.into_iter().map(|c| c.id).collect();
        let mut archived: Vec<ChatSummary> = self
            .chats
            .values()
            .filter(|c| c.in_archive)
            .cloned()
            .collect();
        archived.sort_by(|a, b| {
            b.archive_order
                .cmp(&a.archive_order)
                .then(b.id.0.cmp(&a.id.0))
        });
        self.archive_order = archived.into_iter().map(|c| c.id).collect();
    }

    pub fn open_chat(&mut self, chat_id: ChatId) {
        if self.chat_search.chat_id != Some(chat_id) {
            self.chat_search.close();
        }
        self.open_chat = Some(chat_id);
        self.view_generation.bump();
        let history = self.histories.entry(chat_id.0).or_default();
        history.view_generation = self.view_generation;
        history.viewed.clear();
        history.viewing.clear();
    }

    /// Server message ids in the open history that have not yet been sent to `viewMessages`.
    pub fn message_ids_to_view(&self, chat_id: ChatId) -> Vec<MessageId> {
        let Some(history) = self.histories.get(&chat_id.0) else {
            return Vec::new();
        };
        history
            .messages
            .values()
            .filter(|message| {
                message.id.0 > 0
                    && !history.viewed.contains(&message.id.0)
                    && !history.viewing.contains(&message.id.0)
            })
            .map(|message| message.id)
            .collect()
    }

    pub fn mark_viewed(&mut self, chat_id: ChatId, ids: &[MessageId]) {
        let history = self.histories.entry(chat_id.0).or_default();
        for id in ids {
            history.viewing.remove(&id.0);
            history.viewed.insert(id.0);
        }
    }

    /// After a successful `viewMessages` send, hold ids until TDLib ok/error.
    pub fn begin_viewing(&mut self, chat_id: ChatId, ids: &[MessageId]) {
        let history = self.histories.entry(chat_id.0).or_default();
        for id in ids {
            history.viewing.insert(id.0);
        }
    }

    fn commit_viewed(&mut self, chat_id: ChatId) {
        let history = self.histories.entry(chat_id.0).or_default();
        history.viewed.extend(history.viewing.drain());
    }

    fn abort_viewing(&mut self, chat_id: ChatId) {
        if let Some(history) = self.histories.get_mut(&chat_id.0) {
            history.viewing.clear();
        }
    }

    pub fn ordered_chats(&self) -> Vec<&ChatSummary> {
        self.main_order
            .iter()
            .filter_map(|id| self.chats.get(&id.0))
            .filter(|chat| chat.in_main_list)
            .collect()
    }

    /// Chats in `chatListArchive`, highest TDLib order first (same as main).
    pub fn ordered_archived_chats(&self) -> Vec<&ChatSummary> {
        self.archive_order
            .iter()
            .filter_map(|id| self.chats.get(&id.0))
            .filter(|chat| chat.in_archive)
            .collect()
    }

    pub fn request(&mut self, purpose: RequestPurpose, chat_id: Option<ChatId>) -> RequestId {
        let view = if matches!(purpose, RequestPurpose::GetHistory) {
            Some(self.view_generation)
        } else {
            None
        };
        self.requests
            .register(self.account_generation, purpose, chat_id, view)
    }

    pub fn request_search(&mut self, purpose: RequestPurpose, search_generation: u64) -> RequestId {
        self.requests
            .register_search(self.account_generation, purpose, search_generation)
    }

    pub fn request_chat_search(
        &mut self,
        purpose: RequestPurpose,
        chat_id: ChatId,
        search_generation: u64,
    ) -> RequestId {
        self.requests.register_chat_search(
            self.account_generation,
            purpose,
            chat_id,
            search_generation,
        )
    }

    pub fn request_history_around(&mut self, chat_id: ChatId, message_id: MessageId) -> RequestId {
        self.requests.register_around(
            self.account_generation,
            chat_id,
            self.view_generation,
            message_id,
        )
    }

    pub fn open_search(&mut self) {
        self.search.open_field();
    }

    pub fn close_search(&mut self) {
        self.search.close();
    }

    pub fn open_chat_search(&mut self) -> bool {
        let Some(chat_id) = self.open_chat else {
            return false;
        };
        if !self
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported())
        {
            return false;
        }
        self.chat_search.open_for(chat_id);
        true
    }

    pub fn close_chat_search(&mut self) {
        self.chat_search.close();
    }

    fn apply_history_around(
        &mut self,
        pending: &PendingRequest,
        messages: &[ParsedMessage],
        seq: u64,
    ) {
        if pending.view_generation != Some(self.view_generation) {
            self.diagnostics.record(Diagnostic {
                category: "reducer",
                type_name: Some("messages".into()),
                extra: Some(pending.id.0),
                seq: Some(seq),
                note: "stale-view-generation",
            });
            return;
        }
        let Some(chat_id) = pending.chat_id else {
            return;
        };
        if self.open_chat != Some(chat_id) {
            self.diagnostics.record(Diagnostic {
                category: "reducer",
                type_name: Some("messages".into()),
                extra: Some(pending.id.0),
                seq: Some(seq),
                note: "stale-chat-history",
            });
            return;
        }
        for message in messages {
            self.upsert_message(message.clone(), false);
        }
        if let Some(message_id) = pending.around_message_id {
            self.finish_history_around(Some(pending), message_id, false, seq);
        }
    }

    fn finish_history_around(
        &mut self,
        pending: Option<&PendingRequest>,
        message_id: MessageId,
        _error: bool,
        seq: u64,
    ) {
        if !matches!(
            self.chat_search.jump,
            ChatSearchJump::Loading { message_id: current } if current == message_id
        ) {
            self.diagnostics.record(Diagnostic {
                category: "reducer",
                type_name: Some("messages".into()),
                extra: pending.map(|p| p.id.0),
                seq: Some(seq),
                note: "stale-chat-search-jump",
            });
            return;
        }
        let Some(chat_id) = pending.and_then(|p| p.chat_id).or(self.chat_search.chat_id) else {
            return;
        };
        let history = self.histories.entry(chat_id.0).or_default();
        // Tombstone, 404/error, or around-load without the id: deleted/inaccessible.
        self.chat_search.jump = if history.contains(message_id) {
            ChatSearchJump::Ready { message_id }
        } else {
            ChatSearchJump::Missing { message_id }
        };
    }

    /// Resolve a hit: already loaded, tombstoned/deleted, or needs `getChatHistory` around.
    pub fn begin_chat_search_jump(&mut self, message_id: MessageId) -> ChatSearchJumpNeed {
        let Some(chat_id) = self.chat_search.chat_id.or(self.open_chat) else {
            self.chat_search.jump = ChatSearchJump::Missing { message_id };
            return ChatSearchJumpNeed::Missing;
        };
        self.chat_search.select_message(message_id);
        let history = self.histories.entry(chat_id.0).or_default();
        if history.is_tombstone(message_id) {
            self.chat_search.jump = ChatSearchJump::Missing { message_id };
            return ChatSearchJumpNeed::Missing;
        }
        if history.contains(message_id) {
            self.chat_search.jump = ChatSearchJump::Ready { message_id };
            return ChatSearchJumpNeed::AlreadyReady;
        }
        self.chat_search.jump = ChatSearchJump::Loading { message_id };
        ChatSearchJumpNeed::LoadAround
    }

    pub fn apply_local_chat_search_filter(&mut self, query: &str) {
        let Some(chat_id) = self.open_chat else {
            return;
        };
        if !self.chat_search.open {
            self.chat_search.open_for(chat_id);
        }
        let trimmed = query.trim();
        if trimmed.is_empty() {
            self.chat_search.clear_query();
            return;
        }
        let _ = self.chat_search.begin_query(trimmed);
        let needle = trimmed.to_lowercase();
        let hits: Vec<SearchMessageHit> = self
            .histories
            .get(&chat_id.0)
            .map(|history| {
                history
                    .ordered()
                    .into_iter()
                    .rev()
                    .filter(|message| message.content.preview().to_lowercase().contains(&needle))
                    .map(|message| SearchMessageHit {
                        chat_id: message.chat_id,
                        message_id: message.id,
                        preview: message.content.preview(),
                        is_outgoing: message.is_outgoing,
                        content: message.content.clone(),
                        reply_to: message.reply_to.clone(),
                        forward_info: message.forward_info.clone(),
                        interaction_info: message.interaction_info.clone(),
                        is_pinned: message.is_pinned,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let total = hits.len() as i32;
        self.chat_search
            .accept_hits(hits, total, MessageId(0), false);
        if let Some(id) = self.chat_search.selected_hit().map(|hit| hit.message_id) {
            let _ = self.begin_chat_search_jump(id);
        }
    }

    /// Newest pinned message in the open chat's loaded history.
    pub fn open_chat_pinned_message(&self) -> Option<&HistoryMessage> {
        let chat_id = self.open_chat?;
        self.histories.get(&chat_id.0)?.newest_pinned()
    }

    /// Insert a found message into that chat's history so open-chat can show it
    /// without a separate history pagination scheme.
    pub fn promote_search_message(&mut self, chat_id: ChatId, message_id: MessageId) {
        let Some(index) = self
            .search
            .messages
            .iter()
            .position(|hit| hit.chat_id == chat_id && hit.message_id == message_id)
        else {
            return;
        };
        let hit = self.search.messages[index].clone();
        let history = self.histories.entry(chat_id.0).or_default();
        history.upsert(hit.into_history());
    }

    /// Main-list chats whose title contains `query` (case-insensitive). Demo-only
    /// local filter when no live TDLib replies are injected.
    pub fn local_search_chats(&self, query: &str) -> Vec<&ChatSummary> {
        let needle = query.trim().to_lowercase();
        self.ordered_chats()
            .into_iter()
            .filter(|chat| needle.is_empty() || chat.title.to_lowercase().contains(&needle))
            .collect()
    }

    pub fn apply_local_search_filter(&mut self, query: &str) {
        self.search.open = true;
        self.search.query = query.to_string();
        self.search.generation = self.search.generation.saturating_add(1);
        self.search.clear_results();
        self.search.recents = query.trim().is_empty();
        self.search.chat_ids = self
            .local_search_chats(query)
            .into_iter()
            .map(|chat| chat.id)
            .collect();
        self.search.chats_done = true;
        self.search.messages_done = true;
        if query.trim().is_empty() {
            self.search.chat_ids.clear();
            self.search.status = SearchStatus::Idle;
        } else {
            self.search.finish_if_complete();
        }
    }

    pub fn begin_close(&mut self) {
        self.shutdown = ShutdownPhase::CloseRequested;
    }

    pub fn begin_logout(&mut self) {
        self.requests.invalidate_account();
        self.account_generation.bump();
        self.shutdown = ShutdownPhase::CloseRequested;
    }
}

fn history_message(message: ParsedMessage, pending: bool) -> HistoryMessage {
    HistoryMessage {
        id: message.id,
        chat_id: message.chat_id,
        is_outgoing: message.is_outgoing,
        content: message.content,
        pending,
        reply_to: message.reply_to,
        forward_info: message.forward_info,
        interaction_info: message.interaction_info,
        is_pinned: message.is_pinned,
    }
}

impl Session {
    /// Compact quote label: chosen `textQuote`, else the loaded original,
    /// else `messageReplyToMessage.content` preview.
    pub fn reply_quote_preview(&self, message: &HistoryMessage) -> Option<String> {
        let reply = message.reply_to.as_ref()?;
        Some(self.resolve_reply_preview(reply, message.chat_id))
    }

    pub fn resolve_reply_preview(&self, reply: &MessageReplyTo, fallback_chat: ChatId) -> String {
        if let Some(quote) = reply
            .quote_text
            .as_ref()
            .map(|text| text.trim())
            .filter(|text| !text.is_empty())
        {
            return quote.to_string();
        }
        let chat_id = if reply.chat_id.0 != 0 {
            reply.chat_id
        } else {
            fallback_chat
        };
        if let Some(original) = self
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&reply.message_id.0))
        {
            return original.content.preview();
        }
        reply
            .content_preview
            .clone()
            .unwrap_or_else(|| "Message".into())
    }

    /// Loaded Main-list destinations for the forward picker. Local title filter
    /// (tdesktop ShareBox search field). Unsupported kinds stay out.
    pub fn forward_destinations(&self, query: &str) -> Vec<&ChatSummary> {
        self.local_search_chats(query)
            .into_iter()
            .filter(|chat| chat.supported())
            .collect()
    }

    /// Official "Forwarded from" label from `messageForwardInfo.origin`.
    pub fn forward_from_label(&self, info: &MessageForwardInfo) -> String {
        match &info.origin {
            MessageOrigin::HiddenUser { sender_name } if !sender_name.trim().is_empty() => {
                format!("Forwarded from {sender_name}")
            }
            MessageOrigin::User { user_id } => self
                .chats
                .values()
                .find(
                    |chat| matches!(chat.kind, ChatKind::Private { user_id: id } if id == *user_id),
                )
                .map(|chat| format!("Forwarded from {}", chat.title))
                .unwrap_or_else(|| "Forwarded message".into()),
            MessageOrigin::Chat {
                chat_id,
                author_signature,
            }
            | MessageOrigin::Channel {
                chat_id,
                author_signature,
                ..
            } => {
                if let Some(chat) = self.chats.get(&chat_id.0) {
                    if author_signature.is_empty() {
                        format!("Forwarded from {}", chat.title)
                    } else {
                        format!("Forwarded from {} ({author_signature})", chat.title)
                    }
                } else if !author_signature.is_empty() {
                    format!("Forwarded from {author_signature}")
                } else {
                    "Forwarded message".into()
                }
            }
            _ => "Forwarded message".into(),
        }
    }

    fn finish_forward(
        &mut self,
        pending: &PendingRequest,
        messages: &[ParsedMessage],
        failed: bool,
    ) {
        let forwarded: Vec<ParsedMessage> = if failed {
            Vec::new()
        } else {
            messages.to_vec()
        };
        for message in &forwarded {
            self.remember_files(&message.files);
            self.upsert_message(message.clone(), message.id.0 < 0);
        }
        let flight = self
            .in_flight_forward
            .take()
            .filter(|flight| flight.extra == pending.id);
        let dest_chat_id = flight
            .as_ref()
            .map(|f| f.dest_chat_id)
            .or(pending.chat_id)
            .unwrap_or(ChatId(0));
        let dest_title = self
            .chats
            .get(&dest_chat_id.0)
            .map(|chat| chat.title.clone())
            .unwrap_or_else(|| format!("chat {}", dest_chat_id.0));
        self.last_forward = Some(ForwardResult {
            dest_chat_id,
            dest_title,
            from_chat_id: flight.as_ref().map(|f| f.from_chat_id).unwrap_or(ChatId(0)),
            requested: flight
                .as_ref()
                .map(|f| f.requested)
                .unwrap_or(forwarded.len()),
            forwarded_ids: forwarded.iter().map(|m| m.id).collect(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::MemorySink;
    use crate::telegram::client::copy_and_parse;
    use std::sync::atomic::AtomicU64;

    fn session() -> (Session, Arc<MemorySink>) {
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        (Session::new(AccountKey::primary(), dyn_sink), sink)
    }

    fn apply_json(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>, json: &str) {
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let owned = copy_and_parse(json, seq, &dyn_sink).unwrap();
        session.apply(owned);
    }

    #[test]
    fn send_success_replaces_pending_id() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_chat(ChatId(1));
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":-1,"chat_id":1,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateMessageSendSucceeded","old_message_id":-1,"message":{"id":88,"chat_id":1,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#,
        );
        let history = session.histories.get(&1).unwrap();
        assert!(!history.messages.contains_key(&-1));
        assert!(history.messages.contains_key(&88));
        assert!(!history.messages.get(&88).unwrap().pending);
    }

    #[test]
    fn stale_history_does_not_mix_chats() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_chat(ChatId(1));
        let extra = session.request(RequestPurpose::GetHistory, Some(ChatId(1)));
        session.open_chat(ChatId(2));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"messages","@extra":"{}","messages":[{{"id":5,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"from-a","entities":[]}}}}}}]}}"#,
                extra.0
            ),
        );
        assert!(
            session
                .histories
                .get(&1)
                .map(|h| h.messages.is_empty())
                .unwrap_or(true)
        );
        assert!(
            session
                .histories
                .get(&2)
                .map(|h| h.messages.is_empty())
                .unwrap_or(true)
        );
    }

    #[test]
    fn permanent_delete_wins_over_old_fetch() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_chat(ChatId(1));
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":9,"chat_id":1,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"bye","entities":[]}}}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateDeleteMessages","chat_id":1,"message_ids":[9],"is_permanent":true,"from_cache":false}"#,
        );
        let extra = session.request(RequestPurpose::GetHistory, Some(ChatId(1)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"messages","@extra":"{}","messages":[{{"id":9,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"bye","entities":[]}}}}}}]}}"#,
                extra.0
            ),
        );
        assert!(!session.histories.get(&1).unwrap().messages.contains_key(&9));
        assert!(session.histories.get(&1).unwrap().tombstones.contains(&9));
    }

    #[test]
    fn cache_eviction_is_not_permanent() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_chat(ChatId(1));
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":9,"chat_id":1,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateDeleteMessages","chat_id":1,"message_ids":[9],"is_permanent":false,"from_cache":true}"#,
        );
        let extra = session.request(RequestPurpose::GetHistory, Some(ChatId(1)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"messages","@extra":"{}","messages":[{{"id":9,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}}]}}"#,
                extra.0
            ),
        );
        assert!(session.histories.get(&1).unwrap().messages.contains_key(&9));
    }

    #[test]
    fn chat_list_sorts_by_tdlib_order_then_id() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":2,"title":"b","type":{"@type":"chatTypePrivate","user_id":2},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":1,"title":"a","type":{"@type":"chatTypePrivate","user_id":1},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatPosition","chat_id":1,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"10","is_pinned":false}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatPosition","chat_id":2,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"10","is_pinned":false}}"#,
        );
        let ids: Vec<i64> = session.ordered_chats().iter().map(|c| c.id.0).collect();
        assert_eq!(ids, vec![2, 1]);
    }

    #[test]
    fn channel_is_gated() {
        let kind = ChatKind::Supergroup {
            supergroup_id: 1,
            is_channel: true,
        };
        assert!(!kind.is_supported_cloud_chat());
        assert!(kind.gate_reason().unwrap().contains("sponsored"));
    }

    #[test]
    fn load_chats_404_marks_exhaustion() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::LoadChats, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","code":404,"message":"Not Found","@extra":"{}"}}"#,
                extra.0
            ),
        );
        assert!(session.chats_exhausted);
        assert!(!sink.rendered().contains("Not Found"));
    }

    #[test]
    fn auth_code_error_is_classified_without_native_message() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::CheckAuthenticationCode, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","code":400,"message":"PHONE_CODE_INVALID CANARY_CODE_999","@extra":"{}"}}"#,
                extra.0
            ),
        );
        let err = session.last_auth_error.expect("classified auth error");
        assert_eq!(err.purpose, RequestPurpose::CheckAuthenticationCode);
        assert_eq!(err.class, ErrorClass::Invalid);
        assert_eq!(err.user_message(), "code not accepted");
        let logs = sink.rendered();
        assert!(!logs.contains("CANARY_CODE"));
        assert!(!logs.contains("PHONE_CODE_INVALID"));
        let debug = format!("{err:?}");
        assert!(!debug.contains("CANARY_CODE"));
    }

    #[test]
    fn new_chat_after_position_keeps_main_list_membership() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatPosition","chat_id":9,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"4","is_pinned":true}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":9,"title":"after","type":{"@type":"chatTypePrivate","user_id":9},"unread_count":2}}"#,
        );
        let chat = session.chats.get(&9).unwrap();
        assert_eq!(chat.title, "after");
        assert!(chat.in_main_list);
        assert!(chat.is_pinned);
        assert_eq!(chat.order, 4);
        assert_eq!(session.ordered_chats().len(), 1);
    }

    #[test]
    fn last_message_positions_replace_main_list_membership() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":3,"title":"c","type":{"@type":"chatTypePrivate","user_id":3},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatLastMessage","chat_id":3,"last_message":{"id":1,"chat_id":3,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_PREVIEW_hi","entities":[]}}},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"8","is_pinned":false}]}"#,
        );
        let chat = session.chats.get(&3).unwrap();
        assert!(chat.in_main_list);
        assert_eq!(chat.last_preview, "CANARY_PREVIEW_hi");
        assert_eq!(session.ordered_chats()[0].id.0, 3);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatLastMessage","chat_id":3,"last_message":null,"positions":[]}"#,
        );
        assert!(!session.chats.get(&3).unwrap().in_main_list);
        assert!(session.ordered_chats().is_empty());
        assert!(!sink.rendered().contains("CANARY_PREVIEW"));
    }

    #[test]
    fn archive_position_does_not_clear_main_list() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":5,"title":"keep","type":{"@type":"chatTypePrivate","user_id":5},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatPosition","chat_id":5,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"6","is_pinned":false}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatPosition","chat_id":5,"position":{"@type":"chatPosition","list":{"@type":"chatListArchive"},"order":"3","is_pinned":false}}"#,
        );
        let chat = session.chats.get(&5).unwrap();
        assert!(chat.in_main_list);
        assert!(chat.in_archive);
        assert_eq!(chat.archive_order, 3);
        assert_eq!(chat.order, 6);
        assert_eq!(session.ordered_chats().len(), 1);
        assert_eq!(session.ordered_archived_chats().len(), 1);
    }

    #[test]
    fn last_message_positions_are_a_full_set_including_archive() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":6,"title":"mixed","type":{"@type":"chatTypePrivate","user_id":6},"unread_count":0}}"#,
        );
        // Archive after Main in the array must not wipe Main membership.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatLastMessage","chat_id":6,"last_message":{"id":2,"chat_id":6,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9","is_pinned":true},{"@type":"chatPosition","list":{"@type":"chatListArchive"},"order":"1","is_pinned":false}]}"#,
        );
        let chat = session.chats.get(&6).unwrap();
        assert!(chat.in_main_list);
        assert!(chat.is_pinned);
        assert_eq!(chat.order, 9);
        // Full set without Main removes from the main list.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatLastMessage","chat_id":6,"last_message":null,"positions":[{"@type":"chatPosition","list":{"@type":"chatListArchive"},"order":"1","is_pinned":false}]}"#,
        );
        let chat = session.chats.get(&6).unwrap();
        assert!(!chat.in_main_list);
        assert!(chat.in_archive);
        assert!(session.ordered_chats().is_empty());
        assert_eq!(session.ordered_archived_chats()[0].id.0, 6);
    }

    #[test]
    fn notification_settings_mute_and_unmute() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"m","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0,"notification_settings":{"@type":"chatNotificationSettings","use_default_mute_for":false,"mute_for":2147483647,"use_default_sound":true,"sound_id":"0","use_default_show_preview":true,"show_preview":false,"use_default_mute_stories":true,"mute_stories":false,"use_default_story_sound":true,"story_sound_id":"0","use_default_show_story_poster":true,"show_story_poster":false,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false}}}"#,
        );
        let chat = session.chats.get(&7).unwrap();
        assert!(chat.is_muted());
        assert!(chat.notification_settings.is_muted_forever());
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatNotificationSettings","chat_id":7,"notification_settings":{"@type":"chatNotificationSettings","use_default_mute_for":false,"mute_for":0,"use_default_sound":true,"sound_id":"0","use_default_show_preview":true,"show_preview":false,"use_default_mute_stories":true,"mute_stories":false,"use_default_story_sound":true,"story_sound_id":"0","use_default_show_story_poster":true,"show_story_poster":false,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false}}"#,
        );
        assert!(!session.chats.get(&7).unwrap().is_muted());
    }

    #[test]
    fn chat_action_typing_then_cancel() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatAction","chat_id":7,"topic_id":null,"sender_id":{"@type":"messageSenderUser","user_id":7},"action":{"@type":"chatActionTyping"}}"#,
        );
        let chat = session.chats.get(&7).unwrap();
        assert!(chat.is_peer_typing());
        assert_eq!(chat.sidebar_preview(), "typing…");
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatAction","chat_id":7,"sender_id":{"@type":"messageSenderUser","user_id":7},"action":{"@type":"chatActionCancel"}}"#,
        );
        let chat = session.chats.get(&7).unwrap();
        assert!(!chat.is_peer_typing());
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatAction","chat_id":7,"sender_id":{"@type":"messageSenderUser","user_id":9},"action":{"@type":"chatActionRecordingVoiceNote"}}"#,
        );
        assert!(!session.chats.get(&7).unwrap().is_peer_typing());
    }

    #[test]
    fn chat_title_and_unread_updates() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":4,"title":"old","type":{"@type":"chatTypePrivate","user_id":4},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatTitle","chat_id":4,"title":"new title"}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatReadInbox","chat_id":4,"last_read_inbox_message_id":1,"unread_count":7}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatAddedToList","chat_id":4,"chat_list":{"@type":"chatListMain"}}"#,
        );
        let chat = session.chats.get(&4).unwrap();
        assert_eq!(chat.title, "new title");
        assert_eq!(chat.unread_count, 7);
        assert_eq!(chat.last_read_inbox_message_id.0, 1);
        assert!(chat.in_main_list);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatRemovedFromList","chat_id":4,"chat_list":{"@type":"chatListMain"}}"#,
        );
        assert!(!session.chats.get(&4).unwrap().in_main_list);
    }

    #[test]
    fn auth_password_ok_clears_last_error() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::CheckAuthenticationPassword, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","code":400,"message":"PASSWORD_HASH_INVALID CANARY_PW","@extra":"{}"}}"#,
                extra.0
            ),
        );
        assert!(session.last_auth_error.is_some());
        let extra = session.request(RequestPurpose::CheckAuthenticationPassword, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        );
        assert!(session.last_auth_error.is_none());
        assert!(!sink.rendered().contains("CANARY_PW"));
    }

    #[test]
    fn unread_badge_hides_when_zero_and_caps_at_99() {
        assert_eq!(unread_badge_text(0), None);
        assert_eq!(unread_badge_text(-1), None);
        assert_eq!(unread_badge_text(1).as_deref(), Some("1"));
        assert_eq!(unread_badge_text(3).as_deref(), Some("3"));
        assert_eq!(unread_badge_text(99).as_deref(), Some("99"));
        assert_eq!(unread_badge_text(100).as_deref(), Some("99+"));
    }

    #[test]
    fn read_inbox_and_outbox_update_cursors() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":4,"title":"inbox","type":{"@type":"chatTypePrivate","user_id":4},"unread_count":3,"last_read_inbox_message_id":10,"last_read_outbox_message_id":0}}"#,
        );
        let chat = session.chats.get(&4).unwrap();
        assert_eq!(chat.unread_count, 3);
        assert_eq!(chat.last_read_inbox_message_id.0, 10);
        assert_eq!(chat.last_read_outbox_message_id.0, 0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatReadInbox","chat_id":4,"last_read_inbox_message_id":13,"unread_count":0}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatReadOutbox","chat_id":4,"last_read_outbox_message_id":12}"#,
        );
        let chat = session.chats.get(&4).unwrap();
        assert_eq!(chat.unread_count, 0);
        assert_eq!(unread_badge_text(chat.unread_count), None);
        assert_eq!(chat.last_read_inbox_message_id.0, 13);
        assert_eq!(chat.last_read_outbox_message_id.0, 12);
        assert!(!sink.rendered().contains("CANARY"));
    }

    #[test]
    fn outbox_receipt_is_honest_when_cursor_is_zero() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_chat(ChatId(1));
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":1,"title":"dm","type":{"@type":"chatTypePrivate","user_id":1},"unread_count":0,"last_read_outbox_message_id":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":20,"chat_id":1,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_OUTBOX_hi","entities":[]}}}}"#,
        );
        let chat = session.chats.get(&1).unwrap();
        let message = session
            .histories
            .get(&1)
            .unwrap()
            .messages
            .get(&20)
            .unwrap();
        assert_eq!(chat.outbox_receipt(message), OutboxReceipt::Sent);
        assert_eq!(
            outgoing_status_label(false, chat.outbox_receipt(message)),
            "You · sent"
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatReadOutbox","chat_id":1,"last_read_outbox_message_id":20}"#,
        );
        let chat = session.chats.get(&1).unwrap();
        let message = session
            .histories
            .get(&1)
            .unwrap()
            .messages
            .get(&20)
            .unwrap();
        assert_eq!(chat.outbox_receipt(message), OutboxReceipt::Read);
        assert_eq!(
            outgoing_status_label(false, chat.outbox_receipt(message)),
            "You · read"
        );
        assert_eq!(
            outgoing_status_label(true, OutboxReceipt::None),
            "You (sending)"
        );
        assert!(!sink.rendered().contains("CANARY_OUTBOX"));
    }

    #[test]
    fn opening_a_chat_clears_viewed_ids_for_that_generation() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_chat(ChatId(1));
        session.mark_viewed(ChatId(1), &[MessageId(5)]);
        assert!(session.histories.get(&1).unwrap().viewed.contains(&5));
        session.open_chat(ChatId(1));
        assert!(session.histories.get(&1).unwrap().viewed.is_empty());
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":5,"chat_id":1,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#,
        );
        assert_eq!(session.message_ids_to_view(ChatId(1)), vec![MessageId(5)]);
        session.mark_viewed(ChatId(1), &[MessageId(5)]);
        assert!(session.message_ids_to_view(ChatId(1)).is_empty());
    }

    #[test]
    fn view_messages_tdlib_error_releases_in_flight_ids() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_chat(ChatId(1));
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":5,"chat_id":1,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#,
        );
        let extra = session.request(RequestPurpose::ViewMessages, Some(ChatId(1)));
        session.begin_viewing(ChatId(1), &[MessageId(5)]);
        assert!(session.message_ids_to_view(ChatId(1)).is_empty());
        assert!(
            session
                .requests
                .has_purpose_for_chat(RequestPurpose::ViewMessages, ChatId(1))
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","code":400,"message":"CANARY_VIEW_FAIL","@extra":"{}"}}"#,
                extra.0
            ),
        );
        assert!(!session.requests.has_purpose(RequestPurpose::ViewMessages));
        assert_eq!(session.message_ids_to_view(ChatId(1)), vec![MessageId(5)]);
        assert!(session.histories.get(&1).unwrap().viewing.is_empty());
        assert!(!session.histories.get(&1).unwrap().viewed.contains(&5));
        assert!(!sink.rendered().contains("CANARY_VIEW_FAIL"));

        let extra = session.request(RequestPurpose::ViewMessages, Some(ChatId(1)));
        session.begin_viewing(ChatId(1), &[MessageId(5)]);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
        );
        assert!(session.message_ids_to_view(ChatId(1)).is_empty());
        assert!(session.histories.get(&1).unwrap().viewed.contains(&5));
        assert!(session.histories.get(&1).unwrap().viewing.is_empty());
    }

    fn media_file_json(id: i32, path: &str, completed: bool) -> String {
        format!(
            r#"{{"@type":"file","id":{id},"size":24,"expected_size":24,"local":{{"@type":"localFile","path":{path},"can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":{completed},"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"CANARY_REMOTE","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}}}"#,
            path = serde_json::to_string(path).unwrap(),
            completed = completed,
        )
    }

    #[test]
    fn photo_and_document_are_stored_and_update_file_completes_path() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_chat(ChatId(1));
        let thumb = media_file_json(1, "", false);
        let full = media_file_json(2, "", false);
        let doc = media_file_json(9, "", false);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":10,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{thumb},"width":320,"height":240,"progressive_sizes":[]}},{{"@type":"photoSize","type":"x","photo":{full},"width":800,"height":600,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"CANARY_PHOTO_body","entities":[]}},"has_spoiler":false,"is_secret":false}}}}}}"#
            ),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":11,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messageDocument","document":{{"@type":"document","file_name":"notes.txt","mime_type":"text/plain","document":{doc}}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}}}"#
            ),
        );
        assert_eq!(
            session
                .histories
                .get(&1)
                .unwrap()
                .messages
                .get(&10)
                .unwrap()
                .content
                .preview(),
            "CANARY_PHOTO_body"
        );
        assert_eq!(
            session
                .histories
                .get(&1)
                .unwrap()
                .messages
                .get(&11)
                .unwrap()
                .content
                .preview(),
            "notes.txt"
        );
        assert!(session.file(FileId(1)).unwrap().needs_download());
        assert_eq!(session.thumb_file_ids_to_download(), vec![FileId(1)]);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateFile","file":{}}}"#,
                media_file_json(1, "/tmp/quill-thumb.jpg", true)
            ),
        );
        assert_eq!(
            session.file(FileId(1)).unwrap().usable_path(),
            Some("/tmp/quill-thumb.jpg")
        );
        assert!(session.thumb_file_ids_to_download().is_empty());
        assert!(!sink.rendered().contains("CANARY_PHOTO"));
        assert!(!sink.rendered().contains("CANARY_REMOTE"));
        assert!(!sink.rendered().contains("/tmp/quill-thumb"));
    }

    #[test]
    fn secret_photo_is_not_auto_thumbed() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.open_chat(ChatId(1));
        let file = media_file_json(3, "", false);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":12,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":100,"height":80,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":false,"is_secret":true}}}}}}"#
            ),
        );
        assert!(session.thumb_file_ids_to_download().is_empty());
        assert!(session.should_download(FileId(3)));
    }

    #[test]
    fn download_error_unsticks_in_flight_file() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request_download(FileId(4));
        session.begin_download(FileId(4));
        assert!(!session.should_download(FileId(4)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","code":400,"message":"CANARY_FILE_ERR","@extra":"{}"}}"#,
                extra.0
            ),
        );
        assert!(session.should_download(FileId(4)));
        assert!(!session.downloading.contains(&4));
        assert!(!sink.rendered().contains("CANARY_FILE_ERR"));
    }

    fn file_reply_json(extra: u64, id: i32, active: bool) -> String {
        format!(
            r#"{{"@type":"file","@extra":"{extra}","id":{id},"size":24,"expected_size":24,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":{active},"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"CANARY_REMOTE","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}}}"#
        )
    }

    #[test]
    fn download_unsticks_after_file_extra_then_idle_update_or_error() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);

        let extra = session.request_download(FileId(4));
        session.begin_download(FileId(4));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &file_reply_json(extra.0, 4, true),
        );
        assert!(session.requests.take(extra).is_none());
        assert!(!session.should_download(FileId(4)));
        assert!(session.downloading.contains(&4));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateFile","file":{}}}"#,
                media_file_json(4, "", false)
            ),
        );
        assert!(session.should_download(FileId(4)));
        assert!(!session.downloading.contains(&4));

        let extra = session.request_download(FileId(5));
        session.begin_download(FileId(5));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &file_reply_json(extra.0, 5, true),
        );
        assert!(!session.should_download(FileId(5)));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","code":400,"message":"CANARY_FILE_ERR2","@extra":"{}"}}"#,
                extra.0
            ),
        );
        assert!(session.should_download(FileId(5)));
        assert!(!session.downloading.contains(&5));
        assert!(!sink.rendered().contains("CANARY_FILE_ERR2"));
    }

    #[test]
    fn nested_idle_message_file_does_not_unstick_in_flight_download() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request_download(FileId(6));
        session.begin_download(FileId(6));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &file_reply_json(extra.0, 6, true),
        );
        let file = media_file_json(6, "", false);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":13,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":100,"height":80,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":false,"is_secret":false}}}}}}"#
            ),
        );
        assert!(session.downloading.contains(&6));
        assert!(!session.should_download(FileId(6)));
    }

    #[test]
    fn search_chats_and_messages_happy_path() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatPosition","chat_id":11,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"4","is_pinned":false}}"#,
        );
        session.open_search();
        let search_gen = session.search.begin_query("hello");
        let chats_extra = session.request_search(RequestPurpose::SearchChats, search_gen);
        let messages_extra = session.request_search(RequestPurpose::SearchMessages, search_gen);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[11]}}"#,
                chats_extra.0
            ),
        );
        assert_eq!(session.search.status, SearchStatus::Searching);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"foundMessages","@extra":"{}","total_count":1,"next_offset":"","messages":[{{"id":101,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"CANARY_SEARCH_hi","entities":[]}}}}}}]}}"#,
                messages_extra.0
            ),
        );
        assert_eq!(session.search.status, SearchStatus::Ready);
        assert_eq!(session.search.chat_ids, vec![ChatId(11)]);
        assert_eq!(session.search.messages.len(), 1);
        assert_eq!(session.search.messages[0].preview, "CANARY_SEARCH_hi");
        session.promote_search_message(ChatId(11), MessageId(101));
        assert!(
            session
                .histories
                .get(&11)
                .unwrap()
                .messages
                .contains_key(&101)
        );
        session.close_search();
        assert_eq!(session.search.status, SearchStatus::Closed);
        assert!(session.search.query.is_empty());
        assert!(!sink.rendered().contains("CANARY_SEARCH"));
    }

    #[test]
    fn search_empty_and_error_and_stale_generation() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let search_gen = session.search.begin_query("zzz");
        let chats_extra = session.request_search(RequestPurpose::SearchChats, search_gen);
        let messages_extra = session.request_search(RequestPurpose::SearchMessages, search_gen);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
                chats_extra.0
            ),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"foundMessages","@extra":"{}","total_count":0,"next_offset":"","messages":[]}}"#,
                messages_extra.0
            ),
        );
        assert_eq!(session.search.status, SearchStatus::Empty);

        let stale = session.search.begin_query("old");
        let stale_chats = session.request_search(RequestPurpose::SearchChats, stale);
        let stale_messages = session.request_search(RequestPurpose::SearchMessages, stale);
        let fresh = session.search.begin_query("new");
        let _fresh_chats = session.request_search(RequestPurpose::SearchChats, fresh);
        let fresh_messages = session.request_search(RequestPurpose::SearchMessages, fresh);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[99]}}"#,
                stale_chats.0
            ),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"foundMessages","@extra":"{}","total_count":1,"next_offset":"","messages":[{{"id":1,"chat_id":99,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"stale","entities":[]}}}}}}]}}"#,
                stale_messages.0
            ),
        );
        assert_eq!(session.search.status, SearchStatus::Searching);
        assert!(session.search.chat_ids.is_empty());
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","code":400,"message":"CANARY_SEARCH_ERR","@extra":"{}"}}"#,
                fresh_messages.0
            ),
        );
        let search_gen = session.search.generation;
        let chats_extra = session.request_search(RequestPurpose::SearchChats, search_gen);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","code":400,"message":"CANARY_SEARCH_ERR2","@extra":"{}"}}"#,
                chats_extra.0
            ),
        );
        assert_eq!(session.search.status, SearchStatus::Failed);
        assert!(!sink.rendered().contains("CANARY_SEARCH_ERR"));
    }

    #[test]
    fn search_recently_found_chats_empty_query() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatPosition","chat_id":11,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"4","is_pinned":false}}"#,
        );
        let search_gen = session.search.begin_recents();
        assert!(session.search.recents);
        let extra = session.request_search(RequestPurpose::SearchRecentlyFoundChats, search_gen);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[11]}}"#,
                extra.0
            ),
        );
        assert_eq!(session.search.status, SearchStatus::Ready);
        assert_eq!(session.search.chat_ids, vec![ChatId(11)]);
        assert!(session.search.messages.is_empty());
        let empty_gen = session.search.begin_recents();
        let empty_extra =
            session.request_search(RequestPurpose::SearchRecentlyFoundChats, empty_gen);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
                empty_extra.0
            ),
        );
        assert_eq!(session.search.status, SearchStatus::Idle);
        assert!(session.search.recents);
    }

    #[test]
    fn chat_search_happy_empty_stale_and_jump() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello already loaded.","entities":[]}}}}"#,
        );
        session.open_chat(ChatId(11));
        assert!(session.open_chat_search());
        let search_gen = session.chat_search.begin_query("hello");
        let extra =
            session.request_chat_search(RequestPurpose::SearchChatMessages, ChatId(11), search_gen);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":2,"next_from_message_id":40,"messages":[{{"id":101,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"CANARY_CHAT_hi","entities":[]}}}}}},{{"id":90,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"older hello","entities":[]}}}}}}]}}"#,
                extra.0
            ),
        );
        assert_eq!(session.chat_search.status, SearchStatus::Ready);
        assert_eq!(session.chat_search.hits.len(), 2);
        assert_eq!(session.chat_search.selected, Some(0));
        assert_eq!(session.chat_search.hits[0].preview, "CANARY_CHAT_hi");
        assert_eq!(
            session.begin_chat_search_jump(MessageId(101)),
            ChatSearchJumpNeed::AlreadyReady
        );
        assert_eq!(
            session.chat_search.jump,
            ChatSearchJump::Ready {
                message_id: MessageId(101)
            }
        );
        assert_eq!(
            session.begin_chat_search_jump(MessageId(90)),
            ChatSearchJumpNeed::LoadAround
        );
        let around = session.request_history_around(ChatId(11), MessageId(90));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"messages","@extra":"{}","total_count":2,"messages":[{{"id":90,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"older hello","entities":[]}}}}}},{{"id":89,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"neighbor","entities":[]}}}}}}]}}"#,
                around.0
            ),
        );
        assert_eq!(
            session.chat_search.jump,
            ChatSearchJump::Ready {
                message_id: MessageId(90)
            }
        );
        assert!(session.histories.get(&11).unwrap().contains(MessageId(90)));
        assert!(session.histories.get(&11).unwrap().contains(MessageId(89)));
        assert!(session.histories.get(&11).unwrap().contains(MessageId(101)));

        let empty_gen = session.chat_search.begin_query("zzz");
        let empty_extra =
            session.request_chat_search(RequestPurpose::SearchChatMessages, ChatId(11), empty_gen);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":0,"next_from_message_id":0,"messages":[]}}"#,
                empty_extra.0
            ),
        );
        assert_eq!(session.chat_search.status, SearchStatus::Empty);

        let stale = session.chat_search.begin_query("old");
        let stale_extra =
            session.request_chat_search(RequestPurpose::SearchChatMessages, ChatId(11), stale);
        let fresh = session.chat_search.begin_query("new");
        let fresh_extra =
            session.request_chat_search(RequestPurpose::SearchChatMessages, ChatId(11), fresh);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":1,"next_from_message_id":0,"messages":[{{"id":1,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"stale","entities":[]}}}}}}]}}"#,
                stale_extra.0
            ),
        );
        assert_eq!(session.chat_search.status, SearchStatus::Searching);
        assert!(session.chat_search.hits.is_empty());
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","code":400,"message":"CANARY_CHAT_ERR","@extra":"{}"}}"#,
                fresh_extra.0
            ),
        );
        assert_eq!(session.chat_search.status, SearchStatus::Failed);
        session.close_chat_search();
        assert_eq!(session.chat_search.status, SearchStatus::Closed);
        assert!(session.histories.get(&11).unwrap().contains(MessageId(101)));
        assert!(!sink.rendered().contains("CANARY_CHAT"));
    }

    #[test]
    fn chat_search_jump_missing_deleted_and_inaccessible() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hello","entities":[]}}}}"#,
        );
        session.open_chat(ChatId(11));
        assert!(session.open_chat_search());
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateDeleteMessages","chat_id":11,"message_ids":[70],"is_permanent":true,"from_cache":false}"#,
        );
        session.chat_search.hits.push(SearchMessageHit {
            chat_id: ChatId(11),
            message_id: MessageId(70),
            preview: "gone".into(),
            is_outgoing: false,
            content: crate::telegram::envelope::MessageContent::Text("gone".into()),
            reply_to: None,
            forward_info: None,
            interaction_info: None,
            is_pinned: false,
        });
        assert_eq!(
            session.begin_chat_search_jump(MessageId(70)),
            ChatSearchJumpNeed::Missing
        );
        assert_eq!(
            session.chat_search.jump,
            ChatSearchJump::Missing {
                message_id: MessageId(70)
            }
        );

        session.chat_search.hits.push(SearchMessageHit {
            chat_id: ChatId(11),
            message_id: MessageId(80),
            preview: "ghost".into(),
            is_outgoing: false,
            content: crate::telegram::envelope::MessageContent::Text("ghost".into()),
            reply_to: None,
            forward_info: None,
            interaction_info: None,
            is_pinned: false,
        });
        assert_eq!(
            session.begin_chat_search_jump(MessageId(80)),
            ChatSearchJumpNeed::LoadAround
        );
        let around = session.request_history_around(ChatId(11), MessageId(80));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"messages","@extra":"{}","total_count":1,"messages":[{{"id":79,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"neighbor only","entities":[]}}}}}}]}}"#,
                around.0
            ),
        );
        assert_eq!(
            session.chat_search.jump,
            ChatSearchJump::Missing {
                message_id: MessageId(80)
            }
        );
        assert!(!session.histories.get(&11).unwrap().contains(MessageId(80)));

        let stale_around = session.request_history_around(ChatId(11), MessageId(80));
        session.chat_search.generation = session.chat_search.generation.saturating_add(1);
        assert_eq!(
            session.begin_chat_search_jump(MessageId(101)),
            ChatSearchJumpNeed::AlreadyReady
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"messages","@extra":"{}","total_count":1,"messages":[{{"id":80,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"late","entities":[]}}}}}}]}}"#,
                stale_around.0
            ),
        );
        assert_eq!(
            session.chat_search.jump,
            ChatSearchJump::Ready {
                message_id: MessageId(101)
            }
        );
        assert!(!sink.rendered().contains("CANARY"));
    }

    #[test]
    fn reply_to_message_preview_and_jump() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}}}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":104,"chat_id":11,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"sounds good","entities":[]}},"reply_to":{"@type":"messageReplyToMessage","chat_id":11,"message_id":101,"quote":null,"checklist_task_id":0,"poll_option_id":""}}}"#,
        );
        session.open_chat(ChatId(11));
        let reply = session
            .histories
            .get(&11)
            .unwrap()
            .messages
            .get(&104)
            .unwrap();
        assert_eq!(
            reply.reply_to.as_ref().map(|r| r.message_id),
            Some(MessageId(101))
        );
        assert_eq!(
            session.reply_quote_preview(reply).as_deref(),
            Some("Hello from injected JSON.")
        );
        assert_eq!(
            session.begin_chat_search_jump(reply.reply_to.as_ref().unwrap().message_id),
            ChatSearchJumpNeed::AlreadyReady
        );
        assert_eq!(
            session.chat_search.jump,
            ChatSearchJump::Ready {
                message_id: MessageId(101)
            }
        );

        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":105,"chat_id":11,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"quoted","entities":[]}},"reply_to":{"@type":"messageReplyToMessage","chat_id":11,"message_id":90,"quote":{"@type":"textQuote","text":{"@type":"formattedText","text":"manual quote","entities":[]},"position":0,"is_manual":true},"checklist_task_id":0,"poll_option_id":""}}}"#,
        );
        let quoted = session
            .histories
            .get(&11)
            .unwrap()
            .messages
            .get(&105)
            .unwrap();
        assert_eq!(
            session.reply_quote_preview(quoted).as_deref(),
            Some("manual quote")
        );
        assert_eq!(
            session.begin_chat_search_jump(MessageId(90)),
            ChatSearchJumpNeed::LoadAround
        );
        let around = session.request_history_around(ChatId(11), MessageId(90));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"messages","@extra":"{}","total_count":1,"messages":[{{"id":90,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"older original","entities":[]}}}}}}]}}"#,
                around.0
            ),
        );
        assert_eq!(
            session.chat_search.jump,
            ChatSearchJump::Ready {
                message_id: MessageId(90)
            }
        );
        assert!(session.histories.get(&11).unwrap().contains(MessageId(90)));
    }

    #[test]
    fn update_message_content_rewrites_own_text() {
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let mut session = Session::new(AccountKey::primary(), dyn_sink);
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Demo","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":102,"chat_id":11,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Reply from the session reducer.","entities":[]}}}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateMessageContent","chat_id":11,"message_id":102,"new_content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_EDITED_own","entities":[]}}}"#,
        );
        let text = session
            .histories
            .get(&11)
            .unwrap()
            .messages
            .get(&102)
            .unwrap()
            .content
            .preview();
        assert_eq!(text, "CANARY_EDITED_own");
        assert!(
            !session
                .histories
                .get(&11)
                .unwrap()
                .is_tombstone(MessageId(102))
        );
        assert!(!sink.rendered().contains("CANARY_EDITED"));
    }

    #[test]
    fn forward_messages_result_upserts_dest_and_labels_origin() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatPosition","chat_id":11,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"2","is_pinned":false}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":12,"title":"Bob","type":{"@type":"chatTypePrivate","user_id":12},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateChatPosition","chat_id":12,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"1","is_pinned":false}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}}}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":105,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"already forwarded","entities":[]}},"forward_info":{"@type":"messageForwardInfo","origin":{"@type":"messageOriginHiddenUser","sender_name":"Ada Lovelace"},"date":1}}}"#,
        );
        let dests: Vec<_> = session
            .forward_destinations("bo")
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(dests, vec![ChatId(12)]);
        assert!(
            session
                .forward_destinations("")
                .iter()
                .all(|c| c.supported())
        );
        let extra = session.request(RequestPurpose::ForwardMessages, Some(ChatId(12)));
        session.in_flight_forward = Some(ForwardFlight {
            extra,
            dest_chat_id: ChatId(12),
            from_chat_id: ChatId(11),
            requested: 1,
        });
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"messages","@extra":"{}","total_count":1,"messages":[{{"id":80,"chat_id":12,"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}}}},"forward_info":{{"@type":"messageForwardInfo","origin":{{"@type":"messageOriginUser","sender_user_id":11}},"date":1}}}}]}}"#,
                extra.0
            ),
        );
        let result = session.last_forward.as_ref().expect("forward result");
        assert_eq!(result.dest_chat_id, ChatId(12));
        assert_eq!(result.dest_title, "Bob");
        assert_eq!(result.forwarded_ids, vec![MessageId(80)]);
        assert_eq!(result.success_label(), "Forwarded to Bob");
        let dest = session
            .histories
            .get(&12)
            .unwrap()
            .messages
            .get(&80)
            .unwrap();
        assert_eq!(
            session.forward_from_label(dest.forward_info.as_ref().unwrap()),
            "Forwarded from Alice"
        );
        let incoming = session
            .histories
            .get(&11)
            .unwrap()
            .messages
            .get(&105)
            .unwrap();
        assert_eq!(
            session.forward_from_label(incoming.forward_info.as_ref().unwrap()),
            "Forwarded from Ada Lovelace"
        );
        assert!(!sink.rendered().contains("CANARY"));
    }

    #[test]
    fn interaction_info_update_sets_chips_and_own_highlight() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Demo","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}},"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"👍"},"total_count":2,"is_chosen":false,"used_sender_id":null,"recent_sender_ids":[]}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":false}}}}"#,
        );
        let incoming = session
            .histories
            .get(&11)
            .unwrap()
            .messages
            .get(&101)
            .unwrap();
        assert!(incoming.can_react());
        assert_eq!(incoming.emoji_reaction_chips().len(), 1);
        assert!(!incoming.chosen_emoji("❤"));
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateMessageInteractionInfo","chat_id":11,"message_id":101,"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"❤"},"total_count":3,"is_chosen":true,"used_sender_id":null,"recent_sender_ids":[]},{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"👍"},"total_count":2,"is_chosen":false,"used_sender_id":null,"recent_sender_ids":[]}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":false}}}"#,
        );
        let updated = session
            .histories
            .get(&11)
            .unwrap()
            .messages
            .get(&101)
            .unwrap();
        let chips = updated.emoji_reaction_chips();
        assert_eq!(chips[0].chip_label().as_deref(), Some("❤ 3"));
        assert!(chips[0].is_chosen);
        assert_eq!(chips[1].chip_label().as_deref(), Some("👍 2"));
        assert!(!chips[1].is_chosen);
        assert!(updated.chosen_emoji("❤"));
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateMessageInteractionInfo","chat_id":11,"message_id":101,"interaction_info":null}"#,
        );
        let cleared = session
            .histories
            .get(&11)
            .unwrap()
            .messages
            .get(&101)
            .unwrap();
        assert!(cleared.emoji_reaction_chips().is_empty());
        assert!(!sink.rendered().contains("CANARY"));
    }

    #[test]
    fn message_is_pinned_update_and_newest_pinned() {
        let sink = Arc::new(MemorySink::new());
        let mut session = Session::new(AccountKey::primary(), sink.clone());
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Demo","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":100,"chat_id":11,"is_outgoing":false,"is_pinned":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"older","entities":[]}}}}"#,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"is_pinned":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}}}}"#,
        );
        session.open_chat = Some(ChatId(11));
        let pinned = session
            .histories
            .get(&11)
            .unwrap()
            .messages
            .get(&101)
            .unwrap();
        assert!(pinned.is_pinned);
        assert!(pinned.can_pin());
        assert_eq!(
            session.open_chat_pinned_message().map(|m| m.id),
            Some(MessageId(101))
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateMessageIsPinned","chat_id":11,"message_id":101,"is_pinned":false}"#,
        );
        assert!(
            !session
                .histories
                .get(&11)
                .unwrap()
                .messages
                .get(&101)
                .unwrap()
                .is_pinned
        );
        assert!(session.open_chat_pinned_message().is_none());
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateMessageIsPinned","chat_id":11,"message_id":100,"is_pinned":true}"#,
        );
        assert_eq!(
            session.open_chat_pinned_message().map(|m| m.id),
            Some(MessageId(100))
        );
        assert!(!sink.rendered().contains("CANARY"));
    }
}
