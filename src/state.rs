use crate::auth::{AuthView, view_for};
use crate::diagnostics::{Diagnostic, DiagnosticSink};
use crate::ids::{
    AccountGeneration, AccountKey, ChatId, FileId, MessageId, RequestId, ViewGeneration,
};
use crate::telegram::client::OwnedEnvelope;
use crate::telegram::envelope::{
    AuthorizationState, ChatKind, ChatList, ChatPositionUpdate, ConnectionState, EnvelopePayload,
    ErrorClass, MessageContent, ParsedFile, ParsedMessage,
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

#[derive(Debug, Clone)]
pub struct PendingRequest {
    pub id: RequestId,
    pub account_generation: AccountGeneration,
    pub purpose: RequestPurpose,
    pub chat_id: Option<ChatId>,
    pub view_generation: Option<ViewGeneration>,
    pub file_id: Option<i32>,
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
    /// Sidebar preview from `updateChatLastMessage`. Not logged.
    pub last_preview: String,
}

impl ChatSummary {
    pub fn supported(&self) -> bool {
        self.kind.is_supported_cloud_chat()
    }

    pub fn sidebar_preview(&self) -> String {
        if let Some(reason) = self.kind.gate_reason() {
            return reason.to_string();
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
        last_preview: String::new(),
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownPhase {
    Running,
    CloseRequested,
    WaitingClosed,
    Closed,
}

pub struct Session {
    pub account: AccountKey,
    pub account_generation: AccountGeneration,
    pub auth: AuthorizationState,
    pub auth_view: AuthView,
    pub connection: ConnectionState,
    pub chats: HashMap<i64, ChatSummary>,
    pub main_order: Vec<ChatId>,
    pub histories: HashMap<i64, HistoryState>,
    pub open_chat: Option<ChatId>,
    pub view_generation: ViewGeneration,
    pub requests: RequestRegistry,
    pub chats_exhausted: bool,
    pub shutdown: ShutdownPhase,
    pub last_seq: u64,
    /// Last classified error for phone / code / password submit. Never a secret.
    pub last_auth_error: Option<AuthRequestError>,
    /// TDLib `file.id` → latest `file` / `localFile` snapshot.
    pub files: HashMap<i32, ParsedFile>,
    /// `downloadFile` in flight (until completed, undownloadable, idle, or error).
    pub downloading: HashSet<i32>,
    /// `@extra` → `file.id` until the download unsticks (survives `file@extra` consuming pending).
    download_extras: HashMap<u64, i32>,
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
            histories: HashMap::new(),
            open_chat: None,
            view_generation: ViewGeneration(1),
            requests: RequestRegistry::default(),
            chats_exhausted: false,
            shutdown: ShutdownPhase::Running,
            last_seq: 0,
            last_auth_error: None,
            files: HashMap::new(),
            downloading: HashSet::new(),
            download_extras: HashMap::new(),
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
                if list == ChatList::Main {
                    self.chats
                        .entry(chat_id.0)
                        .or_insert_with(|| placeholder_chat(chat_id))
                        .in_main_list = true;
                    self.rebuild_main_order();
                }
            }
            EnvelopePayload::UpdateChatRemovedFromList { chat_id, list } => {
                if list == ChatList::Main
                    && let Some(chat) = self.chats.get_mut(&chat_id.0)
                {
                    chat.in_main_list = false;
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
                }
            }
            EnvelopePayload::Messages(messages) => {
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
        }
        if matches!(state, AuthorizationState::LoggingOut) {
            self.requests.invalidate_account();
        }
        if matches!(state, AuthorizationState::Closing) {
            self.shutdown = ShutdownPhase::WaitingClosed;
        }
        self.auth = state;
        self.auth_view = view_for(&self.auth);
        self.last_auth_error = None;
    }

    fn apply_position_fields(&mut self, pos: ChatPositionUpdate) {
        // A position on Archive / a folder is not a Main-list eviction.
        // Main membership changes only via a Main `updateChatPosition`, a full
        // `updateChatLastMessage` positions set, or add/remove-from-list.
        if pos.list != ChatList::Main {
            return;
        }
        let chat = self
            .chats
            .entry(pos.chat_id.0)
            .or_insert_with(|| placeholder_chat(pos.chat_id));
        if pos.order == 0 {
            chat.in_main_list = false;
        } else {
            chat.order = pos.order;
            chat.is_pinned = pos.is_pinned;
            chat.in_main_list = true;
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
            let MessageContent::Photo(photo) = &message.content else {
                continue;
            };
            if photo.is_secret || photo.has_spoiler {
                continue;
            }
            if let Some(size) = photo.thumb_size()
                && self.should_download(size.file_id)
            {
                ids.push(size.file_id);
            }
        }
        ids.sort_by_key(|id| id.0);
        ids.dedup();
        ids
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
    }

    pub fn open_chat(&mut self, chat_id: ChatId) {
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
        assert_eq!(chat.order, 6);
        assert_eq!(session.ordered_chats().len(), 1);
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
        assert!(!session.chats.get(&6).unwrap().in_main_list);
        assert!(session.ordered_chats().is_empty());
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
}
