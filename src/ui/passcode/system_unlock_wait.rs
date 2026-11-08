//! The wait for a Touch ID / system password answer on the lock screen.
//! The window that asked finishes the unlock, with its own state in place;
//! if it closed meanwhile, the main window does, and `busy` never stays set
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
            if ok {
                global_unlock().set(master);
            }
            // The window that asked finishes, with its own state in place.
            let ran = this.update_in(cx, |this, window, cx| {
                this.account.passcode.busy = false;
                if ok {
                    this.in_window_of(window, cx, |this, window, cx| this.unlock_done(window, cx));
                }
                cx.notify();
            });
            // It closed meanwhile: the answer still counts, in the main window.
            if ran.is_err() {
                let _ = this.update(cx, |this, cx| {
                    this.account.passcode.busy = false;
                    if ok {
                        this.unlock_in_main_window(cx);
                    }
                    cx.notify();
                });
            }
        })
        .detach();
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
