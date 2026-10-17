//! The status toast: shows the current status note for a few seconds. Which
//! notes toast, and for how long, comes from the note's kind
//! (`ui/status_note.rs`), not from its words.

use super::*;

impl QuillApp {
    /// Whether the live status toast shows this frame. A new note restarts
    /// its timer and schedules the re-render that hides it; failures stay
    /// up longer than confirmations.
    pub(super) fn status_toast_visible(&mut self, cx: &mut Context<Self>) -> bool {
        self.drain_suggested_post_notes(cx);
        // `QUILL_TRACE_STATUS=1`: print every status note (toasted or not)
        // to stderr, for diagnosing a live session.
        if self.connection.status_note() != self.connection.status_traced {
            self.connection.status_traced = self.connection.status_note().to_string();
            if !self.connection.status_traced.is_empty()
                && std::env::var_os("QUILL_TRACE_STATUS").is_some()
            {
                eprintln!("status: {}", self.connection.status_traced);
            }
        }
        if !self.connection.note().is_toast() {
            return false;
        }
        if self.live.is_none() {
            return self.demo_pins_toast();
        }
        if *self.connection.note() != self.connection.status_seen {
            self.connection.status_seen = self.connection.note().clone();
            self.connection.status_shown_at = Some(std::time::Instant::now());
            let duration = self.connection.note().toast_duration();
            let shown = self.connection.note().clone();
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(duration).await;
                let _ = this.update(cx, |this, cx| {
                    // Expired and unchanged: clear it, so the same message
                    // set again later (a repeated failure) shows again.
                    if *this.connection.note() == shown {
                        this.connection.clear_note();
                        this.connection.status_seen = Default::default();
                    }
                    cx.notify();
                });
            })
            .detach();
        }
        let duration = self.connection.note().toast_duration();
        self.connection
            .status_shown_at
            .is_some_and(|at| at.elapsed() < duration)
    }

    /// Screenshot demos have no live connection, so no toast; with
    /// `QUILL_DEMO_SHOW_TOAST=1` the demo's note stays up as the toast, for
    /// the captures that prove how a toast looks.
    #[cfg(feature = "demo-capture")]
    fn demo_pins_toast(&self) -> bool {
        self.demo_session.is_some() && std::env::var_os("QUILL_DEMO_SHOW_TOAST").is_some()
    }

    #[cfg(not(feature = "demo-capture"))]
    fn demo_pins_toast(&self) -> bool {
        false
    }
}
