//! The tools agents call, as MCP describes them: a catalog for `tools/list`
//! and one dispatcher for `tools/call`.
//!
//! Every tool answers with readable text (what the model reads) and the same
//! data as JSON (`structuredContent`). Every error says what to do next.

mod messages;
mod saved;
mod status;

use crate::agent::td::{self, Client, TdError};
use crate::ids::AccountKey;
use serde_json::{Value, json};

/// What one tool call produced.
pub struct ToolOutput {
    pub text: String,
    pub data: Value,
}

/// A failed call, worded as the next step for the agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolError(pub String);

impl ToolError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl From<TdError> for ToolError {
    fn from(error: TdError) -> Self {
        match error {
            TdError::Td { code: 401, .. } => ToolError::new(
                "Quill is not signed in to Telegram yet. Ask the user to sign in to Quill, then retry.",
            ),
            TdError::Td { code: 429, message } => ToolError::new(format!(
                "Telegram is rate limiting this account ({message}). Wait the number of seconds it names, then retry."
            )),
            TdError::Td { code, message } => {
                ToolError::new(format!("Telegram returned error {code}: {message}."))
            }
            TdError::Timeout => ToolError::new(
                "Quill got no answer from Telegram within 30 seconds. Call quill_status to check the connection, then retry.",
            ),
            TdError::Send => ToolError::new(
                "Quill could not pass the request to Telegram. Call quill_status, then retry.",
            ),
        }
    }
}

/// The account a call acts on and its live client.
pub(crate) struct Ctx {
    pub account: AccountKey,
    pub client: Client,
}

impl Ctx {
    fn active() -> Result<Self, ToolError> {
        let root = crate::settings::safe_app_root().ok_or_else(|| {
            ToolError::new("Quill has no data folder. Restart Quill, then retry.")
        })?;
        let account = crate::settings::active_account(&root);
        let client = td::client_for(&account).ok_or_else(|| {
            ToolError::new(
                "Quill has no connected account yet. Ask the user to open Quill and sign in, then retry.",
            )
        })?;
        Ok(Self { account, client })
    }

    pub fn call(&self, request: Value) -> Result<Value, ToolError> {
        td::call(&self.client, request).map_err(ToolError::from)
    }
}

struct Tool {
    name: &'static str,
    title: &'static str,
    description: &'static str,
    schema: fn() -> Value,
    run: fn(&Value) -> Result<ToolOutput, ToolError>,
    read_only: bool,
}

const TOOLS: &[Tool] = &[
    Tool {
        name: "quill_status",
        title: "Quill status",
        description: "Start here. Shows whether Quill is signed in, which account agents act on, \
which chats this agent may read or write, and which tools work right now. \
Example input: {}. Output: account (name, username, user_id), grants (chat, scope), tools, ui_driver.",
        schema: no_arguments,
        run: status::run,
        read_only: true,
    },
    Tool {
        name: "list_messages",
        title: "List recent messages",
        description: "Use to see what is new in a chat: the latest messages, one line each, oldest first, \
long texts cut to 200 characters. Then call get_message for one message in full or read_history to page further back. \
Example input: {\"chat\": \"saved\", \"limit\": 20}. Output: chat, messages [{id, date, from, kind, text}], older_cursor \
(pass it to read_history as before_message_id).",
        schema: saved::list_schema,
        run: saved::list_messages,
        read_only: true,
    },
    Tool {
        name: "read_history",
        title: "Read chat history",
        description: "Use to read a stretch of a chat in full, paging older or newer from a message id. \
Example input: {\"chat\": \"saved\", \"before_message_id\": 4194304, \"limit\": 50}, or {\"chat\": \"saved\", \
\"after_message_id\": 4194304}. Output: chat, messages [{id, date, from, kind, text, reply_to, forwarded_from, file}] \
oldest first, older_cursor and newer_cursor for the next page (null when there is none).",
        schema: saved::history_schema,
        run: saved::read_history,
        read_only: true,
    },
    Tool {
        name: "search_messages",
        title: "Search messages",
        description: "Use to find messages containing words in a chat. Matches whole words and prefixes, not substrings. \
Example input: {\"chat\": \"saved\", \"query\": \"invoice\", \"limit\": 20}. Output: chat, total_count, messages \
[{id, date, from, kind, text}] newest first, next_cursor (pass it back as before_message_id for more).",
        schema: saved::search_schema,
        run: saved::search_messages,
        read_only: true,
    },
    Tool {
        name: "get_message",
        title: "Get one message",
        description: "Use to read one message in full, with its reply target, forward origin, file details and links. \
Example input: {\"chat\": \"saved\", \"message_id\": 4194304}. Output: one message {id, date, edited, from, kind, text, \
reply_to, forwarded_from, file, links}.",
        schema: saved::get_schema,
        run: saved::get_message,
        read_only: true,
    },
];

fn no_arguments() -> Value {
    json!({"type": "object", "properties": {}, "additionalProperties": false})
}

/// The `tools` array of a `tools/list` answer.
pub fn catalog() -> Vec<Value> {
    TOOLS
        .iter()
        .map(|tool| {
            json!({
                "name": tool.name,
                "title": tool.title,
                "description": tool.description,
                "inputSchema": (tool.schema)(),
                "annotations": {
                    "readOnlyHint": tool.read_only,
                    "openWorldHint": true,
                },
            })
        })
        .collect()
}

/// The names of every tool, for `quill_status`.
pub(crate) fn names() -> Vec<&'static str> {
    TOOLS.iter().map(|tool| tool.name).collect()
}

/// Run tool `name` and shape the MCP `tools/call` result.
pub fn call(name: &str, arguments: &Value) -> Value {
    let result = match TOOLS.iter().find(|tool| tool.name == name) {
        Some(tool) => (tool.run)(arguments),
        None => Err(ToolError::new(format!(
            "There is no tool named {name}. Call tools/list, or quill_status, to see the tools."
        ))),
    };
    match result {
        Ok(output) => json!({
            "content": [{"type": "text", "text": output.text}],
            "structuredContent": output.data,
            "isError": false,
        }),
        Err(ToolError(message)) => json!({
            "content": [{"type": "text", "text": message}],
            "isError": true,
        }),
    }
}

/// An optional bounded integer argument.
pub(crate) fn int_arg(
    args: &Value,
    key: &str,
    default: i64,
    min: i64,
    max: i64,
) -> Result<i64, ToolError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(value) => value
            .as_i64()
            .or_else(|| value.as_str().and_then(|s| s.trim().parse().ok()))
            .map(|n| n.clamp(min, max))
            .ok_or_else(|| {
                ToolError::new(format!(
                    "{key} must be a whole number between {min} and {max}."
                ))
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::{TOOLS, call, catalog};

    #[test]
    fn every_tool_is_described_with_an_example_and_output() {
        for tool in TOOLS {
            assert!(tool.description.contains("Example input"), "{}", tool.name);
            assert!(tool.description.contains("Output"), "{}", tool.name);
            assert!(
                tool.name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_')
            );
        }
        assert_eq!(catalog().len(), TOOLS.len());
    }

    #[test]
    fn unknown_tools_say_what_to_do() {
        let result = call("send_everything", &serde_json::json!({}));
        assert_eq!(result["isError"], true);
        assert!(
            result["content"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .contains("tools/list")
        );
    }
}
