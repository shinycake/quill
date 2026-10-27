//! The UI driver tools: read Quill's window as an element tree, click,
//! hover, scroll, type and press keys in it, each with an optional
//! screenshot after (`ui_shot`). Each call hands a command to
//! the UI thread (`crate::agent::ui_driver::bridge`) and waits for its
//! answer; this thread never touches GPUI.
//!
//! The UI decides everything about what the agent may see and do, from
//! its own state (`src/ui/agent_driver_surface.rs`): the tree comes back
//! cut to the granted chats, a blocked surface comes back as a note, and
//! every refused action is listed in the reply. The one thing finished
//! here is a send the composer handed over ([`finish_ui_send`]): it goes
//! through the same write path as `send_message`.

use super::target::{granted, my_user_id, stranger_bot};
use super::ui_shot::{self, Asked};
use super::write::common;
use super::{Ctx, Tool, ToolError, ToolOutput, current_agent};
use crate::agent::grants::Scope;
use crate::agent::ui_driver::bridge::{
    self, BridgeError, Button, Target, UiCommand, UiReply, UiSend,
};
use crate::agent::ui_driver::keys;
use crate::agent::ui_driver::policy;
use crate::agent::ui_driver::shot::ShotOptions;
use crate::agent::ui_driver::tree::{UiNode, UiTree, parse_id};
use serde_json::{Value, json};
use std::time::Duration;

/// How long a read or an input may take (it waits for frames).
const COMMAND_WAIT: Duration = Duration::from_secs(15);
/// Nodes returned by default and at most.
const DEFAULT_LIMIT: i64 = 300;
const MAX_LIMIT: i64 = 3000;

const SIDE_EFFECTS: &str = "Quill's window acts for real: opening or scrolling a chat marks it read, \
typing in the message box saves a draft and shows the other side that you are typing.";

