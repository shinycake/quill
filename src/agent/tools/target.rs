//! Turning a `chat` argument into a chat the agent may use: by id, by
//! "saved", by @username or t.me link, or by title; then the grant check.

use super::{Ctx, ToolError};
use crate::agent::grants::{self, ChatClass, Scope};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Mutex;

const SAVED_NAMES: [&str; 5] = ["saved", "saved messages", "me", "self", "myself"];
/// Chats a title search looks through.
const TITLE_CANDIDATES: i64 = 20;

/// A chat a call may use.
#[derive(Clone)]
pub(crate) struct Target {
    pub chat_id: i64,
    pub title: String,
    pub class: ChatClass,
    pub scope: Scope,
    /// The signed-in user's id, which messages need to say "You".
    pub me: i64,
}

impl Target {
    /// How later calls can name this chat.
    pub fn chat_ref(&self) -> Value {
        if self.class == ChatClass::Saved {
            json!("saved")
        } else {
            json!(self.chat_id)
        }
    }

    pub fn summary(&self) -> Value {
        json!({"id": self.chat_id, "title": self.title, "chat_ref": self.chat_ref(), "scope": self.scope.as_str()})
    }
}

/// Per client: the signed-in user's id, and each chat's class once known.
#[derive(Default)]
struct Known {
    me: HashMap<i32, i64>,
    classes: HashMap<(i32, i64), ChatClass>,
}

static KNOWN: Mutex<Option<Known>> = Mutex::new(None);

fn with_known<R>(f: impl FnOnce(&mut Known) -> R) -> R {
    f(crate::locks::lock_or_recover(&KNOWN).get_or_insert_with(Known::default))
}

pub(crate) fn my_user_id(ctx: &Ctx) -> Result<i64, ToolError> {
    if let Some(id) = with_known(|known| known.me.get(&ctx.client.id()).copied()) {
        return Ok(id);
    }
    let me = ctx.call(json!({"@type": "getMe"}))?;
    let id = me.get("id").and_then(Value::as_i64).ok_or_else(|| {
        ToolError::new("Quill could not read the signed-in account. Retry in a moment.")
    })?;
    with_known(|known| known.me.insert(ctx.client.id(), id));
    Ok(id)
}

/// The class of `chat`, a TDLib `chat` object.
pub(crate) fn classify(ctx: &Ctx, chat: &Value, me: i64) -> Result<ChatClass, ToolError> {
    let chat_id = chat.get("id").and_then(Value::as_i64).unwrap_or(0);
    if let Some(class) = with_known(|known| known.classes.get(&(ctx.client.id(), chat_id)).copied())
    {
        return Ok(class);
    }
    let kind = chat.get("type").cloned().unwrap_or(Value::Null);
    let class = match kind.get("@type").and_then(Value::as_str) {
        Some("chatTypePrivate") => {
            let user_id = kind.get("user_id").and_then(Value::as_i64).unwrap_or(0);
            if user_id == me {
                ChatClass::Saved
            } else {
                let user = ctx.call(json!({"@type": "getUser", "user_id": user_id}))?;
                let is_bot = user
                    .get("type")
                    .and_then(|t| t.get("@type"))
                    .and_then(Value::as_str)
                    == Some("userTypeBot");
                if is_bot {
                    ChatClass::Bot
                } else {
                    ChatClass::Other
                }
            }
        }
        Some("chatTypeSecret") => ChatClass::Secret,
        Some("chatTypeSupergroup")
            if kind.get("is_channel").and_then(Value::as_bool) == Some(true) =>
        {
            let id = kind
                .get("supergroup_id")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let group = ctx.call(json!({"@type": "getSupergroup", "supergroup_id": id}))?;
            let owner = group
                .get("status")
                .and_then(|s| s.get("@type"))
                .and_then(Value::as_str)
                == Some("chatMemberStatusCreator");
            if owner {
                ChatClass::OwnChannel
            } else {
                ChatClass::Other
            }
        }
        _ => ChatClass::Other,
    };
    with_known(|known| known.classes.insert((ctx.client.id(), chat_id), class));
    Ok(class)
}

pub(crate) fn chat_title(chat: &Value, class: ChatClass) -> String {
    if class == ChatClass::Saved {
        return "Saved Messages".to_string();
    }
    chat.get("title")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
        .unwrap_or("Deleted account")
        .to_string()
}

