use super::*;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::Value;

/// Phase B1: `SecretChatState` (TDLib 1.8.67, `schema/td_api.tl:2795`):
/// `secretChatStatePending` (:2798, "waiting for the other user to get
/// online"), `secretChatStateReady` (:2801), `secretChatStateClosed`
/// (:2804).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretChatState {
    Pending,
    Ready,
    Closed,
    Unknown(String),
}

impl SecretChatState {
    pub fn from_type_name(type_name: &str) -> Self {
        match type_name {
            "secretChatStatePending" => SecretChatState::Pending,
            "secretChatStateReady" => SecretChatState::Ready,
            "secretChatStateClosed" => SecretChatState::Closed,
            other => SecretChatState::Unknown(other.to_string()),
        }
    }
}

/// Phase B1: `secretChat` subset (TDLib 1.8.67, `schema/td_api.tl:2816`):
/// `secretChat id:int32 user_id:int53 state:SecretChatState
/// is_outbound:Bool key_hash:bytes layer:int32 = SecretChat;`
/// `key_hash` (36 little-endian bytes) is kept raw for the B2 key
/// verification UI; the layer is kept for future capability gating.
///
/// Security: key material stays in memory only — never logged, never
/// written to disk. `Debug` is hand-written and prints only the hash
/// *length*, so formatting an envelope can never leak key bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct ParsedSecretChat {
    pub id: i32,
    pub user_id: i64,
    pub state: SecretChatState,
    pub is_outbound: bool,
    pub key_hash: Vec<u8>,
    pub layer: i32,
}

impl std::fmt::Debug for ParsedSecretChat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsedSecretChat")
            .field("id", &self.id)
            .field("user_id", &self.user_id)
            .field("state", &self.state)
            .field("is_outbound", &self.is_outbound)
            .field("key_hash_len", &self.key_hash.len())
            .field("layer", &self.layer)
            .finish()
    }
}

pub(crate) fn parse_secret_chat(value: Option<&Value>) -> Option<ParsedSecretChat> {
    let value = value?;
    Some(ParsedSecretChat {
        id: value.get("id").and_then(Value::as_i64).unwrap_or(0) as i32,
        user_id: int53(value.get("user_id")).ok()?,
        state: SecretChatState::from_type_name(
            value
                .get("state")
                .and_then(|s| s.get("@type"))
                .and_then(Value::as_str)
                .unwrap_or(""),
        ),
        is_outbound: value
            .get("is_outbound")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        key_hash: value
            .get("key_hash")
            .and_then(Value::as_str)
            .and_then(|s| STANDARD.decode(s).ok())
            .unwrap_or_default(),
        layer: value.get("layer").and_then(Value::as_i64).unwrap_or(0) as i32,
    })
}
