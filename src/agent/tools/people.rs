//! `list_contacts`, `download_file` and `wait_for_message`.

use super::messages::{self, Detail, Names, user_name};
use super::target::{chat_schema, granted, my_user_id, require};
use super::{Ctx, ToolError, ToolOutput, int_arg};
use crate::agent::grants::{self, Scope};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn contacts_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "query": {"type": "string", "description": "Find contacts by name or username."},
            "limit": {"type": "integer", "minimum": 1, "maximum": 500, "default": 100},
        },
        "additionalProperties": false,
    })
}

pub fn list_contacts(args: &Value) -> Result<ToolOutput, ToolError> {
    let ctx = Ctx::active()?;
    if !grants::for_account(&ctx.account).contacts {
        return Err(ToolError::new(
            "The contact list is not granted. Call request_access {\"contacts\": true, \"reason\": \"<why>\"} to ask the user.",
        ));
    }
    let limit = int_arg(args, "limit", 100, 1, 500)?;
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|q| !q.is_empty());
    let answer = match query {
        Some(q) => ctx.call(json!({"@type": "searchContacts", "query": q, "limit": limit}))?,
        None => ctx.call(json!({"@type": "getContacts"}))?,
    };
    let ids: Vec<i64> = answer
        .get("user_ids")
        .and_then(Value::as_array)
        .map(|ids| ids.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default();
    let mut contacts = Vec::new();
    for id in ids.into_iter().take(usize::try_from(limit).unwrap_or(100)) {
        let user = ctx.call(json!({"@type": "getUser", "user_id": id}))?;
        let username = user
            .get("usernames")
            .and_then(|u| u.get("active_usernames"))
            .and_then(Value::as_array)
            .and_then(|list| list.first())
            .and_then(Value::as_str)
            .map(|u| format!("@{u}"));
        // Phone numbers stay out: agents get names to find chats, nothing more.
        contacts.push(json!({"user_id": id, "name": user_name(&user), "username": username}));
    }
    let mut text = format!("{} contact(s).\n", contacts.len());
    for contact in &contacts {
        text.push_str(&format!(
            "{} {}{}\n",
            contact.get("user_id").cloned().unwrap_or(Value::Null),
            contact
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            contact
                .get("username")
                .and_then(Value::as_str)
                .map(|u| format!(" {u}"))
                .unwrap_or_default()
        ));
    }
    text.push_str(
        "Their private chats still need a grant: pass a user's name as \"chat\" to other tools.",
    );
    Ok(ToolOutput {
        text,
        data: json!({"contacts": contacts}),
    })
}

/// Longest a download may take.
const DOWNLOAD_WAIT: Duration = Duration::from_secs(300);

pub fn download_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "chat": chat_schema(),
            "message_id": {"type": "integer"},
            "to": {"type": "string", "description": "An absolute folder or file path to copy the file to. Without it you get the path in Quill's cache (read it, do not change it)."},
        },
        "required": ["chat", "message_id"],
        "additionalProperties": false,
    })
}

/// Where to copy a downloaded file: `to` as a file, or `name` inside `to`
/// when it is a folder.
fn destination(to: &Path, name: &str) -> PathBuf {
    if to.is_dir() {
        to.join(name)
    } else {
        to.to_path_buf()
    }
}

