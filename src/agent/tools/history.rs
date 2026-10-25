//! The message read tools, for any chat the agent was granted.

use super::messages::{self, Detail, Names};
use super::target::{Target, chat_schema, require};
use super::{Ctx, ToolError, ToolOutput, int_arg};
use crate::agent::grants::{ChatClass, Scope};
use serde_json::{Value, json};
use std::collections::HashSet;

/// TDLib returns at most this many messages per history request.
const TD_PAGE: i64 = 100;
/// Extra history requests made to fill a page TDLib answered short.
const FILL_ROUNDS: usize = 5;

fn id_of(message: &Value) -> i64 {
    message.get("id").and_then(Value::as_i64).unwrap_or(0)
}

/// Up to `limit` messages older than `before` (newest first); `before == 0`
/// starts at the latest message, which is then included.
fn older(ctx: &Ctx, chat_id: i64, before: i64, limit: i64) -> Result<Vec<Value>, ToolError> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut from = before;
    for _ in 0..=FILL_ROUNDS {
        let want = (limit - out.len() as i64).clamp(1, TD_PAGE);
        let answer = ctx.call(json!({
            "@type": "getChatHistory",
            "chat_id": chat_id,
            "from_message_id": from,
            "offset": 0,
            "limit": want,
            "only_local": false,
        }))?;
        let mut added = false;
        for message in messages::messages_of(&answer) {
            let id = id_of(&message);
            if (before != 0 && id >= before) || !seen.insert(id) {
                continue;
            }
            added = true;
            out.push(message);
        }
        if !added || out.len() as i64 >= limit {
            break;
        }
        from = out.last().map(id_of).unwrap_or(0);
    }
    out.truncate(usize::try_from(limit).unwrap_or(0));
    Ok(out)
}

/// Up to `limit` messages newer than `after` (newest first).
fn newer(ctx: &Ctx, chat_id: i64, after: i64, limit: i64) -> Result<Vec<Value>, ToolError> {
    let limit = limit.clamp(1, TD_PAGE - 1);
    let answer = ctx.call(json!({
        "@type": "getChatHistory",
        "chat_id": chat_id,
        "from_message_id": after,
        "offset": -limit,
        "limit": limit + 1,
        "only_local": false,
    }))?;
    Ok(messages::messages_of(&answer)
        .into_iter()
        .filter(|message| id_of(message) > after)
        .collect())
}

fn shaped_page(ctx: &Ctx, target: &Target, mut page: Vec<Value>, detail: Detail) -> Vec<Value> {
    page.sort_by_key(id_of);
    let mut names = Names::new(ctx);
    page.iter()
        .map(|message| messages::shape(message, target.me, detail, &mut names))
        .collect()
}

fn page_text(title: &str, shaped: &[Value], footer: Option<String>) -> String {
    let mut text = format!("{title}: {} message(s), oldest first.\n", shaped.len());
    for message in shaped {
        text.push_str(&messages::line(message));
        text.push('\n');
    }
    if let Some(footer) = footer {
        text.push_str(&footer);
    }
    text
}

pub fn list_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "chat": chat_schema(),
            "limit": {"type": "integer", "minimum": 1, "maximum": 100, "default": 20},
        },
        "additionalProperties": false,
    })
}

pub fn list_messages(args: &Value) -> Result<ToolOutput, ToolError> {
    let ctx = Ctx::active()?;
    let target = require(&ctx, args, Scope::Read)?;
    let limit = int_arg(args, "limit", 20, 1, 100)?;
    let page = older(&ctx, target.chat_id, 0, limit)?;
    let shaped = shaped_page(&ctx, &target, page, Detail::Summary);
    let older_cursor = shaped.first().and_then(|m| m.get("id")).cloned();
    let footer = older_cursor.as_ref().map(|id| {
        format!(
            "Older: read_history {{\"chat\": {}, \"before_message_id\": {id}}}",
            target.chat_ref()
        )
    });
    Ok(ToolOutput {
        text: page_text(&target.title, &shaped, footer),
        data: json!({
            "chat": target.summary(),
            "messages": shaped,
            "older_cursor": older_cursor,
        }),
    })
}

