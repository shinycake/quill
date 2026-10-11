//! Methods moved out of `history.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    pub(in crate::ui) fn load_older_action(&mut self, cx: &mut Context<Self>) {
        match self.pane_mode() {
            PaneMode::Ready => {
                if self.live.is_some() {
                    // Phase 5.1: a topic view pages its own history.
                    let topic_open = self
                        .live
                        .as_ref()
                        .and_then(|live| live.driver.session.open_topic)
                        .is_some();
                    let result = if self.thread_active() {
                        self.live
                            .as_mut()
                            .expect("live")
                            .driver
                            .fetch_thread_history()
                    } else if topic_open {
                        self.live
                            .as_mut()
                            .expect("live")
                            .driver
                            .fetch_topic_history()
                    } else {
                        self.live.as_mut().expect("live").driver.fetch_history()
                    };
                    self.connection.status_note = match result {
                        Ok(Some(_)) => "loading older messages".into(),
                        Ok(None) => "no older messages to load".into(),
                        Err(_) => "could not load history".into(),
                    };
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