fn chat_id_of(chat: &Value) -> Option<i64> {
    chat.get("id").and_then(Value::as_i64)
}

/// The chat id `wanted` names. `None` means Saved Messages.
fn resolve_id(ctx: &Ctx, wanted: Option<&Value>, me: i64, reach: Reach) -> Result<i64, ToolError> {
    let text = match wanted {
        None | Some(Value::Null) => return saved_chat_id(ctx, me),
        Some(Value::Number(n)) => {
            return n
                .as_i64()
                .ok_or_else(|| ToolError::new("chat ids are whole numbers."));
        }
        Some(Value::String(text)) => text.trim().to_string(),
        Some(_) => {
            return Err(ToolError::new(
                "chat must be a chat id, \"saved\", an @username or a chat title.",
            ));
        }
    };
    if let Ok(id) = text.parse::<i64>() {
        return Ok(id);
    }
    let lower = text.to_lowercase();
    if SAVED_NAMES.contains(&lower.as_str()) || text.is_empty() {
        return saved_chat_id(ctx, me);
    }
    if let Some(username) = username_of(&text) {
        let chat = ctx
            .call(json!({"@type": "searchPublicChat", "username": username}))
            .map_err(|_| {
                ToolError::new(format!(
                    "No chat has the username @{username}. Check the spelling, or call list_chats."
                ))
            })?;
        return chat_id_of(&chat).ok_or_else(|| ToolError::new("Telegram returned no chat id."));
    }
    by_title(ctx, &text, me, reach)
}

fn saved_chat_id(ctx: &Ctx, me: i64) -> Result<i64, ToolError> {
    let chat = ctx.call(json!({"@type": "createPrivateChat", "user_id": me, "force": false}))?;
    Ok(chat_id_of(&chat).unwrap_or(me))
}

/// `@name`, `t.me/name` or `https://t.me/name` → `name`.
fn username_of(text: &str) -> Option<&str> {
    let rest = text
        .strip_prefix('@')
        .or_else(|| text.strip_prefix("https://t.me/"))
        .or_else(|| text.strip_prefix("http://t.me/"))
        .or_else(|| text.strip_prefix("t.me/"))?;
    let name = rest.split(['/', '?']).next().unwrap_or_default();
    let valid = name.len() >= 4 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    valid.then_some(name)
}

/// Which chats a name may resolve to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reach {
    /// Granted chats only: tools never learn that other chats exist.
    Granted,
    /// Any chat, for `request_access`, which shows the match to the user
    /// and tells the agent nothing about it until the user allows it.
    Any,
}

