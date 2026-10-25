//! What orients an agent: the MCP server instructions and the resources.

use serde_json::{Value, json};

/// The MCP `instructions` sent at `initialize`.
pub const INSTRUCTIONS: &str = "Quill is the user's desktop Telegram client. Through this server you use \
the user's Telegram account via the Quill that is already running on this computer. \
Start with quill_status: it shows the signed-in account, the chats you may use and the tools that work now. \
Capability comes in four layers: curated tools, raw TDLib calls, a UI driver and events; this version has \
read-only curated tools. The user grants chats in Quill; right now agents may read Saved Messages only \
(pass \"chat\": \"saved\"). Errors say what to do next. Read quill://guide for recipes.";

pub const GUIDE_URI: &str = "quill://guide";
pub const GRANTS_URI: &str = "quill://grants";
const GUIDE: &str = include_str!("guide.md");

/// The `resources` array of a `resources/list` answer.
pub fn resources() -> Vec<Value> {
    vec![
        json!({
            "uri": GUIDE_URI,
            "name": "guide",
            "title": "Using Quill as an agent",
            "description": "What Quill can do for an agent, the tools, grants, recipes and errors.",
            "mimeType": "text/markdown",
        }),
        json!({
            "uri": GRANTS_URI,
            "name": "grants",
            "title": "Granted chats",
            "description": "The chats agents may use and with what scope (read or read+write).",
            "mimeType": "application/json",
        }),
    ]
}

/// The chats agents may use. Saved Messages, read only, in this version.
fn grants() -> Value {
    json!({"grants": [{"chat": "Saved Messages", "chat_ref": "saved", "scope": "read"}]})
}

/// A `resources/read` result, or `None` for an unknown URI.
pub fn read_resource(uri: &str) -> Option<Value> {
    let (mime, text) = match uri {
        GUIDE_URI => ("text/markdown", GUIDE.to_string()),
        GRANTS_URI => ("application/json", grants().to_string()),
        _ => return None,
    };
    Some(json!({"contents": [{"uri": uri, "mimeType": mime, "text": text}]}))
}
