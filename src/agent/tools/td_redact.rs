//! What `td_call` hides in a TDLib answer: anything from a chat the agent
//! may not read (dropped from lists, which then count only what is left;
//! replaced by a bare stub elsewhere, with no id), other chats' ids,
//! phone numbers and vCards, and the sponsored flag on ads.

use serde_json::{Map, Value, json};

pub const HIDDEN: &str = "not granted to agents";
/// Telegram's offsets between a group or channel id and its chat id.
pub const SUPERGROUP_BASE: i64 = -1_000_000_000_000;
pub const SECRET_BASE: i64 = -2_000_000_000_000;

/// The chat an object is: a chat, group or secret chat by its own id, or
/// an object with a `chat_id` (a message, a sender, a reply).
fn chat_of(fields: &Map<String, Value>) -> Option<i64> {
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
        _ => int("chat_id"),
    };
    id.filter(|id| *id != 0)
}

/// The chat behind one id field other than the object's own, if any:
/// `linked_chat_id`, `personal_chat_id`, `supergroup_id`...
fn chat_of_field(key: &str, value: &Value) -> Option<i64> {
    let id = value.as_i64().filter(|id| *id != 0)?;
    match key {
        "chat_id" => None,
        "supergroup_id" => Some(SUPERGROUP_BASE.saturating_sub(id)),
        "basic_group_id" => Some(id.saturating_neg()),
        "secret_chat_id" => Some(SECRET_BASE.saturating_add(id)),
        key if key.ends_with("_chat_id") => Some(id),
        _ => None,
    }
}

fn is_chat_list(key: &str) -> bool {
    key == "chat_ids" || key.ends_with("_chat_ids")
}

fn is_ad(kind: &str) -> bool {
    kind.starts_with("sponsored") || kind.contains("Advertisement")
}

struct Redactor<'a> {
    allows: &'a mut dyn FnMut(i64) -> bool,
    dropped: bool,
}

impl Redactor<'_> {
    /// Redact `value` in place; false when the whole value must go.
    fn keep(&mut self, value: &mut Value) -> bool {
        match value {
            Value::Object(fields) => self.keep_object(fields),
            Value::Array(items) => {
                self.keep_list(items);
                true
            }
            _ => true,
        }
    }

    fn keep_list(&mut self, items: &mut Vec<Value>) {
        let before = items.len();
        items.retain_mut(|item| self.keep(item));
        self.dropped |= items.len() < before;
    }

    fn keep_object(&mut self, fields: &mut Map<String, Value>) -> bool {
        if chat_of(fields).is_some_and(|id| !(self.allows)(id)) {
            return false;
        }
        fields.remove("phone_number");
        fields.remove("vcard");
        if fields
            .get("@type")
            .and_then(Value::as_str)
            .is_some_and(is_ad)
        {
            fields.insert("sponsored".into(), Value::Bool(true));
        }
        let hidden_ids: Vec<String> = fields
            .iter()
            .filter(|(key, value)| chat_of_field(key, value).is_some_and(|id| !(self.allows)(id)))
            .map(|(key, _)| key.clone())
            .collect();
        for key in hidden_ids {
            fields.insert(key, json!(0));
        }
        let mut filtered = None;
        for (key, inner) in fields.iter_mut() {
            if let Value::Array(items) = inner {
                if is_chat_list(key) {
                    let before = items.len();
                    items.retain(|id| id.as_i64().is_some_and(&mut *self.allows));
                    self.dropped |= items.len() < before;
                    filtered = Some(items.len());
                } else {
                    let of_chats = items
                        .iter()
                        .any(|item| item.as_object().and_then(chat_of).is_some());
                    self.keep_list(items);
                    if of_chats {
                        filtered = Some(items.len());
                    }
                }
            } else if !self.keep(inner) {
                *inner = json!({"hidden": HIDDEN});
            }
        }
        // A count of the whole list would tell how much is hidden, even
        // when this page happens to hold only granted items.
        if let Some(kept) = filtered
            && fields.contains_key("total_count")
        {
            fields.insert("total_count".into(), json!(kept));
        }
        true
    }
}

/// Hide what the agent may not see in a TDLib answer. `allows` says
/// whether the agent may read a chat. True when anything was dropped.
pub fn redact(value: &mut Value, allows: &mut dyn FnMut(i64) -> bool) -> bool {
    let mut redactor = Redactor {
        allows,
        dropped: false,
    };
    if !redactor.keep(value) {
        *value = json!({"hidden": HIDDEN});
        return true;
    }
    redactor.dropped
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

        let mut granted = json!({"@type": "chats", "total_count": 132, "chat_ids": [1]});
        redact(&mut granted, &mut |_| true);
        assert_eq!(granted["total_count"], json!(1));

        let mut info = json!({"@type": "supergroupFullInfo", "linked_chat_id": 0,
            "direct_messages_chat_id": 5, "description": "x"});
        redact(&mut info, &mut |id| id != 5);
        assert_eq!(info["direct_messages_chat_id"], json!(0));
        assert_eq!(info["description"], json!("x"));

        let mut group = json!({"@type": "supergroup", "id": 77, "username": "secret"});
        redact(&mut group, &mut |_| false);
        assert_eq!(group, json!({"hidden": "not granted to agents"}));

        let mut user = json!({"@type": "user", "id": 9, "phone_number": "15551234"});
        redact(&mut user, &mut |_| true);
        assert!(user.get("phone_number").is_none());
        let mut contact = json!({"@type": "contact", "vcard": "TEL:15551234", "user_id": 9});
        redact(&mut contact, &mut |_| true);
        assert!(contact.get("vcard").is_none());

        let mut ad = json!({"@type": "sponsoredMessages", "messages": [{"@type": "sponsoredMessage", "message_id": 1}]});
        redact(&mut ad, &mut |_| true);
        assert_eq!(ad["messages"][0]["sponsored"], json!(true));
        let mut video_ad = json!({"@type": "videoMessageAdvertisement", "unique_id": 1});
        redact(&mut video_ad, &mut |_| true);
        assert_eq!(video_ad["sponsored"], json!(true));
    }
}
