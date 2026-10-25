//! `quill_status`: where an agent starts.

use super::messages::user_name;
use super::{Ctx, ToolError, ToolOutput};
use crate::agent::ui_driver::takeover;
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
    let ui_driver = crate::agent::access::ui_driver();
    let driving = matches!(takeover::current(), takeover::Phase::Driving { .. });
    let paused = takeover::paused();
    let stopped = takeover::stopped();
    text.push_str(if !ui_driver {
        "UI driver: off. The user can turn on \"Let agents drive Quill's window\" in Settings > Advanced > Agent access.\n"
    } else if stopped {
        "UI driver: stopped. The user pressed Stop in Quill; don't drive its window again in this session.\n"
    } else if paused {
        "UI driver: paused. The user took control of Quill's window; wait, then check quill_status again.\n"
    } else {
        "UI driver: on (read_ui_tree, take_screenshot, click_element, type_text, press_keys, scroll_view, hover_element, open_menu). \
Screenshots: one a second and 20 a minute, cut to your grants like the tree.\n"
    });
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
            "ui_driver": {
                "available": ui_driver && !stopped,
                "active": driving,
                "paused": paused,
                "stopped": stopped,
                "screenshots": {
                    "available": ui_driver && !stopped && crate::agent::ui_driver::shot::CAPTURE_BUILT,
                    "per_minute": crate::agent::ui_driver::shot::PER_MINUTE,
                    "min_interval_ms": crate::agent::ui_driver::shot::MIN_INTERVAL.as_millis() as u64,
                    "max_long_side": crate::agent::ui_driver::shot::MAX_LONG_SIDE,
                },
            },
        }),
    })
}
