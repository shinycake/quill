//! Input the UI driver injects into the window: the same `PlatformInput`
//! events the platform would send, through `Window::dispatch_event`, so
//! they reach the same handlers a real click or key does. Every injection
//! runs under [`injecting`], which `presence::input_probe` reads to tell
//! the user's input from the agent's.
#![deny(clippy::indexing_slicing)]

use gpui_kit::gpui::{
    App, Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    PlatformInput, Point, ScrollDelta, ScrollWheelEvent, TouchPhase, Window, point, px,
};
use quill::agent::ui_driver::bridge::{Button, Target, UiCommand};
use quill::agent::ui_driver::keys::{self, KeyChord};
use quill::agent::ui_driver::policy;
use quill::agent::ui_driver::tree::{UiNode, UiTree};
use quill::composer::SendKeyMode;
use std::cell::Cell;

thread_local! {
    static INJECTING: Cell<bool> = const { Cell::new(false) };
}

/// Whether the input being handled right now came from the driver.
pub(super) fn injecting() -> bool {
    INJECTING.with(Cell::get)
}

fn inject(window: &mut Window, cx: &mut App, events: Vec<PlatformInput>) {
    INJECTING.with(|flag| flag.set(true));
    for event in events {
        window.dispatch_event(event, cx);
    }
    INJECTING.with(|flag| flag.set(false));
    window.refresh();
}

fn inject_keystrokes(window: &mut Window, cx: &mut App, keystrokes: Vec<Keystroke>) {
    INJECTING.with(|flag| flag.set(true));
    for keystroke in keystrokes {
        window.dispatch_keystroke(keystroke, cx);
    }
    INJECTING.with(|flag| flag.set(false));
    window.refresh();
}

/// Where a target lands, and the node there (for the guarded-control check).
fn resolve<'a>(
    target: &Target,
    tree: &'a UiTree,
    window: &Window,
) -> Result<(Point<Pixels>, Option<&'a UiNode>), String> {
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
            Ok((point(px(x), px(y)), Some(node)))
        }
        Target::Point { x, y } => {
            let size = window.viewport_size();
            let (width, height) = (size.width.as_f32(), size.height.as_f32());
            if *x < 0.0 || *y < 0.0 || *x > width || *y > height {
                return Err(policy::point_outside(*x, *y, width, height));
            }
            Ok((point(px(*x), px(*y)), tree.hit_test(*x, *y)))
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
    let mut events = vec![move_to(position)];
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
        '\t' => "tab".to_string(),
        other => other.to_lowercase().to_string(),
    };
    Keystroke {
        modifiers: Modifiers::default(),
        key,
        key_char: Some(ch.to_string()),
    }
}

fn focus_in_text(tree: &UiTree) -> Option<bool> {
    let focus = tree.focus.as_deref()?;
    let node = tree.node(focus)?;
    Some(matches!(
        node.role.as_str(),
        "TextInput" | "MultilineTextInput" | "SearchInput" | "EmailInput" | "PasswordInput"
    ))
}

/// Inject `command` against `tree`, the window as last drawn. An `Err`
/// says why nothing was sent.
pub(super) fn perform(
    command: &UiCommand,
    tree: &UiTree,
    send_key_mode: SendKeyMode,
    window: &mut Window,
    cx: &mut App,
) -> Result<(), String> {
    match command {
        UiCommand::State | UiCommand::ReadTree => Ok(()),
        UiCommand::Click {
            target,
            button,
            count,
        } => {
            let (position, node) = resolve(target, tree, window)?;
            if *button == Button::Left
                && let Some(node) = node
                && let Some(refusal) = tree.click_refusal(&node.id)
            {
                return Err(refusal);
            }
            inject(window, cx, clicks(position, mouse_button(*button), *count));
            Ok(())
        }
        UiCommand::OpenMenu { target } => {
            let (position, _) = resolve(target, tree, window)?;
            inject(window, cx, clicks(position, MouseButton::Right, 1));
            Ok(())
        }
        UiCommand::Hover { target } => {
            let (position, _) = resolve(target, tree, window)?;
            inject(window, cx, vec![move_to(position)]);
            Ok(())
        }
        UiCommand::Scroll { target, dx, dy } => {
            let (position, _) = resolve(target, tree, window)?;
            // Agents say "dy > 0 scrolls down"; a wheel delta moves the
            // content, so it points the other way.
            let delta = ScrollDelta::Pixels(point(px(-dx), px(-dy)));
            inject(
                window,
                cx,
                vec![
                    move_to(position),
                    PlatformInput::ScrollWheel(ScrollWheelEvent {
                        position,
                        delta,
                        modifiers: Modifiers::default(),
                        touch_phase: TouchPhase::Moved,
                    }),
                ],
            );
            Ok(())
        }
        UiCommand::TypeText { text } => {
            let newline = keys::newline_chord(send_key_mode);
            let keystrokes = text
                .chars()
                .filter(|ch| *ch != '\r')
                .map(|ch| {
                    if ch == '\n' {
                        keystroke(&newline)
                    } else {
                        typed(ch)
                    }
                })
                .collect();
            inject_keystrokes(window, cx, keystrokes);
            Ok(())
        }
        UiCommand::PressKeys { chords } => {
            let in_text = focus_in_text(tree);
            for chord in chords {
                if let Some(refusal) = policy::chord_refusal(chord, in_text, send_key_mode) {
                    return Err(refusal);
                }
            }
            inject_keystrokes(window, cx, chords.iter().map(keystroke).collect());
            Ok(())
        }
    }
}
