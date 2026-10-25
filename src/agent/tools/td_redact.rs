//! What `td_call` hides in a TDLib answer: anything from a chat the agent
//! may not read (dropped from lists, which then count only what is left;
//! replaced by a bare stub elsewhere, with no id), phone numbers, and the
//! sponsored flag on ads.

use serde_json::{Value, json};

pub const HIDDEN: &str = "not granted to agents";
/// Telegram's offsets between a group or channel id and its chat id.
pub const SUPERGROUP_BASE: i64 = -1_000_000_000_000;
pub const SECRET_BASE: i64 = -2_000_000_000_000;

/// The chat an object belongs to, if any.
fn chat_of(fields: &serde_json::Map<String, Value>) -> Option<i64> {
    let int = |key: &str| fields.get(key).and_then(Value::as_i64);
    let kind = fields
        .get("@type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let id = match kind {
        "chat" => int("id"),
        "supergroup" => int("id").map(|id| SUPERGROUP_BASE.saturating_sub(id)),
        "basicGroup" => int("id").map(i64::saturating_neg),
        "secretChat" => int("id").map(|id| SECRET_BASE.saturating_add(id)),
        _ => fields
            .iter()
            .find(|(key, value)| key.ends_with("chat_id") && value.is_i64())
            .and_then(|(_, value)| value.as_i64()),
    };
    id.filter(|id| *id != 0)
}

fn is_chat_list(key: &str) -> bool {
    key == "chat_ids" || key.ends_with("_chat_ids")
}

/// Redact `value` in place; false when the whole value must go.
fn keep(value: &mut Value, allows: &mut dyn FnMut(i64) -> bool) -> bool {
    match value {
        Value::Object(fields) => {
            if chat_of(fields).is_some_and(|id| !allows(id)) {
                return false;
            }
            fields.remove("phone_number");
            if fields
                .get("@type")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind.starts_with("sponsored"))
            {
                fields.insert("sponsored".into(), Value::Bool(true));
            }
            let mut kept_after_drop = None;
            for (key, inner) in fields.iter_mut() {
                if let Value::Array(items) = inner {
                    let before = items.len();
                    if is_chat_list(key) {
                        items.retain(|id| id.as_i64().is_some_and(&mut *allows));
                    } else {
                        items.retain_mut(|item| keep(item, allows));
                    }
                    if items.len() < before {
                        kept_after_drop = Some(items.len());
                    }
                } else if !keep(inner, allows) {
                    *inner = json!({"hidden": HIDDEN});
                }
            }
            // A count of the whole list would tell how much is hidden.
            if let Some(kept) = kept_after_drop
                && fields.contains_key("total_count")
            {
                fields.insert("total_count".into(), json!(kept));
            }
            true
        }
        Value::Array(items) => {
            items.retain_mut(|item| keep(item, allows));
            true
        }
        _ => true,
    }
}

/// Hide what the agent may not see in a TDLib answer. `allows` says
/// whether the agent may read a chat.
pub fn redact(value: &mut Value, allows: &mut dyn FnMut(i64) -> bool) {
    if !keep(value, allows) {
        *value = json!({"hidden": HIDDEN});
    }
}

#[cfg(test)]
mod tests {
    use super::redact;
    use serde_json::json;

    #[test]
    fn hidden_chats_leave_no_trace() {
        let mut found = json!({"@type": "foundMessages", "total_count": 40, "messages": [
            {"@type": "message", "chat_id": 1, "id": 5},
            {"@type": "message", "chat_id": 2, "id": 6, "sender_id": {"@type": "messageSenderUser", "user_id": 9}},
        ], "next_offset": ""});
        redact(&mut found, &mut |id| id == 1);
        assert_eq!(found["total_count"], json!(1));
        assert_eq!(found["messages"].as_array().map(Vec::len), Some(1));

        let mut chats = json!({"@type": "chats", "total_count": 132, "chat_ids": [1, 2, 3]});
        redact(&mut chats, &mut |id| id != 2);
        assert_eq!(
            chats,
            json!({"@type": "chats", "total_count": 2, "chat_ids": [1, 3]})
        );

        let mut folder =
            json!({"@type": "chatFolder", "pinned_chat_ids": [1, 2], "included_chat_ids": [2]});
        redact(&mut folder, &mut |id| id == 1);
        assert_eq!(folder["pinned_chat_ids"], json!([1]));
        assert_eq!(folder["included_chat_ids"], json!([]));

        let mut group = json!({"@type": "supergroup", "id": 77, "username": "secret"});
        redact(&mut group, &mut |_| false);
        assert_eq!(group, json!({"hidden": "not granted to agents"}));

        let mut user = json!({"@type": "user", "id": 9, "phone_number": "15551234"});
        redact(&mut user, &mut |_| true);
        assert!(user.get("phone_number").is_none());

        let mut ad = json!({"@type": "sponsoredMessages", "messages": [{"@type": "sponsoredMessage", "message_id": 1}]});
        redact(&mut ad, &mut |_| true);
        assert_eq!(ad["messages"][0]["sponsored"], json!(true));
    }
}
