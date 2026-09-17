use crate::auth::{AuthView, view_for};
use crate::diagnostics::{Diagnostic, DiagnosticSink};
use crate::ids::{AccountGeneration, AccountKey, ChatId, MessageId, RequestId, ViewGeneration};
use crate::telegram::client::OwnedEnvelope;
use crate::telegram::envelope::{
    AuthorizationState, ChatKind, ChatList, ChatPositionUpdate, ConnectionState, EnvelopePayload,
    ErrorClass, MessageContent, ParsedMessage,
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
    SendText,
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

    pub fn has_purpose(&self, purpose: RequestPurpose) -> bool {
        self.pending.values().any(|p| p.purpose == purpose)
    }

    pub fn has_purpose_for_chat(&self, purpose: RequestPurpose, chat_id: ChatId) -> bool {
        self.pending
            .values()
            .any(|p| p.purpose == purpose && p.chat_id == Some(chat_id))
    }
}

#[derive(Debug, Clone)]
pub struct ChatSummary {
    pub id: ChatId,
    pub title: String,
    pub kind: ChatKind,
    pub unread_count: i32,
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
}

fn placeholder_chat(chat_id: ChatId) -> ChatSummary {
    ChatSummary {
        id: chat_id,
        title: format!("chat {}", chat_id.0),
        kind: ChatKind::Unknown,
        unread_count: 0,
        order: 0,
        is_pinned: false,
        in_main_list: false,
        last_preview: String::new(),
    }
}

fn preview_from_content(content: &MessageContent) -> String {
    match content {
        MessageContent::Text(text) => text.chars().take(80).collect(),
        MessageContent::Unsupported { type_name } => format!("({type_name})"),
    }
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
        let pending = owned.envelope.extra.and_then(|id| self.requests.take(id));
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
        self.apply_payload(owned.envelope.payload, pending.as_ref(), owned.seq);
    }

    fn apply_payload(
        &mut self,
        payload: EnvelopePayload,
        pending: Option<&PendingRequest>,
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
            } => {
                let chat = self
                    .chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id));
                chat.title = title;
                chat.kind = kind;
                chat.unread_count = unread_count;
            }
            EnvelopePayload::UpdateChatTitle { chat_id, title } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .title = title;
            }
            EnvelopePayload::UpdateChatReadInbox {
                chat_id,
                unread_count,
            } => {
                self.chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id))
                    .unread_count = unread_count;
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
                let chat = self
                    .chats
                    .entry(chat_id.0)
                    .or_insert_with(|| placeholder_chat(chat_id));
                chat.last_preview = last_message
                    .as_ref()
                    .map(|message| preview_from_content(&message.content))
                    .unwrap_or_default();
                let saw_main = positions.iter().any(|pos| pos.list == ChatList::Main);
                for pos in positions {
                    self.apply_position_fields(pos);
                }
                if !saw_main && let Some(chat) = self.chats.get_mut(&chat_id.0) {
                    chat.in_main_list = false;
                }
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
                let history = self.histories.entry(message.chat_id.0).or_default();
                history.replace_id(old_message_id, history_message(message, false));
            }
            EnvelopePayload::UpdateMessageSendFailed {
                message,
                old_message_id,
                ..
            } => {
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
                        let history = self.histories.entry(chat_id.0).or_default();
                        if messages.is_empty() {
                            history.loaded_complete = true;
                        }
                        for message in messages {
                            history.upsert(history_message(message, false));
                        }
                    }
                }
            }
            EnvelopePayload::Message(message) => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::SendText) {
                    self.upsert_message(message, true);
                } else {
                    self.upsert_message(message, false);
                }
            }
            EnvelopePayload::Ok => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::LoadChats) {
                    // A short OK is not exhaustion; 404 is.
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
        if pos.list != ChatList::Main {
            if let Some(chat) = self.chats.get_mut(&pos.chat_id.0) {
                chat.in_main_list = false;
            }
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

    fn upsert_message(&mut self, message: ParsedMessage, pending: bool) {
        let history = self.histories.entry(message.chat_id.0).or_default();
        history.upsert(history_message(message, pending));
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
        self.histories.entry(chat_id.0).or_default().view_generation = self.view_generation;
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
}
