//! Input the UI driver injects into the window: the same `PlatformInput`
//! events the platform would send, through `Window::dispatch_event`, so
//! they reach the same handlers a real click or key does. A command is
//! split into [`Step`]s (one pointer action, one typed character, one key
//! chord) and `agent_driver` re-checks the surface, the switches and the
//! grants before each. Every injection runs under
//! `quill::agent::ui_driver::guard::injecting`, set by `agent_driver`
//! around the whole update, which the action guards and
//! `presence::input_probe` read.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::gpui::{
    App, KeyDownEvent, Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, PlatformInput, Point, ScrollDelta, ScrollWheelEvent, TouchPhase, Window,
    point, px,
};
use quill::agent::ui_driver::bridge::{Button, Target, UiCommand};
use quill::agent::ui_driver::guard;
use quill::agent::ui_driver::keys::{self, KeyChord};
use quill::agent::ui_driver::policy;
use quill::agent::ui_driver::redact;
use quill::agent::ui_driver::tree::UiTree;
use quill::composer::SendKeyMode;

/// What a pointer step does once the pointer is over its target.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Pointer {
    Click { button: Button, count: u32 },
    Menu,
    Hover,
    Scroll { dx: f32, dy: f32 },
}

/// One injection, checked on its own.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Step {
    Pointer {
        target: Target,
        pointer: Pointer,
    },
    /// One typed character, into the focused text box only.
    Char(char),
    /// A line break: the chord that inserts one without sending.
    Newline,
    Chord(KeyChord),
}

impl Step {
    /// Whether a frame should pass before this step, so the focus and the
    /// surface reflect the step before it. Plain characters run back to
    /// back; anything that could activate a control waits.
    pub(super) fn waits_frame(&self) -> bool {
        !matches!(self, Step::Char(ch) if *ch != ' ')
    }
}

/// The steps of `command`; `Err` when it cannot run at all.
pub(super) fn steps(command: &UiCommand) -> Result<Vec<Step>, String> {
    let pointer = |target: &Target, pointer: Pointer| Step::Pointer {
        target: target.clone(),
        pointer,
    };
    Ok(match command {
        UiCommand::State | UiCommand::ReadTree | UiCommand::ComposerSent { .. } => Vec::new(),
        UiCommand::Click {
            target,
            button,
            count,
        } => vec![pointer(
            target,
            Pointer::Click {
                button: *button,
                count: *count,
            },
        )],
        UiCommand::OpenMenu { target } => vec![pointer(target, Pointer::Menu)],
        UiCommand::Hover { target } => vec![pointer(target, Pointer::Hover)],
        UiCommand::Scroll { target, dx, dy } => {
            vec![pointer(target, Pointer::Scroll { dx: *dx, dy: *dy })]
        }
        UiCommand::TypeText { text } => {
            if text.contains('\t') {
                return Err(policy::tab_in_text());
            }
            text.chars()
                .filter(|ch| *ch != '\r')
                .map(|ch| {
                    if ch == '\n' {
                        Step::Newline
                    } else {
                        Step::Char(ch)
                    }
                })
                .collect()
        }
        UiCommand::PressKeys { chords } => chords.iter().cloned().map(Step::Chord).collect(),
    })
}

/// The caller holds `guard::set_injecting` around the whole update: a
/// handler's effects (an emitted event, a subscription) run when the
/// update ends, and they must still count as the agent's.
fn inject(window: &mut Window, cx: &mut App, events: Vec<PlatformInput>) {
    debug_assert!(guard::injecting(), "inject outside the injecting scope");
    for event in events {
        window.dispatch_event(event, cx);
    }
    window.refresh();
}

/// A typed character: the key down, then the character to the focused
/// input handler, as `Window::dispatch_keystroke` does.
fn inject_keystroke(window: &mut Window, cx: &mut App, keystroke: Keystroke) {
    debug_assert!(guard::injecting(), "inject outside the injecting scope");
    window.dispatch_keystroke(keystroke, cx);
    window.refresh();
}

