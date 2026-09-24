use crate::ids::{ChatId, FileId, MessageId, RequestId, UserId};
use serde::Deserialize;
use serde_json::Value;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    pub type_name: String,
    pub extra: Option<RequestId>,
    pub client_id: Option<i32>,
    pub payload: EnvelopePayload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvelopePayload {
    UpdateAuthorizationState(AuthorizationState),
    UpdateNewMessage(ParsedMessage),
    UpdateMessageSendSucceeded {
        message: ParsedMessage,
        old_message_id: MessageId,
    },
    UpdateMessageSendFailed {
        message: ParsedMessage,
        old_message_id: MessageId,
        error: TdError,
    },
    UpdateMessageSendAcknowledged {
        chat_id: ChatId,
        message_id: MessageId,
    },
    UpdateDeleteMessages {
        chat_id: ChatId,
        message_ids: Vec<MessageId>,
        is_permanent: bool,
        from_cache: bool,
    },
    UpdateMessageContent {
        chat_id: ChatId,
        message_id: MessageId,
        content: MessageContent,
        files: Vec<ParsedFile>,
    },
    UpdateChatPosition(ChatPositionUpdate),
    UpdateChatTitle {
        chat_id: ChatId,
        title: String,
    },
    UpdateChatLastMessage {
        chat_id: ChatId,
        last_message: Option<ParsedMessage>,
        positions: Vec<ChatPositionUpdate>,
    },
    UpdateChatAddedToList {
        chat_id: ChatId,
        list: ChatList,
    },
    UpdateChatRemovedFromList {
        chat_id: ChatId,
        list: ChatList,
    },
    UpdateChatReadInbox {
        chat_id: ChatId,
        last_read_inbox_message_id: MessageId,
        unread_count: i32,
    },
    UpdateChatReadOutbox {
        chat_id: ChatId,
        last_read_outbox_message_id: MessageId,
    },
    UpdateConnectionState(ConnectionState),
    UpdateNewChat {
        chat_id: ChatId,
        title: String,
        kind: ChatKind,
        unread_count: i32,
        last_read_inbox_message_id: MessageId,
        last_read_outbox_message_id: MessageId,
    },
    Ok,
    Error(TdError),
    Messages(Vec<ParsedMessage>),
    Message(ParsedMessage),
    /// `chats` — `searchChats` / `searchRecentlyFoundChats` / similar.
    Chats {
        total_count: i32,
        chat_ids: Vec<ChatId>,
    },
    /// `foundMessages` — `searchMessages` (and secret-chat search).
    FoundMessages {
        total_count: i32,
        messages: Vec<ParsedMessage>,
        next_offset: String,
    },
    /// `foundChatMessages` — `searchChatMessages`.
    FoundChatMessages {
        total_count: i32,
        messages: Vec<ParsedMessage>,
        next_from_message_id: MessageId,
    },
    UpdateFile(ParsedFile),
    File(ParsedFile),
    Unknown(UnknownKind),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownKind {
    pub type_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TdError {
    pub code: i32,
    /// Error *class* only. The TDLib `message` field is not stored; it can
    /// contain phone numbers or other secrets.
    pub class: ErrorClass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    NotFound,
    Unauthorized,
    Flood,
    Invalid,
    Other,
}

impl TdError {
    pub fn from_code(code: i32) -> Self {
        let class = match code {
            404 => ErrorClass::NotFound,
            401 => ErrorClass::Unauthorized,
            429 => ErrorClass::Flood,
            400 => ErrorClass::Invalid,
            _ => ErrorClass::Other,
        };
        Self { code, class }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizationState {
    WaitTdlibParameters,
    WaitPhoneNumber,
    WaitPremiumPurchase,
    WaitEmailAddress,
    WaitEmailCode,
    WaitCode { code_length: Option<i32> },
    WaitOtherDeviceConfirmation,
    WaitRegistration,
    WaitPassword { has_recovery_email: bool },
    Ready,
    LoggingOut,
    Closing,
    Closed,
    Unknown(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    WaitingForNetwork,
    ConnectingToProxy,
    Connecting,
    Updating,
    Ready,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatKind {
    Private {
        user_id: UserId,
    },
    BasicGroup {
        basic_group_id: i64,
    },
    Supergroup {
        supergroup_id: i64,
        is_channel: bool,
    },
    Secret {
        secret_chat_id: i32,
        user_id: UserId,
    },
    Unknown,
}

impl ChatKind {
    pub fn is_supported_cloud_chat(&self) -> bool {
        match self {
            ChatKind::Private { .. } | ChatKind::BasicGroup { .. } => true,
            ChatKind::Supergroup { is_channel, .. } => !is_channel,
            ChatKind::Secret { .. } | ChatKind::Unknown => false,
        }
    }

    pub fn gate_reason(&self) -> Option<&'static str> {
        match self {
            ChatKind::Supergroup {
                is_channel: true, ..
            } => Some("Channels are unavailable until sponsored-content handling exists."),
            ChatKind::Secret { .. } => Some("Secret chats are out of scope for this client."),
            ChatKind::Unknown => Some("This conversation type is not supported yet."),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatPositionUpdate {
    pub chat_id: ChatId,
    pub list: ChatList,
    pub order: i64,
    pub is_pinned: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatList {
    Main,
    Archive,
    Folder(i32),
    Unknown,
}

/// `message.forward_info.origin` (TDLib 1.8.67 `MessageOrigin`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageOrigin {
    User {
        user_id: UserId,
    },
    HiddenUser {
        sender_name: String,
    },
    Chat {
        chat_id: ChatId,
        author_signature: String,
    },
    Channel {
        chat_id: ChatId,
        message_id: MessageId,
        author_signature: String,
    },
}

/// Typed `messageForwardInfo`. `source` (Saved Messages / Replies) stays out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageForwardInfo {
    pub origin: MessageOrigin,
    pub date: i32,
}

/// Same-chat / known-chat reply metadata from `message.reply_to`.
/// Stories and unknown `MessageReplyTo` variants are dropped (out of scope).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageReplyTo {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub quote_text: Option<String>,
    pub content_preview: Option<String>,
}

impl MessageReplyTo {
    pub fn is_same_chat(&self, open_chat: ChatId) -> bool {
        self.chat_id.0 == 0 || self.chat_id == open_chat
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMessage {
    pub id: MessageId,
    pub chat_id: ChatId,
    pub is_outgoing: bool,
    pub content: MessageContent,
    pub files: Vec<ParsedFile>,
    pub reply_to: Option<MessageReplyTo>,
    pub forward_info: Option<MessageForwardInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageContent {
    Text(String),
    Photo(PhotoContent),
    Document(DocumentContent),
    Unsupported { type_name: String },
}

impl MessageContent {
    pub fn preview(&self) -> String {
        match self {
            MessageContent::Text(text) => text.chars().take(80).collect(),
            MessageContent::Photo(photo) if photo.caption.is_empty() => "Photo".into(),
            MessageContent::Photo(photo) => photo.caption.chars().take(80).collect(),
            MessageContent::Document(doc) if !doc.caption.is_empty() => {
                doc.caption.chars().take(80).collect()
            }
            MessageContent::Document(doc) if !doc.file_name.is_empty() => {
                doc.file_name.chars().take(80).collect()
            }
            MessageContent::Document(_) => "Document".into(),
            MessageContent::Unsupported { type_name } => format!("({type_name})"),
        }
    }
}

/// `photo` + caption flags from `messagePhoto` (TDLib 1.8.67).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoContent {
    pub caption: String,
    pub sizes: Vec<PhotoSizeView>,
    pub is_secret: bool,
    pub has_spoiler: bool,
}

impl PhotoContent {
    /// Prefer `photoSize.type == "m"` (box 320), else the largest size ≤ 320px wide.
    pub fn thumb_size(&self) -> Option<&PhotoSizeView> {
        self.sizes
            .iter()
            .find(|size| size.type_name == "m")
            .or_else(|| {
                self.sizes
                    .iter()
                    .filter(|size| size.width > 0 && size.width <= 320)
                    .max_by_key(|size| size.width)
            })
            .or_else(|| {
                self.sizes
                    .iter()
                    .min_by_key(|size| (size.width, size.height))
            })
    }

    pub fn largest_size(&self) -> Option<&PhotoSizeView> {
        self.sizes
            .iter()
            .max_by_key(|size| i64::from(size.width) * i64::from(size.height))
    }

    pub fn open_file_id(&self) -> Option<FileId> {
        self.largest_size()
            .or_else(|| self.thumb_size())
            .map(|size| size.file_id)
    }

    /// Secret photos must not download on placeholder click (schema: show only while tapped).
    pub fn click_requests_download(&self) -> bool {
        !self.is_secret
    }

    /// Placeholder copy follows file state for secret and spoiler photos.
    pub fn placeholder_label(&self, downloading: bool, ready: bool) -> String {
        let kind = if self.is_secret {
            "Secret photo"
        } else if self.has_spoiler {
            "Photo (spoiler)"
        } else {
            "Photo"
        };
        let state = if ready {
            "ready"
        } else if downloading {
            "downloading…"
        } else {
            "not downloaded"
        };
        format!("{kind} — {state}")
    }
}

/// `photoSize` fields used for display / download (schema: type, photo, width, height).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoSizeView {
    pub type_name: String,
    pub width: i32,
    pub height: i32,
    pub file_id: FileId,
}

/// `document` + caption from `messageDocument`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentContent {
    pub file_name: String,
    pub mime_type: String,
    pub caption: String,
    pub file_id: FileId,
}

/// Typed `file` + `localFile` (no `remoteFile.id` — that can be an HTTP URL).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFile {
    pub id: FileId,
    pub size: i64,
    pub expected_size: i64,
    pub local: LocalFileState,
}

impl ParsedFile {
    pub fn usable_path(&self) -> Option<&str> {
        if self.local.is_downloading_completed && !self.local.path.is_empty() {
            Some(self.local.path.as_str())
        } else {
            None
        }
    }

    pub fn needs_download(&self) -> bool {
        self.usable_path().is_none() && self.local.can_be_downloaded
    }

    pub fn display_size(&self) -> i64 {
        if self.size > 0 {
            self.size
        } else {
            self.expected_size
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalFileState {
    pub path: String,
    pub can_be_downloaded: bool,
    pub is_downloading_active: bool,
    pub is_downloading_completed: bool,
}

impl LocalFileState {
    /// Download is neither in flight nor finished (`file` / `updateFile` idle).
    pub fn is_idle_incomplete(&self) -> bool {
        !self.is_downloading_active && !self.is_downloading_completed
    }
}

#[derive(Debug, Deserialize)]
struct RawEnvelope {
    #[serde(rename = "@type")]
    type_name: String,
    #[serde(rename = "@extra")]
    extra: Option<Value>,
    #[serde(rename = "@client_id")]
    client_id: Option<i32>,
}

pub fn parse_envelope(json: &str) -> Result<Envelope, ParseError> {
    let raw: RawEnvelope = serde_json::from_str(json).map_err(|_| ParseError::InvalidJson)?;
    let extra = parse_extra(raw.extra.as_ref());
    let payload = parse_payload(&raw.type_name, json)?;
    Ok(Envelope {
        type_name: raw.type_name,
        extra,
        client_id: raw.client_id,
        payload,
    })
}

fn parse_extra(value: Option<&Value>) -> Option<RequestId> {
    match value {
        Some(Value::String(s)) => RequestId::from_str(s).ok(),
        Some(Value::Number(n)) => n.as_u64().map(RequestId),
        _ => None,
    }
}

fn parse_payload(type_name: &str, json: &str) -> Result<EnvelopePayload, ParseError> {
    let value: Value = serde_json::from_str(json).map_err(|_| ParseError::InvalidJson)?;
    match type_name {
        "updateAuthorizationState" => {
            let state = value
                .get("authorization_state")
                .ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::UpdateAuthorizationState(parse_auth(state)))
        }
        "authorizationStateWaitTdlibParameters"
        | "authorizationStateWaitPhoneNumber"
        | "authorizationStateWaitPremiumPurchase"
        | "authorizationStateWaitEmailAddress"
        | "authorizationStateWaitEmailCode"
        | "authorizationStateWaitCode"
        | "authorizationStateWaitOtherDeviceConfirmation"
        | "authorizationStateWaitRegistration"
        | "authorizationStateWaitPassword"
        | "authorizationStateReady"
        | "authorizationStateLoggingOut"
        | "authorizationStateClosing"
        | "authorizationStateClosed" => Ok(EnvelopePayload::UpdateAuthorizationState(parse_auth(
            &value,
        ))),
        "updateNewMessage" => {
            let message = value.get("message").ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::UpdateNewMessage(parse_message(message)?))
        }
        "updateMessageSendSucceeded" => Ok(EnvelopePayload::UpdateMessageSendSucceeded {
            message: parse_message(value.get("message").ok_or(ParseError::MissingField)?)?,
            old_message_id: MessageId(int53(value.get("old_message_id"))?),
        }),
        "updateMessageSendFailed" => Ok(EnvelopePayload::UpdateMessageSendFailed {
            message: parse_message(value.get("message").ok_or(ParseError::MissingField)?)?,
            old_message_id: MessageId(int53(value.get("old_message_id"))?),
            error: parse_error(value.get("error")),
        }),
        "updateMessageSendAcknowledged" => Ok(EnvelopePayload::UpdateMessageSendAcknowledged {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            message_id: MessageId(int53(value.get("message_id"))?),
        }),
        "updateDeleteMessages" => Ok(EnvelopePayload::UpdateDeleteMessages {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            message_ids: int53_array(value.get("message_ids")),
            is_permanent: value
                .get("is_permanent")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            from_cache: value
                .get("from_cache")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        "updateMessageContent" => {
            let (content, files) = parse_content(value.get("new_content"));
            Ok(EnvelopePayload::UpdateMessageContent {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                message_id: MessageId(int53(value.get("message_id"))?),
                content,
                files,
            })
        }
        "updateChatPosition" => Ok(EnvelopePayload::UpdateChatPosition(parse_position(&value)?)),
        "updateChatTitle" => Ok(EnvelopePayload::UpdateChatTitle {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            title: value
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        "updateChatLastMessage" => {
            let chat_id = ChatId(int53(value.get("chat_id"))?);
            Ok(EnvelopePayload::UpdateChatLastMessage {
                chat_id,
                last_message: match value.get("last_message") {
                    None | Some(Value::Null) => None,
                    Some(message) => parse_message(message).ok(),
                },
                positions: parse_position_list(chat_id, value.get("positions")),
            })
        }
        "updateChatAddedToList" => Ok(EnvelopePayload::UpdateChatAddedToList {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            list: parse_chat_list(value.get("chat_list")),
        }),
        "updateChatRemovedFromList" => Ok(EnvelopePayload::UpdateChatRemovedFromList {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            list: parse_chat_list(value.get("chat_list")),
        }),
        "updateChatReadInbox" => Ok(EnvelopePayload::UpdateChatReadInbox {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            last_read_inbox_message_id: MessageId(int53_or_zero(
                value.get("last_read_inbox_message_id"),
            )),
            unread_count: value
                .get("unread_count")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
        }),
        "updateChatReadOutbox" => Ok(EnvelopePayload::UpdateChatReadOutbox {
            chat_id: ChatId(int53(value.get("chat_id"))?),
            last_read_outbox_message_id: MessageId(int53_or_zero(
                value.get("last_read_outbox_message_id"),
            )),
        }),
        "updateConnectionState" => Ok(EnvelopePayload::UpdateConnectionState(parse_connection(
            value.get("state"),
        ))),
        "updateNewChat" => {
            let chat = value.get("chat").ok_or(ParseError::MissingField)?;
            Ok(EnvelopePayload::UpdateNewChat {
                chat_id: ChatId(int53(chat.get("id"))?),
                title: chat
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                kind: parse_chat_kind(chat.get("type")),
                unread_count: chat
                    .get("unread_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
                last_read_inbox_message_id: MessageId(int53_or_zero(
                    chat.get("last_read_inbox_message_id"),
                )),
                last_read_outbox_message_id: MessageId(int53_or_zero(
                    chat.get("last_read_outbox_message_id"),
                )),
            })
        }
        "ok" => Ok(EnvelopePayload::Ok),
        "error" => Ok(EnvelopePayload::Error(parse_error(Some(&value)))),
        "messages" => {
            let messages = value
                .get("messages")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let parsed = messages
                .iter()
                .filter_map(|m| parse_message(m).ok())
                .collect();
            Ok(EnvelopePayload::Messages(parsed))
        }
        "chats" => Ok(EnvelopePayload::Chats {
            total_count: value
                .get("total_count")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            chat_ids: value
                .get("chat_ids")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|v| int53(Some(v)).ok())
                .map(ChatId)
                .collect(),
        }),
        "foundMessages" => {
            let messages = value
                .get("messages")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let parsed = messages
                .iter()
                .filter_map(|m| parse_message(m).ok())
                .collect();
            Ok(EnvelopePayload::FoundMessages {
                total_count: value
                    .get("total_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
                messages: parsed,
                next_offset: value
                    .get("next_offset")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            })
        }
        "foundChatMessages" => {
            let messages = value
                .get("messages")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let parsed = messages
                .iter()
                .filter_map(|m| parse_message(m).ok())
                .collect();
            Ok(EnvelopePayload::FoundChatMessages {
                total_count: value
                    .get("total_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
                messages: parsed,
                next_from_message_id: MessageId(int53_or_zero(value.get("next_from_message_id"))),
            })
        }
        "message" => Ok(EnvelopePayload::Message(parse_message(&value)?)),
        "updateFile" => Ok(EnvelopePayload::UpdateFile(parse_file(value.get("file"))?)),
        "file" => Ok(EnvelopePayload::File(parse_file(Some(&value))?)),
        other => Ok(EnvelopePayload::Unknown(UnknownKind {
            type_name: other.to_string(),
        })),
    }
}

fn parse_auth(value: &Value) -> AuthorizationState {
    let ty = value.get("@type").and_then(Value::as_str).unwrap_or("");
    match ty {
        "authorizationStateWaitTdlibParameters" => AuthorizationState::WaitTdlibParameters,
        "authorizationStateWaitPhoneNumber" => AuthorizationState::WaitPhoneNumber,
        "authorizationStateWaitPremiumPurchase" => AuthorizationState::WaitPremiumPurchase,
        "authorizationStateWaitEmailAddress" => AuthorizationState::WaitEmailAddress,
        "authorizationStateWaitEmailCode" => AuthorizationState::WaitEmailCode,
        "authorizationStateWaitCode" => AuthorizationState::WaitCode {
            code_length: value
                .get("code_info")
                .and_then(|info| info.get("type"))
                .and_then(|ty| ty.get("length"))
                .and_then(Value::as_i64)
                .map(|n| n as i32),
        },
        "authorizationStateWaitOtherDeviceConfirmation" => {
            AuthorizationState::WaitOtherDeviceConfirmation
        }
        "authorizationStateWaitRegistration" => AuthorizationState::WaitRegistration,
        "authorizationStateWaitPassword" => AuthorizationState::WaitPassword {
            has_recovery_email: value
                .get("has_recovery_email_address")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        "authorizationStateReady" => AuthorizationState::Ready,
        "authorizationStateLoggingOut" => AuthorizationState::LoggingOut,
        "authorizationStateClosing" => AuthorizationState::Closing,
        "authorizationStateClosed" => AuthorizationState::Closed,
        other => AuthorizationState::Unknown(other.to_string()),
    }
}

fn parse_connection(value: Option<&Value>) -> ConnectionState {
    match value.and_then(|v| v.get("@type")).and_then(Value::as_str) {
        Some("connectionStateWaitingForNetwork") => ConnectionState::WaitingForNetwork,
        Some("connectionStateConnectingToProxy") => ConnectionState::ConnectingToProxy,
        Some("connectionStateConnecting") => ConnectionState::Connecting,
        Some("connectionStateUpdating") => ConnectionState::Updating,
        Some("connectionStateReady") => ConnectionState::Ready,
        _ => ConnectionState::Unknown,
    }
}

fn parse_chat_kind(value: Option<&Value>) -> ChatKind {
    let Some(value) = value else {
        return ChatKind::Unknown;
    };
    match value.get("@type").and_then(Value::as_str) {
        Some("chatTypePrivate") => ChatKind::Private {
            user_id: UserId(int53(value.get("user_id")).unwrap_or(0)),
        },
        Some("chatTypeBasicGroup") => ChatKind::BasicGroup {
            basic_group_id: int53(value.get("basic_group_id")).unwrap_or(0),
        },
        Some("chatTypeSupergroup") => ChatKind::Supergroup {
            supergroup_id: int53(value.get("supergroup_id")).unwrap_or(0),
            is_channel: value
                .get("is_channel")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        Some("chatTypeSecret") => ChatKind::Secret {
            secret_chat_id: value
                .get("secret_chat_id")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            user_id: UserId(int53(value.get("user_id")).unwrap_or(0)),
        },
        _ => ChatKind::Unknown,
    }
}

fn parse_position(value: &Value) -> Result<ChatPositionUpdate, ParseError> {
    let chat_id = ChatId(int53(value.get("chat_id"))?);
    let position = value.get("position").unwrap_or(value);
    Ok(parse_position_entry(chat_id, position))
}

fn parse_position_entry(chat_id: ChatId, position: &Value) -> ChatPositionUpdate {
    ChatPositionUpdate {
        chat_id,
        list: parse_chat_list(position.get("list")),
        order: int64(position.get("order")).unwrap_or(0),
        is_pinned: position
            .get("is_pinned")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

fn parse_position_list(chat_id: ChatId, value: Option<&Value>) -> Vec<ChatPositionUpdate> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|position| parse_position_entry(chat_id, position))
        .collect()
}

fn parse_chat_list(value: Option<&Value>) -> ChatList {
    match value.and_then(|v| v.get("@type")).and_then(Value::as_str) {
        Some("chatListMain") => ChatList::Main,
        Some("chatListArchive") => ChatList::Archive,
        Some("chatListFolder") => ChatList::Folder(
            value
                .and_then(|v| v.get("chat_folder_id"))
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
        ),
        _ => ChatList::Unknown,
    }
}

fn parse_message(value: &Value) -> Result<ParsedMessage, ParseError> {
    let (content, files) = parse_content(value.get("content"));
    Ok(ParsedMessage {
        id: MessageId(int53(value.get("id"))?),
        chat_id: ChatId(int53(value.get("chat_id"))?),
        is_outgoing: value
            .get("is_outgoing")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        content,
        files,
        reply_to: parse_reply_to(value.get("reply_to")),
        forward_info: parse_forward_info(value.get("forward_info")),
    })
}

fn parse_forward_info(value: Option<&Value>) -> Option<MessageForwardInfo> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    match value.get("@type").and_then(Value::as_str) {
        Some("messageForwardInfo") => {
            let origin = parse_message_origin(value.get("origin"))?;
            Some(MessageForwardInfo {
                origin,
                date: value.get("date").and_then(Value::as_i64).unwrap_or(0) as i32,
            })
        }
        _ => None,
    }
}

fn parse_message_origin(value: Option<&Value>) -> Option<MessageOrigin> {
    let value = value?;
    match value.get("@type").and_then(Value::as_str) {
        Some("messageOriginUser") => Some(MessageOrigin::User {
            user_id: UserId(int53_or_zero(value.get("sender_user_id"))),
        }),
        Some("messageOriginHiddenUser") => Some(MessageOrigin::HiddenUser {
            sender_name: value
                .get("sender_name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        Some("messageOriginChat") => Some(MessageOrigin::Chat {
            chat_id: ChatId(int53_or_zero(value.get("sender_chat_id"))),
            author_signature: value
                .get("author_signature")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        Some("messageOriginChannel") => Some(MessageOrigin::Channel {
            chat_id: ChatId(int53_or_zero(value.get("chat_id"))),
            message_id: MessageId(int53_or_zero(value.get("message_id"))),
            author_signature: value
                .get("author_signature")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        _ => None,
    }
}

fn parse_reply_to(value: Option<&Value>) -> Option<MessageReplyTo> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    match value.get("@type").and_then(Value::as_str) {
        Some("messageReplyToMessage") => {
            let message_id = int53_or_zero(value.get("message_id"));
            if message_id == 0 {
                return None;
            }
            let quote_text = value.get("quote").and_then(|quote| {
                if quote.is_null() {
                    return None;
                }
                let text = parse_formatted_text(quote.get("text"));
                if text.is_empty() { None } else { Some(text) }
            });
            let content_preview = value.get("content").and_then(|content| {
                if content.is_null() {
                    return None;
                }
                let preview = parse_content(Some(content)).0.preview();
                if preview.is_empty() {
                    None
                } else {
                    Some(preview)
                }
            });
            Some(MessageReplyTo {
                chat_id: ChatId(int53_or_zero(value.get("chat_id"))),
                message_id: MessageId(message_id),
                quote_text,
                content_preview,
            })
        }
        _ => None,
    }
}

fn parse_content(value: Option<&Value>) -> (MessageContent, Vec<ParsedFile>) {
    let Some(value) = value else {
        return (
            MessageContent::Unsupported {
                type_name: "missing".into(),
            },
            Vec::new(),
        );
    };
    match value.get("@type").and_then(Value::as_str) {
        Some("messageText") => (
            MessageContent::Text(parse_formatted_text(value.get("text"))),
            Vec::new(),
        ),
        Some("messagePhoto") => parse_message_photo(value),
        Some("messageDocument") => parse_message_document(value),
        Some(other) => (
            MessageContent::Unsupported {
                type_name: other.to_string(),
            },
            Vec::new(),
        ),
        None => (
            MessageContent::Unsupported {
                type_name: "unknown".into(),
            },
            Vec::new(),
        ),
    }
}

fn parse_formatted_text(value: Option<&Value>) -> String {
    value
        .and_then(|text| text.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn parse_message_photo(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let photo = value.get("photo");
    let mut files = Vec::new();
    let mut sizes = Vec::new();
    if let Some(entries) = photo.and_then(|p| p.get("sizes")).and_then(Value::as_array) {
        for entry in entries {
            let Ok(file) = parse_file(entry.get("photo")) else {
                continue;
            };
            sizes.push(PhotoSizeView {
                type_name: entry
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                width: int53_or_zero(entry.get("width")) as i32,
                height: int53_or_zero(entry.get("height")) as i32,
                file_id: file.id,
            });
            files.push(file);
        }
    }
    (
        MessageContent::Photo(PhotoContent {
            caption: parse_formatted_text(value.get("caption")),
            sizes,
            is_secret: value
                .get("is_secret")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            has_spoiler: value
                .get("has_spoiler")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        files,
    )
}

fn parse_message_document(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    let document = value.get("document");
    let mut files = Vec::new();
    let file_id = match document.and_then(|d| parse_file(d.get("document")).ok()) {
        Some(file) => {
            let id = file.id;
            files.push(file);
            id
        }
        None => FileId(0),
    };
    if let Some(thumb) = document.and_then(|d| d.get("thumbnail"))
        && let Ok(thumb_file) = parse_file(thumb.get("file"))
    {
        files.push(thumb_file);
    }
    (
        MessageContent::Document(DocumentContent {
            file_name: document
                .and_then(|d| d.get("file_name"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            mime_type: document
                .and_then(|d| d.get("mime_type"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            caption: parse_formatted_text(value.get("caption")),
            file_id,
        }),
        files,
    )
}

fn parse_file(value: Option<&Value>) -> Result<ParsedFile, ParseError> {
    let value = value.ok_or(ParseError::MissingField)?;
    let id = i32::try_from(int53(value.get("id"))?).map_err(|_| ParseError::BadInt)?;
    let local = value.get("local");
    Ok(ParsedFile {
        id: FileId(id),
        size: int53_or_zero(value.get("size")),
        expected_size: int53_or_zero(value.get("expected_size")),
        local: LocalFileState {
            path: local
                .and_then(|l| l.get("path"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            can_be_downloaded: local
                .and_then(|l| l.get("can_be_downloaded"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            is_downloading_active: local
                .and_then(|l| l.get("is_downloading_active"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            is_downloading_completed: local
                .and_then(|l| l.get("is_downloading_completed"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
    })
}

fn parse_error(value: Option<&Value>) -> TdError {
    TdError::from_code(
        value
            .and_then(|v| v.get("code"))
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
    )
}

fn int53(value: Option<&Value>) -> Result<i64, ParseError> {
    match value {
        Some(Value::Number(n)) => n.as_i64().ok_or(ParseError::BadInt),
        Some(Value::String(s)) => s.parse().map_err(|_| ParseError::BadInt),
        _ => Err(ParseError::MissingField),
    }
}

fn int53_or_zero(value: Option<&Value>) -> i64 {
    int53(value).unwrap_or(0)
}

fn int64(value: Option<&Value>) -> Option<i64> {
    match value {
        Some(Value::String(s)) => s.parse().ok(),
        Some(Value::Number(n)) => n.as_i64(),
        _ => None,
    }
}

fn int53_array(value: Option<&Value>) -> Vec<MessageId> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| int53(Some(v)).ok())
        .map(MessageId)
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    InvalidJson,
    MissingField,
    BadInt,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_variant_does_not_keep_raw_json() {
        let json =
            r#"{"@type":"updateSomethingSecret","secret":"CANARY_PHONE_+1555","@extra":"1"}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::Unknown(ref kind) => {
                assert_eq!(kind.type_name, "updateSomethingSecret")
            }
            other => panic!("unexpected {other:?}"),
        }
        let debug = format!("{env:?}");
        assert!(!debug.contains("CANARY_PHONE"));
        assert!(!debug.contains("+1555"));
    }

    #[test]
    fn int64_order_is_not_float() {
        let json = r#"{"@type":"updateChatPosition","chat_id":42,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9223372036854775806","is_pinned":false}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateChatPosition(pos) => {
                assert_eq!(pos.order, 9223372036854775806);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn send_message_uses_topic_id_field_in_schema() {
        // Guard against obsolete message_thread_id examples.
        let schema = include_str!("../../schema/td_api.tl");
        let send = schema
            .lines()
            .find(|l| l.starts_with("sendMessage "))
            .expect("sendMessage");
        assert!(send.contains("topic_id:MessageTopic"));
        assert!(!send.contains("message_thread_id"));
    }

    #[test]
    fn read_inbox_and_outbox_are_typed() {
        let inbox = parse_envelope(
            r#"{"@type":"updateChatReadInbox","chat_id":4,"last_read_inbox_message_id":88,"unread_count":3}"#,
        )
        .unwrap();
        match inbox.payload {
            EnvelopePayload::UpdateChatReadInbox {
                chat_id,
                last_read_inbox_message_id,
                unread_count,
            } => {
                assert_eq!(chat_id.0, 4);
                assert_eq!(last_read_inbox_message_id.0, 88);
                assert_eq!(unread_count, 3);
            }
            other => panic!("{other:?}"),
        }
        let outbox = parse_envelope(
            r#"{"@type":"updateChatReadOutbox","chat_id":4,"last_read_outbox_message_id":91}"#,
        )
        .unwrap();
        match outbox.payload {
            EnvelopePayload::UpdateChatReadOutbox {
                chat_id,
                last_read_outbox_message_id,
            } => {
                assert_eq!(chat_id.0, 4);
                assert_eq!(last_read_outbox_message_id.0, 91);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn new_chat_carries_read_cursors() {
        let json = r#"{"@type":"updateNewChat","chat":{"id":9,"title":"n","type":{"@type":"chatTypePrivate","user_id":9},"unread_count":2,"last_read_inbox_message_id":10,"last_read_outbox_message_id":11}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewChat {
                unread_count,
                last_read_inbox_message_id,
                last_read_outbox_message_id,
                ..
            } => {
                assert_eq!(unread_count, 2);
                assert_eq!(last_read_inbox_message_id.0, 10);
                assert_eq!(last_read_outbox_message_id.0, 11);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn chats_and_found_messages_are_typed() {
        let chats = parse_envelope(
            r#"{"@type":"chats","@extra":"4","total_count":2,"chat_ids":[11,"12"]}"#,
        )
        .unwrap();
        match chats.payload {
            EnvelopePayload::Chats {
                total_count,
                chat_ids,
            } => {
                assert_eq!(total_count, 2);
                assert_eq!(chat_ids, vec![ChatId(11), ChatId(12)]);
            }
            other => panic!("{other:?}"),
        }
        let found = parse_envelope(
            r#"{"@type":"foundMessages","@extra":"5","total_count":1,"next_offset":"n1","messages":[{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_FOUND_hi","entities":[]}}}]}"#,
        )
        .unwrap();
        match found.payload {
            EnvelopePayload::FoundMessages {
                total_count,
                messages,
                next_offset,
            } => {
                assert_eq!(total_count, 1);
                assert_eq!(next_offset, "n1");
                assert_eq!(messages.len(), 1);
                assert_eq!(messages[0].id.0, 101);
                assert_eq!(messages[0].chat_id.0, 11);
                assert_eq!(messages[0].content.preview(), "CANARY_FOUND_hi");
            }
            other => panic!("{other:?}"),
        }
        let schema = include_str!("../../schema/td_api.tl");
        assert!(schema.lines().any(|l| l.starts_with("searchChats ")));
        assert!(schema.lines().any(|l| l.starts_with("searchMessages ")));
        assert!(
            schema
                .lines()
                .any(|l| l.starts_with("searchRecentlyFoundChats "))
        );
        assert!(
            schema
                .lines()
                .any(|l| l.starts_with("addRecentlyFoundChat "))
        );
        assert!(schema.lines().any(|l| l.starts_with("chats ")));
        assert!(schema.lines().any(|l| l.starts_with("foundMessages ")));
        let in_chat = parse_envelope(
            r#"{"@type":"foundChatMessages","@extra":"6","total_count":2,"next_from_message_id":"40","messages":[{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_CHAT_FOUND","entities":[]}}}]}"#,
        )
        .unwrap();
        match in_chat.payload {
            EnvelopePayload::FoundChatMessages {
                total_count,
                messages,
                next_from_message_id,
            } => {
                assert_eq!(total_count, 2);
                assert_eq!(next_from_message_id.0, 40);
                assert_eq!(messages.len(), 1);
                assert_eq!(messages[0].id.0, 101);
                assert_eq!(messages[0].content.preview(), "CANARY_CHAT_FOUND");
            }
            other => panic!("{other:?}"),
        }
        assert!(schema.lines().any(|l| l.starts_with("searchChatMessages ")));
        assert!(schema.lines().any(|l| l.starts_with("foundChatMessages ")));
    }

    #[test]
    fn view_messages_schema_matches_1_8_67() {
        let schema = include_str!("../../schema/td_api.tl");
        let view = schema
            .lines()
            .find(|l| l.starts_with("viewMessages "))
            .expect("viewMessages");
        assert!(view.contains("message_ids:vector<int53>"));
        assert!(view.contains("source:MessageSource"));
        assert!(view.contains("force_read:Bool"));
        assert!(
            schema
                .lines()
                .any(|l| l.starts_with("updateChatReadOutbox "))
        );
        assert!(schema.lines().any(|l| l.starts_with("openChat ")));
        assert!(schema.lines().any(|l| l.starts_with("closeChat ")));
        assert!(
            schema
                .lines()
                .any(|l| l.starts_with("messageSourceChatHistory"))
        );
    }

    #[test]
    fn last_message_positions_are_typed() {
        let json = r#"{"@type":"updateChatLastMessage","chat_id":3,"last_message":{"id":1,"chat_id":3,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"preview","entities":[]}}},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"5","is_pinned":true}]}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateChatLastMessage {
                chat_id,
                last_message,
                positions,
            } => {
                assert_eq!(chat_id.0, 3);
                assert_eq!(
                    last_message.unwrap().content,
                    MessageContent::Text("preview".into())
                );
                assert_eq!(positions.len(), 1);
                assert_eq!(positions[0].order, 5);
                assert!(positions[0].is_pinned);
                assert_eq!(positions[0].list, ChatList::Main);
            }
            other => panic!("{other:?}"),
        }
    }

    fn local_file_json(id: i32, path: &str, completed: bool, can_download: bool) -> String {
        format!(
            r#"{{"@type":"file","id":{id},"size":12,"expected_size":12,"local":{{"@type":"localFile","path":{path},"can_be_downloaded":{can_download},"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":{completed},"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"CANARY_REMOTE_ID","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":12}}}}"#,
            path = serde_json::to_string(path).unwrap(),
            can_download = can_download,
            completed = completed,
        )
    }

    #[test]
    fn message_photo_parses_sizes_caption_and_flags() {
        let thumb = local_file_json(1, "", false, true);
        let full = local_file_json(2, "/tmp/quill-photo.jpg", true, true);
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":9,"chat_id":4,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{thumb},"width":320,"height":240,"progressive_sizes":[]}},{{"@type":"photoSize","type":"x","photo":{full},"width":800,"height":600,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"CANARY_PHOTO_caption","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
        );
        let env = parse_envelope(&json).unwrap();
        match &env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                let MessageContent::Photo(photo) = &message.content else {
                    panic!("{:?}", message.content);
                };
                assert_eq!(photo.caption, "CANARY_PHOTO_caption");
                assert!(!photo.is_secret);
                assert!(!photo.has_spoiler);
                assert_eq!(photo.sizes.len(), 2);
                assert_eq!(photo.thumb_size().unwrap().type_name, "m");
                assert_eq!(photo.thumb_size().unwrap().file_id.0, 1);
                assert_eq!(photo.largest_size().unwrap().file_id.0, 2);
                assert_eq!(photo.open_file_id().unwrap().0, 2);
                assert_eq!(message.files.len(), 2);
                assert_eq!(message.files[0].id.0, 1);
                assert!(message.files[0].needs_download());
                assert_eq!(message.files[1].usable_path(), Some("/tmp/quill-photo.jpg"));
            }
            other => panic!("{other:?}"),
        }
        let debug = format!("{env:?}");
        assert!(!debug.contains("CANARY_REMOTE_ID"));
    }

    #[test]
    fn message_document_parses_name_mime_and_file() {
        let file = local_file_json(8, "", false, true);
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":3,"chat_id":4,"is_outgoing":false,"content":{{"@type":"messageDocument","document":{{"@type":"document","file_name":"notes.txt","mime_type":"text/plain","document":{file}}},"caption":{{"@type":"formattedText","text":"CANARY_DOC_caption","entities":[]}}}}}}}}"#
        );
        let env = parse_envelope(&json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                let MessageContent::Document(doc) = message.content else {
                    panic!("{:?}", message.content);
                };
                assert_eq!(doc.file_name, "notes.txt");
                assert_eq!(doc.mime_type, "text/plain");
                assert_eq!(doc.caption, "CANARY_DOC_caption");
                assert_eq!(doc.file_id.0, 8);
                assert_eq!(message.files[0].id.0, 8);
                assert!(message.files[0].needs_download());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn update_file_and_file_response_are_typed() {
        let file = local_file_json(4, "/tmp/done.bin", true, true);
        let update = parse_envelope(&format!(r#"{{"@type":"updateFile","file":{file}}}"#)).unwrap();
        match update.payload {
            EnvelopePayload::UpdateFile(parsed) => {
                assert_eq!(parsed.id.0, 4);
                assert_eq!(parsed.usable_path(), Some("/tmp/done.bin"));
            }
            other => panic!("{other:?}"),
        }
        let response = parse_envelope(
            r#"{"@type":"file","@extra":"12","id":4,"size":12,"expected_size":12,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":true,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":12}}"#,
        )
        .unwrap();
        match response.payload {
            EnvelopePayload::File(parsed) => {
                assert_eq!(parsed.id.0, 4);
                assert!(parsed.local.is_downloading_active);
                assert!(parsed.needs_download());
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(response.extra, Some(crate::ids::RequestId(12)));
    }

    #[test]
    fn media_schema_matches_1_8_67() {
        let schema = include_str!("../../schema/td_api.tl");
        let photo = schema
            .lines()
            .find(|l| l.starts_with("messagePhoto "))
            .expect("messagePhoto");
        assert!(photo.contains("photo:photo"));
        assert!(photo.contains("caption:formattedText"));
        assert!(photo.contains("is_secret:Bool"));
        assert!(photo.contains("has_spoiler:Bool"));
        let document = schema
            .lines()
            .find(|l| l.starts_with("messageDocument "))
            .expect("messageDocument");
        assert!(document.contains("document:document"));
        assert!(document.contains("caption:formattedText"));
        let download = schema
            .lines()
            .find(|l| l.starts_with("downloadFile "))
            .expect("downloadFile");
        assert!(download.contains("file_id:int32"));
        assert!(download.contains("priority:int32"));
        assert!(download.contains("offset:int53"));
        assert!(download.contains("limit:int53"));
        assert!(download.contains("synchronous:Bool"));
        assert!(schema.lines().any(|l| l.starts_with("updateFile ")));
        assert!(schema.lines().any(|l| l.starts_with("localFile ")));
        assert!(schema.lines().any(|l| l.starts_with("photoSize ")));
    }

    #[test]
    fn secret_photo_is_flagged() {
        let file = local_file_json(1, "", false, true);
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":1,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":100,"height":80,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":true,"is_secret":true}}}}}}"#
        );
        let env = parse_envelope(&json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                let MessageContent::Photo(photo) = message.content else {
                    panic!("{:?}", message.content);
                };
                assert!(photo.is_secret);
                assert!(photo.has_spoiler);
                assert!(!photo.click_requests_download());
                assert_eq!(
                    photo.placeholder_label(false, false),
                    "Secret photo — not downloaded"
                );
                assert_eq!(
                    photo.placeholder_label(true, false),
                    "Secret photo — downloading…"
                );
                assert_eq!(photo.placeholder_label(false, true), "Secret photo — ready");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn spoiler_placeholder_follows_file_state_and_may_download() {
        let file = local_file_json(1, "", false, true);
        let json = format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":1,"chat_id":1,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":100,"height":80,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":true,"is_secret":false}}}}}}"#
        );
        let env = parse_envelope(&json).unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                let MessageContent::Photo(photo) = message.content else {
                    panic!("{:?}", message.content);
                };
                assert!(photo.click_requests_download());
                assert_eq!(
                    photo.placeholder_label(false, false),
                    "Photo (spoiler) — not downloaded"
                );
                assert_eq!(
                    photo.placeholder_label(true, false),
                    "Photo (spoiler) — downloading…"
                );
                assert_eq!(
                    photo.placeholder_label(false, true),
                    "Photo (spoiler) — ready"
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn message_reply_to_message_is_typed() {
        let env = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":104,"chat_id":11,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"sounds good","entities":[]}},"reply_to":{"@type":"messageReplyToMessage","chat_id":11,"message_id":101,"quote":{"@type":"textQuote","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]},"position":0,"is_manual":false},"checklist_task_id":0,"poll_option_id":""}}}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                let reply = message.reply_to.expect("reply_to");
                assert_eq!(reply.chat_id.0, 11);
                assert_eq!(reply.message_id.0, 101);
                assert_eq!(
                    reply.quote_text.as_deref(),
                    Some("Hello from injected JSON.")
                );
                assert!(reply.is_same_chat(ChatId(11)));
            }
            other => panic!("{other:?}"),
        }
        let story = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":2,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"story","entities":[]}},"reply_to":{"@type":"messageReplyToStory","story_poster_chat_id":11,"story_id":3}}}"#,
        )
        .unwrap();
        match story.payload {
            EnvelopePayload::UpdateNewMessage(message) => assert_eq!(message.reply_to, None),
            other => panic!("{other:?}"),
        }
        let schema = include_str!("../../schema/td_api.tl");
        assert!(
            schema
                .lines()
                .any(|l| l.starts_with("messageReplyToMessage "))
        );
        assert!(
            schema
                .lines()
                .any(|l| l.starts_with("inputMessageReplyToMessage "))
        );
        assert!(schema.lines().any(|l| l.starts_with("textQuote ")));
        assert!(schema.lines().any(|l| l.starts_with("inputTextQuote ")));
    }

    #[test]
    fn update_message_content_is_typed() {
        let env = parse_envelope(
            r#"{"@type":"updateMessageContent","chat_id":11,"message_id":102,"new_content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARY_EDITED","entities":[]}}}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UpdateMessageContent {
                chat_id,
                message_id,
                content,
                files,
            } => {
                assert_eq!(chat_id.0, 11);
                assert_eq!(message_id.0, 102);
                assert_eq!(content, MessageContent::Text("CANARY_EDITED".into()));
                assert!(files.is_empty());
            }
            other => panic!("{other:?}"),
        }
        let schema = include_str!("../../schema/td_api.tl");
        assert!(
            schema
                .lines()
                .any(|l| l.starts_with("updateMessageContent "))
        );
        assert!(schema.lines().any(|l| l.starts_with("editMessageText ")));
        assert!(schema.lines().any(|l| l.starts_with("editMessageCaption ")));
        assert!(schema.lines().any(|l| l.starts_with("deleteMessages ")));
    }

    #[test]
    fn message_forward_info_and_messages_are_typed() {
        let env = parse_envelope(
            r#"{"@type":"updateNewMessage","message":{"id":105,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"fwd body","entities":[]}},"forward_info":{"@type":"messageForwardInfo","origin":{"@type":"messageOriginHiddenUser","sender_name":"Ada Lovelace"},"date":1700000000,"source":null,"public_service_announcement_type":""}}}"#,
        )
        .unwrap();
        match env.payload {
            EnvelopePayload::UpdateNewMessage(message) => {
                let info = message.forward_info.expect("forward_info");
                assert_eq!(
                    info.origin,
                    MessageOrigin::HiddenUser {
                        sender_name: "Ada Lovelace".into()
                    }
                );
                assert_eq!(info.date, 1_700_000_000);
            }
            other => panic!("{other:?}"),
        }
        let messages = parse_envelope(
            r#"{"@type":"messages","@extra":"34","total_count":2,"messages":[{"id":80,"chat_id":12,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello from injected JSON.","entities":[]}},"forward_info":{"@type":"messageForwardInfo","origin":{"@type":"messageOriginUser","sender_user_id":11},"date":1}},null]}"#,
        )
        .unwrap();
        match messages.payload {
            EnvelopePayload::Messages(parsed) => {
                assert_eq!(parsed.len(), 1);
                assert_eq!(parsed[0].id.0, 80);
                assert_eq!(parsed[0].chat_id.0, 12);
                assert!(matches!(
                    parsed[0].forward_info.as_ref().map(|i| &i.origin),
                    Some(MessageOrigin::User { user_id }) if user_id.0 == 11
                ));
            }
            other => panic!("{other:?}"),
        }
        let schema = include_str!("../../schema/td_api.tl");
        assert!(schema.lines().any(|l| l.starts_with("forwardMessages ")));
        assert!(schema.lines().any(|l| l.starts_with("messages ")));
        assert!(schema.lines().any(|l| l.starts_with("messageForwardInfo ")));
        assert!(schema.lines().any(|l| l.starts_with("messageOriginUser ")));
        assert!(
            schema
                .lines()
                .any(|l| l.starts_with("messageOriginHiddenUser "))
        );
    }
}
