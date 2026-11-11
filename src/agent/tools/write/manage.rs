//! Tools that change how a chat sits in the user's Quill: draft, read
//! state, notifications, archive and folders.

use super::super::target::{chat_schema, require};
use super::super::{Ctx, ToolError, ToolOutput};
use super::common::{self, TEXT_LIMIT, change, change_built, change_steps, formatted};
use crate::agent::grants::Scope;
use serde_json::{Value, json};

fn done(text: String, data: Value) -> ToolOutput {
    ToolOutput { text, data }
}

pub fn draft_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "chat": chat_schema(),
            "text": {"type": "string", "description": "The draft the user sees in the message box. An empty string clears it."},
            "format": common::format_schema(),
            "reply_to": common::reply_schema(),
        },
        "required": ["chat", "text"],
        "additionalProperties": false,
    })
}

pub fn set_draft(args: &Value) -> Result<ToolOutput, ToolError> {
    let text = args
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let ctx = Ctx::active()?;
    let target = require(&ctx, args, Scope::ReadWrite)?;
    let draft = if text.trim().is_empty() {
        Value::Null
    } else {
        json!({
            "@type": "draftMessage",
            "reply_to": common::reply_to(args)?,
            "date": 0,
            "content": {
                "@type": "draftMessageContentText",
                "text": formatted(&ctx, &text, args, TEXT_LIMIT)?,
                "link_preview_options": Value::Null,
            },
            "effect_id": "0",
            "suggested_post_info": Value::Null,
        })
    };
    let cleared = draft.is_null();
    let summary = if cleared {
        "Cleared the draft".to_string()
    } else {
        "Set the draft".to_string()
    };
    let (kind, detail) = if cleared {
        ("clear_draft", String::new())
    } else {
        (
            "set_draft",
            format!("\"{}\"", text.chars().take(60).collect::<String>()),
        )
    };
    change(
        &ctx,
        &target,
        kind,
        &detail,
        json!({"@type": "setChatDraftMessage", "chat_id": target.chat_id, "topic_id": Value::Null, "draft_message": draft}),
    )?;
    Ok(done(
        format!(
            "{summary} in \"{}\". The user sends it from Quill.",
            target.title
        ),
        json!({"chat": target.summary(), "draft": if cleared { Value::Null } else { Value::String(text) }}),
    ))
}

pub fn mark_read_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"chat": chat_schema()},
        "required": ["chat"],
        "additionalProperties": false,
    })
}

pub fn mark_read(args: &Value) -> Result<ToolOutput, ToolError> {
    let ctx = Ctx::active()?;
    let target = require(&ctx, args, Scope::ReadWrite)?;
    let chat = ctx.call(json!({"@type": "getChat", "chat_id": target.chat_id}))?;
    change_steps(
        &ctx,
        &target,
        "mark_read",
        "",
        mark_read_requests(&chat, target.chat_id),
    )?;
    Ok(done(
        format!("Marked \"{}\" as read.", target.title),
        json!({"chat": target.summary(), "read": true}),
    ))
}

/// What marking `chat` read takes: reading up to its last message, and
/// clearing the "marked as unread" mark.
fn mark_read_requests(chat: &Value, chat_id: i64) -> Vec<Value> {
    let last = chat
        .get("last_message")
        .and_then(|m| m.get("id"))
        .and_then(Value::as_i64);
    let marked = chat.get("is_marked_as_unread").and_then(Value::as_bool) == Some(true);
    let view = last.map(|last| {
        json!({
            "@type": "viewMessages",
            "chat_id": chat_id,
            "message_ids": [last],
            "source": {"@type": "messageSourceChatList"},
            "force_read": true,
        })
    });
    let unmark = marked.then(|| {
        json!({"@type": "toggleChatIsMarkedAsUnread", "chat_id": chat_id, "is_marked_as_unread": false})
    });
    view.into_iter().chain(unmark).collect()
}

pub fn mute_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "chat": chat_schema(),
            "minutes": {"type": "integer", "minimum": 0, "description": "Mute for this many minutes. 0 unmutes. Leave out to mute until the user unmutes."},
        },
        "required": ["chat"],
        "additionalProperties": false,
    })
}

