//! `quill_status`: where an agent starts.

use super::messages::user_name;
use super::{Ctx, ToolError, ToolOutput};
use serde_json::{Value, json};

pub fn run(_args: &Value) -> Result<ToolOutput, ToolError> {
    let ctx = Ctx::active()?;
    let me = ctx.call(json!({"@type": "getMe"}))?;
    let name = user_name(&me);
    let username = me
        .get("usernames")
        .and_then(|u| u.get("active_usernames"))
        .and_then(Value::as_array)
        .and_then(|list| list.first())
        .and_then(Value::as_str)
        .map(str::to_string);
    let tools = super::names();
    let grants = json!([{"chat": "Saved Messages", "chat_ref": "saved", "scope": "read"}]);
    let mut text = format!("Quill {} is signed in as {name}", crate::version::APP);
    if let Some(username) = &username {
        text.push_str(&format!(" (@{username})"));
    }
    text.push_str(&format!(" [account {}].\n", ctx.account.0));
    text.push_str("Granted chats: Saved Messages (read). Other chats are not available to agents in this version.\n");
    text.push_str(&format!("Tools: {}.\n", tools.join(", ")));
    text.push_str("UI driver: not available in this version.\n");
    text.push_str("Next: list_messages {\"chat\": \"saved\"} shows the latest Saved Messages.");
    Ok(ToolOutput {
        text,
        data: json!({
            "quill_version": crate::version::APP,
            "account": {
                "key": ctx.account.0,
                "name": name,
                "username": username,
                "user_id": me.get("id"),
            },
            "grants": grants,
            "tools": tools,
            "ui_driver": {"available": false, "active": false},
        }),
    })
}
