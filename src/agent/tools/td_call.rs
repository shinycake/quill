//! `td_call`: any TDLib request, for what the curated tools do not cover.
//!
//! [`td_policy`](super::td_policy) classes the method. Grants apply to every
//! chat the request names (and to the private chat of a user, the chat of a
//! group or channel id); sends share the paced queue; the deny list waits
//! for the user's click. Answers pass through [`redact`]: chats the agent
//! may not read are hidden wherever they appear, phone numbers are dropped,
//! and sponsored messages are labelled as ads.

use super::target::{self, Target};
use super::td_policy::{self, Class};
use super::write::common::{ANSWER_WAIT, admit, log_in, write_error};
use super::{Ctx, Tool, ToolError, ToolOutput, current_agent};
use crate::agent::grants::{self, Scope};
use crate::agent::prompts::{self, Ask, Decision, Risk};
use crate::agent::td;
use serde_json::{Map, Value, json};
use std::collections::HashMap;

pub(super) const TOOLS: &[Tool] = &[Tool {
    name: "td_call",
    title: "Raw TDLib call",
    description: "Use only when no other tool does the job: sends one TDLib request (https://core.telegram.org/tdlib/docs/td__api_8h.html) \
as the user and returns TDLib's answer. Chat grants apply to every chat, user, group or channel id in the request; reads need read, \
changes need read_write; sends are paced like send_message. Requests that touch the account (sign-out, sessions, password, phone, \
profile) or can spend Stars, TON or money wait for the user to click Allow in Quill: pass reason. Chats the agent may not read come \
back as {\"@type\": \"chat\", \"id\": …, \"hidden\": …}; phone numbers are removed; sponsored messages carry \"sponsored\": true. \
Example input: {\"request\": {\"@type\": \"getChatPinnedMessage\", \"chat_id\": -1001234567890}}, or {\"request\": {\"@type\": \"setBio\", \
\"bio\": \"Building things\"}, \"reason\": \"The user asked me to update their bio.\"}. Output: result (TDLib's answer object).",
    schema,
    run,
    read_only: false,
}];

/// Text answers longer than this are cut; `structuredContent` keeps all.
const TEXT_CAP: usize = 20_000;
const HIDDEN: &str = "not granted to agents";

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "request": {"type": "object", "description": "The TDLib request: an object with \"@type\" (the method) and its fields, as in the TDLib JSON interface."},
            "reason": {"type": "string", "description": "One sentence for the user on why, shown when Quill must ask (account changes, anything that can spend money)."},
        },
        "required": ["request"],
        "additionalProperties": false,
    })
}

/// Telegram's offsets between a group or channel id and its chat id.
const SUPERGROUP_BASE: i64 = -1_000_000_000_000;
const SECRET_BASE: i64 = -2_000_000_000_000;

/// The chats a request names: (chat id, whether it is acted on rather than
/// only read from). Top-level chat ids are acted on, except `from_chat_id`;
/// ids deeper in the request are sources.
fn chats_named(request: &Value, write: bool) -> Vec<(i64, bool)> {
    let mut named = Vec::new();
    let Some(fields) = request.as_object() else {
        return named;
    };
    let int = |key: &str| fields.get(key).and_then(Value::as_i64).filter(|id| *id != 0);
    if let Some(id) = int("chat_id") {
        named.push((id, true));
    }
    if let Some(id) = int("from_chat_id") {
        named.push((id, false));
    }
    for id in fields
        .get("chat_ids")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
    {
        named.push((id, true));
    }
    if let Some(id) = int("supergroup_id") {
        named.push((SUPERGROUP_BASE.saturating_sub(id), true));
    }
    if let Some(id) = int("basic_group_id") {
        named.push((id.saturating_neg(), true));
    }
    if let Some(id) = int("secret_chat_id") {
        named.push((SECRET_BASE.saturating_add(id), true));
    }
    // A profile is not a chat to read, but a change aimed at a user acts in
    // the private chat with them.
    if write && let Some(id) = int("user_id") {
        named.push((id, true));
    }
    for (key, value) in fields {
        if !matches!(key.as_str(), "chat_id" | "from_chat_id" | "chat_ids") {
            nested_chats(value, &mut named);
        }
    }
    named
}

