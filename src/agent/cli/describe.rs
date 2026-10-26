//! `quill agent describe`: everything an agent needs to drive Quill, as
//! JSON, generated from the tool registry so it cannot drift.

use super::Exit;
use super::flags;
use serde_json::{Value, json};

pub fn describe() -> Value {
    let tools: Vec<Value> = flags::tools()
        .iter()
        .map(|tool| {
            json!({
                "name": tool.name,
                "title": tool.title,
                "description": tool.summary(),
                "read_only": tool.read_only,
                "input_schema": tool.schema,
                "example": tool.example(),
                "output": tool.output(),
                "cli": {
                    "command": tool.alias().unwrap_or(&tool.name),
                    "example": flags::example_command(tool),
                },
            })
        })
        .collect();
    let exit_codes: Vec<Value> = Exit::ALL
        .iter()
        .map(|exit| json!({"code": exit.code(), "kind": exit.kind(), "meaning": exit.meaning()}))
        .collect();
    let mcp = crate::agent::link::executable()
        .map(|exe| json!({"command": exe.to_string_lossy(), "args": ["mcp"]}));
    json!({
        "name": "quill",
        "version": crate::version::APP,
        "about": "Quill is a desktop Telegram client. With Agent access on (Quill: Settings > Advanced > Agent access), agents on this computer use the user's Telegram account through the running Quill, under the user's grants.",
        "transport": {
            "cli": "quill agent <command> [flags] [--json]; quill agent call <tool> --json '{…}' reaches every tool.",
            "mcp": mcp,
            "socket": {
                "protocol": crate::agent::server::HEADER,
                "framing": "line-delimited JSON-RPC 2.0; the first request is auth with the token in the agent-token file of Quill's data folder",
            },
        },
        "tools": tools,
        "resources": crate::agent::guide::resources(),
        "exit_codes": exit_codes,
        "errors": {
            "rule": "Every tool error says what to do next. With --json a failure prints {\"error\": {\"kind\", \"exit_code\", \"message\"}}.",
            "common": [
                {"message": "Agent access is off in Quill", "do": "Ask the user to turn it on."},
                {"message": "Quill is not signed in", "do": "Ask the user to sign in to Quill."},
                {"message": "Chat not granted", "do": "Call request_access with the chat, the scope and a reason."},
                {"message": "Several chats match", "do": "Pass the chat id from the list."},
                {"message": "rate limiting / slow down", "do": "Wait the seconds named, then retry."},
                {"message": "granted for reading only", "do": "Call request_access with scope read_write."},
                {"message": "declined this in Quill", "do": "The user said no. Do not retry."},
            ],
        },
    })
}
