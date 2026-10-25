//! The UI thread's side of the UI driver (`quill::agent::ui_driver`): on
//! every agent wake-up the main window forces GPUI's accessibility tree
//! while an agent drives, and answers the commands tool threads filed on
//! the bridge. Each command runs as a task that waits for a frame with a
//! fresh tree, injects its input one step at a time (`agent_driver_input`)
//! and answers with the tree of the frame after it, so an agent always
//! sees what its action did.
//!
//! Every tree and every step is produced inside one `update` that first
//! decides the surface from the app's state (`agent_driver_surface`) and
//! checks the switches, the grants' generation and the tool's patience;
//! a change stops the command. Nothing here blocks: a frame that never
//! comes answers with a refusal after [`FRAME_TRIES`] polls.
#![deny(clippy::indexing_slicing)]

use super::agent_driver_input::{self as input, Step, steps};
use super::agent_state::active_account;
use super::app::QuillApp;
use gpui_kit::*;
use quill::agent::activity::{self, Action};
use quill::agent::grants;
use quill::agent::ui_driver::bridge::{self, Pending, Target, UiCommand, UiReply, UiSend, UiState};
use quill::agent::ui_driver::guard;
use quill::agent::ui_driver::policy;
use quill::agent::ui_driver::surface::{self, Surface};
use quill::agent::ui_driver::tree::UiTree;
use quill::composer::SendKeyMode;
use std::time::Duration;

/// How often a task looks for a new frame, and how many times.
const FRAME_WAIT: Duration = Duration::from_millis(16);
const FRAME_TRIES: u32 = 90;
/// The agent task wakes this often while the tree is forced, to turn it
/// off once the agent stops driving (`bridge::DRIVE_WINDOW`).
pub(super) const FORCED_WAKE: Duration = Duration::from_secs(5);

impl QuillApp {
    /// Keeps the tree forced while an agent drives and starts every waiting
    /// command. Returns whether the tree is forced now.
    pub(super) fn agent_driver_tick(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        window.set_a11y_forced(bridge::wants_forced());
        for pending in bridge::take() {
            self.agent_driver_start(pending, window, cx);
        }
        window.is_a11y_forced()
    }

    fn agent_driver_start(
        &mut self,
        pending: Pending,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(refusal) = policy::switch_refusal() {
            pending.answer(UiReply::Refused(refusal));
            return;
        }
        match &pending.request.command {
            UiCommand::State => {
                pending.answer(UiReply::State(UiState {
                    open_chat: self.open_chat_id().map(|chat| chat.0),
                    window_active: window.is_window_active(),
                    forced: window.is_a11y_forced(),
                }));
                return;
            }
            UiCommand::ComposerSent { chat_id, text } => {
                self.agent_composer_sent(*chat_id, text, window, cx);
                pending.answer(UiReply::Ok);
                return;
            }
            _ => {}
        }
        // Decided before any frame is waited for, so a blocked surface
        // answers at once.
        if window.is_a11y_system_active() {
            pending.answer(UiReply::Refused(surface::screen_reader_active()));
            return;
        }
        let surface = self.driver_surface();
        let refusal = if pending.request.command.is_input() {
            surface.input_refusal()
        } else {
            surface.read_refusal()
        };
        if let Some(refusal) = refusal {
            pending.answer(UiReply::Refused(refusal));
            return;
        }
        bridge::note_drive();
        window.set_a11y_forced(true);
        let send_key_mode = self.chat_prefs.send_key_mode;
        let generation = grants::generation();
        let before = window.a11y_tree_frame();
        QuillApp::spawn_main_in(window, cx, async move |app, cx| {
            let (mut surface, tree) = match wait_for_tree(&app, cx, before).await {
                Ok(found) => found,
                Err(refusal) => {
                    pending.answer(UiReply::Refused(refusal));
                    return;
                }
            };
            let command = pending.request.command.clone();
            if !command.is_input() {
                pending.answer(UiReply::Tree(tree));
                return;
            }
            let steps = match steps(&command) {
                Ok(steps) => steps,
                Err(refusal) => {
                    pending.answer(UiReply::Refused(refusal));
                    return;
                }
            };
            let mut refusals: Vec<String> = Vec::new();
            let mut ui_send: Option<UiSend> = None;
            let mut stopped: Option<String> = None;
            let mut done = 0usize;
            // The frame the last step was injected into: a step that could
            // activate something waits for the frame after it, so the
            // focus and the surface it meets are the ones its predecessor
            // left (a timer would race the redraw).
            let mut injected_frame: Option<u64> = None;
            for step in &steps {
                if let Some(frame) = injected_frame
                    && step.waits_frame()
                    && let Err(note) = wait_for_frame(&app, cx, frame).await
                {
                    stopped = Some(note);
                    break;
                }
                // Three synchronous parts on the UI thread, nothing runs in
                // between: the checks hold the app, the injection holds only
                // the window (its handlers update the app themselves).
                let outcome = app
                    .update_in(cx, |app, window, _| {
                        (
                            check_step(&pending, generation, &surface, app, window),
                            window.a11y_tree_frame(),
                        )
                    })
                    .map_err(|_| ())
                    .map(|(checked, frame)| match checked {
                        Ok(now) => {
                            surface = now;
                            injected_frame = Some(frame);
                            perform_step(&app, cx, step, &tree, send_key_mode)
                        }
                        Err(refusal) => Err(refusal),
                    });
                refusals.extend(guard::take_refusals());
                let send = app.update(cx, |app, _| app.agent_ui.ui_send.take());
                match outcome {
                    Err(()) => {
                        pending.answer(UiReply::Refused(policy::window_closed()));
                        return;
                    }
                    Ok(Err(refusal)) => {
                        stopped = Some(refusal);
                        break;
                    }
                    Ok(Ok(())) => {
                        done += 1;
                        if let Ok(Some(send)) = send {
                            ui_send = Some(send);
                        }
                    }
                }
            }
            if done == 0 {
                pending.answer(UiReply::Refused(
                    stopped.unwrap_or_else(policy::window_closed),
                ));
                return;
            }
            let _ = app.update(cx, |app, _| {
                app.log_agent_command(
                    &pending.request.agent,
                    &command,
                    &surface,
                    &tree,
                    stopped.is_none(),
                );
            });
            refusals.extend(stopped);
            // A click often takes a frame to show (a dialog, a menu, focus),
            // so the answer waits for the one after the input's own. A
            // surface the agent may not see answers with its note instead.
            let (tree, added) = match wait_for_tree(&app, cx, tree.frame.saturating_add(1)).await {
                Ok((_, after)) => {
                    let added = after.new_since(&tree).into_iter().cloned().collect();
                    (after, added)
                }
                Err(note) => {
                    refusals.push(note);
                    (
                        UiTree {
                            width: tree.width,
                            height: tree.height,
                            scale: tree.scale,
                            frame: tree.frame,
                            ..UiTree::default()
                        },
                        Vec::new(),
                    )
                }
            };
            pending.answer(UiReply::Done {
                tree,
                added,
                refusals,
                ui_send,
            });
        })
        .detach();
    }