pub fn history_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "chat": chat_schema(),
            "before_message_id": {"type": "integer", "description": "Read messages older than this id."},
            "after_message_id": {"type": "integer", "description": "Read messages newer than this id."},
            "limit": {"type": "integer", "minimum": 1, "maximum": 99, "default": 50},
        },
        "additionalProperties": false,
    })
}

pub fn read_history(args: &Value) -> Result<ToolOutput, ToolError> {
    let before = int_arg(args, "before_message_id", 0, 0, i64::MAX)?;
    let after = int_arg(args, "after_message_id", 0, 0, i64::MAX)?;
    if before != 0 && after != 0 {
        return Err(ToolError::new(
            "Pass before_message_id or after_message_id, not both.",
        ));
    }
    let ctx = Ctx::active()?;
    let target = require(&ctx, args, Scope::Read)?;
    let limit = int_arg(args, "limit", 50, 1, 99)?;
    let page = if after != 0 {
        newer(&ctx, target.chat_id, after, limit)?
    } else {
        older(&ctx, target.chat_id, before, limit)?
    };
    let full = page.len() as i64 >= limit;
    let shaped = shaped_page(&ctx, &target, page, Detail::Full);
    let first = shaped.first().and_then(|m| m.get("id")).cloned();
    let last = shaped.last().and_then(|m| m.get("id")).cloned();
    // Paging back from the latest message has nothing newer; paging forward
    // that came back short has reached it.
    let older_cursor = if after != 0 || full { first } else { None };
    let newer_cursor = if before != 0 || (after != 0 && full) {
        last
    } else {
        None
    };
    let mut footer = Vec::new();
    if let Some(id) = &older_cursor {
        footer.push(format!("Older: before_message_id {id}"));
    }
    if let Some(id) = &newer_cursor {
        footer.push(format!("Newer: after_message_id {id}"));
    }
    if shaped.is_empty() {
        footer.push("No messages in that direction.".to_string());
    }
    Ok(ToolOutput {
        text: page_text(&target.title, &shaped, Some(footer.join(" · "))),
        data: json!({
            "chat": target.summary(),
            "messages": shaped,
            "older_cursor": older_cursor,
            "newer_cursor": newer_cursor,
        }),
    })
}

pub fn search_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "chat": chat_schema(),
            "query": {"type": "string", "minLength": 1},
            "before_message_id": {"type": "integer", "description": "In one chat: the next_cursor of a previous search."},
            "cursor": {"type": "string", "description": "Across chats: the next_cursor of a previous search."},
            "limit": {"type": "integer", "minimum": 1, "maximum": 100, "default": 20},
        },
        "required": ["query"],
        "additionalProperties": false,
    })
}

pub fn search_messages(args: &Value) -> Result<ToolOutput, ToolError> {
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .ok_or_else(|| {
            ToolError::new("query is required, e.g. {\"chat\": \"saved\", \"query\": \"invoice\"}.")
        })?;
    let ctx = Ctx::active()?;
    let limit = int_arg(args, "limit", 20, 1, 100)?;
    if args.get("chat").is_none_or(Value::is_null) {
        return search_everywhere(&ctx, query, args, limit);
    }
    let target = require(&ctx, args, Scope::Read)?;
    let from = int_arg(args, "before_message_id", 0, 0, i64::MAX)?;
    let answer = if target.class == ChatClass::Saved {
        ctx.call(json!({
            "@type": "searchSavedMessages",
            "saved_messages_topic_id": 0,
            "tag": null,
            "query": query,
            "from_message_id": from,
            "offset": 0,
            "limit": limit,
        }))?
    } else {
        ctx.call(json!({
            "@type": "searchChatMessages",
            "chat_id": target.chat_id,
            "topic_id": null,
            "query": query,
            "sender_id": null,
            "from_message_id": from,
            "offset": 0,
            "limit": limit,
            "filter": null,
        }))?
    };
    let total = answer
        .get("total_count")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let found = messages::messages_of(&answer);
    // TDLib names a next page even after the last match.
    let next = answer
        .get("next_from_message_id")
        .and_then(Value::as_i64)
        .filter(|id| *id != 0 && found.len() as i64 >= limit);
    let mut names = Names::new(&ctx);
    let shaped: Vec<Value> = found
        .iter()
        .map(|message| messages::shape(message, target.me, Detail::Summary, &mut names))
        .collect();
    let mut text = format!(
        "{}: {} match(es) for \"{query}\" (of {total}), newest first.\n",
        target.title,
        shaped.len()
    );
    for message in &shaped {
        text.push_str(&messages::line(message));
        text.push('\n');
    }
    match next {
        Some(id) => text.push_str(&format!("More: before_message_id {id}")),
        None if shaped.is_empty() => text.push_str("Nothing found. Try fewer or different words."),
        None => {}
    }
    Ok(ToolOutput {
        text,
        data: json!({
            "chat": target.summary(),
            "total_count": total,
            "messages": shaped,
            "next_cursor": next,
        }),
    })
}

