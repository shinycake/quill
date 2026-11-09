//! What was asked while Quill was locked and waits for the unlock.
//!
//! The rule (docs/decisions/lock-is-a-render-state.md, "What runs while
//! locked"): work that comes from the user or from a late answer and would
//! show or start something waits for the unlock, or is refused. It is never
//! dropped without a word. A pump that drains stored state waits by not
//! running (`lock_waits`); a starter reached outside those pumps, such as an
//! OS open from a late answer, queues itself here. Each intent runs the
//! starter again after the unlock, so it asks every gate a second time.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use gpui_kit::Context;

type Intent = Box<dyn FnOnce(&mut QuillApp, &mut Context<QuillApp>)>;

/// The queue. Late answers are few (one per request the user made before
/// the lock), so it is not capped.
#[derive(Default)]
pub(in crate::ui) struct UnlockIntents {
    queued: Vec<Intent>,
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
        self.account
            .passcode
            .unlock_intents
            .queued
            .push(Box::new(intent));
    }

    /// The unlock: what waited runs now. An intent that finds Quill locked
    /// again (it locked inside an earlier one) queues itself once more.
    pub(super) fn run_unlock_intents(&mut self, cx: &mut Context<Self>) {
        let intents = std::mem::take(&mut self.account.passcode.unlock_intents.queued);
        for intent in intents {
            intent(self, cx);
        }
    }
}
