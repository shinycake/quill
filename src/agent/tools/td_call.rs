//! `td_call`: any TDLib request, for what the curated tools do not cover.
//!
//! The request is checked against the schema Quill is built with
//! ([`td_schema`](super::td_schema)): unknown methods are refused, unknown
//! fields dropped, chat ids made integers. [`td_policy`](super::td_policy)
//! classes the method. Grants apply to every chat the request names; writes
//! respect "Confirm each send" and the paced queue, sends wait for delivery;
//! the deny list waits for the user's click. Answers pass through
//! [`td_redact`](super::td_redact).

use super::target::{self, Target};
use super::td_policy::{self, Class, Shape};
use super::td_request::{chats_named, has_type, local_paths, sanitize, spends};
use super::td_schema;
use super::write::common::{self, ANSWER_WAIT, log_in, record, take_write_turn, write_error};
use super::write::send::in_quill_data;
use super::{Ctx, Tool, ToolError, ToolOutput, current_agent, still_allowed};
use super::{td_offset, td_redact};
use crate::agent::grants::{self, Scope};
use crate::agent::prompts::{self, Ask, Decision, Risk};
use crate::agent::{access, td};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::path::Path;

pub(super) const TOOLS: &[Tool] = &[Tool {
    name: "td_call",
    title: "Raw TDLib call",
    description: "Use only when no other tool does the job: sends one TDLib request (https://core.telegram.org/tdlib/docs/td__api_8h.html) \
as the user and returns TDLib's answer. Grants apply to every chat id in the request: reads need read, changes to one chat need \
read_write, and sends are paced like send_message and return final message ids. Changes that name no chat, anything touching the \
account (sessions, password, phone, privacy, web logins, bot buttons) and anything that can spend Stars, TON or money wait for the \
user to click Allow in Quill: pass reason. Raw file access and Quill's own options never run. Answers leave out chats the agent may \
not read (lists count only what is left), phone numbers are removed, sponsored messages carry \"sponsored\": true. \
Example input: {\"request\": {\"@type\": \"getChatPinnedMessage\", \"chat_id\": -1001234567890}}, or {\"request\": {\"@type\": \"setBio\", \
\"bio\": \"Building things\"}, \"reason\": \"The user asked me to update their bio.\"}. Output: method, result (TDLib's answer object).",
    schema,
    run,
    read_only: false,
    confirms_sends: true,
}];

