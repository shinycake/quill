//! What was asked while Quill was locked and waits for the unlock.
//!
//! The rule (docs/decisions/lock-is-a-render-state.md, "What runs while
//! locked"): work that comes from the user or from a late answer and would
//! show or start something waits for the unlock, or is refused. It is never
//! dropped without a word. A pump that drains stored state waits by not
//! running (`lock_waits`); a starter reached outside those pumps, such as an
//! OS open from a late answer, queues itself here. Each intent runs the
//! starter again after the unlock, so it asks every gate a second time.
//!
//! An intent keeps who asked: the account on screen and the window. The
//! lock card's Answer can switch accounts under the lock, and the unlock
//! runs in the window the passcode was typed in, so neither is the one in
//! place when it runs. It runs as its window (the main window when that
//! one closed), and is dropped when its account is no longer on screen: a
//! link resolved for one account must not open in another.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use gpui_kit::Context;
use quill::ids::{AccountKey, ChatId};

type Intent = Box<dyn FnOnce(&mut QuillApp, &mut Context<QuillApp>)>;

struct Queued {
    account: Option<AccountKey>,
    /// The window that asked (`None`: the main window).
    window: Option<ChatId>,
    run: Intent,
}

/// The queue. Late answers are few (one per request the user made before
/// the lock), so it is not capped.
#[derive(Default)]
pub(in crate::ui) struct UnlockIntents {
    queued: Vec<Queued>,
}

impl UnlockIntents {
    #[cfg(test)]
    pub(in crate::ui) fn len(&self) -> usize {
        self.queued.len()
    }

    /// The lock screen's Log out wiped the accounts these were for.
    pub(in crate::ui) fn drop_all(&mut self) {
        self.queued.clear();
    }
}

impl QuillApp {
    /// Run `intent` after the unlock, in the order asked.
    pub(in crate::ui) fn wait_for_unlock(
        &mut self,
        intent: impl FnOnce(&mut QuillApp, &mut Context<QuillApp>) + 'static,
    ) {
        let queued = Queued {
            account: self.current_account(),
            window: self.asking_window_or_last_press(),
            run: Box::new(intent),
        };
        self.account.passcode.unlock_intents.queued.push(queued);
    }

    /// The unlock: what waited runs now. An intent that finds Quill locked
    /// again (it locked inside an earlier one) queues itself once more.
    pub(super) fn run_unlock_intents(&mut self, cx: &mut Context<Self>) {
        let intents = std::mem::take(&mut self.account.passcode.unlock_intents.queued);
        for Queued {
            account,
            window,
            run,
        } in intents
        {
            if account != self.current_account() {
                continue;
            }
            let window = window.filter(|chat| self.windows.has(*chat));
            self.in_window_scope(window, cx, |this, cx| {
                this.as_asked_by(window, |this| run(this, cx))
            });
        }
    }
}
