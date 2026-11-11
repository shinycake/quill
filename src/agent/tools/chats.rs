//! Chat-level read tools: the chat list, folders, one chat's details, its
//! members, and what is unread. Only granted chats ever appear.

use super::messages::{self, Detail, Names, user_name};
use super::target::{Target, chat_schema, granted, my_user_id, require};
use super::{Ctx, ToolError, ToolOutput, int_arg};
use crate::agent::grants::{ChatClass, Scope};
use crate::agent::td::{self, TdError};
use serde_json::{Value, json};

/// Chats looked at per list request.
const SCAN: i64 = 200;

fn kind_word(chat: &Value, class: ChatClass) -> &'static str {
    match class {
        ChatClass::Saved => return "saved",
        ChatClass::Bot => return "bot",
        ChatClass::Secret => return "secret",
        ChatClass::OwnChannel => return "channel",
        ChatClass::Other => {}
    }
    let kind = chat.get("type");
    match kind.and_then(|t| t.get("@type")).and_then(Value::as_str) {
        Some("chatTypePrivate") => "private",
        Some("chatTypeBasicGroup") => "group",
        Some("chatTypeSupergroup")
            if kind
                .and_then(|t| t.get("is_channel"))
                .and_then(Value::as_bool)
                == Some(true) =>
        {
            "channel"
        }
        Some("chatTypeSupergroup") => "group",
        _ => "chat",
    }
}

/// The chat list `args` names: main (default), archive, or a folder by
/// name or id.
fn chat_list(ctx: &Ctx, args: &Value) -> Result<(Value, String), ToolError> {
    if args.get("archived").and_then(Value::as_bool) == Some(true) {
        return Ok((json!({"@type": "chatListArchive"}), "Archive".into()));
    }
    let Some(folder) = args.get("folder").filter(|f| !f.is_null()) else {
        return Ok((json!({"@type": "chatListMain"}), "All chats".into()));
    };
    let folders = crate::agent::events::folders(ctx.client.id());
    let wanted = match folder {
        Value::Number(n) => n
            .as_i64()
            .and_then(|id| folders.iter().find(|(f, _)| *f == id)),
        Value::String(name) => folders
            .iter()
            .find(|(_, n)| n.to_lowercase() == name.trim().to_lowercase()),
        _ => None,
    };
    let (id, name) = wanted.cloned().ok_or_else(|| {
        let names: Vec<&str> = folders.iter().map(|(_, n)| n.as_str()).collect();
        ToolError::new(if names.is_empty() {
            "This account has no chat folders. Leave folder out to list all chats.".to_string()
        } else {
            format!(
                "No folder is called that. The folders are: {}.",
                names.join(", ")
            )
        })
    })?;
    Ok((
        json!({"@type": "chatListFolder", "chat_folder_id": id}),
        name,
    ))
}

fn last_message_line(chat: &Value) -> Option<String> {
    let message = chat.get("last_message")?;
    let date = message.get("date").and_then(Value::as_i64).unwrap_or(0);
    let text = messages::preview(message);
    Some(format!("{} {}", messages::iso_time(date), text))
}

fn chat_entry(chat: &Value, target: &Target) -> Value {
    json!({
        "id": target.chat_id,
        "chat_ref": target.chat_ref(),
        "title": target.title,
        "kind": kind_word(chat, target.class),
        "scope": target.scope.as_str(),
        "unread": chat.get("unread_count").and_then(Value::as_i64).unwrap_or(0),
        "mentions": chat.get("unread_mention_count").and_then(Value::as_i64).unwrap_or(0),
        "last_message": last_message_line(chat),
    })
}

/// The most chats one call looks through; past it the answer says it
/// stopped.
const MAX_SCAN: i64 = 2000;

/// How a scan of a chat list ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scanned {
    /// The visitor had enough, or every chat of the list was looked at.
    Done,
    /// [`MAX_SCAN`] chats were looked at and the list goes on.
    Capped,
}

