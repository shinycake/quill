//! Methods moved out of `history.rs` to keep files under 1000 lines.

use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr};

impl QuillApp {
    pub(in crate::ui) fn load_older_action(&mut self, cx: &mut Context<Self>) {
        match self.pane_mode() {
            PaneMode::Ready => {
                // Phase 5.1: a topic view pages its own history.
                let topic_open = self
                    .live
                    .as_ref()
                    .and_then(|live| live.driver.session.open_topic)
                    .is_some();
                let thread_active = self.thread_active();
                if let Some(live) = self.live.as_mut() {
                    let result = if thread_active {
                        live.driver.fetch_thread_history()
                    } else if topic_open {
                        live.driver.fetch_topic_history()
                    } else {
                        live.driver.fetch_history()
                    };
                    self.connection.set_note(match result {
                        Ok(Some(_)) => {
                            StatusNote::silent(tr(keys::Q_LEFTOVERS_LOADING_OLDER_MESSAGES))
                        }
                        Ok(None) => {
                            StatusNote::silent(tr(keys::Q_LEFTOVERS_NO_OLDER_MESSAGES_TO_LOAD))
                        }
                        Err(_) => StatusNote::error(tr(keys::Q_LEFTOVERS_COULD_NOT_LOAD_HISTORY)),
                    });
                }
                cx.notify();
            }
            PaneMode::Synthetic => {
                self.chat.update(cx, |chat, cx| chat.prepend_older(cx));
            }
            PaneMode::Connecting => {}
        }
    }
}
