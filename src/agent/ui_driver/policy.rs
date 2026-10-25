//! What the UI driver refuses, worded as the agent's next step. The driver
//! never gets around phase 3: input runs only in a chat granted
//! `read_write`, the window is read only while the open chat is granted
//! `read`, and the controls that send, delete or pay are never clicked
//! (`super::tree::guarded_by_name`).

use super::keys::{self, KeyChord};
use crate::agent::grants::Scope;
use crate::composer::SendKeyMode;

/// The switches: Agent access, "Let agents drive Quill's window", and the
/// pause. `None` when the driver may run.
pub fn switch_refusal() -> Option<String> {
    if !crate::agent::access::enabled() {
        return Some(
            "Agent access is off in Quill. Ask the user to turn it on (Quill: Settings > Advanced > Agent access), then retry."
                .into(),
        );
    }
    if !crate::agent::access::ui_driver() {
        return Some(
            "Driving Quill's window is off. Ask the user to turn on \"Let agents drive Quill's window\" (Quill: Settings > Advanced > Agent access). The chat tools (list_chats, read_history, send_message...) work without it."
                .into(),
        );
    }
    if super::bridge::paused() {
        return Some(
            "The UI driver is paused because the user is using Quill's window. Wait a moment and retry, or use the chat tools instead."
                .into(),
        );
    }
    None
}

/// Reading the window while `open_chat` is open with `scope` granted.
pub fn read_refusal(open_chat: Option<i64>, scope: Option<Scope>) -> Option<String> {
    let chat = open_chat?;
    if scope.is_some() {
        return None;
    }
    Some(format!(
        "The chat open in Quill's window ({chat}) is not granted to agents, so the window cannot be read. Call request_access {{\"chat\": {chat}, \"scope\": \"read\", \"reason\": ...}}, or ask the user to open another chat."
    ))
}

/// Sending input while `open_chat` is open with `scope` granted.
pub fn input_refusal(open_chat: Option<i64>, scope: Option<Scope>) -> Option<String> {
    let chat = open_chat?;
    match scope {
        Some(scope) if scope.allows(Scope::ReadWrite) => None,
        Some(_) => Some(format!(
            "The chat open in Quill's window ({chat}) is granted for reading only, so no clicks or keys go to it. Call request_access {{\"chat\": {chat}, \"scope\": \"read_write\", \"reason\": ...}}, or ask the user to open another chat."
        )),
        None => Some(format!(
            "The chat open in Quill's window ({chat}) is not granted to agents, so no clicks or keys go to it. Call request_access {{\"chat\": {chat}, \"scope\": \"read_write\", \"reason\": ...}}, or ask the user to open another chat."
        )),
    }
}

/// Why `chord` is not pressed: it leaves Quill, or it would send the
/// message in the focused text box under `mode`.
pub fn chord_refusal(chord: &KeyChord, focus_in_text: Option<bool>, mode: SendKeyMode) -> Option<String> {
    if let Some(effect) = keys::forbidden(chord) {
        return Some(format!(
            "{} would {effect}; agents never press it.",
            chord.describe()
        ));
    }
    if keys::sends_message(chord, mode) && focus_in_text != Some(false) {
        return Some(format!(
            "{} would send the message in the focused text box, which skips Quill's pacing and confirmations. Use send_message to send, or {} for a line break.",
            chord.describe(),
            keys::newline_chord(mode).describe()
        ));
    }
    None
}

/// The window drew no new frame in time.
pub fn no_frame() -> String {
    "Quill's window did not redraw in time. Make sure it is open and visible, then retry.".into()
}

/// The window closed before the command ran.
pub fn window_closed() -> String {
    "Quill's window closed before the command ran.".into()
}

/// A node id that is not in the current tree.
pub fn node_gone(id: &str) -> String {
    format!("Node {id} is not on screen any more. Call read_ui_tree again and use a current id.")
}

/// A node that is in the tree but hidden.
pub fn node_hidden(id: &str, role: &str, name: &str) -> String {
    format!("Node {id} ({role} \"{name}\") is hidden right now. Call read_ui_tree again.")
}

/// A node without a rectangle, so nothing can be pointed at it.
pub fn node_without_bounds(id: &str, role: &str, name: &str) -> String {
    format!(
        "Node {id} ({role} \"{name}\") has no position on screen, so it cannot be pointed at. Pick a node with bounds."
    )
}

/// A point outside the window.
pub fn point_outside(x: f32, y: f32, width: f32, height: f32) -> String {
    format!("The point ({x}, {y}) is outside the window ({width}x{height} px).")
}

/// The open chat changed between the grant check and the command.
pub fn moved_refusal(expected: Option<i64>, open: Option<i64>) -> String {
    format!(
        "The open chat changed (expected {}, now {}) before the command ran, so nothing was sent. Call the tool again.",
        describe_chat(expected),
        describe_chat(open)
    )
}

fn describe_chat(chat: Option<i64>) -> String {
    chat.map_or_else(|| "no chat".to_string(), |id| id.to_string())
}

#[cfg(test)]
mod tests {
    use super::{chord_refusal, input_refusal, moved_refusal, read_refusal};
    use crate::agent::grants::Scope;
    use crate::agent::ui_driver::keys::parse_chord;
    use crate::composer::SendKeyMode;

    #[test]
    fn reading_needs_read_on_the_open_chat() {
        assert_eq!(read_refusal(None, None), None, "no chat open");
        assert_eq!(read_refusal(Some(7), Some(Scope::Read)), None);
        assert!(read_refusal(Some(7), None).is_some_and(|r| r.contains("request_access")));
    }

    #[test]
    fn input_needs_read_write_on_the_open_chat() {
        assert_eq!(input_refusal(None, None), None);
        assert_eq!(input_refusal(Some(7), Some(Scope::ReadWrite)), None);
        assert!(input_refusal(Some(7), Some(Scope::Read)).is_some_and(|r| r.contains("reading only")));
        assert!(input_refusal(Some(7), None).is_some_and(|r| r.contains("not granted")));
    }

    #[test]
    fn chords_that_quit_or_send_are_refused() {
        let quit = parse_chord("cmd-q").unwrap();
        assert!(chord_refusal(&quit, Some(false), SendKeyMode::Enter).is_some_and(|r| r.contains("quit")));
        let enter = parse_chord("enter").unwrap();
        assert!(chord_refusal(&enter, Some(true), SendKeyMode::Enter).is_some_and(|r| r.contains("shift-enter")));
        assert!(chord_refusal(&enter, None, SendKeyMode::Enter).is_some(), "unknown focus is refused");
        assert_eq!(chord_refusal(&enter, Some(false), SendKeyMode::Enter), None);
        assert_eq!(chord_refusal(&enter, Some(true), SendKeyMode::CtrlEnter), None);
        let cmd_enter = parse_chord("cmd-enter").unwrap();
        assert!(chord_refusal(&cmd_enter, Some(true), SendKeyMode::CtrlEnter).is_some());
        assert_eq!(chord_refusal(&parse_chord("cmd-k").unwrap(), None, SendKeyMode::Enter), None);
    }

    #[test]
    fn a_moved_chat_is_named() {
        let text = moved_refusal(Some(1), None);
        assert!(text.contains("expected 1, now no chat"));
    }
}
