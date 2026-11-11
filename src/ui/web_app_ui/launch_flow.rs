//! What a mini-app launch is waiting on, and the helper window's own
//! stages. One value each, so a launch never waits on two things at once
//! and a reply is matched to the bot that asked.

use super::{Awaited, WebAppConfirm};
use quill::ids::RequestId;

/// The launch in progress: waiting on one answer, or on the person's box.
/// A newer launch replaces an older one; an answer of another request is
/// dropped and the flow keeps waiting for its own.
#[derive(Default)]
pub(super) enum LaunchFlow {
    #[default]
    Idle,
    /// `searchWebApp` before the open box; only the answer of `request`
    /// goes on (another link of the same bot is another request).
    Searching {
        request: RequestId,
        awaited: Awaited,
    },
    /// `getAttachmentMenuBot` (deep link) before the launch can go on; only
    /// the answer of `request` goes on.
    FetchingAttachBot {
        request: RequestId,
        awaited: Awaited,
    },
    /// `toggleBotIsAddedToAttachmentMenu` before the launch; only the
    /// outcome of `request` goes on.
    Adding {
        request: RequestId,
        awaited: Awaited,
    },
    /// A box (terms, add to menu, remove from menu) waits for the person.
    Confirming(WebAppConfirm),
}

impl LaunchFlow {
    /// Something still has to answer.
    pub(super) fn is_waiting(&self) -> bool {
        matches!(
            self,
            Self::Searching { .. } | Self::FetchingAttachBot { .. } | Self::Adding { .. }
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

    pub(super) fn search(&mut self, request: RequestId, awaited: Awaited) {
        *self = Self::Searching { request, awaited };
    }

    pub(super) fn fetch_attach_bot(&mut self, request: RequestId, awaited: Awaited) {
        *self = Self::FetchingAttachBot { request, awaited };
    }

    pub(super) fn add(&mut self, request: RequestId, awaited: Awaited) {
        *self = Self::Adding { request, awaited };
    }

    /// The search answer of `request`; the flow ends with it.
    pub(super) fn take_search_for(&mut self, answered: RequestId) -> Option<Awaited> {
        match std::mem::take(self) {
            Self::Searching { request, awaited } if request == answered => Some(awaited),
            other => {
                *self = other;
                None
            }
        }
    }

    pub(super) fn take_attach_bot_for(&mut self, answered: RequestId) -> Option<Awaited> {
        match std::mem::take(self) {
            Self::FetchingAttachBot { request, awaited } if request == answered => Some(awaited),
            other => {
                *self = other;
                None
            }
        }
    }

    pub(super) fn take_add_for(&mut self, answered: RequestId) -> Option<Awaited> {
        match std::mem::take(self) {
            Self::Adding { request, awaited } if request == answered => Some(awaited),
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
        flow.search(RequestId(5), waiting(7));
        assert!(flow.take_search_for(RequestId(4)).is_none());
        assert!(flow.is_waiting());
        assert!(
            flow.take_attach_bot_for(RequestId(5)).is_none(),
            "wrong stage"
        );
        assert!(flow.take_search_for(RequestId(5)).is_some());
        assert!(!flow.is_waiting());
    }

    #[test]
    fn a_newer_launch_replaces_the_older_wait() {
        let mut flow = LaunchFlow::default();
        flow.search(RequestId(5), waiting(7));
        flow.add(RequestId(6), waiting(9));
        assert!(flow.take_search_for(RequestId(5)).is_none());
        assert!(flow.take_add_for(RequestId(5)).is_none(), "another request");
        assert!(flow.take_add_for(RequestId(6)).is_some());
    }

    #[test]
    fn closing_the_box_leaves_a_pending_answer() {
        let mut flow = LaunchFlow::default();
        flow.fetch_attach_bot(RequestId(8), waiting(7));
        flow.clear_confirm();
        assert!(flow.take_attach_bot_for(RequestId(8)).is_some());
    }
}
