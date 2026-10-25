//! The UI driver tools: read Quill's window as an element tree, click,
//! hover, scroll, type and press keys in it. Each call hands a command to
//! the UI thread (`crate::agent::ui_driver::bridge`) and waits for its
//! answer; this thread never touches GPUI.
//!
//! Gating (`crate::agent::ui_driver::policy`): the "Let agents drive
//! Quill's window" switch, the open chat's grant (`read` to read, `read_write`
//! for input), and never the controls that send, delete or pay.

use super::target::{granted, my_user_id};
use super::{Ctx, Tool, ToolError, ToolOutput, current_agent};
use crate::agent::ui_driver::bridge::{
    self, BridgeError, Button, Target, UiCommand, UiReply, UiState,
};
use crate::agent::ui_driver::keys;
use crate::agent::ui_driver::policy;
use crate::agent::ui_driver::tree::{UiNode, UiTree, parse_id};
use serde_json::{Value, json};
use std::time::Duration;

/// How long `State` may take (no frame needed).
const STATE_WAIT: Duration = Duration::from_secs(5);
/// How long a read or an input may take (it waits for frames).
const COMMAND_WAIT: Duration = Duration::from_secs(15);
/// Nodes returned by default and at most.
const DEFAULT_LIMIT: i64 = 300;
const MAX_LIMIT: i64 = 3000;

pub(super) const TOOLS: &[Tool] = &[
    Tool {
        name: "read_ui_tree",
        title: "Read Quill's window",
        description: "Use to see what Quill's window shows: its elements as a tree (role, name, value, bounds in window \
pixels, states, children), to find something to click or type into. Needs \"Let agents drive Quill's window\" on in Quill. \
Example input: {}, or {\"filter\": \"search\"}, or {\"role\": \"Button\", \"limit\": 100}. \
Output: window {width, height, scale}, focus (node id), nodes [{id, role, name, value, bounds [x, y, w, h], states, actions, children}], total, truncated.",
        schema: read_schema,
        run: read_ui_tree,
        read_only: true,
    },
    Tool {
        name: "click_element",
        title: "Click in Quill's window",
        description: "Use to click an element from read_ui_tree (by node id) or a window point. Controls that send, delete or pay \
are refused: use send_message, delete_messages and the other write tools for those. \
Example input: {\"target\": {\"node\": \"3f2a\"}}, or {\"target\": {\"x\": 120, \"y\": 40}, \"button\": \"right\", \"count\": 2}. \
Output: {clicked {id, role, name}, focus {id, role, name}, added [nodes that appeared]}.",
        schema: click_schema,
        run: click_element,
        read_only: false,
    },
    Tool {
        name: "type_text",
        title: "Type in Quill's window",
        description: "Use to type text into the focused field (click it first). A line break is typed as the chord that \
inserts one in Quill's message box, never as a send. \
Example input: {\"text\": \"hello\"}. Output: {typed (characters), focus {id, role, name, value}}.",
        schema: type_schema,
        run: type_text,
        read_only: false,
    },
    Tool {
        name: "press_keys",
        title: "Press keys in Quill's window",
        description: "Use to press one or more key chords, GPUI style: modifiers ctrl, alt, shift, cmd (Win on Linux and Windows), \
joined with - or +, chords separated by spaces. Keys that quit or close Quill, and Enter while it would send the message, are refused. \
Example input: {\"keys\": \"cmd-k\"}, or {\"keys\": \"escape escape\"}, or {\"keys\": \"ctrl-shift-a\"}. \
Output: {pressed [chords], focus {id, role, name}, added [nodes that appeared]}.",
        schema: keys_schema,
        run: press_keys,
        read_only: false,
    },
    Tool {
        name: "scroll_view",
        title: "Scroll in Quill's window",
        description: "Use to scroll the element under a node or point by dx, dy pixels: dy > 0 scrolls down (shows what is below), \
dy < 0 scrolls up. Example input: {\"target\": {\"node\": \"3f2a\"}, \"dy\": 400}, or {\"target\": {\"x\": 600, \"y\": 300}, \"dy\": -200}. \
Output: {scrolled {dx, dy}, focus, added}.",
        schema: scroll_schema,
        run: scroll_view,
        read_only: false,
    },
    Tool {
        name: "hover_element",
        title: "Hover in Quill's window",
        description: "Use to move the pointer over an element, for a tooltip or hover controls, then read_ui_tree to see them. \
Example input: {\"target\": {\"node\": \"3f2a\"}}. Output: {hovered {id, role, name}, added [nodes that appeared]}.",
        schema: target_only_schema,
        run: hover_element,
        read_only: false,
    },
    Tool {
        name: "open_menu",
        title: "Open a context menu in Quill's window",
        description: "Use to right-click an element and get the menu that opens. Then click_element one of the menu nodes, or press_keys \
{\"keys\": \"escape\"} to close it. Example input: {\"target\": {\"node\": \"3f2a\"}}. \
Output: {menu [{id, role, name}], added [nodes that appeared]}.",
        schema: target_only_schema,
        run: open_menu,
        read_only: false,
    },
];