pub(super) const TOOLS: &[Tool] = &[
    Tool {
        name: "read_ui_tree",
        title: "Read Quill's window",
        description: "Use to see what Quill's window shows: its elements as a tree (role, name, value, bounds in window \
pixels, states, children), to find something to click or type into. Needs \"Let agents drive Quill's window\" on in Quill. \
The tree follows the chat grants: it has the chat list's granted rows and the open chat when it is granted; with \"All other chats\" on, \
most of the window. Settings, agent prompts, payments and the lock screen are never shown: the reply says what is in the way. \
Example input: {}, or {\"filter\": \"search\"}, or {\"role\": \"Button\", \"limit\": 100}. \
Output: window {width, height, scale}, focus (node id), nodes [{id, role, name, value, bounds [x, y, w, h], states, actions, children, tag}], total, truncated.",
        schema: read_schema,
        run: read_ui_tree,
        read_only: true,
        confirms_sends: false,
    },
    Tool {
        name: "click_element",
        title: "Click in Quill's window",
        description: "Use to click an element from read_ui_tree (by node id) or a window point, inside the parts of the window \
agents may use (the point must be over them). Pressing Send in the message box sends through the agent path (grants, pacing, \
\"Confirm each send\", the activity log) and the reply says so; other sends, deletions, payments and settings changes are refused and listed under refusals. \
Opening or scrolling a chat marks it read. \
Add \"screenshot\": true for a picture of the window after the click (as take_screenshot). \
Example input: {\"target\": {\"node\": \"3f2a\"}}, or {\"target\": {\"x\": 120, \"y\": 40}, \"button\": \"right\", \"count\": 2}, \
or {\"target\": {\"node\": \"3f2a\"}, \"screenshot\": true}. \
Output: {clicked {id, role, name}, focus {id, role, name}, added [nodes that appeared], refusals, sent, screenshot}, plus a PNG image when asked.",
        schema: click_schema,
        run: click_element,
        read_only: false,
        confirms_sends: true,
    },
    Tool {
        name: "type_text",
        title: "Type in Quill's window",
        description: "Use to type text into the focused text box (click it first; nothing is typed while a button or a list has the focus). \
A line break is typed as the chord that inserts one in Quill's message box, never as a send; a tab character is refused. \
Typing in the message box saves a draft and shows the other side that you are typing. \
Add \"screenshot\": true for a picture of the window after typing. \
Example input: {\"text\": \"hello\"}. Output: {typed (characters), focus {id, role, name, value}, refusals, screenshot}, plus a PNG image when asked.",
        schema: type_schema,
        run: type_text,
        read_only: false,
        confirms_sends: false,
    },
    Tool {
        name: "press_keys",
        title: "Press keys in Quill's window",
        description: "Use to press one or more key chords, GPUI style: modifiers ctrl, alt, shift, cmd (Win on Linux and Windows), \
joined with - or +, chords separated by spaces. Each chord is checked on its own: keys that quit or close Quill are refused, \
Enter and Space only go to a focused text box (Enter in the message box sends through the agent path), and a chord that \
moves to an ungranted chat stops the sequence. Chords never type characters; use type_text for text. \
Add \"screenshot\": true for a picture of the window after the keys. \
Example input: {\"keys\": \"cmd-k\"}, or {\"keys\": \"escape escape\"}, or {\"keys\": \"ctrl-shift-a\"}. \
Output: {pressed [chords], focus {id, role, name}, added [nodes that appeared], refusals, screenshot}, plus a PNG image when asked.",
        schema: keys_schema,
        run: press_keys,
        read_only: false,
        confirms_sends: true,
    },
    Tool {
        name: "scroll_view",
        title: "Scroll in Quill's window",
        description: "Use to scroll the element under a node or point by dx, dy pixels: dy > 0 scrolls down (shows what is below), \
dy < 0 scrolls up. Scrolling a chat's history marks what comes into view as read. \
Add \"screenshot\": true for a picture of the window after scrolling. \
Example input: {\"target\": {\"node\": \"3f2a\"}, \"dy\": 400}, or {\"target\": {\"x\": 600, \"y\": 300}, \"dy\": -200}. \
Output: {scrolled {dx, dy}, focus, added, refusals, screenshot}, plus a PNG image when asked.",
        schema: scroll_schema,
        run: scroll_view,
        read_only: false,
        confirms_sends: false,
    },
    Tool {
        name: "hover_element",
        title: "Hover in Quill's window",
        description: "Use to move the pointer over an element, for a tooltip or hover controls, then read_ui_tree to see them. \
Add \"screenshot\": true to see the tooltip or hover state as a picture. \
Example input: {\"target\": {\"node\": \"3f2a\"}}. Output: {hovered {id, role, name}, added [nodes that appeared], screenshot}, plus a PNG image when asked.",
        schema: target_only_schema,
        run: hover_element,
        read_only: false,
        confirms_sends: false,
    },
    Tool {
        name: "open_menu",
        title: "Open a context menu in Quill's window",
        description: "Use to right-click an element and get the menu that opens. Then click_element one of the menu nodes, or press_keys \
{\"keys\": \"escape\"} to close it. Menu items that delete, report, block or leave are refused when clicked. \
Add \"screenshot\": true for a picture of the open menu. \
Example input: {\"target\": {\"node\": \"3f2a\"}}. Output: {menu [{id, role, name}], added [nodes that appeared], screenshot}, plus a PNG image when asked.",
        schema: target_only_schema,
        run: open_menu,
        read_only: false,
        confirms_sends: false,
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
    ui_shot::with_options(json!({
        "type": "object",
        "properties": {
            "target": target_schema(),
            "button": {"type": "string", "enum": ["left", "right", "middle"], "default": "left"},
            "count": {"type": "integer", "minimum": 1, "maximum": 3, "default": 1, "description": "2 for a double click."},
        },
        "required": ["target"],
        "additionalProperties": false,
    }))
}

fn type_schema() -> Value {
    ui_shot::with_options(json!({
        "type": "object",
        "properties": {
            "text": {"type": "string", "maxLength": 4000},
        },
        "required": ["text"],
        "additionalProperties": false,
    }))
}

fn keys_schema() -> Value {
    ui_shot::with_options(json!({
        "type": "object",
        "properties": {
            "keys": {"type": "string", "description": "Chords separated by spaces, e.g. \"cmd-k\", \"escape\", \"ctrl-shift-a tab\"."},
        },
        "required": ["keys"],
        "additionalProperties": false,
    }))
}

fn scroll_schema() -> Value {
    ui_shot::with_options(json!({
        "type": "object",
        "properties": {
            "target": target_schema(),
            "dx": {"type": "number", "default": 0},
            "dy": {"type": "number", "default": 0, "description": "Positive scrolls down."},
        },
        "required": ["target"],
        "additionalProperties": false,
    }))
}

fn target_only_schema() -> Value {
    ui_shot::with_options(json!({
        "type": "object",
        "properties": {"target": target_schema()},
        "required": ["target"],
        "additionalProperties": false,
    }))
}

pub(super) fn bridge_error(error: BridgeError) -> ToolError {
    match error {
        BridgeError::Busy => ToolError::new(
            "Quill's window is busy with other agent commands. Wait a moment, then retry.",
        ),
        BridgeError::Timeout => ToolError::new(
            "Quill's window did not answer within 15 seconds. Make sure Quill's window is open and not minimised, then retry.",
        ),
    }
}

/// The switches, then `command` on the UI thread, which decides the rest
/// from its own state.
fn run(command: UiCommand) -> Result<UiReply, ToolError> {
    run_shot(command, None)
}

/// [`run`] with a screenshot after the command.
fn run_shot(command: UiCommand, shot: Option<ShotOptions>) -> Result<UiReply, ToolError> {
    if let Some(refusal) = policy::switch_refusal() {
        return Err(ToolError::new(refusal));
    }
    match bridge::send_with_shot(&current_agent(), command, shot, COMMAND_WAIT)
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

/// A send the composer handed over: the same path as `send_message`
/// (grant at read_write, no stranger bots, "Confirm each send", the paced
/// queue, the activity log), then the box is told to clear.
fn finish_ui_send(send: UiSend) -> Result<String, ToolError> {
    let ctx = Ctx::active()?;
    let me = my_user_id(&ctx)?;
    let target = granted(&ctx, send.chat_id, me)?.ok_or_else(|| {
        ToolError::new(format!(
            "The message stays in Quill's message box: chat {} is not granted to agents. Call request_access {{\"chat\": {}, \"scope\": \"read_write\", \"reason\": ...}}.",
            send.chat_id, send.chat_id
        ))
    })?;
    if !target.scope.allows(Scope::ReadWrite) {
        return Err(ToolError::new(format!(
            "The message stays in Quill's message box: \"{}\" is granted for reading only. Call request_access {{\"chat\": {}, \"scope\": \"read_write\", \"reason\": ...}}.",
            target.title, target.chat_id
        )));
    }
    if stranger_bot(&ctx, &target) {
        return Err(ToolError::new(format!(
            "The message stays in Quill's message box: \"{}\" is a bot the user never wrote to. Ask the user to start it, or request_access for it.",
            target.title
        )));
    }
    if let Some(what) = not_plain(&send.request) {
        return Err(keep_draft(ToolError::new(format!(
            "The request the window built {what}; the agent path sends plain text messages only. Use send_message, or ask the user."
        ))));
    }
    common::confirm(&ctx, &target, "ui_send", &send.text).map_err(keep_draft)?;
    let sent =
        common::send(&ctx, &target, "ui_send", &send.text, send.request, 1).map_err(keep_draft)?;
    let _ = bridge::post(
        &current_agent(),
        UiCommand::ComposerSent {
            chat_id: send.chat_id,
            text: send.text,
        },
    );
    let id = sent
        .first()
        .and_then(|message| message.get("id"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    Ok(format!(
        "Sent to \"{}\" through the agent path (message {id}).",
        target.title
    ))
}

/// What makes `request` more than a plain text `sendMessage`, if anything.
/// The window builds the request from the composer's state and is trusted
/// no further than this: a schedule, suggested-post terms, an effect, a
/// paid message or any other content type stays with the user.
fn not_plain(request: &Value) -> Option<&'static str> {
    fn type_of(value: Option<&Value>) -> Option<&str> {
        value.and_then(|v| v.get("@type")).and_then(Value::as_str)
    }
    if type_of(Some(request)) != Some("sendMessage") {
        return Some("is not a sendMessage");
    }
    if type_of(request.get("input_message_content")) != Some("inputMessageText") {
        return Some("is not a text message");
    }
    if super::td_request::spends(request) {
        return Some("would spend Stars");
    }
    let option = |field: &str| request.get("options").and_then(|o| o.get(field));
    let set = |field: &str| option(field).is_some_and(|v| !v.is_null());
    if set("scheduling_state") {
        return Some("is scheduled");
    }
    if set("suggested_post_info") {
        return Some("is a suggested post");
    }
    let effect = option("effect_id").is_some_and(|v| match v {
        Value::String(s) => s != "0" && !s.is_empty(),
        Value::Number(n) => n.as_i64() != Some(0),
        _ => false,
    });
    if effect {
        return Some("carries a message effect");
    }
    None
}

fn keep_draft(error: ToolError) -> ToolError {
    ToolError::new(format!(
        "The message stays in Quill's message box, not sent. {}",
        error.0
    ))
}

/// The answer of an input command as text and JSON, with `lead` first;
/// the screenshot asked for rides along as an image (`ui_shot`).
fn done_output(
    reply: UiReply,
    lead: String,
    extra: Value,
    asked: Asked,
) -> Result<ToolOutput, ToolError> {
    let UiReply::Done {
        tree,
        added,
        refusals,
        ui_send,
        shot,
    } = reply
    else {
        return Err(ToolError::new(
            "Quill's window gave an unexpected answer. Retry.",
        ));
    };
    let sent = ui_send.map(finish_ui_send).transpose()?;
    let focus = tree.focus.as_deref().and_then(|id| tree.node(id));
    let mut text = lead;
    if let Some(sent) = &sent {
        text.push('\n');
        text.push_str(sent);
    }
    for refusal in &refusals {
        text.push_str("\nRefused: ");
        text.push_str(refusal);
    }
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
        "refusals": refusals,
        "sent": sent,
        "frame": tree.frame,
    });
    if let (Some(map), Some(extra)) = (data.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            map.insert(key.clone(), value.clone());
        }
    }
    ui_shot::add_to_answer(shot, asked.skipped, tree.frame, &mut text, &mut data);
    Ok(ToolOutput { text, data })
}

