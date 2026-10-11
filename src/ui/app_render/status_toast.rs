//! The status toast: which status notes show as a toast, and for how long.

use super::*;

impl QuillApp {
    /// Whether the live status toast shows this frame. A new note restarts
    /// its timer and schedules the re-render that hides it; failures stay
    /// up longer than confirmations.
    pub(super) fn status_toast_visible(&mut self, cx: &mut Context<Self>) -> bool {
        // `QUILL_TRACE_STATUS=1`: print every status note (toasted or not)
        // to stderr, for diagnosing a live session.
        if self.connection.status_note != self.connection.status_traced {
            self.connection.status_traced = self.connection.status_note.clone();
            if !self.connection.status_note.is_empty()
                && std::env::var_os("QUILL_TRACE_STATUS").is_some()
            {
                eprintln!("status: {}", self.connection.status_note);
            }
        }
        if self.live.is_none()
            || self.connection.status_note.is_empty()
            || !status_note_is_toast(&self.connection.status_note)
        {
            return false;
        }
        if self.connection.status_note != self.connection.status_seen {
            self.connection.status_seen = self.connection.status_note.clone();
            self.connection.status_shown_at = Some(std::time::Instant::now());
            let duration = status_toast_duration(&self.connection.status_note);
            let shown = self.connection.status_note.clone();
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(duration).await;
                let _ = this.update(cx, |this, cx| {
                    // Expired and unchanged: clear it, so the same message
                    // set again later (a repeated failure) shows again.
                    if this.connection.status_note == shown {
                        this.connection.status_note.clear();
                        this.connection.status_seen.clear();
                    }
                    cx.notify();
                });
            })
            .detach();
        }
        self.connection
            .status_shown_at
            .is_some_and(|at| at.elapsed() < status_toast_duration(&self.connection.status_note))
    }
}

/// Whether a status note deserves a toast. Like Telegram Desktop, only
/// failures, restrictions (Premium, slow mode, permissions) and
/// confirmations of actions with no visible result (copied, saved) toast.
/// Progress chatter ("sending…", "reaction updated") stays silent: the
/// screen already shows it, e.g. the bubble's sending clock.
pub(super) fn status_note_is_toast(note: &str) -> bool {
    let lower = note.to_lowercase();
    const FAILURE: [&str; 13] = [
        "fail",
        "could not",
        "couldn't",
        "can't",
        "cannot",
        "error",
        "offline",
        "denied",
        "not allowed",
        "unavailable",
        "not available",
        "too large",
        "unsupported",
    ];
    const RESTRICTION: [&str; 10] = [
        "disabled by admins",
        "is restricted",
        "premium",
        "slow mode",
        "wait ",
        "need",
        "requires",
        "only ",
        "limit",
        "will send when",
    ];
    const CONFIRMATION: [&str; 7] = [
        "copied",
        "saved to",
        "exported",
        "downloaded",
        "link",
        "hold ",
        "archived",
    ];
    FAILURE
        .iter()
        .chain(RESTRICTION.iter())
        .chain(CONFIRMATION.iter())
        .any(|word| lower.contains(word))
}

/// How long a status note stays on screen.
pub(super) fn status_toast_duration(note: &str) -> std::time::Duration {
    let lower = note.to_lowercase();
    let failure = [
        "fail",
        "could not",
        "couldn't",
        "can't",
        "cannot",
        "error",
        "offline",
    ]
    .iter()
    .any(|word| lower.contains(word));
    std::time::Duration::from_millis(if failure { 6000 } else { 3000 })
}
