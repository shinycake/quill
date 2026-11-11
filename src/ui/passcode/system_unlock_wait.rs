//! The wait for a Touch ID / system password answer on the lock screen.
//! The window that asked finishes the unlock, with its own state in place;
//! if it closed meanwhile, the main window does, and the job never stays running
//! (a stuck `busy` refused every later unlock until a restart).

use super::*;

impl QuillApp {
    pub(in crate::ui) fn spawn_system_unlock_wait(
        &mut self,
        receiver: std::sync::mpsc::Receiver<bool>,
        master: MasterKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // screen-scope: the passcode lock is the app's, not an account's; unlocking
        // finishes whichever account is on screen (it starts a deferred connect).
        Self::spawn_app_in(window, cx, async move |this, cx| {
            let ok = loop {
                match receiver.try_recv() {
                    Ok(ok) => break ok,
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        cx.background_executor()
                            .timer(Duration::from_millis(100))
                            .await;
                    }
                    Err(_) => break false,
                }
            };
            // The master key goes with the answer, unless a log-out from the
            // lock screen cancelled the job meanwhile.
            let master = std::rc::Rc::new(std::cell::RefCell::new(Some(master)));
            let held = master.clone();
            // The window that asked finishes, with its own state in place.
            let ran = this.update_in(cx, move |this, window, cx| {
                let counts = this.take_system_unlock_job() && ok;
                if let Some(master) = held.borrow_mut().take().filter(|_| counts) {
                    global_unlock().set(master);
                    this.in_window_of(window, cx, |this, window, cx| this.unlock_done(window, cx));
                }
                cx.notify();
            });
            // It closed meanwhile: the answer still counts, in the main window.
            if ran.is_err() {
                let _ = this.update(cx, |this, cx| {
                    let counts = this.take_system_unlock_job() && ok;
                    if let Some(master) = master.borrow_mut().take().filter(|_| counts) {
                        global_unlock().set(master);
                        this.unlock_in_main_window(cx);
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// The job ends. Whether its answer still counts: a log-out from the
    /// lock screen cancelled it.
    fn take_system_unlock_job(&mut self) -> bool {
        std::mem::take(&mut self.account.passcode.job) != PasscodeJob::Cancelled
    }

    fn unlock_in_main_window(&mut self, cx: &mut Context<Self>) {
        let Some(main) = self.main_window.handle else {
            return;
        };
        let app = cx.weak_entity();
        // screen-scope: the lock is the app's; unlocking finishes whichever
        // account is on screen, as the window that asked would have.
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                let _ = app.update(cx, |this, cx| {
                    this.in_window_scope(None, cx, |this, cx| this.unlock_done(window, cx));
                });
            });
        });
    }
}
