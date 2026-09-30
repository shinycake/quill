use crate::ids::RequestId;
use serde_json::json;

/// Phase B1: `createNewSecretChat` (TDLib 1.8.67, `schema/td_api.tl:13340`):
/// `createNewSecretChat user_id:int53 = Chat;`
/// "Creates a new secret chat. Returns the newly created chat". The new
/// chat also arrives as `updateNewChat` with `chatTypeSecret`.
pub fn create_new_secret_chat(extra: RequestId, user_id: i64) -> String {
    json!({
        "@type": "createNewSecretChat",
        "@extra": extra.as_extra(),
        "user_id": user_id,
    })
    .to_string()
}

/// Phase B1: `getSecretChat` (TDLib 1.8.67, `schema/td_api.tl:11516`):
/// `getSecretChat secret_chat_id:int32 = SecretChat;`
/// "Returns information about a secret chat by its identifier. This is an
/// offline method" — used to learn the initial state of a secret chat
/// whose `updateSecretChat` was never seen (e.g. loaded from the local DB).
pub fn get_secret_chat(extra: RequestId, secret_chat_id: i32) -> String {
    json!({
        "@type": "getSecretChat",
        "@extra": extra.as_extra(),
        "secret_chat_id": secret_chat_id,
    })
    .to_string()
}

/// Phase B1: `closeSecretChat` (TDLib 1.8.67, `schema/td_api.tl:15242`):
/// `closeSecretChat secret_chat_id:int32 = Ok;`
/// "Closes a secret chat, effectively transferring its state to
/// secretChatStateClosed". The state change itself arrives as
/// `updateSecretChat`.
pub fn close_secret_chat(extra: RequestId, secret_chat_id: i32) -> String {
    json!({
        "@type": "closeSecretChat",
        "@extra": extra.as_extra(),
        "secret_chat_id": secret_chat_id,
    })
    .to_string()
}

/// Phase S1: `toggleSessionCanAcceptSecretChats` (TDLib 1.8.67,
/// `schema/td_api.tl:15117`):
/// `toggleSessionCanAcceptSecretChats session_id:int64 can_accept_secret_chats:Bool = Ok;`
/// Per-session toggle — the session accepts (or rejects) new secret chats.
/// TGX surfaces it in the session editor ("Secret Chats" Accept/Reject,
/// `EditSessionController`); slice A4 wires it into A3's session rows,
/// and this request-layer builder sends the raw i64 `session_id` (numeric
/// in the JSON body — asserted by
/// `toggle_session_can_accept_secret_chats_shape_matches_1_8_67`).
pub fn toggle_session_can_accept_secret_chats(
    extra: RequestId,
    session_id: i64,
    can_accept_secret_chats: bool,
) -> String {
    json!({
        "@type": "toggleSessionCanAcceptSecretChats",
        "@extra": extra.as_extra(),
        "session_id": session_id,
        "can_accept_secret_chats": can_accept_secret_chats,
    })
    .to_string()
}
