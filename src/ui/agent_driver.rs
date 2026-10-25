//! The UI thread's side of the UI driver (`quill::agent::ui_driver`): on
//! every agent wake-up the main window forces GPUI's accessibility tree
//! while an agent drives, and answers the commands tool threads filed on
//! the bridge. Each command runs as a task that waits for a frame with a
//! fresh tree, injects its input (`agent_driver_input`) and answers with
//! the tree of the frame after it, so an agent always sees what its action
//! did. Nothing here blocks: a frame that never comes answers with a
//! refusal after [`FRAME_TRIES`] polls.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::*;
use quill::agent::ui_driver::bridge::{self, Pending, UiCommand, UiReply, UiState};
use quill::agent::ui_driver::policy;
use quill::agent::ui_driver::tree::UiTree;
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
    pub(super) fn agent_driver_tick(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        window.set_a11y_forced(bridge::wants_forced());
        for pending in bridge::take() {
            self.agent_driver_start(pending, window, cx);
        }
        window.is_a11y_forced()
    }

    fn agent_driver_start(&mut self, pending: Pending, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(refusal) = policy::switch_refusal() {
            pending.answer(UiReply::Refused(refusal));
            return;
        }
        let open_chat = self.open_chat_id().map(|chat| chat.0);
        if pending.request.command == UiCommand::State {
            pending.answer(UiReply::State(UiState {
                open_chat,
                window_active: window.is_window_active(),
                forced: window.is_a11y_forced(),
            }));
            return;
        }
        if let Some(expected) = pending.request.expect_open_chat
            && expected != open_chat
        {
            pending.answer(UiReply::Refused(policy::moved_refusal(expected, open_chat)));
            return;
        }
        bridge::note_drive();
        window.set_a11y_forced(true);
        let send_key_mode = self.chat_prefs.send_key_mode;
        let before = window.a11y_tree_frame();
        QuillApp::spawn_main_in(window, cx, async move |app, cx| {
            let Some(tree) = wait_for_tree(cx, before).await else {
                pending.answer(UiReply::Refused(policy::no_frame()));
                return;
            };
            if !pending.request.command.is_input() {
                pending.answer(UiReply::Tree(tree));
                return;
            }
            let command = pending.request.command.clone();
            let expected = pending.request.expect_open_chat;
            // The open chat is read through the app; the input goes through
            // the window alone, since its handlers update the app themselves.
            let Ok(open) = app.update(cx, |app, _| app.open_chat_id().map(|chat| chat.0)) else {
                pending.answer(UiReply::Refused(policy::window_closed()));
                return;
            };
            if let Some(expected) = expected
                && expected != open
            {
                pending.answer(UiReply::Refused(policy::moved_refusal(expected, open)));
                return;
            }
            let outcome = cx.update(|window, cx| {
                super::agent_driver_input::perform(&command, &tree, send_key_mode, window, cx)
            });
            match outcome {
                Err(_) => {
                    pending.answer(UiReply::Refused(policy::window_closed()));
                    return;
                }
                Ok(Err(refusal)) => {
                    pending.answer(UiReply::Refused(refusal));
                    return;
                }
                Ok(Ok(())) => {}
            }
            // A click often takes a frame to show (a dialog, a menu, focus),
            // so the answer waits for the one after the input's own.
            let Some(after) = wait_for_tree(cx, tree.frame.saturating_add(1)).await else {
                pending.answer(UiReply::Refused(policy::no_frame()));
                return;
            };
            let added = after.new_since(&tree).into_iter().cloned().collect();
            pending.answer(UiReply::Done { tree: after, added });
        })
        .detach();
    }
}

/// The first tree from a frame later than `after`, asking for a redraw
/// until one comes. `None` when the window closed or no frame came.
async fn wait_for_tree(cx: &mut AsyncWindowContext, after: u64) -> Option<UiTree> {
    for _ in 0..FRAME_TRIES {
        let found = cx.update(|window, _| {
            window.set_a11y_forced(true);
            if window.a11y_tree_frame() > after {
                super::agent_driver_tree::read(window)
            } else {
                window.refresh();
                None
            }
        });
        match found {
            Err(_) => return None,
            Ok(Some(tree)) => return Some(tree),
            Ok(None) => {}
        }
        cx.background_executor().timer(FRAME_WAIT).await;
    }
    None
}

#[cfg(test)]
#[path = "agent_driver_tests.rs"]
mod tests;
