//! The mini-app poll: it sleeps until the helper's reader thread or an
//! applied mini-app answer pokes [`quill::wake::MINI_APPS`], so a hidden or
//! idle app costs no wake-ups. Only the stages that watch other state
//! (a suggested emoji lookup, the phone and payment flows) tick on a short
//! timer while they last.

use super::*;
use crate::ui::status_note::StatusNote;
use crate::ui::wake_wait::{PassEnd, PollPace, backlog_hold, run_poll_loop};
use quill::lang::{keys, tr};

/// A poke is applied after at most `gap`; `ceiling` is the longest an
/// answer with no poke of its own can wait.
const POLL_PACE: PollPace = PollPace {
    gap: Duration::from_millis(30),
    ceiling: Duration::from_secs(5),
};
/// How often a stage that watches other state looks again.
const WATCH_TICK: Duration = Duration::from_millis(250);

impl QuillApp {
    /// Poll the helper and the session's mini-app answers while something
    /// is going on; stops by itself when nothing is left.
    pub(in crate::ui) fn ensure_mini_app_poll(&mut self, cx: &mut Context<Self>) {
        if let Some(window) = self.asking_window() {
            self.mini_apps.from_window = window;
        }
        if self.mini_apps.poll.is_some() {
            return;
        }
        self.mini_apps.generation += 1;
        let generation = self.mini_apps.generation;
        // window-scope: the mini app is the screen's; a step aimed at a chat names its chat.
        let task = self.spawn_screen(cx, async move |this, cx| {
            let executor = cx.background_executor().clone();
            let hold = backlog_hold(&executor);
            run_poll_loop(executor, &quill::wake::MINI_APPS, POLL_PACE, hold, || {
                this.update(cx, |this, cx| {
                    let keep = this.mini_apps.generation == generation
                        && this.as_asked_by(this.mini_apps.from_window, |this| {
                            this.poll_mini_apps(cx)
                        });
                    if keep {
                        return Some(PassEnd {
                            work_left: false,
                            due: this.mini_app_watch_due(),
                        });
                    }
                    if this.mini_apps.generation == generation {
                        this.mini_apps.poll = None;
                    }
                    None
                })
                .ok()
                .flatten()
            })
            .await;
        });
        self.mini_apps.poll = Some(task);
    }

    /// One poll step; `true` while there is still something to wait for.
    pub(super) fn poll_mini_apps(&mut self, cx: &mut Context<Self>) -> bool {
        self.poll_mini_app_session(cx);
        let events: Vec<HelperEvent> = match self.mini_apps.window.as_ref() {
            Some(window) => window.events.try_iter().collect(),
            None => Vec::new(),
        };
        for event in events {
            self.handle_helper_event(event, cx);
        }
        let overdue = self.mini_apps.window.as_ref().is_some_and(|window| {
            window.helper.start_due_in(std::time::Instant::now()) == Some(Duration::ZERO)
        });
        if overdue {
            // The helper started but never said ready: close the launch.
            self.close_mini_app_window(cx);
            self.set_status_note(
                StatusNote::error(tr(keys::Q_MEDIA_THE_MINI_APP_WINDOW_COULDNT)),
                cx,
            );
        }
        self.mini_apps.window.is_some()
            || self.mini_apps.flow.is_waiting()
            || self.session().is_some_and(|session| {
                session.bots.web_apps.opening.is_some() || session.web_app_answer_due()
            })
    }

    /// When a stage that watches other state (nothing pokes for it) wants
    /// its next look: while one is open.
    fn mini_app_watch_due(&self) -> Option<Duration> {
        let window = self.mini_apps.window.as_ref()?;
        if let Some(due) = window.helper.start_due_in(std::time::Instant::now()) {
            return Some(due.max(WATCH_TICK));
        }
        let watching = window.emoji.is_active()
            || window.phone != phone::PhoneFlow::Idle
            || window.invoice.is_some();
        watching.then_some(WATCH_TICK)
    }
}
