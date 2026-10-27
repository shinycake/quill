//! `quill_status`: where an agent starts.

use super::messages::user_name;
use super::{Ctx, ToolError, ToolOutput};
use crate::agent::summary::WindowState as WindowStatus;
use crate::agent::ui_driver::bridge::{self, BridgeError, UiCommand, UiReply, WindowState};
use crate::agent::ui_driver::policy;
use serde_json::{Value, json};
use std::time::Duration;

/// How long `quill_status` waits for the window to say what state it is in.
const WINDOW_WAIT: Duration = Duration::from_millis(1500);

/// The main window's state as the UI reads it (`UiCommand::State`, the
/// same reading the driver's missing-frame answer uses). `None` while the
/// UI answers no driver command (Agent access or the driver off, a pause,
/// Stop) or the queue is full. A UI that does not answer in time is `not
/// answering`: the window may be there with its thread busy.
fn window_state() -> Option<WindowState> {
    if policy::switch_refusal().is_some() {
        return None;
    }
    match bridge::send(&super::current_agent(), UiCommand::State, WINDOW_WAIT) {
        Ok(UiReply::State(state)) => Some(state.window),
        Err(BridgeError::Timeout) => Some(WindowState::NotAnswering),
        Ok(_) | Err(BridgeError::Busy) => None,
    }
}

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
    let access = crate::agent::summary::live(&ctx.account);
    let mut text = format!("Quill {} is signed in as {name}", crate::version::APP);
    if let Some(username) = &username {
        text.push_str(&format!(" (@{username})"));
    }
    text.push_str(&format!(" [account {}].\n", ctx.account.0));
    text.push_str("Access (read_write also allows sending):\n");
    for line in access.text_lines() {
        text.push_str(&format!("  {line}\n"));
    }
    text.push_str("A chat that is not granted can be asked for with request_access.\n");
    text.push_str(&format!("Tools: {}.\n", tools.join(", ")));
    let ui_driver = access.window.switch;
    let driving = matches!(access.window.state, WindowStatus::Driving { .. });
    let paused = access.window.state == WindowStatus::Paused;
    let stopped = access.window.state == WindowStatus::Stopped;
    text.push_str(if !ui_driver {
        concat!(
            "UI driver: off. The user can turn on \"Let agents drive Quill's window\" in ",
            settings_place!(),
            ".\n"
        )
    } else if stopped {
        "UI driver: stopped. The user pressed Stop in Quill; don't drive its window again. Reconnecting does not undo it; only the user can, in Quill's settings.\n"
    } else if paused {
        "UI driver: paused. The user took control of Quill's window and it stays paused, across reconnects, until they press \"Let it continue\" in Quill; wait, then check quill_status again.\n"
    } else {
        "UI driver: on (read_ui_tree, take_screenshot, click_element, type_text, press_keys, scroll_view, hover_element, open_menu). \
Screenshots: one a second and 20 a minute, cut to your grants like the tree.\n"
    });
    let window = window_state();
    match window {
        Some(WindowState::Visible) | None => {}
        Some(WindowState::NotAnswering) => text.push_str(
            "Quill's window is not answering (it may be busy): the UI tools may time out; retry in a moment.\n",
        ),
        Some(state) => text.push_str(&format!(
            "Quill's window is {}: it draws no frames, so the UI tools answer that it did not redraw until the user brings it back.\n",
            state.as_str()
        )),
    }
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
            // Everything the Agent access page shows: chats, switches,
            // each tool's state and why.
            "access": access.describe(),
            "ui_driver": {
                "available": ui_driver && !stopped,
                "active": driving,
                "paused": paused,
                "stopped": stopped,
                // "visible", "hidden", "minimised", "not open" or "not
                // answering"; null while
                // the driver is off, paused or stopped.
                "window": window.map(WindowState::as_str),
                // What lifts the hold; neither clears when the agent reconnects.
                "resumes_when": if stopped {
                    Value::from("the user turns \"Let agents drive Quill's window\" off and on, or restarts Quill")
                } else if paused {
                    Value::from("the user presses \"Let it continue\" in Quill")
                } else {
                    Value::Null
                },
                "screenshots": {
                    "available": ui_driver && !stopped && access.window.screenshots,
                    "per_minute": crate::agent::ui_driver::shot::PER_MINUTE,
                    "min_interval_ms": crate::agent::ui_driver::shot::MIN_INTERVAL.as_millis() as u64,
                    "max_long_side": crate::agent::ui_driver::shot::MAX_LONG_SIDE,
                },
            },
        }),
    })
}