/// What to ask TDLib next after `got` chat ids came back for `scan`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Next {
    /// The page was full: ask for a longer one.
    Widen(i64),
    /// Fewer than asked: TDLib may not have loaded the rest yet.
    Load,
    /// The page was full and the cap is reached.
    Capped,
}

fn next_step(scan: i64, got: usize) -> Next {
    if i64::try_from(got).unwrap_or(i64::MAX) < scan {
        Next::Load
    } else if scan >= MAX_SCAN {
        Next::Capped
    } else {
        Next::Widen((scan + SCAN).min(MAX_SCAN))
    }
}

/// Walk the granted chats of a chat list in its order, with their TDLib
/// chat, loading further pages until `visit` returns false, the list ends or
/// [`MAX_SCAN`] chats were looked at.
fn scan_granted(
    ctx: &Ctx,
    list: &Value,
    mut visit: impl FnMut(Value, Target) -> Result<bool, ToolError>,
) -> Result<Scanned, ToolError> {
    let me = my_user_id(ctx)?;
    let mut scan = SCAN;
    let mut seen = 0usize;
    loop {
        let answer = ctx.call(json!({"@type": "getChats", "chat_list": list, "limit": scan}))?;
        let ids: Vec<i64> = answer
            .get("chat_ids")
            .and_then(Value::as_array)
            .map(|ids| ids.iter().filter_map(Value::as_i64).collect())
            .unwrap_or_default();
        for id in ids.iter().skip(seen) {
            let Some(target) = granted(ctx, *id, me)? else {
                continue;
            };
            let chat = ctx.call(json!({"@type": "getChat", "chat_id": id}))?;
            if !visit(chat, target)? {
                return Ok(Scanned::Done);
            }
        }
        seen = ids.len();
        match next_step(scan, ids.len()) {
            Next::Widen(wider) => scan = wider,
            Next::Capped => return Ok(Scanned::Capped),
            Next::Load => {
                let loaded = td::call(
                    &ctx.client,
                    json!({"@type": "loadChats", "chat_list": list, "limit": SCAN}),
                );
                match loaded {
                    Ok(_) => {}
                    Err(TdError::Td { code: 404, .. }) => return Ok(Scanned::Done),
                    Err(err) => return Err(ToolError::from(err)),
                }
            }
        }
    }
}

const CAPPED_NOTE: &str = "Only the first 2000 chats of this list were looked at; granted chats further down are not shown.";

pub fn list_chats_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "folder": {"type": ["string", "integer"], "description": "A chat folder's name or id (see list_folders)."},
            "archived": {"type": "boolean", "description": "List the archive instead."},
            "unread_only": {"type": "boolean"},
            "query": {"type": "string", "description": "Keep chats whose title contains this."},
            "limit": {"type": "integer", "minimum": 1, "maximum": 200, "default": 50},
        },
        "additionalProperties": false,
    })
}

pub fn list_chats(args: &Value) -> Result<ToolOutput, ToolError> {
    let ctx = Ctx::active()?;
    let (list, list_name) = chat_list(&ctx, args)?;
    let limit = usize::try_from(int_arg(args, "limit", 50, 1, 200)?).unwrap_or(50);
    let unread_only = args.get("unread_only").and_then(Value::as_bool) == Some(true);
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .map(|q| q.trim().to_lowercase())
        .filter(|q| !q.is_empty());
    let mut entries = Vec::new();
    let scanned = scan_granted(&ctx, &list, |chat, target| {
        let entry = chat_entry(&chat, &target);
        if unread_only && entry.get("unread").and_then(Value::as_i64) == Some(0) {
            return Ok(true);
        }
        if query
            .as_ref()
            .is_some_and(|q| !target.title.to_lowercase().contains(q))
        {
            return Ok(true);
        }
        entries.push(entry);
        Ok(entries.len() < limit)
    })?;
    let mut text = format!("{list_name}: {} granted chat(s).\n", entries.len());
    for entry in &entries {
        text.push_str(&format!(
            "[{}] {} ({}, {}){}{}\n",
            entry.get("id").cloned().unwrap_or(Value::Null),
            entry
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            entry
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            entry
                .get("scope")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            match entry.get("unread").and_then(Value::as_i64) {
                Some(n) if n > 0 => format!(" · {n} unread"),
                _ => String::new(),
            },
            entry
                .get("last_message")
                .and_then(Value::as_str)
                .map(|m| format!(" · {m}"))
                .unwrap_or_default(),
        ));
    }
    if entries.is_empty() {
        text.push_str("No granted chats here. Call quill_status to see the grants, or request_access to ask for a chat.");
    }
    let truncated = scanned == Scanned::Capped;
    if truncated {
        text.push_str(&format!("\n{CAPPED_NOTE}"));
    }
    Ok(ToolOutput {
        text,
        data: json!({"list": list_name, "chats": entries, "truncated": truncated}),
        images: Vec::new(),
    })
}

