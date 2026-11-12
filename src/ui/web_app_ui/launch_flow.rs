//! What a mini-app launch is waiting on, and the helper window's own
//! stages. One value each, so a launch never waits on two things at once
//! and a reply is matched to the request that asked.

use super::{Awaited, PopupId, WebAppConfirm};
use quill::ids::RequestId;

/// The launch in progress: waiting on one answer, or on the person's box.
/// A newer launch replaces an older one; an answer for another request is
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

    /// The request the flow waits on, if it waits on one.
    pub(super) fn awaited_request(&self) -> Option<RequestId> {
        match self {
            Self::Searching(awaited) | Self::FetchingAttachBot(awaited) | Self::Adding(awaited) => {
                Some(awaited.request)
            }
            Self::Idle | Self::Confirming(_) => None,
        }
    }

    /// The wait ends with no answer (its request left flight: dropped by
    /// the stale sweep or a reconnect): the awaited launch comes back and
    /// the flow is idle.
    pub(super) fn take_lost(&mut self) -> Option<Awaited> {
        match std::mem::take(self) {
            Self::Searching(awaited) | Self::FetchingAttachBot(awaited) | Self::Adding(awaited) => {
                Some(awaited)
            }
            other => {
                *self = other;
                None
            }
        }
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

    /// The search answer of `request`; the flow ends with it.
    pub(super) fn take_search_for(&mut self, request: RequestId) -> Option<Awaited> {
        match std::mem::take(self) {
            Self::Searching(awaited) if awaited.request == request => Some(awaited),
            other => {
                *self = other;
                None
            }
        }
    }

    pub(super) fn take_attach_bot_for(&mut self, request: RequestId) -> Option<Awaited> {
        match std::mem::take(self) {
            Self::FetchingAttachBot(awaited) if awaited.request == request => Some(awaited),
            other => {
                *self = other;
                None
            }
        }
    }

    pub(super) fn take_add_for(&mut self, request: RequestId) -> Option<Awaited> {
        match std::mem::take(self) {
            Self::Adding(awaited) if awaited.request == request => Some(awaited),
            other => {
                *self = other;
                None
            }
        }
    }
}

/// The helper process: starting, loaded, or killed after it stopped reading.
/// A start that says nothing for [`HELPER_START_TIMEOUT`] is given up: the
/// window closes and TDLib hears `closeWebApp`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HelperState {
    Starting { since: std::time::Instant },
    Ready,
    Wedged,
}

/// How long a helper may take to report ready.
pub(super) const HELPER_START_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

impl HelperState {
    pub(super) fn starting(now: std::time::Instant) -> Self {
        Self::Starting { since: now }
    }

    /// How long until a start that has not reported ready is given up
    /// (zero once it is); `None` for a helper that is ready or wedged.
    pub(super) fn start_due_in(self, now: std::time::Instant) -> Option<std::time::Duration> {
        match self {
            Self::Starting { since } => {
                Some(HELPER_START_TIMEOUT.saturating_sub(now.saturating_duration_since(since)))
            }
            Self::Ready | Self::Wedged => None,
        }
    }
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
    /// The person has the consent popup with this id; only its answer
    /// moves the stage.
    Asking {
        popup: PopupId,
    },
    /// `allowBotToSendMessages` is out, after the person said yes.
    Allowing(quill::ids::RequestId),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::web_app_ui::PendingOpen;
    use core::prelude::v1::test;

    fn waiting(request: u64, bot_id: i64) -> Awaited {
        Awaited {
            request: RequestId(request),
            then: PendingOpen::Attach {
                chat_id: quill::ids::ChatId(1),
                bot_id,
            },
            window: None,
        }
    }

    #[test]
    fn an_answer_for_another_request_keeps_the_flow_waiting() {
        let mut flow = LaunchFlow::default();
        flow.search(waiting(1, 7));
        assert!(flow.take_search_for(RequestId(2)).is_none());
        assert!(flow.is_waiting());
        assert!(
            flow.take_attach_bot_for(RequestId(1)).is_none(),
            "wrong stage"
        );
        assert!(flow.take_search_for(RequestId(1)).is_some());
        assert!(!flow.is_waiting());
    }

    #[test]
    fn two_links_of_one_bot_each_take_only_their_own_answer() {
        let mut flow = LaunchFlow::default();
        flow.search(waiting(1, 7));
        flow.search(waiting(2, 7));
        assert!(
            flow.take_search_for(RequestId(1)).is_none(),
            "the older link's answer is nobody's"
        );
        assert!(flow.take_search_for(RequestId(2)).is_some());
    }

    #[test]
    fn a_helper_that_never_reports_ready_is_given_up() {
        use std::time::{Duration, Instant};
        let began = Instant::now();
        let helper = HelperState::starting(began);
        assert_eq!(helper.start_due_in(began), Some(HELPER_START_TIMEOUT));
        let late = began + HELPER_START_TIMEOUT + Duration::from_secs(1);
        assert_eq!(helper.start_due_in(late), Some(Duration::ZERO));
        assert_eq!(HelperState::Ready.start_due_in(late), None);
        assert_eq!(HelperState::Wedged.start_due_in(late), None);
    }

    #[test]
    fn a_newer_launch_replaces_the_older_wait() {
        let mut flow = LaunchFlow::default();
        flow.search(waiting(1, 7));
        flow.add(waiting(2, 9));
        assert!(flow.take_search_for(RequestId(1)).is_none());
        assert!(flow.take_add_for(RequestId(2)).is_some());
    }

    #[test]
    fn a_wait_whose_request_left_flight_is_given_up() {
        let mut flow = LaunchFlow::default();
        flow.search(waiting(1, 7));
        assert_eq!(flow.awaited_request(), Some(RequestId(1)));
        let lost = flow.take_lost();
        assert_eq!(lost.map(|awaited| awaited.request), Some(RequestId(1)));
        assert!(!flow.is_waiting());
        // A box waiting for the person is not a request.
        flow.ask(WebAppConfirm::RemoveFromMenu {
            bot_id: 7,
            name: String::new(),
        });
        assert_eq!(flow.awaited_request(), None);
        assert!(flow.take_lost().is_none());
        assert!(flow.confirm().is_some());
    }

    #[test]
    fn closing_the_box_leaves_a_pending_answer() {
        let mut flow = LaunchFlow::default();
        flow.fetch_attach_bot(waiting(1, 7));
        flow.clear_confirm();
        assert!(flow.take_attach_bot_for(RequestId(1)).is_some());
    }
}
