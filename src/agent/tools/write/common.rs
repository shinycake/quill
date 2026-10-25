//! What every write tool shares: text formats, reply and send options, the
//! "Confirm each send" prompt, the paced send with its delivery wait, and
//! the action log.

use super::super::target::Target;
use super::super::{Ctx, ToolError, current_agent};
use crate::agent::activity::{self, Action};
use crate::agent::events::{self, Sent};
use crate::agent::pacing::{self, Refusal};
use crate::agent::prompts::{self, Ask, Decision};
use crate::agent::td::{self, TdError};
use crate::agent::{access, grants::ChatClass};
use serde_json::{Value, json};
use std::time::Duration;

/// How long the user has to answer a confirmation.
const ANSWER_WAIT: Duration = Duration::from_secs(120);
/// How long a send waits for Telegram to deliver it.
const DELIVERY_WAIT: Duration = Duration::from_secs(30);
/// Telegram's limits on one message's text and on a caption.
pub const TEXT_LIMIT: usize = 4096;
pub const CAPTION_LIMIT: usize = 1024;

pub fn format_schema() -> Value {
    json!({
        "type": "string",
        "enum": ["plain", "markdown", "html"],
        "default": "plain",
        "description": "How to read the text: plain (as is), markdown (Telegram MarkdownV2: *bold* _italic_ `code` [link](https://…); escape _*[]()~`>#+-=|{}.! with \\ elsewhere) or html (<b>, <i>, <code>, <pre>, <a href>).",
    })
}

pub fn reply_schema() -> Value {
    json!({"type": "integer", "description": "The id of a message in the same chat to reply to."})
}

pub fn schedule_schema() -> Value {
    json!({
        "type": ["string", "integer"],
        "description": "Send later instead of now: a UTC time like \"2026-10-25T18:00:00Z\" (an offset like +02:00 works too) or Unix seconds. Scheduled messages wait in the chat's scheduled list.",
    })
}

pub fn silent_schema() -> Value {
    json!({"type": "boolean", "default": false, "description": "Send without a notification sound."})
}

pub fn ids_schema(what: &str) -> Value {
    json!({"type": "array", "items": {"type": "integer"}, "minItems": 1, "maxItems": 100, "description": what})
}

/// A required, non-empty string argument.
pub fn required_text(args: &Value, key: &str) -> Result<String, ToolError> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| ToolError::new(format!("{key} is required and may not be empty.")))
}

/// An optional message id argument.
pub fn message_id(args: &Value, key: &str) -> Result<Option<i64>, ToolError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_i64()
            .or_else(|| value.as_str().and_then(|s| s.trim().parse().ok()))
            .map(Some)
            .ok_or_else(|| ToolError::new(format!("{key} must be a message id (a number)."))),
    }
}

/// A required list of 1 to 100 message ids.
pub fn message_ids(args: &Value, key: &str) -> Result<Vec<i64>, ToolError> {
    let ids: Vec<i64> = match args.get(key) {
        Some(Value::Array(list)) => list.iter().filter_map(Value::as_i64).collect(),
        Some(value) => value.as_i64().into_iter().collect(),
        None => Vec::new(),
    };
    if ids.is_empty() || ids.len() > 100 {
        return Err(ToolError::new(format!(
            "{key} must list 1 to 100 message ids, e.g. [4194304]. Message ids come from list_messages or read_history."
        )));
    }
    Ok(ids)
}

/// `text` as a TDLib `formattedText`, read in the `format` argument's way.
pub fn formatted(ctx: &Ctx, text: &str, args: &Value, limit: usize) -> Result<Value, ToolError> {
    let format = args
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("plain");
    let parsed = match format {
        "plain" => json!({"@type": "formattedText", "text": text, "entities": []}),
        "markdown" | "html" => {
            let mode = if format == "html" {
                json!({"@type": "textParseModeHTML"})
            } else {
                json!({"@type": "textParseModeMarkdown", "version": 2})
            };
            td::call(
                &ctx.client,
                json!({"@type": "parseTextEntities", "text": text, "parse_mode": mode}),
            )
            .map_err(|error| match error {
                TdError::Td { message, .. } if format == "html" => ToolError::new(format!(
                    "The text is not valid Telegram HTML ({message}). Fix the tags, or pass \"format\": \"plain\"."
                )),
                TdError::Td { message, .. } => ToolError::new(format!(
                    "The text is not valid Telegram MarkdownV2 ({message}). Escape _*[]()~`>#+-=|{{}}.! with a backslash where they are not formatting, or pass \"format\": \"plain\"."
                )),
                other => ToolError::from(other),
            })?
        }
        other => {
            return Err(ToolError::new(format!(
                "format is \"plain\", \"markdown\" or \"html\", not \"{other}\"."
            )));
        }
    };
    let length = parsed
        .get("text")
        .and_then(Value::as_str)
        .map_or(0, |t| t.encode_utf16().count());
    if length > limit {
        return Err(ToolError::new(format!(
            "The text is {length} characters; Telegram allows {limit}. Split it into several messages."
        )));
    }
    Ok(parsed)
}

