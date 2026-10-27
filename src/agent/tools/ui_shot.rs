//! Screenshots of Quill's window for the UI driver: `take_screenshot`, and
//! the `screenshot` option of the input tools (`super::ui`). The window
//! draws a frame with only what the agent may see and encodes it
//! (`src/ui/agent_driver_shot.rs`); here the call is checked against the
//! switches and the rate limit (`crate::agent::ui_driver::shot`), and the
//! PNG is attached to the MCP answer as an image block.

use super::{Tool, ToolError, ToolOutput, current_agent};
use crate::agent::ui_driver::bridge::{self, UiCommand, UiReply};
use crate::agent::ui_driver::policy;
use crate::agent::ui_driver::shot::{self, Shot, ShotOptions, ShotScope};
use base64::Engine as _;
use serde_json::{Value, json};
use std::cell::RefCell;

pub(super) const TOOLS: &[Tool] = &[Tool {
    name: "take_screenshot",
    title: "Screenshot Quill's window",
    description: "Use to see Quill's window as a picture when read_ui_tree does not tell you enough: the layout, an image, \
whether something rendered. Needs \"Let agents drive Quill's window\" on in Quill. It shows exactly what read_ui_tree may show: \
with the chat grants alone, everything outside the granted chats and the chat list's own parts is painted over in grey; \
with \"All other chats\" on, the whole window except chats that still need their own grant. Settings, agent prompts, payments \
and the lock screen are refused with the same note as read_ui_tree. Quill's takeover glow, ghost cursor and pill are left out. \
Screenshots are for operating Quill, not for archiving chats: at most one a second and 20 a minute. \
The input tools take \"screenshot\": true for a picture after their step. \
Example input: {}, or {\"full_resolution\": true}. \
Output: a PNG image (long side at most 1280 px unless full_resolution) and {width, height, scale (image pixels per window \
pixel: divide image coordinates by it for click_element), scope (granted or all_chats), frame}.",
    schema: screenshot_schema,
    run: take_screenshot,
    read_only: true,
    confirms_sends: false,
}];

const COMMAND_WAIT: std::time::Duration = std::time::Duration::from_secs(15);

fn screenshot_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "full_resolution": full_resolution_property(),
        },
        "additionalProperties": false,
    })
}

fn full_resolution_property() -> Value {
    json!({"type": "boolean", "default": false, "description": "Keep the window's device pixels instead of shrinking the long side to 1280."})
}

/// `schema` with the input tools' screenshot options added.
pub(super) fn with_options(mut schema: Value) -> Value {
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        properties.insert(
            "screenshot".into(),
            json!({"type": "boolean", "default": false, "description": "Also return a screenshot of the window after the step, as take_screenshot would (counts against its limits)."}),
        );
        properties.insert("full_resolution".into(), full_resolution_property());
    }
    schema
}

/// What an input tool asked for: the screenshot to take, or why none is
/// taken although one was asked for.
pub(super) struct Asked {
    pub options: Option<ShotOptions>,
    pub skipped: Option<String>,
}

/// The screenshot options in `args`, counted against the limits. Over the
/// limits the input still runs, without a screenshot.
pub(super) fn asked(args: &Value) -> Asked {
    let wanted = args
        .get("screenshot")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !wanted {
        return Asked {
            options: None,
            skipped: None,
        };
    }
    match shot::reserve() {
        Ok(()) => Asked {
            options: Some(options(args)),
            skipped: None,
        },
        Err(note) => Asked {
            options: None,
            skipped: Some(note),
        },
    }
}

