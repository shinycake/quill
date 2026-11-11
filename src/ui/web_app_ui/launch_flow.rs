//! What a mini-app launch is waiting on, and the helper window's own
//! stages. One value each, so a launch never waits on two things at once
//! and a reply is matched to the bot that asked.

use super::{Awaited, WebAppConfirm};

/// The launch in progress: waiting on one answer, or on the person's box.
/// A newer launch replaces an older one; an answer for another bot is
/// dropped and the flow keeps waiting for its own.
#[derive(Default)]
pub(super) enum LaunchFlow {
    #[default]
    Idle,
    /// `searchWebApp` before the open box.
    Searching(Awaited),
    /// `getAttachmentMenuBot` (deep link) before the launch can go on.
    FetchingAttachBot(Awaited),
    /// `toggleBotIsAddedToAttachmentMenu` before the launch.
    Adding(Awaited),
    /// A box (terms, add to menu, remove from menu) waits for the person.
    Confirming(WebAppConfirm),
}

impl LaunchFlow {
    /// Something still has to answer.
    pub(super) fn is_waiting(&self) -> bool {
        matches!(
            self,
            Self::Searching(_) | Self::FetchingAttachBot(_) | Self::Adding(_)
        )
    }

    pub(super) fn confirm(&self) -> Option<&WebAppConfirm> {
        match self {
            Self::Confirming(confirm) => Some(confirm),
            _ => None,
        }
    }

    pub(super) fn confirm_mut(&mut self) -> Option<&mut WebAppConfirm> {
        match self {
            Self::Confirming(confirm) => Some(confirm),
            _ => None,
        }
    }

    pub(super) fn ask(&mut self, confirm: WebAppConfirm) {
        *self = Self::Confirming(confirm);
    }

    pub(super) fn take_confirm(&mut self) -> Option<WebAppConfirm> {
        match std::mem::take(self) {
            Self::Confirming(confirm) => Some(confirm),
            other => {
                *self = other;
                None
            }
        }
    }

    /// The box closed: only a waiting box goes, never a pending answer.
    pub(super) fn clear_confirm(&mut self) {
        drop(self.take_confirm());
    }

    pub(super) fn search(&mut self, awaited: Awaited) {
        *self = Self::Searching(awaited);
    }

    pub(super) fn fetch_attach_bot(&mut self, awaited: Awaited) {
        *self = Self::FetchingAttachBot(awaited);
    }

    pub(super) fn add(&mut self, awaited: Awaited) {
        *self = Self::Adding(awaited);
    }

    /// The search answer for `bot_id`; the flow ends with it.
    pub(super) fn take_search_for(&mut self, bot_id: i64) -> Option<Awaited> {
        match std::mem::take(self) {
            Self::Searching(awaited) if awaited.bot_id() == bot_id => Some(awaited),
            other => {
                *self = other;
                None
            }
        }
    }

    pub(super) fn take_attach_bot_for(&mut self, bot_id: i64) -> Option<Awaited> {
        match std::mem::take(self) {
            Self::FetchingAttachBot(awaited) if awaited.bot_id() == bot_id => Some(awaited),
            other => {
                *self = other;
                None
            }
        }
    }

    pub(super) fn take_add_for(&mut self, bot_id: i64) -> Option<Awaited> {
        match std::mem::take(self) {
            Self::Adding(awaited) if awaited.bot_id() == bot_id => Some(awaited),
            other => {
                *self = other;
                None
            }
        }
    }
}

/// The helper process: starting, loaded, or killed after it stopped reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HelperState {
    Starting,
    Ready,
    Wedged,
}

/// The app's "allow messaging" request: none, or one on its way. A second
/// one is refused while one waits. Each wait holds the request it is for,
/// and only that request's answer moves it; the person's popup is the way
/// out of `Asking`, and a closing window drops the lot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WriteAccess {
    Idle,
    /// `canBotSendMessages` is out.
    Checking(quill::ids::RequestId),
    /// The person has the consent popup.
    Asking,
    /// `allowBotToSendMessages` is out, after the person said yes.
    Allowing(quill::ids::RequestId),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::web_app_ui::PendingOpen;
    use core::prelude::v1::test;

    fn waiting(bot_id: i64) -> Awaited {
        Awaited {
            then: PendingOpen::Attach {
                chat_id: quill::ids::ChatId(1),
                bot_id,
            },
            window: None,
        }
    }

    #[test]
    fn an_answer_for_another_bot_keeps_the_flow_waiting() {
        let mut flow = LaunchFlow::default();
        flow.search(waiting(7));
        assert!(flow.take_search_for(8).is_none());
        assert!(flow.is_waiting());
        assert!(flow.take_attach_bot_for(7).is_none(), "wrong stage");
        assert!(flow.take_search_for(7).is_some());
        assert!(!flow.is_waiting());
    }

    #[test]
    fn a_newer_launch_replaces_the_older_wait() {
        let mut flow = LaunchFlow::default();
        flow.search(waiting(7));
        flow.add(waiting(9));
        assert!(flow.take_search_for(7).is_none());
        assert!(flow.take_add_for(9).is_some());
    }

    #[test]
    fn closing_the_box_leaves_a_pending_answer() {
        let mut flow = LaunchFlow::default();
        flow.fetch_attach_bot(waiting(7));
        flow.clear_confirm();
        assert!(flow.take_attach_bot_for(7).is_some());
    }
}