fn target_json(target: &Target, tree: &UiTree) -> Value {
    match target {
        Target::Node(id) => node_json(tree.node(id)),
        Target::Point { x, y } => node_json(tree.hit_test(*x, *y)),
    }
}

pub fn read_ui_tree(args: &Value) -> Result<ToolOutput, ToolError> {
    let filter = args.get("filter").and_then(Value::as_str);
    let role = args.get("role").and_then(Value::as_str);
    let limit = super::int_arg(args, "limit", DEFAULT_LIMIT, 1, MAX_LIMIT)? as usize;
    let UiReply::Tree(tree) = run(UiCommand::ReadTree)? else {
        return Err(ToolError::new(
            "Quill's window gave an unexpected answer. Retry.",
        ));
    };
    let nodes = tree.filter(filter, role);
    let mut text = format!(
        "Quill window {}x{} px (scale {}), {} elements{}. {SIDE_EFFECTS}\n",
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
    let asked = ui_shot::asked(args);
    let reply = run_shot(
        UiCommand::Click {
            target: target.clone(),
            button,
            count,
        },
        asked.options,
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
    done_output(reply, lead, json!({"clicked": clicked}), asked)
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
    if text.contains('\t') {
        return Err(ToolError::new(policy::tab_in_text()));
    }
    let asked = ui_shot::asked(args);
    let reply = run_shot(
        UiCommand::TypeText {
            text: text.to_string(),
        },
        asked.options,
    )?;
    let typed = text.chars().count();
    done_output(
        reply,
        format!("Typed {typed} characters."),
        json!({"typed": typed}),
        asked,
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
    let pressed: Vec<String> = chords.iter().map(|chord| chord.describe()).collect();
    let asked = ui_shot::asked(args);
    let reply = run_shot(UiCommand::PressKeys { chords }, asked.options)?;
    done_output(
        reply,
        format!("Pressed {}.", pressed.join(", ")),
        json!({"pressed": pressed}),
        asked,
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
    let asked = ui_shot::asked(args);
    let reply = run_shot(UiCommand::Scroll { target, dx, dy }, asked.options)?;
    done_output(
        reply,
        format!("Scrolled by ({dx}, {dy})."),
        json!({"scrolled": {"dx": dx, "dy": dy}}),
        asked,
    )
}

pub fn hover_element(args: &Value) -> Result<ToolOutput, ToolError> {
    let target = target_arg(args)?;
    let asked = ui_shot::asked(args);
    let reply = run_shot(
        UiCommand::Hover {
            target: target.clone(),
        },
        asked.options,
    )?;
    let hovered = match &reply {
        UiReply::Done { tree, .. } => target_json(&target, tree),
        _ => Value::Null,
    };
    done_output(
        reply,
        "Hovering.".to_string(),
        json!({"hovered": hovered}),
        asked,
    )
}

pub fn open_menu(args: &Value) -> Result<ToolOutput, ToolError> {
    let target = target_arg(args)?;
    let asked = ui_shot::asked(args);
    let reply = run_shot(UiCommand::OpenMenu { target }, asked.options)?;
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
    done_output(reply, lead, json!({"menu": menu}), asked)
}

#[cfg(test)]
mod tests {
    use super::{click_element, not_plain, read_ui_tree, target_arg, type_text};
    use crate::agent::ui_driver::bridge::Target;
    use crate::composer::{ComposerScheduling, SendOptions};
    use crate::ids::{ChatId, RequestId};
    use crate::telegram::requests::send_text;
    use serde_json::{Value, json};

    fn built(options: &SendOptions) -> Value {
        let request = send_text(RequestId(0), ChatId(11), None, "hi", None, options);
        serde_json::from_str(&request).unwrap_or(Value::Null)
    }

    #[test]
    fn only_a_plain_text_send_passes_the_agent_path() {
        assert_eq!(not_plain(&built(&SendOptions::default())), None);
        let scheduled = SendOptions {
            scheduling: ComposerScheduling::SendWhenOnline,
            ..SendOptions::default()
        };
        assert_eq!(not_plain(&built(&scheduled)), Some("is scheduled"));
        let paid = SendOptions {
            paid_message_star_count: 5,
            ..SendOptions::default()
        };
        assert_eq!(not_plain(&built(&paid)), Some("would spend Stars"));
        let effect = SendOptions {
            effect_id: 5_046_509_860_389_126_442,
            ..SendOptions::default()
        };
        assert_eq!(not_plain(&built(&effect)), Some("carries a message effect"));
        let mut suggested = built(&SendOptions::default());
        if let Some(options) = suggested.get_mut("options").and_then(Value::as_object_mut) {
            options.insert(
                "suggested_post_info".into(),
                json!({"@type": "inputSuggestedPostInfo"}),
            );
        }
        assert_eq!(not_plain(&suggested), Some("is a suggested post"));
        let mut photo = built(&SendOptions::default());
        if let Some(request) = photo.as_object_mut() {
            request.insert(
                "input_message_content".into(),
                json!({"@type": "inputMessagePhoto"}),
            );
        }
        assert_eq!(not_plain(&photo), Some("is not a text message"));
        assert_eq!(
            not_plain(&json!({"@type": "forwardMessages"})),
            Some("is not a sendMessage")
        );
    }

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

    #[test]
    fn a_tab_character_is_refused_before_the_window_is_asked() {
        let error = type_text(&json!({"text": "a\tb"}))
            .err()
            .map(|e| e.0)
            .unwrap_or_default();
        assert!(error.contains("tab"), "{error}");
    }
}