    /// One line in the activity log per input command: what the agent did
    /// in the window and in which chat.
    fn log_agent_command(
        &self,
        agent: &str,
        command: &UiCommand,
        surface: &Surface,
        tree: &UiTree,
        ok: bool,
    ) {
        let Some(tool) = command.log_name() else {
            return;
        };
        let chat_id = surface.open_chat().unwrap_or(0);
        let chat_title = self
            .session()
            .and_then(|session| session.chats.get(&chat_id))
            .map(|chat| chat.title.clone())
            .unwrap_or_default();
        activity::record(Action {
            time: 0,
            account: active_account(),
            agent: agent.to_string(),
            tool: tool.to_string(),
            chat_id,
            chat_title,
            summary: command_summary(command, tree),
            ok,
            declined: false,
        });
    }
}

/// `click "Search"`, `type "hello"`, `keys cmd-k`...
fn command_summary(command: &UiCommand, tree: &UiTree) -> String {
    let target = |target: &Target| match target {
        Target::Node(id) => match tree.node(id) {
            Some(node) => format!(
                "\"{}\"",
                node.name
                    .as_deref()
                    .or(node.value.as_deref())
                    .unwrap_or(&node.role)
            ),
            None => format!("node {id}"),
        },
        Target::Point { x, y } => format!("({x:.0}, {y:.0})"),
    };
    match command {
        UiCommand::Click {
            target: at, count, ..
        } if *count > 1 => format!("click x{count} {}", target(at)),
        UiCommand::Click { target: at, .. } => format!("click {}", target(at)),
        UiCommand::OpenMenu { target: at } => format!("menu {}", target(at)),
        UiCommand::Hover { target: at } => format!("hover {}", target(at)),
        UiCommand::Scroll { dx, dy, .. } => format!("scroll ({dx:.0}, {dy:.0})"),
        UiCommand::TypeText { text } => format!("type {text:?}"),
        UiCommand::PressKeys { chords } => format!(
            "keys {}",
            chords
                .iter()
                .map(|chord| chord.describe())
                .collect::<Vec<_>>()
                .join(" ")
        ),
        UiCommand::State | UiCommand::ReadTree | UiCommand::ComposerSent { .. } => String::new(),
    }
}