pub fn download_file(args: &Value) -> Result<ToolOutput, ToolError> {
    let message_id = int_arg(args, "message_id", 0, 0, i64::MAX)?;
    let to = args
        .get("to")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(PathBuf::from);
    if to.as_ref().is_some_and(|t| !t.is_absolute()) {
        return Err(ToolError::new("to must be an absolute path."));
    }
    let ctx = Ctx::active()?;
    let target = require(&ctx, args, Scope::Read)?;
    let message = ctx.call(
        json!({"@type": "getMessage", "chat_id": target.chat_id, "message_id": message_id}),
    )?;
    let mut names = Names::new(&ctx);
    let shaped = messages::shape(&message, target.me, Detail::Full, &mut names);
    let file = shaped.get("file").cloned().unwrap_or(Value::Null);
    let file_id = file.get("file_id").and_then(Value::as_i64).ok_or_else(|| {
        ToolError::new(format!(
            "Message {message_id} has no file. get_message shows what it holds."
        ))
    })?;
    let downloaded = crate::agent::td::call_waiting(
        &ctx.client,
        json!({"@type": "downloadFile", "file_id": file_id, "priority": 16, "offset": 0, "limit": 0, "synchronous": true}),
        DOWNLOAD_WAIT,
    )
    .map_err(|error| match error {
        crate::agent::td::TdError::Timeout => ToolError::new(
            "The download did not finish within 5 minutes. Retry; it continues where it stopped.",
        ),
        other => ToolError::from(other),
    })?;
    let local = downloaded
        .get("local")
        .and_then(|l| l.get("path"))
        .and_then(Value::as_str)
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| ToolError::new("Telegram did not deliver the file. Retry in a moment."))?;
    let name = file
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| local.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| format!("file-{message_id}"));
    let path = match to {
        Some(to) => {
            let dest = destination(&to, &name);
            std::fs::copy(&local, &dest).map_err(|e| {
                ToolError::new(format!(
                    "Could not copy the file to {} ({e}). Pick a folder you can write to.",
                    dest.display()
                ))
            })?;
            dest
        }
        None => local,
    };
    Ok(ToolOutput {
        text: format!("Saved {name} at {}", path.display()),
        data: json!({"path": path.to_string_lossy(), "name": name, "size": file.get("size")}),
    })
}

/// Longest an agent may wait in one call.
const MAX_WAIT_SECONDS: i64 = 300;

pub fn wait_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "chat": chat_schema(),
            "timeout_seconds": {"type": "integer", "minimum": 1, "maximum": MAX_WAIT_SECONDS, "default": 60},
            "include_own": {"type": "boolean", "description": "Also return messages the user sends."},
        },
        "additionalProperties": false,
    })
}

pub fn wait_for_message(args: &Value) -> Result<ToolOutput, ToolError> {
    let ctx = Ctx::active()?;
    let since = crate::agent::events::latest_seq();
    let timeout = int_arg(args, "timeout_seconds", 60, 1, MAX_WAIT_SECONDS)?;
    let include_own = args.get("include_own").and_then(Value::as_bool) == Some(true);
    let only = match args.get("chat").filter(|c| !c.is_null()) {
        Some(_) => Some(require(&ctx, args, Scope::Read)?),
        None => None,
    };
    let me = my_user_id(&ctx)?;
    let mut checked = std::collections::HashMap::new();
    let arrived = crate::agent::events::wait_for(
        ctx.client.id(),
        since,
        Duration::from_secs(u64::try_from(timeout).unwrap_or(60)),
        |new| {
            if new.outgoing && !include_own {
                return false;
            }
            if let Some(only) = &only {
                return new.chat_id == only.chat_id;
            }
            *checked
                .entry(new.chat_id)
                .or_insert_with(|| granted(&ctx, new.chat_id, me).ok().flatten().is_some())
        },
    );
    if arrived.is_empty() {
        return Ok(ToolOutput {
            text: format!("No new message in {timeout} seconds. Call again to keep waiting."),
            data: json!({"messages": []}),
        });
    }
    let mut names = Names::new(&ctx);
    let mut shaped = Vec::new();
    let mut text = String::new();
    for new in &arrived {
        let Ok(Some(target)) = granted(&ctx, new.chat_id, me) else {
            continue;
        };
        let mut one = messages::shape(&new.message, me, Detail::Full, &mut names);
        if let Some(fields) = one.as_object_mut() {
            fields.insert("chat".into(), target.summary());
        }
        text.push_str(&format!("{} · {}\n", target.title, messages::line(&one)));
        shaped.push(one);
    }
    Ok(ToolOutput {
        text,
        data: json!({"messages": shaped}),
    })
}
