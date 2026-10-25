//! Tools that change messages already in a chat: edit, delete, react, pin.
//! Edits and deletes touch only the user's own messages.

use super::super::target::{Target, chat_schema, require};
use super::super::{Ctx, ToolError, ToolOutput, int_arg};
use super::common::{self, TEXT_LIMIT, change, confirm, formatted, message_ids, required_text};
use crate::agent::grants::Scope;
use serde_json::{Value, json};

fn message_id_schema(what: &str) -> Value {
    json!({"type": "integer", "description": what})
}

fn get_message(ctx: &Ctx, target: &Target, id: i64) -> Result<Value, ToolError> {
    ctx.call(json!({"@type": "getMessage", "chat_id": target.chat_id, "message_id": id}))
        .map_err(|_| {
            ToolError::new(format!(
                "There is no message {id} in \"{}\". Message ids come from list_messages or read_history.",
                target.title
            ))
        })
}

fn properties(ctx: &Ctx, target: &Target, id: i64) -> Value {
    ctx.call(json!({"@type": "getMessageProperties", "chat_id": target.chat_id, "message_id": id}))
        .unwrap_or(Value::Null)
}

fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn done(text: String, data: Value) -> ToolOutput {
    ToolOutput { text, data }
}

pub fn edit_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "chat": chat_schema(),
            "message_id": message_id_schema("One of the user's own messages."),
            "text": {"type": "string", "description": "The new text (or the new caption of a photo, video or file)."},
            "format": common::format_schema(),
        },
        "required": ["chat", "message_id", "text"],
        "additionalProperties": false,
    })
}

pub fn edit_message(args: &Value) -> Result<ToolOutput, ToolError> {
    let id = int_arg(args, "message_id", 0, 1, i64::MAX)?;
    let text = required_text(args, "text")?;
    let ctx = Ctx::active()?;
    let target = require(&ctx, args, Scope::ReadWrite)?;
    let message = get_message(&ctx, &target, id)?;
    if !flag(&message, "is_outgoing") || !flag(&properties(&ctx, &target, id), "can_be_edited") {
        return Err(ToolError::new(format!(
            "Message {id} cannot be edited: agents edit only the user's own messages, and Telegram allows edits for 48 hours in most chats."
        )));
    }
    let is_text = message
        .get("content")
        .and_then(|c| c.get("@type"))
        .and_then(Value::as_str)
        == Some("messageText");
    let body = formatted(
        &ctx,
        &text,
        args,
        if is_text {
            TEXT_LIMIT
        } else {
            common::CAPTION_LIMIT
        },
    )?;
    let request = if is_text {
        json!({
            "@type": "editMessageText",
            "chat_id": target.chat_id,
            "message_id": id,
            "reply_markup": Value::Null,
            "input_message_content": {"@type": "inputMessageText", "text": body, "link_preview_options": Value::Null, "clear_draft": false},
        })
    } else {
        json!({
            "@type": "editMessageCaption",
            "chat_id": target.chat_id,
            "message_id": id,
            "reply_markup": Value::Null,
            "caption": body,
            "show_caption_above_media": false,
        })
    };
    confirm(&ctx, &target, "edit_message", "Edit a message", &text)?;
    change(
        &ctx,
        &target,
        "edit_message",
        &format!("Edited message {id}"),
        request,
    )?;
    Ok(done(
        format!("Edited message {id} in \"{}\".", target.title),
        json!({"chat": target.summary(), "message_id": id, "edited": true}),
    ))
}

pub fn delete_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "chat": chat_schema(),
            "message_ids": common::ids_schema("The user's own messages to delete."),
            "for_everyone": {"type": "boolean", "default": true, "description": "Delete for everyone in the chat, not only for the user, where Telegram allows it."},
        },
        "required": ["chat", "message_ids"],
        "additionalProperties": false,
    })
}