/// The checks before one step, in the update that injects it: the tool
/// still waits, the switches and the grants are as they were, no screen
/// reader took over, and the surface still takes input. Returns the
/// surface to hold the next step to (a key that opened another granted
/// chat moves it there).
fn check_step(
    pending: &Pending,
    generation: u64,
    expected: &Surface,
    app: &QuillApp,
    window: &Window,
) -> Result<Surface, String> {
    if pending.cancelled() {
        return Err(policy::cancelled());
    }
    if let Some(refusal) = policy::switch_refusal() {
        return Err(refusal);
    }
    if grants::generation() != generation {
        return Err(policy::grants_changed());
    }
    if window.is_a11y_system_active() {
        return Err(surface::screen_reader_active());
    }
    let now = app.driver_surface();
    if let Some(refusal) = now.input_refusal() {
        return Err(if now.same_as(expected) {
            refusal
        } else {
            surface::changed_refusal(expected, &now)
        });
    }
    Ok(now)
}

/// One step: the window injects it; a pointer step asks the app whether
/// the spot GPUI reports under the pointer is allowed before pressing.
/// The injecting flag spans each update, so the effects a handler queues
/// (an emitted event, a subscription) run under it too.
fn perform_step(
    app: &super::chat_window::ScopedApp,
    cx: &mut AsyncWindowContext,
    step: &Step,
    tree: &UiTree,
    send_key_mode: SendKeyMode,
) -> Result<(), String> {
    guard::set_injecting(true);
    let result = perform_step_injecting(app, cx, step, tree, send_key_mode);
    guard::set_injecting(false);
    result
}

fn perform_step_injecting(
    app: &super::chat_window::ScopedApp,
    cx: &mut AsyncWindowContext,
    step: &Step,
    tree: &UiTree,
    send_key_mode: SendKeyMode,
) -> Result<(), String> {
    fn closed<E>(_: E) -> String {
        policy::window_closed()
    }
    let Step::Pointer { target, pointer } = step else {
        let focus_allowed = app
            .update_in(cx, |app, window, _| app.agent_focus_allowed(window))
            .map_err(closed)?;
        return cx
            .update(|window, cx| {
                input::perform_key_step(step, send_key_mode, focus_allowed, window, cx)
            })
            .map_err(closed)?;
    };
    let position = cx
        .update(|window, cx| input::point_at(target, pointer, tree, window, cx))
        .map_err(closed)??;
    let allowed = app
        .update_in(cx, |app, window, _| app.agent_pointer_allowed(window))
        .map_err(closed)?;
    if !allowed {
        return Err(policy::point_outside_region(
            position.x.as_f32(),
            position.y.as_f32(),
        ));
    }
    cx.update(|window, cx| input::press_at(position, pointer, window, cx))
        .map_err(closed)
}

/// A frame later than `after` drawn, asking for a redraw until one comes.
/// `Err` when the window closed or no frame came.
async fn wait_for_frame(
    app: &super::chat_window::ScopedApp,
    cx: &mut AsyncWindowContext,
    after: u64,
) -> Result<(), String> {
    for _ in 0..FRAME_TRIES {
        let drawn = app.update_in(cx, |_, window, _| {
            window.set_a11y_forced(true);
            if window.a11y_tree_frame() > after {
                true
            } else {
                window.refresh();
                false
            }
        });
        match drawn {
            Err(_) => return Err(policy::window_closed()),
            Ok(true) => return Ok(()),
            Ok(false) => {}
        }
        cx.background_executor().timer(FRAME_WAIT).await;
    }
    Err(policy::no_frame())
}

/// The surface and the first tree from a frame later than `after`, asking
/// for a redraw until one comes. `Err` when the window closed, the surface
/// may not be read, or no frame came.
async fn wait_for_tree(
    app: &super::chat_window::ScopedApp,
    cx: &mut AsyncWindowContext,
    after: u64,
) -> Result<(Surface, UiTree), String> {
    for _ in 0..FRAME_TRIES {
        let found = app.update_in(cx, |app, window, _| {
            window.set_a11y_forced(true);
            if window.a11y_tree_frame() > after {
                app.agent_tree(window)
                    .map(|(surface, tree)| tree.map(|tree| (surface, tree)))
            } else {
                window.refresh();
                Ok(None)
            }
        });
        match found {
            Err(_) => return Err(policy::window_closed()),
            Ok(Err(note)) => return Err(note),
            Ok(Ok(Some(found))) => return Ok(found),
            Ok(Ok(None)) => {}
        }
        cx.background_executor().timer(FRAME_WAIT).await;
    }
    Err(policy::no_frame())
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "agent_driver_tests.rs"]
mod tests;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "agent_driver_redact_tests.rs"]
mod redact_tests;