/// The one chat whose title matches `title`: an exact match (ignoring case)
/// wins, then a single search result. With [`Reach::Granted`], chats that
/// are not granted are left out of the matches and of every error.
fn by_title(ctx: &Ctx, title: &str, me: i64, reach: Reach) -> Result<i64, ToolError> {
    let found = ctx.call(json!({"@type": "searchChats", "query": title, "type_filter": null, "limit": TITLE_CANDIDATES}))?;
    let ids: Vec<i64> = found
        .get("chat_ids")
        .and_then(Value::as_array)
        .map(|ids| ids.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default();
    let grants = grants::for_account(&ctx.account);
    let mut candidates = Vec::new();
    for id in ids {
        let chat = ctx.call(json!({"@type": "getChat", "chat_id": id}))?;
        let class = classify(ctx, &chat, me)?;
        if reach == Reach::Granted && grants.scope_for(id, class).is_none() {
            continue;
        }
        candidates.push((id, chat_title(&chat, class)));
    }
    let exact: Vec<&(i64, String)> = candidates
        .iter()
        .filter(|(_, name)| name.to_lowercase() == title.to_lowercase())
        .collect();
    match (exact.as_slice(), candidates.as_slice()) {
        ([(id, _)], _) | ([], [(id, _)]) => Ok(*id),
        // One answer whether or not an ungranted chat matched, so trying
        // names tells an agent nothing about chats it can't see.
        ([], []) => Err(ToolError::new(format!(
            "No granted chat is called \"{title}\". Call list_chats for the chats you can use. If the user wants you to use a chat by that name, call request_access {{\"chat\": \"{title}\", \"scope\": \"read\", \"reason\": \"<why>\"}}; Quill shows the user the match and asks."
        ))),
        _ if reach == Reach::Any => Err(ToolError::new(format!(
            "Several chats match \"{title}\". Ask with the chat's exact title or @username, or with its id if you have it."
        ))),
        (many, _) => {
            let pool: Vec<&(i64, String)> = if many.is_empty() {
                candidates.iter().collect()
            } else {
                many.to_vec()
            };
            let names: Vec<String> = pool
                .iter()
                .take(8)
                .map(|(id, name)| format!("{name} (chat {id})"))
                .collect();
            Err(ToolError::new(format!(
                "Several granted chats match \"{title}\": {}. Pass the chat id.",
                names.join(", ")
            )))
        }
    }
}

/// The scope, title and class of chat `chat_id`, or `None` when it is not
/// granted (or unknown).
pub(crate) fn granted(ctx: &Ctx, chat_id: i64, me: i64) -> Result<Option<Target>, ToolError> {
    let Ok(chat) = ctx.call(json!({"@type": "getChat", "chat_id": chat_id})) else {
        return Ok(None);
    };
    let class = classify(ctx, &chat, me)?;
    let scope = grants::for_account(&ctx.account).scope_for(chat_id, class);
    Ok(scope.map(|scope| Target {
        chat_id,
        title: chat_title(&chat, class),
        class,
        scope,
        me,
    }))
}

/// A chat found by the `chat` argument, before any grant check.
pub(crate) struct Found {
    pub chat_id: i64,
    pub title: String,
    pub class: ChatClass,
    pub me: i64,
}

/// The chat `args.chat` names.
pub(crate) fn lookup(ctx: &Ctx, args: &Value, reach: Reach) -> Result<Found, ToolError> {
    let me = my_user_id(ctx)?;
    let chat_id = resolve_id(ctx, args.get("chat"), me, reach)?;
    let chat = ctx
        .call(json!({"@type": "getChat", "chat_id": chat_id}))
        .map_err(|_| {
            ToolError::new(format!(
                "Quill does not know chat {chat_id}. Call list_chats for chat ids, or pass a title or @username."
            ))
        })?;
    let class = classify(ctx, &chat, me)?;
    Ok(Found {
        chat_id,
        title: chat_title(&chat, class),
        class,
        me,
    })
}

/// The chat `args.chat` names, if agents may use it at `needed` scope.
pub(crate) fn require(ctx: &Ctx, args: &Value, needed: Scope) -> Result<Target, ToolError> {
    let Found {
        chat_id,
        title,
        class,
        me,
    } = lookup(ctx, args, Reach::Granted)?;
    let scope = grants::for_account(&ctx.account).scope_for(chat_id, class);
    match scope {
        Some(scope) if scope.allows(needed) => Ok(Target {
            chat_id,
            title,
            class,
            scope,
            me,
        }),
        Some(_) => Err(ToolError::new(format!(
            "\"{title}\" is granted for reading only. Call request_access {{\"chat\": {chat_id}, \"scope\": \"read_write\", \"reason\": \"<why>\"}} to ask the user for more."
        ))),
        // No title: an agent must not learn what an ungranted chat is.
        None => Err(ToolError::new(format!(
            "Chat not granted: {}. Call request_access {{\"chat\": {chat_id}, \"scope\": \"{}\", \"reason\": \"<why>\"}} to ask the user.",
            chat_id,
            needed.as_str()
        ))),
    }
}

/// The schema of a `chat` argument.
pub(crate) fn chat_schema() -> Value {
    json!({
        "type": ["string", "integer"],
        "description": "The chat: a chat id, \"saved\" for Saved Messages, an @username, or the chat's title.",
    })
}

#[cfg(test)]
mod tests {
    use super::username_of;

    #[test]
    fn usernames_come_from_handles_and_links() {
        assert_eq!(username_of("@durov"), Some("durov"));
        assert_eq!(username_of("https://t.me/telegram/12"), Some("telegram"));
        assert_eq!(username_of("t.me/quill_app?start=1"), Some("quill_app"));
        assert_eq!(username_of("Ann Lee"), None);
        assert_eq!(username_of("@a b"), None);
    }
}