/// Search every chat, keeping only matches in granted chats.
fn search_everywhere(
    ctx: &Ctx,
    query: &str,
    args: &Value,
    limit: i64,
) -> Result<ToolOutput, ToolError> {
    let me = super::target::my_user_id(ctx)?;
    let offset = args
        .get("cursor")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let answer = ctx.call(json!({
        "@type": "searchMessages",
        "chat_list": null,
        "query": query,
        "offset": offset,
        "limit": limit,
        "filter": null,
        "chat_type_filter": null,
        "min_date": 0,
        "max_date": 0,
    }))?;
    let next = answer
        .get("next_offset")
        .and_then(Value::as_str)
        .filter(|next| !next.is_empty())
        .map(str::to_string);
    let mut chats: std::collections::HashMap<i64, Option<Target>> = Default::default();
    let mut names = Names::new(ctx);
    let mut shaped = Vec::new();
    for message in messages::messages_of(&answer) {
        let chat_id = message.get("chat_id").and_then(Value::as_i64).unwrap_or(0);
        if let std::collections::hash_map::Entry::Vacant(slot) = chats.entry(chat_id) {
            slot.insert(super::target::granted(ctx, chat_id, me)?);
        }
        let Some(Some(target)) = chats.get(&chat_id) else {
            continue;
        };
        let mut one = messages::shape(&message, me, Detail::Summary, &mut names);
        if let Some(fields) = one.as_object_mut() {
            fields.insert("chat".into(), target.summary());
        }
        shaped.push(one);
    }
    let mut text = format!(
        "{} match(es) for \"{query}\" in granted chats, newest first.\n",
        shaped.len()
    );
    for message in &shaped {
        let title = message
            .get("chat")
            .and_then(|c| c.get("title"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        text.push_str(&format!("{title} · {}\n", messages::line(message)));
    }
    match &next {
        Some(cursor) => text.push_str(&format!("More: pass \"cursor\": \"{cursor}\"")),
        None if shaped.is_empty() => text.push_str("Nothing found. Try fewer or different words."),
        None => {}
    }
    Ok(ToolOutput {
        text,
        data: json!({
            "messages": shaped,
            "next_cursor": next,
        }),
    })
}

pub fn get_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "chat": chat_schema(),
            "message_id": {"type": "integer"},
        },
        "required": ["message_id"],
        "additionalProperties": false,
    })
}

pub fn get_message(args: &Value) -> Result<ToolOutput, ToolError> {
    let message_id = int_arg(args, "message_id", 0, 0, i64::MAX)?;
    if message_id == 0 {
        return Err(ToolError::new(
            "message_id is required. Get ids from list_messages, read_history or search_messages.",
        ));
    }
    let ctx = Ctx::active()?;
    let target = require(&ctx, args, Scope::Read)?;
    let message = ctx
        .call(json!({"@type": "getMessage", "chat_id": target.chat_id, "message_id": message_id}))
        .map_err(|error| match error {
            ToolError(text) if text.contains("error 404") => ToolError::new(format!(
                "There is no message {message_id} in {}. Get ids from list_messages or search_messages.",
                target.title
            )),
            other => other,
        })?;
    let mut names = Names::new(&ctx);
    let shaped = messages::shape(&message, target.me, Detail::Full, &mut names);
    Ok(ToolOutput {
        text: messages::line(&shaped),
        data: shaped,
    })
}