/// A chord: the key down only. `Window::dispatch_keystroke` would also hand
/// a character to the focused text box for Enter, Tab and Space, which a
/// platform never does for a key the window let through (macOS sends Return
/// as `insertNewline:`, a command, not text), so Enter in the message box
/// would send the text and append a line break to the draft.
fn inject_chord(window: &mut Window, cx: &mut App, keystroke: Keystroke) {
    debug_assert!(guard::injecting(), "inject outside the injecting scope");
    window.dispatch_event(
        PlatformInput::KeyDown(KeyDownEvent {
            keystroke,
            is_held: false,
            prefer_character_input: false,
        }),
        cx,
    );
    window.refresh();
}

/// Where a target lands: a node's centre (the node must be on screen) or
/// a point inside the window.
fn resolve(target: &Target, tree: &UiTree, window: &Window) -> Result<Point<Pixels>, String> {
    let size = window.viewport_size();
    let (width, height) = (size.width.as_f32(), size.height.as_f32());
    match target {
        Target::Node(id) => {
            let node = tree.node(id).ok_or_else(|| policy::node_gone(id))?;
            let name = node.name.as_deref().unwrap_or("");
            if node.states.iter().any(|s| s == "hidden") {
                return Err(policy::node_hidden(id, &node.role, name));
            }
            let (x, y) = node
                .center()
                .ok_or_else(|| policy::node_without_bounds(id, &node.role, name))?;
            if x < 0.0 || y < 0.0 || x > width || y > height {
                return Err(policy::node_off_screen(id));
            }
            Ok(point(px(x), px(y)))
        }
        Target::Point { x, y } => {
            if *x < 0.0 || *y < 0.0 || *x > width || *y > height {
                return Err(policy::point_outside(*x, *y, width, height));
            }
            Ok(point(px(*x), px(*y)))
        }
    }
}

fn mouse_button(button: Button) -> MouseButton {
    match button {
        Button::Left => MouseButton::Left,
        Button::Right => MouseButton::Right,
        Button::Middle => MouseButton::Middle,
    }
}

fn move_to(position: Point<Pixels>) -> PlatformInput {
    PlatformInput::MouseMove(MouseMoveEvent {
        position,
        pressed_button: None,
        modifiers: Modifiers::default(),
    })
}

fn clicks(position: Point<Pixels>, button: MouseButton, count: u32) -> Vec<PlatformInput> {
    let mut events = Vec::new();
    for click in 1..=count.clamp(1, 3) as usize {
        events.push(PlatformInput::MouseDown(MouseDownEvent {
            button,
            position,
            modifiers: Modifiers::default(),
            click_count: click,
            first_mouse: false,
        }));
        events.push(PlatformInput::MouseUp(MouseUpEvent {
            button,
            position,
            modifiers: Modifiers::default(),
            click_count: click,
        }));
    }
    events
}

fn keystroke(chord: &KeyChord) -> Keystroke {
    Keystroke {
        modifiers: Modifiers {
            control: chord.ctrl,
            alt: chord.alt,
            shift: chord.shift,
            platform: chord.cmd,
            function: chord.function,
        },
        key: chord.key.clone(),
        key_char: None,
    }
}

/// One typed character as the platform would report it: the key is its
/// lowercase form, the character rides in `key_char` for the input handler.
fn typed(ch: char) -> Keystroke {
    let key = match ch {
        ' ' => "space".to_string(),
        other => other.to_lowercase().to_string(),
    };
    Keystroke {
        modifiers: Modifiers::default(),
        key,
        key_char: Some(ch.to_string()),
    }
}