fn target_schema() -> Value {
    json!({
        "type": "object",
        "description": "A node id from read_ui_tree ({\"node\": \"3f2a\"}) or a window point in logical pixels ({\"x\": 120, \"y\": 40}).",
        "properties": {
            "node": {"type": "string"},
            "x": {"type": "number"},
            "y": {"type": "number"},
        },
        "additionalProperties": false,
    })
}

fn read_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "filter": {"type": "string", "description": "Keep nodes whose name, value or role contains this (case-insensitive)."},
            "role": {"type": "string", "description": "Keep nodes with this role only (Button, TextInput, MenuItem, ListItem...)."},
            "limit": {"type": "integer", "minimum": 1, "maximum": MAX_LIMIT, "default": DEFAULT_LIMIT},
        },
        "additionalProperties": false,
    })
}

fn click_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "target": target_schema(),
            "button": {"type": "string", "enum": ["left", "right", "middle"], "default": "left"},
            "count": {"type": "integer", "minimum": 1, "maximum": 3, "default": 1, "description": "2 for a double click."},
        },
        "required": ["target"],
        "additionalProperties": false,
    })
}

fn type_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "text": {"type": "string", "maxLength": 4000},
        },
        "required": ["text"],
        "additionalProperties": false,
    })
}

fn keys_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "keys": {"type": "string", "description": "Chords separated by spaces, e.g. \"cmd-k\", \"escape\", \"ctrl-shift-a tab\"."},
        },
        "required": ["keys"],
        "additionalProperties": false,
    })
}

fn scroll_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "target": target_schema(),
            "dx": {"type": "number", "default": 0},
            "dy": {"type": "number", "default": 0, "description": "Positive scrolls down."},
        },
        "required": ["target"],
        "additionalProperties": false,
    })
}

fn target_only_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"target": target_schema()},
        "required": ["target"],
        "additionalProperties": false,
    })
}

/// The switches, the open chat and its grant. Returns the open chat the
/// grant was checked for, which the UI holds the command to.
fn begin(input: bool) -> Result<Option<i64>, ToolError> {
    if let Some(refusal) = policy::switch_refusal() {
        return Err(ToolError::new(refusal));
    }
    let ctx = Ctx::active()?;
    let state = ui_state()?;
    let scope = match state.open_chat {
        Some(chat) => {
            let me = my_user_id(&ctx)?;
            granted(&ctx, chat, me)?.map(|target| target.scope)
        }
        None => None,
    };
    let refusal = if input {
        policy::input_refusal(state.open_chat, scope)
    } else {
        policy::read_refusal(state.open_chat, scope)
    };
    match refusal {
        Some(refusal) => Err(ToolError::new(refusal)),
        None => Ok(state.open_chat),
    }
}

fn bridge_error(error: BridgeError) -> ToolError {
    match error {
        BridgeError::Busy => ToolError::new(
            "Quill's window is busy with other agent commands. Wait a moment, then retry.",
        ),
        BridgeError::Timeout => ToolError::new(
            "Quill's window did not answer within 15 seconds. Make sure Quill's window is open and not minimised, then retry.",
        ),
    }
}

