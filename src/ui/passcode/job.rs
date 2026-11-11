//! The slow passcode jobs (PBKDF2 off the UI thread) and where their
//! answer lands.
use super::{PasscodeJob, QuillApp};
use gpui_kit::*;

impl QuillApp {
    /// Run a slow passcode operation (PBKDF2) off the UI thread. `done`
    /// runs in the window that started it, with that window's state in
    /// place: a wrong passcode empties and refocuses the lock field it was
    /// typed in (a chat window's is in its slot). If that window closed
    /// meanwhile, the answer lands in the main window.
    pub(in crate::ui) fn run_passcode_job<T: Send + 'static>(
        &mut self,
        job: impl FnOnce() -> T + Send + 'static,
        done: impl FnOnce(&mut Self, T, &mut Window, &mut Context<Self>) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.account.passcode.job = PasscodeJob::Running;
        self.account.passcode.error = None;
        let task = cx.background_executor().spawn(async move { job() });
        // screen-scope: the passcode is one lock over every account; only its window matters.
        // window-scope: `done` runs in its window (`in_window_of`); a held
        // lock reaches every window itself (`in_every_window`).
        cx.spawn_in(window, async move |this, cx| {
            // The answer waits here for whichever window takes it: the one
            // that asked, or the main window when that one closed meanwhile.
            let answer = std::rc::Rc::new(std::cell::RefCell::new(Some((task.await, done))));
            let asked = answer.clone();
            let ran = this.update_in(cx, move |this, window, cx| {
                let job = std::mem::take(&mut this.account.passcode.job);
                if let Some((out, done)) = asked.borrow_mut().take() {
                    this.in_window_of(window, cx, |this, window, cx| done(this, out, window, cx));
                }
                this.lock_held_for_job(job, cx);
                cx.notify();
            });
            if ran.is_err() {
                let _ = this.update(cx, |this, cx| {
                    let job = std::mem::take(&mut this.account.passcode.job);
                    match answer.borrow_mut().take() {
                        Some((out, done)) => {
                            this.finish_passcode_job_in_main_window(out, done, job, cx)
                        }
                        None => this.lock_held_for_job(job, cx),
                    }
                    cx.notify();
                });
            }
        })
        .detach();
        cx.notify();
    }

    /// The window that started a passcode job closed before it answered:
    /// the answer still lands, in the main window. A correct unlock would
    /// otherwise leave the master key set and every window locked, and a
    /// finished Turn off would leave the passcode on in settings with none
    /// on disk.
    fn finish_passcode_job_in_main_window<T: 'static>(
        &mut self,
        out: T,
        done: impl FnOnce(&mut Self, T, &mut Window, &mut Context<Self>) + 'static,
        job: PasscodeJob,
        cx: &mut Context<Self>,
    ) {
        let Some(main) = self.main_window.handle else {
            self.lock_held_for_job(job, cx);
            return;
        };
        let app = cx.weak_entity();
        // screen-scope: the passcode is the app's; the answer finishes the
        // account on screen, as the window that asked would have.
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                let _ = app.update(cx, |this, cx| {
                    this.in_window_scope(None, cx, |this, cx| {
                        done(this, out, window, cx);
                        this.lock_held_for_job(job, cx);
                    });
                });
            });
        });
    }
}