/// `inputMessageReplyToMessage` for the `reply_to` argument, or null.
pub fn reply_to(args: &Value) -> Result<Value, ToolError> {
    Ok(match message_id(args, "reply_to")? {
        None => Value::Null,
        Some(id) => json!({
            "@type": "inputMessageReplyToMessage",
            "message_id": id,
            "quote": Value::Null,
            "checklist_task_id": 0,
            "poll_option_id": "",
        }),
    })
}

/// `messageSendOptions` for `silent` and `schedule_at`.
pub fn send_options(args: &Value) -> Result<Value, ToolError> {
    let silent = args.get("silent").and_then(Value::as_bool).unwrap_or(false);
    let scheduling = match args.get("schedule_at") {
        None | Some(Value::Null) => Value::Null,
        Some(value) => {
            let at = value
                .as_i64()
                .or_else(|| value.as_str().and_then(parse_time))
                .ok_or_else(|| {
                    ToolError::new(
                        "schedule_at must be a UTC time like \"2026-10-25T18:00:00Z\" or Unix seconds.",
                    )
                })?;
            if at <= activity_now() + 10 {
                return Err(ToolError::new(
                    "schedule_at is in the past. Leave it out to send now, or pass a later time.",
                ));
            }
            let at =
                i32::try_from(at).map_err(|_| ToolError::new("schedule_at is too far ahead."))?;
            json!({"@type": "messageSchedulingStateSendAtDate", "send_date": at, "repeat_period": 0})
        }
    };
    Ok(json!({
        "@type": "messageSendOptions",
        "suggested_post_info": Value::Null,
        "disable_notification": silent,
        "from_background": false,
        "protect_content": false,
        "allow_paid_broadcast": false,
        "paid_message_star_count": 0,
        "update_order_of_installed_sticker_sets": false,
        "scheduling_state": scheduling,
        "effect_id": "0",
        "sending_id": 0,
        "only_preview": false,
    }))
}

fn activity_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// Unix seconds of "YYYY-MM-DDTHH:MM[:SS](Z|±HH:MM)".
pub fn parse_time(text: &str) -> Option<i64> {
    let text = text.trim();
    let num = |range: std::ops::Range<usize>| -> Option<i64> {
        let part = text.get(range)?;
        part.bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| part.parse().ok())?
    };
    let (year, month, day) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (hour, minute) = (num(11..13)?, num(14..16)?);
    let separators = [(4, b'-'), (7, b'-'), (13, b':')];
    if separators
        .iter()
        .any(|(at, want)| text.as_bytes().get(*at) != Some(want))
        || !matches!(text.as_bytes().get(10), Some(b'T' | b't' | b' '))
    {
        return None;
    }
    let (second, rest) = if text.as_bytes().get(16) == Some(&b':') {
        (num(17..19)?, text.get(19..)?)
    } else {
        (0, text.get(16..)?)
    };
    let rest = rest.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    let offset = match rest {
        "Z" | "z" => 0,
        _ => {
            let sign = match rest.as_bytes().first()? {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            let hours: i64 = rest.get(1..3)?.parse().ok()?;
            let minutes: i64 = rest.get(4..6).map_or(Some(0), |m| m.parse().ok())?;
            sign * (hours * 3600 + minutes * 60)
        }
    };
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    // Days from the civil date (Howard Hinnant's algorithm).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hour * 3600 + minute * 60 + second - offset)
}

