//! What orients an agent: the MCP server instructions and the resources.

use serde_json::{Value, json};

/// The MCP `instructions` sent at `initialize`.
pub const INSTRUCTIONS: &str = "Quill is the user's desktop Telegram client. Through this server you use \
the user's Telegram account via the Quill that is already running on this computer. \
Start with quill_status: it shows the signed-in account, the chats you may use and the tools that work now. \
Capability comes in four layers: curated tools, raw TDLib calls, a UI driver and events; this version has \
curated read and write tools, events (wait_for_message) and td_call for anything else TDLib can do. You act as the user: sends go out in their name and \
show in their activity log in Quill. The user grants chats in Quill: by default Saved Messages and bots (read and \
write) and the user's own channels (read); anything else answers \"Chat not granted\", and request_access asks the user. \
Chats can be named by id, title, @username or \"saved\". Errors say what to do next. Read quill://guide for recipes. \
Scripts and agents without MCP reach the same tools with the `quill agent` command line (quill agent --help), \
and `quill agent install-skill` installs a Claude Code skill for Quill.";

pub const GUIDE_URI: &str = "quill://guide";
pub const GRANTS_URI: &str = "quill://grants";
/// Unread messages in granted chats; subscribable.
pub const UNREAD_URI: &str = "quill://unread";
pub const GUIDE: &str = include_str!("guide.md");

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
        json!({
            "uri": UNREAD_URI,
            "name": "unread",
            "title": "Unread in granted chats",
            "description": "Granted chats with unread messages and the newest of them. Subscribe to hear when a new message arrives.",
            "mimeType": "application/json",
        }),
    ]
}

/// The grants of the account agents act on.
fn grants() -> Value {
    let account = crate::agent::grants::open_account();
    json!({
        "account": account.0,
        "grants": crate::agent::grants::for_account(&account).describe(),
        "access": crate::agent::summary::live(&account).describe(),
    })
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
