//! Reading a raw TDLib request before `td_call` runs it: dropping fields
//! the schema does not know, turning every chat id into a plain integer,
//! and listing the chats it names, the local files it would upload and the
//! types that can spend money.

use super::td_redact::{SECRET_BASE, SUPERGROUP_BASE};
use super::td_schema::Schema;
use serde_json::{Map, Value};

/// A field that holds one chat id: `chat_id`, `from_chat_id`,
/// `story_poster_chat_id`...
fn is_chat_field(key: &str) -> bool {
    key == "chat_id" || key.ends_with("_chat_id")
}

fn is_chat_list(key: &str) -> bool {
    key == "chat_ids" || key.ends_with("_chat_ids")
}

/// A chat id as TDLib would read it: an integer, or digits in a string.
fn chat_id_of(value: &Value) -> Option<i64> {
    match value {
        Value::Number(_) => value.as_i64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

/// Keep only the fields `@type` has in the schema, recursively, and make
/// every chat id an integer. Objects without a known `@type` keep their
/// fields, but their chat ids are still normalized and checked.
pub fn sanitize(value: &mut Value, schema: &Schema, top: bool) -> Result<(), String> {
    match value {
        Value::Object(fields) => {
            let kind = fields
                .get("@type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let entry = if top {
                schema.functions.get(&kind)
            } else {
                schema.constructors.get(&kind)
            };
            if let Some(entry) = entry {
                fields.retain(|key, _| key == "@type" || entry.has(key));
            }
            normalize_ids(fields)?;
            for inner in fields.values_mut() {
                sanitize(inner, schema, false)?;
            }
            Ok(())
        }
        Value::Array(items) => items
            .iter_mut()
            .try_for_each(|item| sanitize(item, schema, false)),
        _ => Ok(()),
    }
}

fn normalize_ids(fields: &mut Map<String, Value>) -> Result<(), String> {
    for (key, value) in fields.iter_mut() {
        if value.is_null() {
            continue;
        }
        let single = is_chat_field(key)
            || matches!(
                key.as_str(),
                "supergroup_id" | "basic_group_id" | "secret_chat_id"
            );
        if single {
            let id = chat_id_of(value).ok_or_else(|| format!("{key} must be an integer id."))?;
            *value = Value::from(id);
        } else if is_chat_list(key)
            && let Value::Array(items) = value
        {
            for item in items.iter_mut() {
                let id = chat_id_of(item).ok_or_else(|| format!("{key} must list integer ids."))?;
                *item = Value::from(id);
            }
        }
    }
    Ok(())
}

/// Types whose chat id is only read from: a forward's or a quote's source.
const SOURCE_TYPES: &[&str] = &[
    "inputMessageForwarded",
    "inputMessageReplyToExternalMessage",
];

/// The chats a (sanitized) request names: (chat id, acted on). Every chat
/// is acted on except `from_chat_id` and the chat inside a forward or an
/// external reply, which are only read from.
pub fn chats_named(request: &Value) -> Vec<(i64, bool)> {
    let mut named = Vec::new();
    walk_chats(request, false, &mut named);
    named
}

fn walk_chats(value: &Value, source: bool, named: &mut Vec<(i64, bool)>) {
    match value {
        Value::Object(fields) => {
            let source = source
                || fields
                    .get("@type")
                    .and_then(Value::as_str)
                    .is_some_and(|kind| SOURCE_TYPES.contains(&kind));
            for (key, inner) in fields {
                let id = inner.as_i64().filter(|id| *id != 0);
                let chat = match (key.as_str(), id) {
                    ("supergroup_id", Some(id)) => Some(SUPERGROUP_BASE.saturating_sub(id)),
                    ("basic_group_id", Some(id)) => Some(id.saturating_neg()),
                    ("secret_chat_id", Some(id)) => Some(SECRET_BASE.saturating_add(id)),
                    (key, Some(id)) if is_chat_field(key) => Some(id),
                    _ => None,
                };
                if let Some(chat) = chat {
                    named.push((chat, !(source || key == "from_chat_id")));
                } else if is_chat_list(key) {
                    for id in inner
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_i64)
                    {
                        named.push((id, !source));
                    }
                } else {
                    walk_chats(inner, source, named);
                }
            }
        }
        Value::Array(items) => items
            .iter()
            .for_each(|item| walk_chats(item, source, named)),
        _ => {}
    }
}

/// Local paths the request would upload (`inputFileLocal`, and the source
/// of `inputFileGenerated`).
pub fn local_paths(value: &Value, paths: &mut Vec<String>) {
    match value {
        Value::Object(fields) => {
            let kind = fields.get("@type").and_then(Value::as_str);
            let key = match kind {
                Some("inputFileLocal") => Some("path"),
                Some("inputFileGenerated") => Some("original_path"),
                _ => None,
            };
            if let Some(path) = key.and_then(|key| fields.get(key)).and_then(Value::as_str) {
                paths.push(path.to_string());
            }
            fields.values().for_each(|inner| local_paths(inner, paths));
        }
        Value::Array(items) => items.iter().for_each(|item| local_paths(item, paths)),
        _ => {}
    }
}

/// Whether any type below the top level is `kind`.
pub fn has_type(value: &Value, kind: &str) -> bool {
    match value {
        Value::Object(fields) => fields.iter().any(|(key, inner)| {
            (key == "@type" && inner.as_str() == Some(kind)) || has_type(inner, kind)
        }),
        Value::Array(items) => items.iter().any(|item| has_type(item, kind)),
        _ => false,
    }
}

/// Whether anything inside the request can spend money: a nested type
/// named for Stars, TON or payments (`reactionTypePaid`,
/// `inputMessageStakeDice`, `suggestedPostPriceGram`), or a price.
pub fn spends(request: &Value) -> bool {
    fn inner(value: &Value) -> bool {
        match value {
            Value::Object(fields) => fields.iter().any(|(key, value)| {
                (key == "@type" && value.as_str().is_some_and(super::td_policy::names_money))
                    || (matches!(key.as_str(), "paid_message_star_count" | "star_count")
                        && value.as_i64().is_some_and(|n| n > 0))
                    || inner(value)
            }),
            Value::Array(items) => items.iter().any(inner),
            _ => false,
        }
    }
    request.as_object().is_some_and(|fields| {
        fields
            .iter()
            .any(|(key, value)| key != "@type" && inner(value))
    })
}

#[cfg(test)]
mod tests {
    use super::{chats_named, local_paths, sanitize, spends};
    use crate::agent::tools::td_schema;
    use serde_json::json;

    #[test]
    fn unknown_fields_and_decoy_ids_are_dropped() {
        let mut join = json!({"@type": "joinChatByInviteLink", "invite_link": "x", "chat_id": 5,
            "junk": {"chat_id": 6}});
        assert_eq!(sanitize(&mut join, td_schema::get(), true), Ok(()));
        assert_eq!(
            join,
            json!({"@type": "joinChatByInviteLink", "invite_link": "x"})
        );
        assert!(chats_named(&join).is_empty());
    }

    #[test]
    fn string_ids_are_checked_like_integers() {
        let mut info = json!({"@type": "getSupergroupFullInfo", "supergroup_id": "77"});
        assert_eq!(sanitize(&mut info, td_schema::get(), true), Ok(()));
        assert_eq!(chats_named(&info), vec![(-1_000_000_000_077, true)]);
        let mut bad = json!({"@type": "getChat", "chat_id": "ann"});
        assert!(sanitize(&mut bad, td_schema::get(), true).is_err());
    }

    #[test]
    fn forwards_read_from_their_source_and_act_elsewhere() {
        let mut forward = json!({"@type": "sendMessage", "chat_id": 5, "input_message_content":
            {"@type": "inputMessageForwarded", "from_chat_id": 9, "message_id": 1}});
        assert_eq!(sanitize(&mut forward, td_schema::get(), true), Ok(()));
        assert_eq!(chats_named(&forward), vec![(5, true), (9, false)]);
        let call = json!({"@type": "joinVideoChat", "group_call_id": 1,
            "participant_id": {"@type": "messageSenderChat", "chat_id": 8}});
        assert_eq!(chats_named(&call), vec![(8, true)]);
    }

    #[test]
    fn uploads_and_prices_are_found_anywhere() {
        let send = json!({"@type": "sendMessage", "input_message_content": {"@type": "inputMessageDocument",
            "document": {"@type": "inputFileLocal", "path": "/x/td.binlog"}}});
        let mut paths = Vec::new();
        local_paths(&send, &mut paths);
        assert_eq!(paths, vec!["/x/td.binlog".to_string()]);
        assert!(spends(
            &json!({"@type": "addMessageReaction", "reaction_type": {"@type": "reactionTypePaid"}})
        ));
        assert!(spends(
            &json!({"@type": "sendMessage", "options": {"paid_message_star_count": 10}})
        ));
        assert!(!spends(
            &json!({"@type": "getStarTransactions", "limit": 5})
        ));
        assert!(!spends(
            &json!({"@type": "sendMessage", "input_message_content": {"@type": "inputMessageText"}})
        ));
    }
}