pub fn list_folders(_args: &Value) -> Result<ToolOutput, ToolError> {
    let ctx = Ctx::active()?;
    let folders = crate::agent::events::folders(ctx.client.id());
    let list: Vec<Value> = folders
        .iter()
        .map(|(id, name)| json!({"id": id, "name": name}))
        .collect();
    let text = if folders.is_empty() {
        "This account has no chat folders. list_chats with no folder lists all chats.".to_string()
    } else {
        let names: Vec<String> = folders
            .iter()
            .map(|(id, n)| format!("{n} ({id})"))
            .collect();
        format!(
            "Folders: {}. Pass a name or id to list_chats as \"folder\". Only granted chats appear in them.",
            names.join(", ")
        )
    };
    Ok(ToolOutput {
        text,
        data: json!({"folders": list}),
        images: Vec::new(),
    })
}

pub fn chat_schema_only() -> Value {
    json!({
        "type": "object",
        "properties": {"chat": chat_schema()},
        "required": ["chat"],
        "additionalProperties": false,
    })
}

fn usernames(value: &Value) -> Option<String> {
    value
        .get("usernames")
        .and_then(|u| u.get("active_usernames"))
        .and_then(Value::as_array)
        .and_then(|list| list.first())
        .and_then(Value::as_str)
        .map(|name| format!("@{name}"))
}

pub fn get_chat_info(args: &Value) -> Result<ToolOutput, ToolError> {
    let ctx = Ctx::active()?;
    let target = require(&ctx, args, Scope::Read)?;
    let chat = ctx.call(json!({"@type": "getChat", "chat_id": target.chat_id}))?;
    let kind = chat.get("type").cloned().unwrap_or(Value::Null);
    let mut info = json!({
        "id": target.chat_id,
        "chat_ref": target.chat_ref(),
        "title": target.title,
        "kind": kind_word(&chat, target.class),
        "scope": target.scope.as_str(),
        "unread": chat.get("unread_count"),
        "muted": chat
            .get("notification_settings")
            .and_then(|n| n.get("mute_for"))
            .and_then(Value::as_i64)
            .is_some_and(|m| m > 0),
    });
    let extra = match kind.get("@type").and_then(Value::as_str) {
        Some("chatTypePrivate") | Some("chatTypeSecret") => {
            let user_id = kind.get("user_id").and_then(Value::as_i64).unwrap_or(0);
            let user = ctx.call(json!({"@type": "getUser", "user_id": user_id}))?;
            let full = ctx.call(json!({"@type": "getUserFullInfo", "user_id": user_id}))?;
            json!({
                "user_id": user_id,
                "name": user_name(&user),
                "username": usernames(&user),
                "is_contact": user.get("is_contact"),
                "about": full
                    .get("bio")
                    .and_then(|b| b.get("text"))
                    .and_then(Value::as_str)
                    .filter(|b| !b.is_empty()),
            })
        }
        Some("chatTypeSupergroup") => {
            let id = kind
                .get("supergroup_id")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let group = ctx.call(json!({"@type": "getSupergroup", "supergroup_id": id}))?;
            let full = ctx.call(json!({"@type": "getSupergroupFullInfo", "supergroup_id": id}))?;
            json!({
                "username": usernames(&group),
                "member_count": group.get("member_count"),
                "about": full
                    .get("description")
                    .and_then(Value::as_str)
                    .filter(|d| !d.is_empty()),
                "invite_link": full
                    .get("invite_link")
                    .and_then(|l| l.get("invite_link")),
            })
        }
        Some("chatTypeBasicGroup") => {
            let id = kind
                .get("basic_group_id")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let full = ctx.call(json!({"@type": "getBasicGroupFullInfo", "basic_group_id": id}))?;
            json!({
                "member_count": full.get("members").and_then(Value::as_array).map(Vec::len),
                "about": full
                    .get("description")
                    .and_then(Value::as_str)
                    .filter(|d| !d.is_empty()),
            })
        }
        _ => json!({}),
    };
    if let (Some(info), Some(extra)) = (info.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            if !value.is_null() {
                info.insert(key.clone(), value.clone());
            }
        }
    }
    let mut text = format!(
        "{} ({}, agents: {})",
        target.title,
        info.get("kind").and_then(Value::as_str).unwrap_or_default(),
        target.scope.as_str()
    );
    for key in ["username", "member_count", "unread", "about", "invite_link"] {
        if let Some(value) = info.get(key).filter(|v| !v.is_null()) {
            let shown = value
                .as_str()
                .map_or_else(|| value.to_string(), str::to_string);
            text.push_str(&format!("\n{key}: {shown}"));
        }
    }
    Ok(ToolOutput {
        text,
        data: info,
        images: Vec::new(),
    })
}

