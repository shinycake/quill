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
    let grants = crate::agent::grants::for_account(&ctx.account);
    let mut text = format!("Quill {} is signed in as {name}", crate::version::APP);
    if let Some(username) = &username {
        text.push_str(&format!(" (@{username})"));
    }
    text.push_str(&format!(" [account {}].\n", ctx.account.0));
    text.push_str("Grants (read_write also allows sending, once write tools ship):\n");
    for line in grants.summary_lines() {
        text.push_str(&format!("  {line}\n"));
    }
    text.push_str("A chat that is not granted can be asked for with request_access.\n");
    text.push_str(&format!("Tools: {}.\n", tools.join(", ")));
    text.push_str("UI driver: not available in this version.\n");
    text.push_str("Next: list_unread to catch up, or list_chats to find a chat.");
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
            "grants": grants.describe(),
            "tools": tools,
            "ui_driver": {"available": false, "active": false},
        }),
    })
}