/// A pointer step's first half: resolve the target and move the pointer
/// there, so GPUI's own hit test says what is under it. Returns the point.
pub(super) fn point_at(
    target: &Target,
    pointer: &Pointer,
    tree: &UiTree,
    window: &mut Window,
    cx: &mut App,
) -> Result<Point<Pixels>, String> {
    let position = resolve(target, tree, window)?;
    if let (
        Target::Node(id),
        Pointer::Click {
            button: Button::Left,
            ..
        },
    ) = (target, pointer)
        && let Some(refusal) = tree.click_refusal(id)
    {
        return Err(refusal);
    }
    inject(window, cx, vec![move_to(position)]);
    Ok(position)
}

/// A pointer step's second half, once the spot under the pointer passed
/// (`QuillApp::agent_pointer_allowed`).
pub(super) fn press_at(
    position: Point<Pixels>,
    pointer: &Pointer,
    window: &mut Window,
    cx: &mut App,
) {
    let events = match pointer {
        Pointer::Click { button, count } => clicks(position, mouse_button(*button), *count),
        Pointer::Menu => clicks(position, MouseButton::Right, 1),
        Pointer::Hover => Vec::new(),
        Pointer::Scroll { dx, dy } => {
            // Agents say "dy > 0 scrolls down"; a wheel delta moves the
            // content, so it points the other way.
            vec![PlatformInput::ScrollWheel(ScrollWheelEvent {
                position,
                delta: ScrollDelta::Pixels(point(px(-dx), px(-dy))),
                modifiers: Modifiers::default(),
                touch_phase: TouchPhase::Moved,
            })]
        }
    };
    if !events.is_empty() {
        inject(window, cx, events);
    }
}

/// A typed character, a line break or a chord: only into a focused text
/// box, or a chord that activates nothing.
pub(super) fn perform_key_step(
    step: &Step,
    send_key_mode: SendKeyMode,
    window: &mut Window,
    cx: &mut App,
) -> Result<(), String> {
    let in_text = window.focused_text_input();
    match step {
        Step::Pointer { .. } => Ok(()),
        Step::Char(ch) => {
            if !in_text {
                return Err(policy::no_text_focus());
            }
            inject_keystroke(window, cx, typed(*ch));
            Ok(())
        }
        Step::Newline => {
            if !in_text {
                return Err(policy::no_text_focus());
            }
            inject_chord(window, cx, keystroke(&keys::newline_chord(send_key_mode)));
            Ok(())
        }
        Step::Chord(chord) => {
            if let Some(refusal) = policy::chord_refusal(chord, in_text) {
                return Err(refusal);
            }
            inject_chord(window, cx, keystroke(chord));
            Ok(())
        }
    }
}

impl QuillApp {
    /// Whether the elements GPUI reports under the pointer now (clipped and
    /// layered as drawn) are ones the driver may act on.
    pub(super) fn agent_pointer_allowed(&self, window: &Window) -> bool {
        let tags: Vec<String> = window
            .hovered_a11y_tags()
            .into_iter()
            .map(|tag| tag.to_string())
            .collect();
        redact::point_allowed(&tags, self.driver_scope(), &|chat_id| {
            self.driver_may_open_chat(chat_id)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Step, steps};
    use quill::agent::ui_driver::bridge::UiCommand;

    #[test]
    fn text_splits_into_characters_and_refuses_tabs() {
        let typed = steps(&UiCommand::TypeText {
            text: "a b\r\nc".into(),
        })
        .unwrap_or_default();
        assert_eq!(
            typed,
            [
                Step::Char('a'),
                Step::Char(' '),
                Step::Char('b'),
                Step::Newline,
                Step::Char('c')
            ]
        );
        assert!(!Step::Char('a').waits_frame());
        assert!(Step::Char(' ').waits_frame());
        assert!(Step::Newline.waits_frame());
        assert!(
            steps(&UiCommand::TypeText {
                text: "a\tb".into()
            })
            .is_err_and(|e| e.contains("tab"))
        );
        assert!(steps(&UiCommand::ReadTree).unwrap_or_default().is_empty());
    }
}