fn ui_state() -> Result<UiState, ToolError> {
    match bridge::send(&current_agent(), UiCommand::State, None, STATE_WAIT)
        .map_err(bridge_error)?
    {
        UiReply::State(state) => Ok(state),
        UiReply::Refused(reason) => Err(ToolError::new(reason)),
        _ => Err(ToolError::new(
            "Quill's window gave an unexpected answer. Retry.",
        )),
    }
}

/// Run `command` on the UI thread, holding it to `expect` as the open chat.
fn run(command: UiCommand, expect: Option<i64>) -> Result<UiReply, ToolError> {
    match bridge::send(&current_agent(), command, Some(expect), COMMAND_WAIT)
        .map_err(bridge_error)?
    {
        UiReply::Refused(reason) => Err(ToolError::new(reason)),
        reply => Ok(reply),
    }
}

fn target_arg(args: &Value) -> Result<Target, ToolError> {
    let usage = "target is required: {\"node\": \"<id from read_ui_tree>\"} or {\"x\": 120, \"y\": 40} in window pixels.";
    let target = args
        .get("target")
        .filter(|t| t.is_object())
        .ok_or_else(|| ToolError::new(usage))?;
    if let Some(node) = target.get("node").and_then(Value::as_str) {
        return parse_id(node)
            .map(|id| Target::Node(crate::agent::ui_driver::tree::id_string(id)))
            .ok_or_else(|| {
                ToolError::new(format!(
                    "\"{node}\" is not a node id. Use an id from read_ui_tree, like \"3f2a\"."
                ))
            });
    }
    match (
        target.get("x").and_then(Value::as_f64),
        target.get("y").and_then(Value::as_f64),
    ) {
        (Some(x), Some(y)) if x.is_finite() && y.is_finite() => Ok(Target::Point {
            x: x as f32,
            y: y as f32,
        }),
        _ => Err(ToolError::new(usage)),
    }
}

fn node_json(node: Option<&UiNode>) -> Value {
    match node {
        Some(node) => {
            json!({"id": node.id, "role": node.role, "name": node.name, "value": node.value})
        }
        None => Value::Null,
    }
}

fn describe(node: Option<&UiNode>) -> String {
    match node {
        Some(node) => format!(
            "{} {} \"{}\"",
            node.id,
            node.role,
            node.name.as_deref().or(node.value.as_deref()).unwrap_or("")
        ),
        None => "none".to_string(),
    }
}

/// The answer of an input command as text and JSON, with `lead` first.
fn done_output(reply: UiReply, lead: String, extra: Value) -> Result<ToolOutput, ToolError> {
    let UiReply::Done { tree, added } = reply else {
        return Err(ToolError::new(
            "Quill's window gave an unexpected answer. Retry.",
        ));
    };
    let focus = tree.focus.as_deref().and_then(|id| tree.node(id));
    let mut text = lead;
    text.push_str(&format!("\nFocus: {}.", describe(focus)));
    let added_refs: Vec<&UiNode> = added.iter().collect();
    if added.is_empty() {
        text.push_str("\nNo new elements appeared. Call read_ui_tree to see the window.");
    } else {
        text.push_str(&format!("\n{} new elements:\n", added.len()));
        text.push_str(&tree.text_lines(&added_refs, 80));
    }
    let mut data = json!({
        "ok": true,
        "focus": node_json(focus),
        "added": added.iter().take(80).map(|n| serde_json::to_value(n).unwrap_or(Value::Null)).collect::<Vec<_>>(),
        "frame": tree.frame,
    });
    if let (Some(map), Some(extra)) = (data.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            map.insert(key.clone(), value.clone());
        }
    }
    Ok(ToolOutput { text, data })
}

fn target_json(target: &Target, tree: &UiTree) -> Value {
    match target {
        Target::Node(id) => node_json(tree.node(id)),
        Target::Point { x, y } => node_json(tree.hit_test(*x, *y)),
    }
}