/// Text answers longer than this are cut; `structuredContent` keeps all.
const TEXT_CAP: usize = 20_000;

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "request": {"type": "object", "description": "The TDLib request: an object with \"@type\" (the method) and its fields, as in the TDLib JSON interface."},
            "reason": {"type": "string", "description": "One sentence for the user on why, shown when Quill must ask (account changes, changes that name no chat, anything that can spend money)."},
        },
        "required": ["request"],
        "additionalProperties": false,
    })
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

    /// The chat a write acts in, if granted and not a stranger's bot.
    fn writable(&mut self, chat_id: i64) -> Result<Target, ToolError> {
        let ctx = self.ctx;
        let target = self
            .target(chat_id)
            .cloned()
            .ok_or_else(|| ToolError::new("This change needs chat_id, the chat it acts in."))?;
        if target::stranger_bot(ctx, &target) {
            return Err(ToolError::new(format!(
                "\"{}\" is a bot the user has never talked to, so the bots default does not cover writing to it. Call request_access {{\"chat\": {chat_id}, \"scope\": \"read_write\", \"reason\": \"<why>\"}} to ask the user.",
                target.title
            )));
        }
        Ok(target)
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

/// The longest request the user is shown; longer ones are refused, so
/// what the user allows is always the whole request.
/// Fits a full 4096-character message with its JSON around it.
const PREVIEW_CAP: usize = 6000;

fn preview_of(request: &Value) -> Result<String, ToolError> {
    let preview = serde_json::to_string_pretty(request).unwrap_or_default();
    if preview.chars().count() > PREVIEW_CAP {
        return Err(ToolError::new(format!(
            "This request is too long to show the user in full (over {PREVIEW_CAP} characters). Shorten its text fields and retry."
        )));
    }
    Ok(preview)
}

/// Wait for the user to allow a deny-listed call.
fn ask_user(
    ctx: &Ctx,
    method: &str,
    risk: Risk,
    request: &Value,
    reason: &str,
) -> Result<(), ToolError> {
    if reason.trim().is_empty() {
        return Err(ToolError::new(format!(
            "{method} needs the user's click in Quill. Call td_call again with \"reason\": one sentence on why, which the user reads."
        )));
    }
    let ask = Ask::Raw {
        method: method.to_string(),
        risk,
        preview: preview_of(request)?,
    };
    let (id, rx) = prompts::file(ctx.account.clone(), current_agent(), ask, reason.to_string())
        .ok_or_else(|| {
            ToolError::new("Several requests are already waiting for the user in Quill. Wait for those answers, then retry.")
        })?;
    match prompts::wait(id, &rx, ANSWER_WAIT) {
        Some(Decision::Allowed) => still_allowed(),
        Some(Decision::Denied) => {
            record(ctx, 0, "", "td_call", method, false, true);
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
        .map_or(1, |list| {
            u32::try_from(list.len().max(1)).unwrap_or(u32::MAX)
        })
}

/// Send through the paced queue, waiting for delivery so the answer
/// carries final message ids; `common::send` logs it.
fn send(ctx: &Ctx, target: &Target, method: &str, request: Value) -> Result<Value, ToolError> {
    let count = send_count(&request);
    let mut sent = common::send(ctx, target, "td_call", method, request, count)?;
    if sent.len() == 1 && method != "sendMessageAlbum" && method != "forwardMessages" {
        Ok(sent.remove(0))
    } else {
        Ok(json!({"@type": "messages", "total_count": sent.len(), "messages": sent}))
    }
}

/// Refuses uploads from Quill's own data folder (sign-in keys, the agent
/// token, the cache).
fn check_uploads(request: &Value) -> Result<(), ToolError> {
    let mut paths = Vec::new();
    local_paths(request, &mut paths);
    match paths.iter().find(|path| in_quill_data(Path::new(path))) {
        Some(path) => Err(ToolError::new(format!(
            "{path} is inside Quill's own data folder; agents cannot upload files from there."
        ))),
        None => Ok(()),
    }
}

/// What `td_call` answers while the user has turned raw TDLib calls off.
pub const RAW_OFF_MESSAGE: &str = "The user turned off raw TDLib calls in Quill (Settings > Agent access > What agents can do). Use the other tools, or ask the user to turn raw calls back on.";

fn run(args: &Value) -> Result<ToolOutput, ToolError> {
    if !crate::agent::access::raw_calls() {
        return Err(ToolError::new(RAW_OFF_MESSAGE));
    }
    let mut request = args
        .get("request")
        .filter(|r| r.is_object())
        .cloned()
        .ok_or_else(|| ToolError::new("Pass request: a TDLib request object with \"@type\"."))?;
    let method = method_of(&request)?;
    let reason = args
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let schema = td_schema::get();
    let entry = schema.functions.get(&method).ok_or_else(|| {
        ToolError::new(format!(
            "TDLib has no method {method} (Quill is built with TDLib 1.8.69). Check the name and case in the TDLib API reference."
        ))
    })?;
    let shape = Shape {
        takes_file: entry.has("file_id"),
        chat_scoped: entry.type_of("chat_id") == Some("int53"),
        group_call: entry.has("group_call_id"),
    };
    sanitize(&mut request, schema, true).map_err(ToolError::new)?;
    td_offset::reveal(&mut request).map_err(ToolError::new)?;
    let mut class = td_policy::classify(&method, shape);
    if has_type(&request, "callbackQueryPayloadDataWithPassword") {
        class = Class::Blocked("agents never send the user's password.");
    }
    // Both name a file by its local id (inputFileGenerated through a
    // "#file_id#" conversion TDLib runs itself), and ids reach any chat.
    if has_type(&request, "inputFileId") || has_type(&request, "inputFileGenerated") {
        class = Class::Blocked(
            "file ids reach files from any chat; upload with inputFileLocal, or forward the message.",
        );
    }
    if !matches!(class, Class::Blocked(_) | Class::Ask(_)) && spends(&request) {
        class = Class::Ask(Risk::Money);
    }
    if let Class::Blocked(why) = class {
        return Err(ToolError::new(format!(
            "td_call does not run {method}: {why}"
        )));
    }
    check_uploads(&request)?;
    let ctx = Ctx::active()?;
    let mut grants = Grants::new(&ctx)?;
    let named = chats_named(&request);
    let first_chat = request
        .get("chat_id")
        .and_then(Value::as_i64)
        .or_else(|| named.first().map(|(id, _)| *id))
        .unwrap_or(0);
    let answer = match class {
        Class::Blocked(_) => return Err(ToolError::new("Not available to agents.")),
        Class::Ask(risk) => {
            grants.require(&named, Scope::ReadWrite)?;
            ask_user(&ctx, &method, risk, &request, reason)?;
            if td_policy::SENDS.contains(&method.as_str()) {
                let target = grants.writable(first_chat)?;
                send(&ctx, &target, &method, request)
            } else {
                td::call(&ctx.client, request).map_err(write_error)
            }
        }
        Class::Granted(Scope::Read) => {
            if is_contacts(&method) && !grants::for_account(&ctx.account).contacts {
                return Err(ToolError::new(
                    "The contact list is not granted. Call request_access {\"contacts\": true, \"reason\": \"<why>\"} to ask the user.",
                ));
            }
            grants.require(&named, Scope::Read)?;
            read_page(&ctx, &mut grants, request)
        }
        Class::Granted(Scope::ReadWrite) => {
            grants.require(&named, Scope::ReadWrite)?;
            let target = grants.writable(first_chat)?;
            confirm(&ctx, &target, &request)?;
            take_write_turn(&ctx, target.chat_id)?;
            td::call(&ctx.client, request).map_err(write_error)
        }
        Class::Send => {
            grants.require(&named, Scope::ReadWrite)?;
            let target = grants.writable(first_chat)?;
            confirm(&ctx, &target, &request)?;
            send(&ctx, &target, &method, request)
        }
    };
    // Reads are not logged; sends are logged by `common::send`.
    let sent = td_policy::SENDS.contains(&method.as_str());
    if !matches!(class, Class::Granted(Scope::Read) | Class::Send) && !sent {
        let title = grants
            .target(first_chat)
            .map(|t| t.title.clone())
            .unwrap_or_default();
        log_in(&ctx, first_chat, &title, "td_call", &method, answer.is_ok());
    }
    let mut result = answer?;
    if class != Class::Granted(Scope::Read) {
        redact(&mut grants, &mut result);
    }
    td_offset::hide(&mut result);
    Ok(output(method, result))
}

fn redact(grants: &mut Grants, value: &mut Value) -> bool {
    td_redact::redact(value, &mut |chat_id| grants.allows(chat_id, Scope::Read))
}

/// Pages a read follows when redaction leaves one short, so a short or
/// empty page does not tell that hidden chats matched.
const FILL_PAGES: usize = 10;

/// Run a read and redact it. When items were dropped from a paged answer,
/// fetch the next pages until the page holds `limit` items or ends.
fn read_page(ctx: &Ctx, grants: &mut Grants, mut request: Value) -> Result<Value, ToolError> {
    let limit = request
        .get("limit")
        .and_then(Value::as_u64)
        .and_then(|limit| usize::try_from(limit).ok())
        .unwrap_or(0);
    let mut page = td::call(&ctx.client, request.clone())?;
    let mut dropped = redact(grants, &mut page);
    for _ in 0..FILL_PAGES {
        let next = page
            .get("next_offset")
            .and_then(Value::as_str)
            .filter(|offset| !offset.is_empty())
            .map(str::to_string);
        let (Some(next), Some(key)) = (next, list_key(&page)) else {
            break;
        };
        let kept = page.get(&key).and_then(Value::as_array).map_or(0, Vec::len);
        let pages_by_offset = request.get("offset").is_some_and(Value::is_string);
        if !dropped || kept >= limit || !pages_by_offset {
            break;
        }
        if let Some(fields) = request.as_object_mut() {
            fields.insert("offset".into(), Value::String(next));
        }
        let mut more = td::call(&ctx.client, request.clone())?;
        dropped = redact(grants, &mut more);
        let items = more
            .get_mut(&key)
            .and_then(Value::as_array_mut)
            .map(std::mem::take)
            .unwrap_or_default();
        let Some(fields) = page.as_object_mut() else {
            break;
        };
        if let Some(list) = fields.get_mut(&key).and_then(Value::as_array_mut) {
            list.extend(items);
        }
        let total = fields
            .get(&key)
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        if fields.contains_key("total_count") {
            fields.insert("total_count".into(), json!(total));
        }
        let next = more.get("next_offset").cloned().unwrap_or_default();
        fields.insert("next_offset".into(), next);
    }
    Ok(page)
}

/// The list a paged answer holds: its first array field.
fn list_key(page: &Value) -> Option<String> {
    page.as_object()?
        .iter()
        .find(|(_, value)| value.is_array())
        .map(|(key, _)| key.clone())
}

/// "Confirm each send" covers raw writes too.
fn confirm(ctx: &Ctx, target: &Target, request: &Value) -> Result<(), ToolError> {
    if access::confirm_sends() {
        common::confirm(ctx, target, "td_call", &preview_of(request)?)
    } else {
        Ok(())
    }
}

fn output(method: String, result: Value) -> ToolOutput {
    let mut text = serde_json::to_string(&result).unwrap_or_default();
    if text.len() > TEXT_CAP {
        let cut = (0..=TEXT_CAP)
            .rev()
            .find(|at| text.is_char_boundary(*at))
            .unwrap_or(0);
        text.truncate(cut);
        text.push_str("… (cut; the whole answer is in structuredContent.result)");
    }
    let mut data = Map::new();
    data.insert("method".into(), Value::String(method));
    data.insert("result".into(), result);
    ToolOutput {
        text,
        data: Value::Object(data),
    }
}

/// Methods that read the contact list, which has a grant of its own.
fn is_contacts(method: &str) -> bool {
    matches!(
        method,
        "getContacts"
            | "searchContacts"
            | "getCloseFriends"
            | "searchUserByPhoneNumber"
            | "getImportedContactCount"
    )
}

#[cfg(test)]
mod tests {
    use super::{run, send_count};
    use serde_json::json;

    #[test]
    fn files_named_by_id_never_go_out() {
        for file in [
            json!({"@type": "inputFileId", "id": 1234}),
            json!({"@type": "inputFileGenerated", "original_path": "", "conversion": "#file_id#1234", "expected_size": 0}),
        ] {
            let args = json!({"request": {"@type": "sendMessage", "chat_id": 1,
                "input_message_content": {"@type": "inputMessageDocument", "document": file}}});
            let refused = run(&args).err().map(|error| error.0).unwrap_or_default();
            assert!(refused.contains("file ids reach files"), "{refused}");
        }
    }

    #[test]
    fn albums_and_forwards_pace_per_message() {
        assert_eq!(
            send_count(&json!({"input_message_contents": [{}, {}, {}]})),
            3
        );
        assert_eq!(send_count(&json!({"message_ids": [1, 2]})), 2);
        assert_eq!(send_count(&json!({})), 1);
    }
}