/// While "Confirm each send" is on, wait for the user to allow this write.
pub fn confirm(
    ctx: &Ctx,
    target: &Target,
    tool: &str,
    action: &str,
    preview: &str,
) -> Result<(), ToolError> {
    if !access::confirm_sends() {
        return Ok(());
    }
    let ask = Ask::Write {
        chat_id: target.chat_id,
        title: target.title.clone(),
        action: action.to_string(),
        preview: preview.chars().take(1000).collect(),
    };
    let (id, rx) = prompts::file(ctx.account.clone(), current_agent(), ask, String::new())
        .ok_or_else(|| {
            ToolError::new("Several requests are already waiting for the user in Quill. Wait for those answers, then retry.")
        })?;
    match prompts::wait(id, &rx, ANSWER_WAIT) {
        Some(Decision::Allowed) => Ok(()),
        Some(Decision::Denied) => {
            log(ctx, target, tool, &format!("Declined: {action}"), false);
            Err(ToolError::new(
                "The user declined this in Quill. Do not retry it; ask the user what they want instead.",
            ))
        }
        None => Err(ToolError::new(format!(
            "The user did not confirm within {} seconds (Quill asks before each write because \"Confirm each send\" is on). Tell the user, then retry.",
            ANSWER_WAIT.as_secs()
        ))),
    }
}

/// Add a line to the action log the user sees in Settings.
pub fn log(ctx: &Ctx, target: &Target, tool: &str, summary: &str, ok: bool) {
    activity::record(Action {
        time: 0,
        account: ctx.account.clone(),
        agent: current_agent(),
        tool: tool.to_string(),
        chat_id: target.chat_id,
        chat_title: target.title.clone(),
        summary: summary.chars().take(200).collect(),
        ok,
    });
}

/// Run a write that is not a send, logging how it went.
pub fn change(
    ctx: &Ctx,
    target: &Target,
    tool: &str,
    summary: &str,
    request: Value,
) -> Result<Value, ToolError> {
    let result = td::call(&ctx.client, request).map_err(write_error);
    log(ctx, target, tool, summary, result.is_ok());
    result
}

/// A TDLib error from a write, as the agent's next step. Flood waits also
/// pause every agent send.
pub fn write_error(error: TdError) -> ToolError {
    match error {
        TdError::Td {
            code: 429,
            ref message,
        }
        | TdError::Td {
            code: 420,
            ref message,
        } => {
            let seconds = pacing::flood_seconds(message).unwrap_or(60);
            pacing::flood(seconds);
            ToolError::new(format!(
                "Telegram asked Quill to slow down. Agent sends are paused for {seconds} seconds; retry after that."
            ))
        }
        TdError::Td { ref message, .. } if is_paid_error(message) => paid_refusal(),
        TdError::Td {
            code: 400,
            ref message,
        } if message.contains("CHAT_WRITE_FORBIDDEN")
            || message.contains("Have no write access") =>
        {
            ToolError::new("The user cannot write in this chat (no permission). Tell the user.")
        }
        other => ToolError::from(other),
    }
}

fn is_paid_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("payment") || lower.contains("paid_message") || lower.contains("star")
}

fn paid_refusal() -> ToolError {
    ToolError::new(
        "This chat charges Stars for each message, and agents cannot spend Stars. Ask the user to send it from Quill.",
    )
}

/// Stars the account would pay per message in `chat_id`.
fn stars_per_message(ctx: &Ctx, chat_id: i64) -> Result<i64, ToolError> {
    let chat = ctx.call(json!({"@type": "getChat", "chat_id": chat_id}))?;
    let kind = chat.get("type").cloned().unwrap_or(Value::Null);
    let count = |value: Option<&Value>| -> i64 {
        value
            .and_then(|v| {
                v.as_i64()
                    .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            })
            .unwrap_or(0)
    };
    Ok(match kind.get("@type").and_then(Value::as_str) {
        Some("chatTypePrivate") => {
            let user_id = kind.get("user_id").and_then(Value::as_i64).unwrap_or(0);
            ctx.call(json!({"@type": "getUserFullInfo", "user_id": user_id}))
                .map(|info| count(info.get("outgoing_paid_message_star_count")))
                .unwrap_or(0)
        }
        Some("chatTypeSupergroup") => {
            let id = kind
                .get("supergroup_id")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            ctx.call(json!({"@type": "getSupergroup", "supergroup_id": id}))
                .map(|group| count(group.get("paid_message_star_count")))
                .unwrap_or(0)
        }
        _ => 0,
    })
}