pub fn read_ui_tree(args: &Value) -> Result<ToolOutput, ToolError> {
    let expect = begin(false)?;
    let filter = args.get("filter").and_then(Value::as_str);
    let role = args.get("role").and_then(Value::as_str);
    let limit = super::int_arg(args, "limit", DEFAULT_LIMIT, 1, MAX_LIMIT)? as usize;
    let UiReply::Tree(tree) = run(UiCommand::ReadTree, expect)? else {
        return Err(ToolError::new(
            "Quill's window gave an unexpected answer. Retry.",
        ));
    };
    let nodes = tree.filter(filter, role);
    let mut text = format!(
        "Quill window {}x{} px (scale {}), {} elements{}.\n",
        tree.width,
        tree.height,
        tree.scale,
        nodes.len(),
        if filter.is_some() || role.is_some() {
            " match"
        } else {
            ""
        }
    );
    if nodes.is_empty() {
        text.push_str("Nothing matched. Try a shorter filter, or no filter.\n");
    }
    text.push_str(&tree.text_lines(&nodes, limit));
    let data = tree.to_json(&nodes, limit);
    Ok(ToolOutput { text, data })
}

pub fn click_element(args: &Value) -> Result<ToolOutput, ToolError> {
    let target = target_arg(args)?;
    let button = match args.get("button").and_then(Value::as_str).unwrap_or("left") {
        "left" => Button::Left,
        "right" => Button::Right,
        "middle" => Button::Middle,
        other => {
            return Err(ToolError::new(format!(
                "button \"{other}\" is not one of left, right, middle."
            )));
        }
    };
    let count = super::int_arg(args, "count", 1, 1, 3)? as u32;
    let expect = begin(true)?;
    let reply = run(
        UiCommand::Click {
            target: target.clone(),
            button,
            count,
        },
        expect,
    )?;
    let clicked = match &reply {
        UiReply::Done { tree, .. } => target_json(&target, tree),
        _ => Value::Null,
    };
    let lead = format!(
        "Clicked {} ({:?}, x{count}).",
        clicked
            .get("role")
            .and_then(Value::as_str)
            .map(|role| format!(
                "{role} \"{}\"",
                clicked.get("name").and_then(Value::as_str).unwrap_or("")
            ))
            .unwrap_or_else(|| "the point".to_string()),
        button
    );
    done_output(reply, lead, json!({"clicked": clicked}))
}

pub fn type_text(args: &Value) -> Result<ToolOutput, ToolError> {
    let text = args
        .get("text")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| ToolError::new("text is required and must not be empty."))?;
    if text.chars().count() > 4000 {
        return Err(ToolError::new(
            "text is longer than 4000 characters. Type it in parts.",
        ));
    }
    let expect = begin(true)?;
    let reply = run(
        UiCommand::TypeText {
            text: text.to_string(),
        },
        expect,
    )?;
    let typed = text.chars().count();
    done_output(
        reply,
        format!("Typed {typed} characters."),
        json!({"typed": typed}),
    )
}

pub fn press_keys(args: &Value) -> Result<ToolOutput, ToolError> {
    let text = args
        .get("keys")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new("keys is required, e.g. {\"keys\": \"cmd-k\"}."))?;
    let chords = keys::parse_sequence(text).map_err(ToolError::new)?;
    for chord in &chords {
        if let Some(effect) = keys::forbidden(chord) {
            return Err(ToolError::new(format!(
                "{} would {effect}; agents never press it.",
                chord.describe()
            )));
        }
    }
    let expect = begin(true)?;
    let pressed: Vec<String> = chords.iter().map(|chord| chord.describe()).collect();
    let reply = run(UiCommand::PressKeys { chords }, expect)?;
    done_output(
        reply,
        format!("Pressed {}.", pressed.join(", ")),
        json!({"pressed": pressed}),
    )
}