pub fn members_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "chat": chat_schema(),
            "query": {"type": "string", "description": "Find members by name or username."},
            "limit": {"type": "integer", "minimum": 1, "maximum": 200, "default": 50},
        },
        "required": ["chat"],
        "additionalProperties": false,
    })
}

fn status_word(status: Option<&Value>) -> &'static str {
    match status.and_then(|s| s.get("@type")).and_then(Value::as_str) {
        Some("chatMemberStatusCreator") => "owner",
        Some("chatMemberStatusAdministrator") => "admin",
        Some("chatMemberStatusRestricted") => "restricted",
        Some("chatMemberStatusBanned") => "banned",
        Some("chatMemberStatusLeft") => "left",
        _ => "member",
    }
}

pub fn list_members(args: &Value) -> Result<ToolOutput, ToolError> {
    let ctx = Ctx::active()?;
    let target = require(&ctx, args, Scope::Read)?;
    let limit = int_arg(args, "limit", 50, 1, 200)?;
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|q| !q.is_empty());
    let chat = ctx.call(json!({"@type": "getChat", "chat_id": target.chat_id}))?;
    let kind = chat.get("type").cloned().unwrap_or(Value::Null);
    let members: Vec<Value> = match kind.get("@type").and_then(Value::as_str) {
        Some("chatTypeSupergroup") => {
            let id = kind
                .get("supergroup_id")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let filter = match query {
                Some(q) => json!({"@type": "supergroupMembersFilterSearch", "query": q}),
                None => json!({"@type": "supergroupMembersFilterRecent"}),
            };
            let answer = ctx.call(json!({
                "@type": "getSupergroupMembers",
                "supergroup_id": id,
                "filter": filter,
                "offset": 0,
                "limit": limit,
            }))?;
            answer
                .get("members")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        }
        Some("chatTypeBasicGroup") => {
            let id = kind
                .get("basic_group_id")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let full = ctx.call(json!({"@type": "getBasicGroupFullInfo", "basic_group_id": id}))?;
            full.get("members")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        }
        _ => {
            return Err(ToolError::new(format!(
                "\"{}\" is not a group or channel, so it has no member list. Call get_chat_info instead.",
                target.title
            )));
        }
    };
    let mut out = Vec::new();
    for member in &members {
        let Some(user_id) = member
            .get("member_id")
            .filter(|m| m.get("@type").and_then(Value::as_str) == Some("messageSenderUser"))
            .and_then(|m| m.get("user_id"))
            .and_then(Value::as_i64)
        else {
            continue;
        };
        let user = ctx.call(json!({"@type": "getUser", "user_id": user_id}))?;
        let name = user_name(&user);
        if let Some(q) = query
            .filter(|_| kind.get("@type").and_then(Value::as_str) == Some("chatTypeBasicGroup"))
            && !name.to_lowercase().contains(&q.to_lowercase())
        {
            continue;
        }
        out.push(json!({
            "user_id": user_id,
            "name": name,
            "username": usernames(&user),
            "role": status_word(member.get("status")),
            "bot": user.get("type").and_then(|t| t.get("@type")).and_then(Value::as_str) == Some("userTypeBot"),
        }));
        if out.len() as i64 >= limit {
            break;
        }
    }
    let mut text = format!("{}: {} member(s).\n", target.title, out.len());
    for member in &out {
        text.push_str(&format!(
            "{} {}{} ({})\n",
            member.get("user_id").cloned().unwrap_or(Value::Null),
            member
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            member
                .get("username")
                .and_then(Value::as_str)
                .map(|u| format!(" {u}"))
                .unwrap_or_default(),
            member
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ));
    }
    Ok(ToolOutput {
        text,
        data: json!({"chat": target.summary(), "members": out}),
        images: Vec::new(),
    })
}

