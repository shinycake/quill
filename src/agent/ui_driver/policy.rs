//! What the UI driver refuses, worded as the agent's next step. The gates
//! themselves live where the state is: the switches here, the surface in
//! `super::surface` (computed by the UI from its own state), the action
//! guard in `super::guard`.

use super::keys::{self, KeyChord};

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
            "Driving Quill's window is off. Ask the user to turn on \"Let agents drive Quill's window\" (Quill: Settings > Advanced > Agent access), then retry. The chat tools (list_chats, read_history, send_message...) work without it."
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

/// Why `chord` is not pressed: it leaves Quill, or it is Enter or Space
/// while the focus is not a text box (they would activate a button or a
/// menu item; agents click those instead).
pub fn chord_refusal(chord: &KeyChord, focus_in_text: bool) -> Option<String> {
    if let Some(effect) = keys::forbidden(chord) {
        return Some(format!(
            "{} would {effect}; agents never press it.",
            chord.describe()
        ));
    }
    if matches!(chord.key.as_str(), "enter" | "space") && !focus_in_text {
        return Some(format!(
            "{} is pressed only while a text box has the focus; it would activate the focused control. Click the control with click_element instead.",
            chord.describe()
        ));
    }
    None
}

/// `type_text` while no text box has the focus.
pub fn no_text_focus() -> String {
    "No text box has the focus in Quill's window, so nothing was typed. Click the box first (click_element), then type_text.".into()
}

/// `type_text` with a tab character.
pub fn tab_in_text() -> String {
    "text has a tab character; Tab moves the focus, so it is not typed. Use spaces, or press_keys {\"keys\": \"tab\"} to move the focus.".into()
}

/// The window drew no new frame in time.
pub fn no_frame() -> String {
    "Quill's window did not redraw in time. Make sure it is open and visible, then retry.".into()
}

/// The window closed before the command ran.
pub fn window_closed() -> String {
    "Quill's window closed before the command ran.".into()
}

/// The tool stopped waiting before the UI could inject.
pub fn cancelled() -> String {
    "The tool stopped waiting before Quill's window could act, so nothing was sent.".into()
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

/// A point over something the agent may not use.
pub fn point_outside_region(x: f32, y: f32) -> String {
    format!(
        "The point ({x}, {y}) is not over a part of Quill's window agents may use (the chat list's granted rows and the open granted chat). Call read_ui_tree and click a node from it."
    )
}

/// A key for a focused element that is not inside a part agents may use.
pub fn focus_outside_region() -> String {
    "The focused text box is not inside a part of Quill's window agents may use (the open granted chat and the chat list's granted rows), so the key was not pressed. Click a text box there first (click_element).".into()
}

/// A node whose rectangle is not inside the window.
pub fn node_off_screen(id: &str) -> String {
    format!("Node {id} is outside the visible window. Scroll it into view first (scroll_view).")
}

/// The switches or the grants changed while a command ran.
pub fn changed_mid_flight(what: &str) -> String {
    format!("{what} changed in Quill while the command ran, so it stopped. Call the tool again.")
}

/// The user changed the chat grants while a command ran.
pub fn grants_changed() -> String {
    changed_mid_flight("The chat grants")
}

#[cfg(test)]
mod tests {
    use super::chord_refusal;
    use crate::agent::ui_driver::keys::parse_chord;

    #[test]
    fn quit_chords_and_activating_keys_are_refused() {
        let quit = parse_chord("cmd-q").unwrap();
        assert!(chord_refusal(&quit, true).is_some_and(|r| r.contains("quit")));
        let enter = parse_chord("enter").unwrap();
        assert!(chord_refusal(&enter, false).is_some_and(|r| r.contains("click_element")));
        assert_eq!(chord_refusal(&enter, true), None, "Enter types in a box");
        let space = parse_chord("space").unwrap();
        assert!(chord_refusal(&space, false).is_some());
        assert_eq!(chord_refusal(&space, true), None);
        assert_eq!(chord_refusal(&parse_chord("tab").unwrap(), false), None);
        assert_eq!(chord_refusal(&parse_chord("cmd-k").unwrap(), false), None);
    }
}