pub fn mute_chat(args: &Value) -> Result<ToolOutput, ToolError> {
    let minutes = match args.get("minutes") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            value
                .as_i64()
                .filter(|m| *m >= 0)
                .ok_or_else(|| ToolError::new("minutes must be a whole number, 0 or more."))?,
        ),
    };
    let mute_for: i64 = match minutes {
        None => i64::from(i32::MAX),
        Some(m) => m.saturating_mul(60).min(i64::from(i32::MAX)),
    };
    let ctx = Ctx::active()?;
    let target = require(&ctx, args, Scope::ReadWrite)?;
    let summary = match minutes {
        Some(0) => "Unmuted".to_string(),
        Some(m) => format!("Muted for {m} minutes"),
        None => "Muted".to_string(),
    };
    let (kind, detail) = match minutes {
        Some(0) => ("unmute_chat", String::new()),
        Some(m) if m % 60 == 0 => ("mute_chat", format!("{} h", m / 60)),
        Some(m) => ("mute_chat", format!("{m} min")),
        None => ("mute_chat", "∞".to_string()),
    };
    // TDLib takes the whole settings object, so it is read at the write's
    // turn and only `mute_for` changes: the user's sound or preview change
    // made while the write waited stays.
    change_built(&ctx, &target, kind, &detail, || {
        let chat = ctx.call(json!({"@type": "getChat", "chat_id": target.chat_id}))?;
        let mut settings = chat
            .get("notification_settings")
            .cloned()
            .filter(Value::is_object)
            .ok_or_else(|| {
                ToolError::new("Quill could not read this chat's notification settings. Retry.")
            })?;
        with_mute(&mut settings, mute_for);
        Ok(
            json!({"@type": "setChatNotificationSettings", "chat_id": target.chat_id, "notification_settings": settings}),
        )
    })?;
    Ok(done(
        format!("{summary} \"{}\".", target.title),
        json!({"chat": target.summary(), "mute_for_seconds": mute_for}),
    ))
}

/// `settings` with only the mute changed.
fn with_mute(settings: &mut Value, mute_for: i64) {
    if let Some(fields) = settings.as_object_mut() {
        fields.insert("use_default_mute_for".into(), json!(false));
        fields.insert("mute_for".into(), json!(mute_for));
    }
}

pub fn archive_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "chat": chat_schema(),
            "archive": {"type": "boolean", "default": true, "description": "false moves it back to the main list."},
        },
        "required": ["chat"],
        "additionalProperties": false,
    })
}

pub fn archive_chat(args: &Value) -> Result<ToolOutput, ToolError> {
    let archive = args.get("archive").and_then(Value::as_bool).unwrap_or(true);
    let ctx = Ctx::active()?;
    let target = require(&ctx, args, Scope::ReadWrite)?;
    let list = if archive {
        "chatListArchive"
    } else {
        "chatListMain"
    };
    let summary = if archive { "Archived" } else { "Unarchived" };
    change(
        &ctx,
        &target,
        if archive {
            "archive_chat"
        } else {
            "unarchive_chat"
        },
        "",
        json!({"@type": "addChatToList", "chat_id": target.chat_id, "chat_list": {"@type": list}}),
    )?;
    Ok(done(
        format!("{summary} \"{}\".", target.title),
        json!({"chat": target.summary(), "archived": archive}),
    ))
}

pub fn folder_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "chat": chat_schema(),
            "folder": {"type": ["string", "integer"], "description": "A folder name or id from list_folders."},
            "remove": {"type": "boolean", "default": false, "description": "Take the chat out of the folder instead."},
        },
        "required": ["chat", "folder"],
        "additionalProperties": false,
    })
}

/// The folder id the `folder` argument names.
fn folder_id(ctx: &Ctx, wanted: Option<&Value>) -> Result<(i64, String), ToolError> {
    let folders = crate::agent::events::folders(ctx.client.id());
    let found = match wanted {
        Some(Value::Number(n)) => folders.iter().find(|(id, _)| Some(*id) == n.as_i64()),
        Some(Value::String(name)) => {
            let name = name.trim().to_lowercase();
            folders
                .iter()
                .find(|(id, title)| title.to_lowercase() == name || id.to_string() == name)
        }
        _ => None,
    };
    found
        .cloned()
        .ok_or_else(|| ToolError::new("No such folder. Call list_folders for the names and ids."))
}

/// How a folder's lists hold one chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Membership {
    Pinned,
    Included,
    Excluded,
    /// In no list: in the folder only if its rules (contacts, groups…) say so.
    None,
}

impl Membership {
    fn in_list(folder: &Value, key: &str, chat_id: i64) -> bool {
        folder
            .get(key)
            .and_then(Value::as_array)
            .is_some_and(|list| list.iter().any(|v| v.as_i64() == Some(chat_id)))
    }

    fn of(folder: &Value, chat_id: i64) -> Self {
        if Self::in_list(folder, "pinned_chat_ids", chat_id) {
            Membership::Pinned
        } else if Self::in_list(folder, "included_chat_ids", chat_id) {
            Membership::Included
        } else if Self::in_list(folder, "excluded_chat_ids", chat_id) {
            Membership::Excluded
        } else {
            Membership::None
        }
    }
}