fn nested_chats(value: &Value, named: &mut Vec<(i64, bool)>) {
    match value {
        Value::Object(fields) => {
            for (key, inner) in fields {
                if matches!(key.as_str(), "chat_id" | "from_chat_id")
                    && let Some(id) = inner.as_i64().filter(|id| *id != 0)
                {
                    named.push((id, false));
                } else {
                    nested_chats(inner, named);
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| nested_chats(item, named)),
        _ => {}
    }
}

/// Whether any type in the request can spend money: `reactionTypePaid`,
/// `inputMessageStakeDice`, an invoice.
fn spends(value: &Value) -> bool {
    match value {
        Value::Object(fields) => fields.iter().any(|(key, inner)| {
            (key == "@type" && inner.as_str().is_some_and(td_policy::names_money))
                || (key == "paid_message_star_count" && inner.as_i64().is_some_and(|n| n > 0))
                || spends(inner)
        }),
        Value::Array(items) => items.iter().any(spends),
        _ => false,
    }
}

/// Grant lookups for one call, cached.
struct Grants<'a> {
    ctx: &'a Ctx,
    me: i64,
    seen: HashMap<i64, Option<Target>>,
}

impl<'a> Grants<'a> {
    fn new(ctx: &'a Ctx) -> Result<Self, ToolError> {
        Ok(Self {
            ctx,
            me: target::my_user_id(ctx)?,
            seen: HashMap::new(),
        })
    }

    fn target(&mut self, chat_id: i64) -> Option<&Target> {
        if !self.seen.contains_key(&chat_id) {
            let found = target::granted(self.ctx, chat_id, self.me).ok().flatten();
            self.seen.insert(chat_id, found);
        }
        self.seen.get(&chat_id).and_then(Option::as_ref)
    }

    fn allows(&mut self, chat_id: i64, needed: Scope) -> bool {
        self.target(chat_id)
            .is_some_and(|target| target.scope.allows(needed))
    }

    /// Refuses unless every named chat is granted at the scope it needs.
    fn require(&mut self, named: &[(i64, bool)], acting: Scope) -> Result<(), ToolError> {
        for (chat_id, acted_on) in named {
            let needed = if *acted_on { acting } else { Scope::Read };
            if self.allows(*chat_id, needed) {
                continue;
            }
            return Err(match self.target(*chat_id).map(|t| t.title.clone()) {
                Some(title) => ToolError::new(format!(
                    "\"{title}\" is granted for reading only. Call request_access {{\"chat\": {chat_id}, \"scope\": \"read_write\", \"reason\": \"<why>\"}} to ask the user for more."
                )),
                None => ToolError::new(format!(
                    "Chat not granted: {chat_id}. Call request_access {{\"chat\": {chat_id}, \"scope\": \"{}\", \"reason\": \"<why>\"}} to ask the user.",
                    needed.as_str()
                )),
            });
        }
        Ok(())
    }
}

/// Hide what the agent may not see in a TDLib answer.
fn redact(value: &mut Value, grants: &mut Grants) {
    match value {
        Value::Object(fields) => {
            let kind = fields
                .get("@type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let own_chat = if kind == "chat" {
                fields.get("id").and_then(Value::as_i64)
            } else if kind.is_empty() {
                None
            } else {
                fields.get("chat_id").and_then(Value::as_i64)
            };
            if let Some(chat_id) = own_chat.filter(|id| *id != 0)
                && !grants.allows(chat_id, Scope::Read)
            {
                let key = if kind == "chat" { "id" } else { "chat_id" };
                *value = json!({"@type": kind, key: chat_id, "hidden": HIDDEN});
                return;
            }
            fields.remove("phone_number");
            if kind.starts_with("sponsored") {
                fields.insert("sponsored".into(), Value::Bool(true));
            }
            if let Some(Value::Array(ids)) = fields.get_mut("chat_ids") {
                ids.retain(|id| id.as_i64().is_some_and(|id| grants.allows(id, Scope::Read)));
            }
            for inner in fields.values_mut() {
                redact(inner, grants);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|item| redact(item, grants)),
        _ => {}
    }
}

fn method_of(request: &Value) -> Result<String, ToolError> {
    request
        .get("@type")
        .and_then(Value::as_str)
        .filter(|method| !method.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            ToolError::new(
                "request needs \"@type\", the TDLib method, e.g. {\"request\": {\"@type\": \"getChat\", \"chat_id\": -1001234567890}}.",
            )
        })
}

/// Wait for the user to allow a deny-listed call.
fn ask_user(ctx: &Ctx, method: &str, risk: Risk, request: &Value, reason: &str) -> Result<(), ToolError> {
    if reason.trim().is_empty() {
        return Err(ToolError::new(format!(
            "{method} needs the user's click in Quill. Call td_call again with \"reason\": one sentence on why, which the user reads."
        )));
    }
    let preview: String = serde_json::to_string_pretty(request)
        .unwrap_or_default()
        .chars()
        .take(1000)
        .collect();
    let ask = Ask::Raw {
        method: method.to_string(),
        risk,
        preview,
    };
    let (id, rx) = prompts::file(ctx.account.clone(), current_agent(), ask, reason.to_string())
        .ok_or_else(|| {
            ToolError::new("Several requests are already waiting for the user in Quill. Wait for those answers, then retry.")
        })?;
    match prompts::wait(id, &rx, ANSWER_WAIT) {
        Some(Decision::Allowed) => Ok(()),
        Some(Decision::Denied) => {
            log_in(ctx, 0, "", "td_call", &format!("Declined: {method}"), false);
            Err(ToolError::new(
                "The user declined this in Quill. Do not retry it; ask the user what they want instead.",
            ))
        }
        None => Err(ToolError::new(format!(
            "The user did not answer within {} seconds. Tell the user what you want to do and why, then retry.",
            ANSWER_WAIT.as_secs()
        ))),
    }
}

/// How many messages a send puts out, for pacing.
fn send_count(request: &Value) -> u32 {
    ["input_message_contents", "message_ids"]
        .iter()
        .find_map(|key| request.get(*key).and_then(Value::as_array))
        .map_or(1, |list| u32::try_from(list.len().max(1)).unwrap_or(u32::MAX))
}

fn run(args: &Value) -> Result<ToolOutput, ToolError> {
    let mut request = args
        .get("request")
        .filter(|r| r.is_object())
        .cloned()
        .ok_or_else(|| ToolError::new("Pass request: a TDLib request object with \"@type\"."))?;
    let method = method_of(&request)?;
    let reason = args.get("reason").and_then(Value::as_str).unwrap_or_default();
    if let Some(fields) = request.as_object_mut() {
        // Quill routes answers by @extra; an agent's own would be dropped.
        fields.remove("@extra");
        fields.remove("@client_id");
    }
    let ctx = Ctx::active()?;
    let mut grants = Grants::new(&ctx)?;
    let write_named = !matches!(td_policy::classify(&method, true), Class::Granted(Scope::Read));
    let named = chats_named(&request, write_named);
    let mut class = td_policy::classify(&method, !named.is_empty());
    if !matches!(class, Class::Blocked(_) | Class::Ask(_)) && spends(&request) {
        class = Class::Ask(Risk::Money);
    }
    let first_chat = named.first().map(|(id, _)| *id).unwrap_or(0);
    let answer = match class {
        Class::Blocked(why) => {
            return Err(ToolError::new(format!("td_call does not run {method}: {why}")));
        }
        Class::Ask(risk) => {
            ask_user(&ctx, &method, risk, &request, reason)?;
            td::call(&ctx.client, request).map_err(write_error)
        }
        Class::Granted(scope) => {
            if is_contacts(&method) && !grants::for_account(&ctx.account).contacts {
                return Err(ToolError::new(
                    "The contact list is not granted. Call request_access {\"contacts\": true, \"reason\": \"<why>\"} to ask the user.",
                ));
            }
            grants.require(&named, scope)?;
            let result = td::call(&ctx.client, request);
            if scope == Scope::Read {
                result.map_err(ToolError::from)
            } else {
                result.map_err(write_error)
            }
        }
        Class::Send => {
            grants.require(&named, Scope::ReadWrite)?;
            let target = grants
                .target(first_chat)
                .cloned()
                .ok_or_else(|| ToolError::new("A send needs chat_id, the chat to send in."))?;
            admit(&ctx, &target, send_count(&request))?;
            td::call(&ctx.client, request).map_err(write_error)
        }
    };
    if !matches!(class, Class::Granted(Scope::Read)) {
        let title = grants
            .target(first_chat)
            .map(|t| t.title.clone())
            .unwrap_or_default();
        log_in(&ctx, first_chat, &title, "td_call", &method, answer.is_ok());
    }
    let mut result = answer?;
    redact(&mut result, &mut grants);
    let mut text = serde_json::to_string(&result).unwrap_or_default();
    if text.len() > TEXT_CAP {
        let cut = (0..=TEXT_CAP).rev().find(|at| text.is_char_boundary(*at)).unwrap_or(0);
        text.truncate(cut);
        text.push_str("… (cut; the whole answer is in structuredContent.result)");
    }
    let mut data = Map::new();
    data.insert("method".into(), Value::String(method));
    data.insert("result".into(), result);
    Ok(ToolOutput {
        text,
        data: Value::Object(data),
    })
}

/// Methods that read the contact list, which has a grant of its own.
fn is_contacts(method: &str) -> bool {
    matches!(method, "getContacts" | "searchContacts" | "getCloseFriends")
}

#[cfg(test)]
mod tests {
    use super::{chats_named, send_count, spends};
    use serde_json::json;

    #[test]
    fn every_chat_a_request_names_is_checked() {
        let forward = json!({"@type": "forwardMessages", "chat_id": 5, "from_chat_id": 6, "message_ids": [1, 2]});
        assert_eq!(chats_named(&forward, true), vec![(5, true), (6, false)]);
        let members = json!({"@type": "getSupergroupMembers", "supergroup_id": 77});
        assert_eq!(chats_named(&members, false), vec![(-1_000_000_000_077, true)]);
        let nested = json!({"@type": "sendMessage", "chat_id": 5, "input_message_content":
            {"@type": "inputMessageForwarded", "from_chat_id": 9, "message_id": 1}});
        assert_eq!(chats_named(&nested, true), vec![(5, true), (9, false)]);
        let profile = json!({"@type": "getUserFullInfo", "user_id": 42});
        assert!(chats_named(&profile, false).is_empty());
        let block = json!({"@type": "setUserPersonalProfilePhoto", "user_id": 42});
        assert_eq!(chats_named(&block, true), vec![(42, true)]);
    }

    #[test]
    fn paid_content_needs_the_user() {
        assert!(spends(&json!({"@type": "addMessageReaction", "reaction_type": {"@type": "reactionTypePaid"}})));
        assert!(spends(&json!({"@type": "sendMessage", "options": {"paid_message_star_count": 10}})));
        assert!(!spends(&json!({"@type": "sendMessage", "input_message_content": {"@type": "inputMessageText"}})));
    }

    #[test]
    fn albums_and_forwards_pace_per_message() {
        assert_eq!(send_count(&json!({"input_message_contents": [{}, {}, {}]})), 3);
        assert_eq!(send_count(&json!({"message_ids": [1, 2]})), 2);
        assert_eq!(send_count(&json!({})), 1);
    }
}