fn options(args: &Value) -> ShotOptions {
    ShotOptions {
        full_resolution: args
            .get("full_resolution")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

thread_local! {
    /// Images the tool running on this thread attaches to its answer.
    static ATTACHED: RefCell<Vec<Value>> = const { RefCell::new(Vec::new()) };
}

/// Attach `shot` to the answer of the tool running on this thread.
fn attach(shot: &Shot) {
    let data = base64::engine::general_purpose::STANDARD.encode(&shot.png);
    let block = json!({"type": "image", "data": data, "mimeType": "image/png"});
    let _ = ATTACHED.try_with(|attached| attached.borrow_mut().push(block));
}

/// The image blocks attached since the last call, for the MCP answer.
pub(super) fn take_attached() -> Vec<Value> {
    ATTACHED
        .try_with(|attached| std::mem::take(&mut *attached.borrow_mut()))
        .unwrap_or_default()
}

/// The screenshot's facts for `structuredContent` (the PNG goes as an
/// image block).
fn shot_json(shot: &Shot, frame: u64) -> Value {
    json!({
        "width": shot.width,
        "height": shot.height,
        "scale": shot.scale,
        "scope": shot.scope.as_str(),
        "frame": frame,
        "bytes": shot.png.len(),
    })
}

/// One line on the screenshot for the model.
fn shot_line(shot: &Shot) -> String {
    let scope = match shot.scope {
        ShotScope::Granted => {
            "Grey areas are parts of the window outside your grants; they are not empty."
        }
        ShotScope::AllChats => {
            "Grey areas, if any, are chats that need their own grant; they are not empty."
        }
    };
    format!(
        "Screenshot: {}x{} PNG, {:.3} image pixels per window pixel (divide image coordinates by it for click_element). {scope}",
        shot.width, shot.height, shot.scale
    )
}

/// Attach the screenshot an input step returned and describe it in `text`
/// and `data`; `skipped` says why none was taken.
pub(super) fn add_to_answer(
    result: Option<Result<Shot, String>>,
    skipped: Option<String>,
    frame: u64,
    text: &mut String,
    data: &mut Value,
) {
    let note = match (result, skipped) {
        (Some(Ok(shot)), _) => {
            attach(&shot);
            text.push('\n');
            text.push_str(&shot_line(&shot));
            if let Some(map) = data.as_object_mut() {
                map.insert("screenshot".into(), shot_json(&shot, frame));
            }
            return;
        }
        (Some(Err(note)), _) | (None, Some(note)) => note,
        (None, None) => return,
    };
    text.push_str("\nNo screenshot: ");
    text.push_str(&note);
    if let Some(map) = data.as_object_mut() {
        map.insert("screenshot".into(), json!({"error": note}));
    }
}

pub fn take_screenshot(args: &Value) -> Result<ToolOutput, ToolError> {
    if let Some(refusal) = policy::switch_refusal() {
        return Err(ToolError::new(refusal));
    }
    shot::reserve().map_err(ToolError::new)?;
    let reply = bridge::send_with_shot(
        &current_agent(),
        UiCommand::ReadTree,
        Some(options(args)),
        COMMAND_WAIT,
    )
    .map_err(super::ui::bridge_error)?;
    let (tree, shot) = match reply {
        UiReply::Shot { tree, shot } => (tree, shot),
        UiReply::Refused(reason) => return Err(ToolError::new(reason)),
        _ => {
            return Err(ToolError::new(
                "Quill's window gave an unexpected answer. Retry.",
            ));
        }
    };
    attach(&shot);
    let text = format!(
        "Quill window {}x{} px (scale {}). {}",
        tree.width,
        tree.height,
        tree.scale,
        shot_line(&shot)
    );
    let mut data = shot_json(&shot, tree.frame);
    if let Some(map) = data.as_object_mut() {
        map.insert(
            "window".into(),
            json!({"width": tree.width, "height": tree.height, "scale": tree.scale}),
        );
    }
    Ok(ToolOutput { text, data })
}

#[cfg(test)]
mod tests {
    use super::{add_to_answer, take_attached, take_screenshot, with_options};
    use crate::agent::ui_driver::shot::{Shot, ShotScope};
    use serde_json::json;

    #[test]
    fn the_input_tools_take_the_screenshot_options() {
        let schema = with_options(json!({"type": "object", "properties": {"text": {}}}));
        assert_eq!(schema["properties"]["screenshot"]["type"], "boolean");
        assert_eq!(schema["properties"]["full_resolution"]["type"], "boolean");
        assert!(schema["properties"]["text"].is_object());
    }

    #[test]
    fn a_screenshot_reaches_the_answer_as_an_image_block() {
        let _ = take_attached();
        let shot = Shot {
            png: vec![0x89, b'P', b'N', b'G'],
            width: 640,
            height: 480,
            scale: 0.5,
            scope: ShotScope::Granted,
        };
        let mut text = String::new();
        let mut data = json!({});
        add_to_answer(Some(Ok(shot)), None, 7, &mut text, &mut data);
        assert!(text.contains("640x480"), "{text}");
        assert_eq!(data["screenshot"]["scope"], "granted");
        assert_eq!(data["screenshot"]["frame"], 7);
        let blocks = serde_json::Value::Array(take_attached());
        assert_eq!(blocks.as_array().map(Vec::len), Some(1));
        assert_eq!(blocks[0]["type"], "image");
        assert_eq!(blocks[0]["mimeType"], "image/png");
        assert_eq!(blocks[0]["data"], "iVBORw==");
        assert!(take_attached().is_empty(), "taken once");

        let mut text = String::new();
        let mut data = json!({});
        add_to_answer(None, Some("limited".into()), 1, &mut text, &mut data);
        assert!(text.contains("No screenshot: limited"), "{text}");
        assert_eq!(data["screenshot"]["error"], "limited");
        assert!(take_attached().is_empty());
    }

    #[test]
    fn take_screenshot_refuses_while_the_switch_is_off() {
        let _guard = crate::locks::lock_or_recover(&crate::agent::access::TEST_LOCK);
        crate::fast_buttons::set_persistence(false);
        crate::agent::access::set_enabled(true);
        crate::agent::access::set_ui_driver(false);
        let error = take_screenshot(&json!({}))
            .err()
            .map(|e| e.0)
            .unwrap_or_default();
        assert!(error.contains("Let agents drive Quill's window"), "{error}");
        crate::agent::access::set_enabled(false);
    }
}