pub fn delete_messages(args: &Value) -> Result<ToolOutput, ToolError> {
    let ids = message_ids(args, "message_ids")?;
    let for_everyone = args
        .get("for_everyone")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let ctx = Ctx::active()?;
    let target = require(&ctx, args, Scope::ReadWrite)?;
    let mut refused = Vec::new();
    let mut revoke = for_everyone;
    for id in &ids {
        let message = get_message(&ctx, &target, *id)?;
        let props = properties(&ctx, &target, *id);
        if !flag(&message, "is_outgoing")
            || !(flag(&props, "can_be_deleted_only_for_self")
                || flag(&props, "can_be_deleted_for_all_users"))
        {
            refused.push(id.to_string());
        } else if !flag(&props, "can_be_deleted_for_all_users") {
            revoke = false;
        }
    }
    if !refused.is_empty() {
        return Err(ToolError::new(format!(
            "Agents may delete only the user's own messages. These are not: {}. Nothing was deleted.",
            refused.join(", ")
        )));
    }
    confirm(
        &ctx,
        &target,
        "delete_messages",
        "Delete messages",
        &format!(
            "{} message(s): {}",
            ids.len(),
            ids.iter()
                .map(i64::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    )?;
    change(
        &ctx,
        &target,
        "delete_messages",
        &format!("Deleted {} message(s)", ids.len()),
        json!({"@type": "deleteMessages", "chat_id": target.chat_id, "message_ids": ids, "revoke": revoke}),
    )?;
    let whom = if revoke {
        "for everyone"
    } else {
        "for the user only"
    };
    Ok(done(
        format!(
            "Deleted {} message(s) in \"{}\" {whom}.",
            ids.len(),
            target.title
        ),
        json!({"chat": target.summary(), "deleted": ids, "for_everyone": revoke}),
    ))
}

pub fn react_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "chat": chat_schema(),
            "message_id": message_id_schema("The message to react to."),
            "emoji": {"type": "string", "description": "A reaction emoji such as 👍 ❤️ 🔥 🎉 😁."},
            "remove": {"type": "boolean", "default": false, "description": "Take the reaction back instead."},
        },
        "required": ["chat", "message_id", "emoji"],
        "additionalProperties": false,
    })
}

pub fn react(args: &Value) -> Result<ToolOutput, ToolError> {
    let id = int_arg(args, "message_id", 0, 1, i64::MAX)?;
    let emoji = required_text(args, "emoji")?;
    let remove = args.get("remove").and_then(Value::as_bool).unwrap_or(false);
    let ctx = Ctx::active()?;
    let target = require(&ctx, args, Scope::ReadWrite)?;
    get_message(&ctx, &target, id)?;
    let reaction = json!({"@type": "reactionTypeEmoji", "emoji": emoji.trim()});
    let request = if remove {
        json!({"@type": "removeMessageReaction", "chat_id": target.chat_id, "message_id": id, "reaction_type": reaction})
    } else {
        json!({"@type": "addMessageReaction", "chat_id": target.chat_id, "message_id": id, "reaction_type": reaction, "is_big": false, "update_recent_reactions": true})
    };
    let (action, summary) = if remove {
        (
            "Remove a reaction",
            format!("Removed {} from message {id}", emoji.trim()),
        )
    } else {
        (
            "React to a message",
            format!("Reacted {} to message {id}", emoji.trim()),
        )
    };
    confirm(&ctx, &target, "react", action, emoji.trim())?;
    change(&ctx, &target, "react", &summary, request).map_err(|error| {
        if error.0.contains("REACTION_INVALID") || error.0.contains("Invalid reaction") {
            ToolError::new(format!("{} is not an allowed reaction in this chat. get_chat_info does not list them; try 👍 or ❤️.", emoji.trim()))
        } else {
            error
        }
    })?;
    Ok(done(
        format!("{summary} in \"{}\".", target.title),
        json!({"chat": target.summary(), "message_id": id, "emoji": emoji.trim(), "removed": remove}),
    ))
}

pub fn pin_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "chat": chat_schema(),
            "message_id": message_id_schema("The message to pin or unpin."),
            "unpin": {"type": "boolean", "default": false},
            "silent": common::silent_schema(),
        },
        "required": ["chat", "message_id"],
        "additionalProperties": false,
    })
}

pub fn pin_message(args: &Value) -> Result<ToolOutput, ToolError> {
    let id = int_arg(args, "message_id", 0, 1, i64::MAX)?;
    let unpin = args.get("unpin").and_then(Value::as_bool).unwrap_or(false);
    let silent = args.get("silent").and_then(Value::as_bool).unwrap_or(false);
    let ctx = Ctx::active()?;
    let target = require(&ctx, args, Scope::ReadWrite)?;
    get_message(&ctx, &target, id)?;
    let request = if unpin {
        json!({"@type": "unpinChatMessage", "chat_id": target.chat_id, "message_id": id})
    } else {
        json!({"@type": "pinChatMessage", "chat_id": target.chat_id, "message_id": id, "disable_notification": silent, "only_for_self": false})
    };
    let verb = if unpin { "Unpinned" } else { "Pinned" };
    confirm(
        &ctx,
        &target,
        "pin_message",
        if unpin {
            "Unpin a message"
        } else {
            "Pin a message"
        },
        &format!("Message {id}"),
    )?;
    change(
        &ctx,
        &target,
        "pin_message",
        &format!("{verb} message {id}"),
        request,
    )?;
    Ok(done(
        format!("{verb} message {id} in \"{}\".", target.title),
        json!({"chat": target.summary(), "message_id": id, "pinned": !unpin}),
    ))
}