pub fn unread_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "chats": {"type": "integer", "minimum": 1, "maximum": 50, "default": 20, "description": "At most this many chats."},
            "per_chat": {"type": "integer", "minimum": 1, "maximum": 20, "default": 5, "description": "Newest unread messages shown per chat."},
        },
        "additionalProperties": false,
    })
}

pub fn list_unread(args: &Value) -> Result<ToolOutput, ToolError> {
    let ctx = Ctx::active()?;
    let max_chats = usize::try_from(int_arg(args, "chats", 20, 1, 50)?).unwrap_or(20);
    let per_chat = int_arg(args, "per_chat", 5, 1, 20)?;
    let mut names = Names::new(&ctx);
    let mut digest = Vec::new();
    let mut text = String::new();
    let main = json!({"@type": "chatListMain"});
    let scanned = scan_granted(&ctx, &main, |chat, target| {
        let unread = chat
            .get("unread_count")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        if unread == 0 {
            return Ok(true);
        }
        let last = chat
            .get("last_message")
            .and_then(|m| m.get("id"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let answer = ctx.call(json!({
            "@type": "getChatHistory",
            "chat_id": target.chat_id,
            "from_message_id": last,
            "offset": 0,
            "limit": unread.min(per_chat),
            "only_local": false,
        }))?;
        let mut page = messages::messages_of(&answer);
        page.truncate(usize::try_from(unread.min(per_chat)).unwrap_or(0));
        page.reverse();
        let shaped: Vec<Value> = page
            .iter()
            .map(|m| messages::shape(m, target.me, Detail::Summary, &mut names))
            .collect();
        text.push_str(&format!(
            "{} [{}] · {unread} unread\n",
            target.title, target.chat_id
        ));
        for message in &shaped {
            text.push_str(&format!("  {}\n", messages::line(message)));
        }
        digest.push(json!({"chat": target.summary(), "unread": unread, "messages": shaped}));
        Ok(digest.len() < max_chats)
    })?;
    if digest.is_empty() {
        text.push_str("Nothing unread in granted chats.");
    }
    let truncated = scanned == Scanned::Capped;
    if truncated {
        text.push_str(&format!("\n{CAPPED_NOTE}"));
    }
    Ok(ToolOutput {
        text,
        data: json!({"chats": digest, "truncated": truncated}),
        images: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::{MAX_SCAN, Next, SCAN, next_step};

    #[test]
    fn a_full_page_widens_until_the_cap_and_a_short_one_loads_more() {
        assert_eq!(next_step(SCAN, 200), Next::Widen(400));
        assert_eq!(next_step(MAX_SCAN - 100, 1900), Next::Widen(MAX_SCAN));
        assert_eq!(next_step(MAX_SCAN, 2000), Next::Capped);
        assert_eq!(next_step(SCAN, 150), Next::Load);
        assert_eq!(next_step(SCAN, 0), Next::Load);
    }
}