/// Put `chat_id` in or out of `folder`'s lists as Telegram Desktop does: a
/// chat the lists name is taken off them, and only a chat that is in by the
/// folder's rules alone is excluded (an exclusion spends the folder's limit).
/// A pinned chat is already in; adding it again would double-list it.
fn edit_membership(folder: &mut Value, chat_id: i64, remove: bool) {
    let held = Membership::of(folder, chat_id);
    let edit = |folder: &mut Value, key: &str, keep: bool| {
        if let Some(list) = folder.get_mut(key).and_then(Value::as_array_mut) {
            list.retain(|v| v.as_i64() != Some(chat_id));
            if keep {
                list.push(json!(chat_id));
            }
        }
    };
    match (remove, held) {
        (true, Membership::Pinned | Membership::Included) => {
            edit(folder, "pinned_chat_ids", false);
            edit(folder, "included_chat_ids", false);
        }
        (true, Membership::None) => edit(folder, "excluded_chat_ids", true),
        (true, Membership::Excluded) | (false, Membership::Pinned | Membership::Included) => {}
        (false, Membership::Excluded | Membership::None) => {
            edit(folder, "excluded_chat_ids", false);
            edit(folder, "included_chat_ids", true);
        }
    }
}

pub fn set_chat_folder(args: &Value) -> Result<ToolOutput, ToolError> {
    let remove = args.get("remove").and_then(Value::as_bool).unwrap_or(false);
    let ctx = Ctx::active()?;
    let target = require(&ctx, args, Scope::ReadWrite)?;
    let (id, name) = folder_id(&ctx, args.get("folder"))?;
    let chat_id = target.chat_id;
    let summary = if remove {
        format!("Removed from folder \"{name}\"")
    } else {
        format!("Added to folder \"{name}\"")
    };
    change_built(
        &ctx,
        &target,
        if remove {
            "remove_from_folder"
        } else {
            "set_chat_folder"
        },
        &format!("\"{name}\""),
        || {
            let mut folder = ctx.call(json!({"@type": "getChatFolder", "chat_folder_id": id}))?;
            edit_membership(&mut folder, chat_id, remove);
            Ok(json!({"@type": "editChatFolder", "chat_folder_id": id, "folder": folder}))
        },
    )
    .map_err(|error| {
        if error.0.contains("FILTER_INCLUDE_TOO_MUCH") {
            ToolError::new(format!(
                "Folder \"{name}\" is full (Telegram's limit on chats per folder)."
            ))
        } else {
            error
        }
    })?;
    Ok(done(
        format!("{summary}: \"{}\".", target.title),
        json!({"chat": target.summary(), "folder": {"id": id, "name": name}, "in_folder": !remove}),
    ))
}

#[cfg(test)]
mod tests {
    use super::{edit_membership, mark_read_requests};
    use serde_json::{Value, json};

    fn types(requests: &[Value]) -> Vec<&str> {
        requests
            .iter()
            .filter_map(|request| request.get("@type").and_then(Value::as_str))
            .collect()
    }

    /// Both steps of marking a chat read go out in the one queue turn.
    #[test]
    fn mark_read_is_one_turn_of_up_to_two_requests() {
        let chat = json!({"last_message": {"id": 9}, "is_marked_as_unread": true});
        assert_eq!(
            types(&mark_read_requests(&chat, 4)),
            ["viewMessages", "toggleChatIsMarkedAsUnread"]
        );
        let read = json!({"last_message": {"id": 9}, "is_marked_as_unread": false});
        assert_eq!(types(&mark_read_requests(&read, 4)), ["viewMessages"]);
        assert!(mark_read_requests(&json!({}), 4).is_empty());
    }

    fn folder() -> Value {
        json!({"pinned_chat_ids": [1], "included_chat_ids": [2], "excluded_chat_ids": [3]})
    }

    fn after(chat: i64, remove: bool) -> Value {
        let mut folder = folder();
        edit_membership(&mut folder, chat, remove);
        folder
    }

    #[test]
    fn removing_an_included_chat_does_not_also_exclude_it() {
        let folder = after(2, true);
        assert_eq!(folder["included_chat_ids"], json!([]));
        assert_eq!(folder["excluded_chat_ids"], json!([3]));
        assert_eq!(after(1, true)["pinned_chat_ids"], json!([]));
        assert_eq!(after(1, true)["excluded_chat_ids"], json!([3]));
        assert_eq!(after(9, true)["excluded_chat_ids"], json!([3, 9]));
        assert_eq!(after(3, true), folder_unchanged());
    }

    fn folder_unchanged() -> Value {
        folder()
    }

    #[test]
    fn adding_a_pinned_or_included_chat_changes_nothing() {
        assert_eq!(after(1, false), folder());
        assert_eq!(after(2, false), folder());
        let added = after(9, false);
        assert_eq!(added["included_chat_ids"], json!([2, 9]));
        let readded = after(3, false);
        assert_eq!(readded["excluded_chat_ids"], json!([]));
        assert_eq!(readded["included_chat_ids"], json!([2, 3]));
    }
}