pub fn scroll_view(args: &Value) -> Result<ToolOutput, ToolError> {
    let target = target_arg(args)?;
    let axis = |key: &str| -> Result<f32, ToolError> {
        match args.get(key) {
            None | Some(Value::Null) => Ok(0.0),
            Some(value) => value
                .as_f64()
                .filter(|v| v.is_finite())
                .map(|v| v.clamp(-5000.0, 5000.0) as f32)
                .ok_or_else(|| ToolError::new(format!("{key} must be a number of pixels."))),
        }
    };
    let (dx, dy) = (axis("dx")?, axis("dy")?);
    if dx == 0.0 && dy == 0.0 {
        return Err(ToolError::new(
            "Pass dx or dy: pixels to scroll, dy > 0 scrolls down.",
        ));
    }
    let expect = begin(true)?;
    let reply = run(UiCommand::Scroll { target, dx, dy }, expect)?;
    done_output(
        reply,
        format!("Scrolled by ({dx}, {dy})."),
        json!({"scrolled": {"dx": dx, "dy": dy}}),
    )
}

pub fn hover_element(args: &Value) -> Result<ToolOutput, ToolError> {
    let target = target_arg(args)?;
    let expect = begin(true)?;
    let reply = run(
        UiCommand::Hover {
            target: target.clone(),
        },
        expect,
    )?;
    let hovered = match &reply {
        UiReply::Done { tree, .. } => target_json(&target, tree),
        _ => Value::Null,
    };
    done_output(reply, "Hovering.".to_string(), json!({"hovered": hovered}))
}

pub fn open_menu(args: &Value) -> Result<ToolOutput, ToolError> {
    let target = target_arg(args)?;
    let expect = begin(true)?;
    let reply = run(UiCommand::OpenMenu { target }, expect)?;
    let menu: Vec<Value> = match &reply {
        UiReply::Done { added, .. } => added
            .iter()
            .filter(|node| node.role.contains("Menu") || node.actions.iter().any(|a| a == "Click"))
            .map(|node| node_json(Some(node)))
            .collect(),
        _ => Vec::new(),
    };
    let lead = if menu.is_empty() {
        "Right-clicked; no menu items appeared. Nothing may have a menu there, or it needs another frame: call read_ui_tree."
            .to_string()
    } else {
        format!("A menu with {} items opened.", menu.len())
    };
    done_output(reply, lead, json!({"menu": menu}))
}

#[cfg(test)]
mod tests {
    use super::{click_element, read_ui_tree, target_arg};
    use crate::agent::ui_driver::bridge::Target;
    use serde_json::json;

    #[test]
    fn targets_are_a_node_id_or_a_point() {
        assert_eq!(
            target_arg(&json!({"target": {"node": "3F2A"}})),
            Ok(Target::Node("3f2a".into()))
        );
        assert_eq!(
            target_arg(&json!({"target": {"x": 10, "y": 20.5}})),
            Ok(Target::Point { x: 10.0, y: 20.5 })
        );
        assert!(target_arg(&json!({})).is_err());
        assert!(target_arg(&json!({"target": {"node": "zz"}})).is_err());
        assert!(target_arg(&json!({"target": {"x": 1}})).is_err());
    }

    #[test]
    fn the_driver_refuses_while_its_switch_is_off() {
        let _guard = crate::locks::lock_or_recover(&crate::agent::access::TEST_LOCK);
        crate::fast_buttons::set_persistence(false);
        crate::agent::access::set_enabled(true);
        crate::agent::access::set_ui_driver(false);
        let error = read_ui_tree(&json!({}))
            .err()
            .map(|e| e.0)
            .unwrap_or_default();
        assert!(error.contains("Let agents drive Quill's window"), "{error}");
        let error = click_element(&json!({"target": {"x": 1, "y": 1}}))
            .err()
            .map(|e| e.0)
            .unwrap_or_default();
        assert!(error.contains("Let agents drive Quill's window"), "{error}");
        crate::agent::access::set_enabled(false);
        let error = read_ui_tree(&json!({}))
            .err()
            .map(|e| e.0)
            .unwrap_or_default();
        assert!(error.contains("Agent access is off"), "{error}");
    }
}