/// Whether a message to `target` would be the first to someone who never
/// wrote to the user (a private chat with a non-contact, nothing incoming).
fn is_new_peer(ctx: &Ctx, target: &Target) -> bool {
    if target.class != ChatClass::Other {
        return false;
    }
    let Ok(chat) = ctx.call(json!({"@type": "getChat", "chat_id": target.chat_id})) else {
        return false;
    };
    let kind = chat.get("type");
    if kind.and_then(|k| k.get("@type")).and_then(Value::as_str) != Some("chatTypePrivate") {
        return false;
    }
    let user_id = kind
        .and_then(|k| k.get("user_id"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let is_contact = ctx
        .call(json!({"@type": "getUser", "user_id": user_id}))
        .ok()
        .and_then(|user| user.get("is_contact").and_then(Value::as_bool))
        .unwrap_or(false);
    if is_contact {
        return false;
    }
    let history = ctx.call(json!({
        "@type": "getChatHistory", "chat_id": target.chat_id,
        "from_message_id": 0, "offset": 0, "limit": 50, "only_local": false,
    }));
    !history.is_ok_and(|answer| {
        super::super::messages::messages_of(&answer)
            .iter()
            .any(|m| m.get("is_outgoing").and_then(Value::as_bool) == Some(false))
    })
}

/// Send messages to `target` through the paced queue and wait until
/// Telegram delivers them. Returns the messages with their final ids.
pub fn send(
    ctx: &Ctx,
    target: &Target,
    tool: &str,
    summary: &str,
    request: Value,
    count: u32,
) -> Result<Vec<Value>, ToolError> {
    let result = send_inner(ctx, target, request, count);
    log(ctx, target, tool, summary, result.is_ok());
    result
}

fn send_inner(
    ctx: &Ctx,
    target: &Target,
    request: Value,
    count: u32,
) -> Result<Vec<Value>, ToolError> {
    if stars_per_message(ctx, target.chat_id)? > 0 {
        return Err(paid_refusal());
    }
    let new_peer = is_new_peer(ctx, target);
    pacing::take_turn(ctx.client.id(), target.chat_id, count, new_peer).map_err(|refusal| match refusal {
        Refusal::Flood(seconds) => ToolError::new(format!(
            "Telegram asked Quill to slow down. Agent sends are paused for {seconds} more seconds; retry after that."
        )),
        Refusal::NewPeers => ToolError::new(
            "Agents may start at most 5 new conversations a day (first messages to people who never wrote to the user). Ask the user to send this one from Quill.",
        ),
        Refusal::Busy => ToolError::new("Too many agent messages are waiting to go out. Retry in a minute."),
    })?;
    let answer = td::call(&ctx.client, request).map_err(write_error)?;
    let pending: Vec<Value> = match answer.get("messages").and_then(Value::as_array) {
        Some(list) => list.clone(),
        None => vec![answer],
    };
    let waiting: Vec<i64> = pending
        .iter()
        .filter(|m| m.get("sending_state").is_some_and(|s| !s.is_null()))
        .filter_map(|m| m.get("id").and_then(Value::as_i64))
        .collect();
    let outcomes = events::wait_sent(ctx.client.id(), &waiting, DELIVERY_WAIT);
    let mut delivered = Vec::with_capacity(pending.len());
    for message in pending {
        let id = message.get("id").and_then(Value::as_i64);
        let outcome = waiting
            .iter()
            .position(|w| Some(*w) == id)
            .and_then(|at| outcomes.get(at).cloned().flatten());
        match outcome {
            Some(Sent::Delivered(final_message)) => delivered.push(final_message),
            Some(Sent::Failed(code, message)) => {
                return Err(write_error(TdError::Td { code, message }));
            }
            None => delivered.push(message),
        }
    }
    Ok(delivered)
}

#[cfg(test)]
mod tests {
    use super::parse_time;

    #[test]
    fn times_parse_as_utc() {
        assert_eq!(parse_time("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_time("2026-10-25T18:00:00Z"), Some(1_792_951_200));
        assert_eq!(parse_time("2026-10-25T20:00+02:00"), Some(1_792_951_200));
        assert_eq!(parse_time("2026-10-25 18:00:30.5Z"), Some(1_792_951_230));
        assert_eq!(parse_time("tomorrow"), None);
        assert_eq!(parse_time("2026-13-01T00:00Z"), None);
    }
}
