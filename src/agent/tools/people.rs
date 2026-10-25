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
            "to": {"type": "string", "description": "An absolute folder (the file keeps its name, with (2) added if taken) or a new file path to copy the file to. Existing files are never overwritten. Without it you get the path in Quill's cache (read it, do not change it)."},
        },
        "required": ["chat", "message_id"],
        "additionalProperties": false,
    })
}

/// The sender's file name cut to one harmless path component: no folders,
/// no `..`, no drive letters or control characters.
fn safe_name(name: &str, fallback: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned: String = last
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').trim();
    if cleaned.is_empty() {
        fallback.to_string()
    } else {
        cleaned.chars().take(200).collect()
    }
}

/// Copy `from` to a path that does not exist yet: `to` itself when it is a
/// file path, or `name` inside `to` when it is a folder, with " (2)", " (3)"…
/// added on a clash. Never overwrites.
fn copy_new(from: &Path, to: &Path, name: &str) -> std::io::Result<PathBuf> {
    let candidates: Vec<PathBuf> = if to.is_dir() {
        let (stem, ext) = match name.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() => (stem.to_string(), format!(".{ext}")),
            _ => (name.to_string(), String::new()),
        };
        std::iter::once(to.join(name))
            .chain((2..100).map(|n| to.join(format!("{stem} ({n}){ext}"))))
            .collect()
    } else {
        vec![to.to_path_buf()]
    };
    let mut last_error =
        std::io::Error::new(std::io::ErrorKind::AlreadyExists, "the file already exists");
    for dest in candidates {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest)
        {
            Ok(mut out) => {
                let mut input = std::fs::File::open(from)?;
                std::io::copy(&mut input, &mut out)?;
                return Ok(dest);
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => last_error = err,
            Err(err) => return Err(err),
        }
    }
    Err(last_error)
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
    let fallback = format!("file-{message_id}");
    let name = safe_name(
        file.get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| local.file_name().map(|n| n.to_string_lossy().into_owned()))
            .as_deref()
            .unwrap_or_default(),
        &fallback,
    );
    let path = match to {
        Some(to) => copy_new(&local, &to, &name).map_err(|e| {
            ToolError::new(format!(
                "Could not copy the file to {} ({e}). Pick a folder you can write to, or a file path that does not exist yet.",
                to.display()
            ))
        })?,
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
    let (arrived, _) = crate::agent::events::wait_for(
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

#[cfg(test)]
mod tests {
    use super::{copy_new, safe_name};

    #[test]
    fn sender_file_names_cannot_leave_the_folder() {
        assert_eq!(safe_name("../.zshrc", "f"), "zshrc");
        assert_eq!(
            safe_name("/Users/me/.ssh/authorized_keys", "f"),
            "authorized_keys"
        );
        assert_eq!(safe_name("..\\..\\evil.bat", "f"), "evil.bat");
        assert_eq!(safe_name("C:x.txt", "f"), "Cx.txt");
        assert_eq!(safe_name("..", "f"), "f");
        assert_eq!(safe_name("report.pdf", "f"), "report.pdf");
    }

    #[test]
    fn copies_never_overwrite() {
        let dir = std::env::temp_dir().join(format!("quill-agent-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let made =
            std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(dir.join("src"), b"new"));
        assert!(made.is_ok());
        let first = copy_new(&dir.join("src"), &dir, "a.txt").ok();
        let second = copy_new(&dir.join("src"), &dir, "a.txt").ok();
        assert_eq!(first, Some(dir.join("a.txt")));
        assert_eq!(second, Some(dir.join("a (2).txt")));
        assert!(copy_new(&dir.join("src"), &dir.join("a.txt"), "x").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
